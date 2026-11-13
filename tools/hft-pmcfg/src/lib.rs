//! Polymarket configuration generator (README "使用本组件"): the book-observer sim, the
//! binary-maker sim, the live maker with one or more Live accounts and the live taker probe
//! (pm-taker-probe instead of binary-maker). Every mode writes
//! `refdata.bin`, `engine.bin`, `risk_params.bin`, `strategy_params.bin`, `principals.toml` and
//! `core.toml` (blob paths and SHA-256) into the output directory. The command line is [`cli`].
use std::path::{Path, PathBuf};

use hft_event::{GroupSpec, LegDef, ParamSource, StrategyParamsHdr};
use hft_exec::{ExecAccount, ExecConfig, ExecParams};
use hft_ledger::{Capacities, LedgerConfig};
use hft_oms::OmsConfig;
use hft_refdata::*;
use hft_risk::GwMap;
use hft_sequencer::{CapacitiesRow, EngineSetup, RingLimit, SegClock, SeqParamsRow};
use hft_strategy_registry::Strategies;
use hft_strategy_sdk::{
    StrategySet, VenueSet,
    params::{RawParams, SchemaView, encode_body},
};
use hft_types::*;
use zerocopy::IntoBytes;

pub mod cli;

/// Crossed tolerance of the Polymarket venue and slot insts: the book stays Live through at most this
/// many consecutive messages leaving bid ≥ ask, the next one makes it `Stale(Crossed)` until a snapshot.
/// A taker sweep briefly locks the published book (the new level before the match removes it): on
/// the 2026-10-26 live journal 91.5 % of such episodes clear within 1 message, 99.7 % within 3, all
/// by an incremental update; with 0 every sweep made the book Stale and the maker cancel its quotes.
pub const PM_CROSSED_TOLERANCE: u16 = 3;

/// Coins of `write_polymarket_sim`; slot `<coin>-<up|dn>-<k>` for windows k = 0, 1.
pub const PM_COINS: [&str; 4] = ["btc", "eth", "sol", "xrp"];

/// `venues.pm.presign` of the live configuration: two prices below and two above the last place
/// in 0.01 USDC steps (the Polymarket tick), re-signed within one minute (venue-polymarket "预签名").
pub const PM_LIVE_PRESIGN: &str = "below=2
above=2
step=\"0.01\"
max_age_ms=60000";

/// `PmLive.presign_grid` in 1e-6 shares: digits with at most 6 decimal places, > 0, and a size
/// venue-polymarket's price grid accepts with `PM_LIVE_PRESIGN`'s step (0.01).
pub fn presign_grid_e6(shares: &str) -> Option<u32> {
    let (int, frac) = shares.split_once('.').unwrap_or((shares, ""));
    if int.is_empty() || frac.len() > 6 || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let frac: u32 = format!("{frac:0<6}").parse().ok()?;
    let e6 = int
        .parse::<u32>()
        .ok()?
        .checked_mul(1_000_000)?
        .checked_add(frac)?;
    venue_polymarket::Presign::new(2, 2, 10_000, 60_000)?
        .with_grid(e6)
        .map(|p| p.grid_qty_e6)
}

/// Live `daily_loss_abs` without `PmLive.daily_loss_abs` (15 USDC).
pub const PM_LIVE_DAILY_LOSS: i64 = 15_000_000;

/// Binary-maker parameters of the first live run (`micro_s2_c5_cap10_mc20`, research reply
/// 2026-10-22): 5 shares per quote, net 10, no-chase 1 s, fair-value cooldown 1 s, streak pause
/// (2 fills within 10 s → 10 s), at most 20 USDC bought per window.
/// (Defined by binary-maker as `LIVE_BASE`; the replayer's `--param-base live` uses the same list.)
pub const PM_LIVE_PARAMS: [(&str, i64); 9] = hft_strategy_registry::BINARY_MAKER_LIVE_BASE;

/// Live Polymarket: OMS `ack_timeout_ms` and the REST request timeout (equal: the answer is read
/// as long as the OMS waits for it), and the REST connections' maximum idle age.
pub const PM_REST_TIMEOUT_MS: u32 = 5000;
pub const PM_REST_MAX_IDLE_MS: u32 = 25_000;
/// Live Polymarket: a REST connection of the gw that has not answered a request yet, or has been
/// idle this long, gets the keep-alive `GET /time` (hft-net "9. REST 客户端", 保活).
pub const PM_REST_KEEPALIVE_MS: u32 = 5000;

/// Label of the first Live account (`--live`).
pub const PM_LIVE_LABEL: &str = "pm-live";

/// `rr_max_cross` of the taker probe's price band (`--live --taker-probe`): its sell-back is a
/// risk-reducing Ioc at the best bid, half a spread below mid — 0.005 of 0.105 is 4.8 % — which
/// the maker's 1 % would refuse; 50 % as `max_px_dev`.
pub const PM_PROBE_RR_MAX_CROSS: i64 = 500_000_000;

/// `ioc_max_cross` of the generated price band (hft-risk: how far beyond the touch an Ioc order may
/// bid, as a fraction of the touch, 1e-9): the default, and the most `--ioc-cross` may set.
pub const PM_IOC_CROSS: i64 = 5_000_000;
pub const PM_IOC_CROSS_MAX: i64 = 200_000_000;

/// The strategy of a generated configuration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PmStrategy {
    /// book-observer: one group per slot, never trades (the sim without a mode flag).
    Observer,
    /// binary-maker: one group per slot plus the reference group (and the Coinbase reference).
    Maker,
    /// pm-taker-probe: one group, slot `btc-up-0` (InstId 1).
    TakerProbe,
}

impl PmStrategy {
    /// Registry name of the strategy kind.
    pub fn kind_name(self) -> &'static str {
        match self {
            PmStrategy::Observer => "book-observer",
            PmStrategy::Maker => "binary-maker",
            PmStrategy::TakerProbe => "pm-taker-probe",
        }
    }
}

/// Further Live accounts at most (`--live-account` + `--account`): the gw serves
/// `KillBits::MAX_ACCOUNTS_PER_GW` accounts per venue, one of them is `pm-live`.
pub const MAX_EXTRA_ACCOUNTS: usize = KillBits::MAX_ACCOUNTS_PER_GW as usize - 1;

/// Role of a Live account (`--account LABEL:UID:ROLE`). Only `any` exists until the account-role
/// model (routing by role) is implemented; the generated configuration does not depend on it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AccountRole {
    /// The account takes any order of the instance.
    Any,
}

impl AccountRole {
    /// `any` → `Any`; every other value is refused (no other role is implemented).
    pub fn parse(s: &str) -> Result<AccountRole, String> {
        match s {
            "any" => Ok(AccountRole::Any),
            other => Err(format!(
                "account role `{other}`: only `any` is supported (account roles are not implemented yet)"
            )),
        }
    }
}

/// A further Live account of the live maker (accounts 2, 3, …, in this order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAccount {
    /// Refdata label; `None` = `pm-live-<account id>` (`--live-account`).
    pub label: Option<String>,
    /// Venue UID (the first 24 characters of the account's CLOB API key).
    pub uid: String,
    /// Directory of the account's credential files (`api_key`, `secret`, `passphrase`).
    pub keys_dir: PathBuf,
    pub role: AccountRole,
}

impl LiveAccount {
    /// `--live-account UID KEYS_DIR`: label `pm-live-<id>`, role `any`.
    pub fn shorthand(uid: impl Into<String>, keys_dir: impl Into<PathBuf>) -> LiveAccount {
        LiveAccount {
            label: None,
            uid: uid.into(),
            keys_dir: keys_dir.into(),
            role: AccountRole::Any,
        }
    }
}

/// The live accounts and limits of `write_polymarket_maker_live`.
#[derive(Clone, Debug)]
pub struct PmLive {
    /// Venue UID of `pm-live`: the first 24 characters of the CLOB API key (venue-polymarket
    /// README "账户配置核验").
    pub uid: String,
    /// Directory of the credential files `api_key`, `secret` (`<api secret>|<EOA key>|<deposit
    /// wallet>`) and `passphrase`; hft-core reads and unlinks them at startup.
    pub keys_dir: PathBuf,
    /// Day loss of earlier runs of this UTC day: `(UTC day number, USDC raw 1e-6, < 0)`, booked at
    /// the first day roll of the fresh journal (hft-sequencer ECF1 section 8 fields 7/8).
    pub day_loss_carry: Option<(u32, i64)>,
    /// `daily_loss_abs` of `pm-live` (USDC raw 1e-6, > 0); `None` = 15 USDC.
    pub daily_loss_abs: Option<i64>,
    /// `daily_loss_max_resumes` of `pm-live`; `None` = the hft-risk default (1).
    pub daily_loss_max_resumes: Option<u8>,
    /// `max_account_notional` and `max_venue_notional` of `pm-live` (USDC raw 1e-6, > 0); `None` = 55 USDC.
    pub account_cap: Option<i64>,
    /// `max_group_notional` of every `pm-live` slot group (USDC raw 1e-6, > 0); `None` = 25 USDC.
    pub group_cap: Option<i64>,
    /// `ioc_max_cross` of the price band (fraction of the touch, 1e-9; > 0 and at most
    /// `PM_IOC_CROSS_MAX`); `None` = `PM_IOC_CROSS` (0.5 %, under one cent for every outcome
    /// token). binary-maker rule 18 with `take_slip_c` > 0 needs more.
    pub ioc_max_cross: Option<i64>,
    /// `venues.pm.presign` = `PM_LIVE_PRESIGN`; false (`--no-presign`) = no table (off).
    pub presign: bool,
    /// `--presign-grid SHARES`: `venues.pm.presign.grid_qty` (venue-polymarket "预签名" price
    /// grid: a buy of this many shares pre-signed at every 0.01), decimal text > 0 with at most 6
    /// places; needs `presign`. `None` = not written (no grid).
    pub presign_grid: Option<String>,
    /// `venues.pm.gw.cancel_race` (hft-venue "撤单赛跑"; owner decision 2026-11-06: on in live);
    /// false (`--no-cancel-race`) = off.
    pub cancel_race: bool,
    /// Further Live accounts 2, 3, … (`--live-account`, `--account`), bound to the same
    /// binary-maker instance with `pm-live`'s risk limits; the strategy parameters stay one blob
    /// under `pm-live`.
    pub extra_accounts: Vec<LiveAccount>,
    /// `[venues.cb]` (Coinbase BTC-USD, InstId 8) is written; false (`--no-coinbase`) = the
    /// configuration without it.
    pub coinbase: bool,
    /// `[venues.bp]` (Binance USDT-M perpetual BTCUSDT, market data only) is written; false =
    /// `--no-perp`.
    pub perp: bool,
    /// `[venues.cl]` (Chainlink BTC/USD price and 60 s TWAP, `polymarket-price`) is written,
    /// logging in with `cl_keys` or else `keys_dir`; false = `--no-chainlink`.
    pub chainlink: bool,
    /// `--cl-keys DIR`: the `api_key`/`secret`/`passphrase` files of the Chainlink login; `None`
    /// = `keys_dir` (the same files as `pm-live`'s, read once).
    pub cl_keys: Option<PathBuf>,
    /// `--cb-perp`: `[venues.cp]` (the Coinbase perpetual-style future `PM_CB_PERP_SYMBOL`,
    /// `coinbase-adv`, market data only) is written and binary-maker's `ref_aux` gets bit 2.
    pub cb_perp: bool,
}

impl PmLive {
    /// The reference feeds of this live configuration.
    pub fn refs(&self) -> Refs {
        Refs {
            coinbase: self.coinbase,
            perp: self.perp,
            chainlink: self
                .chainlink
                .then(|| self.cl_keys.clone().unwrap_or_else(|| self.keys_dir.clone())),
            cb_perp: self.cb_perp,
        }
    }
}

/// Reference markets of the maker configurations besides the Binance spot reference (README
/// "参考行情").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refs {
    /// `[venues.cb]`: Coinbase BTC-USD.
    pub coinbase: bool,
    /// `[venues.bp]`: Binance USDT-M perpetual BTCUSDT (bookTicker, depth5@100ms, aggTrade).
    pub perp: bool,
    /// `[venues.cl]`: Chainlink BTC/USD price and TWAP; the directory of the login files
    /// (`api_key`, `secret`, `passphrase`; read and unlinked by hft-core at start).
    pub chainlink: Option<PathBuf>,
    /// `[venues.cp]`: the Coinbase perpetual-style future (level-2 best quotes).
    pub cb_perp: bool,
}

/// The Coinbase Derivatives perpetual-style BTC future on Advanced Trade (tick 5 USD, one
/// contract = 0.01 BTC; `--cb-perp`).
pub const PM_CB_PERP_SYMBOL: &str = "BIP-20DEC30-CDE";

impl PmLive {
    /// Labels of accounts 1, 2, … (`pm-live`, then each extra account's label or `pm-live-<id>`).
    pub fn labels(&self) -> Vec<String> {
        let mut v = vec![PM_LIVE_LABEL.to_string()];
        for (i, a) in self.extra_accounts.iter().enumerate() {
            v.push(
                a.label
                    .clone()
                    .unwrap_or_else(|| format!("{PM_LIVE_LABEL}-{}", i + 2)),
            );
        }
        v
    }

    /// At most `MAX_EXTRA_ACCOUNTS` further accounts; labels non-empty, of `[A-Za-z0-9._-]`, not
    /// starting with `sim-` (a Live account) and unique; UIDs non-empty and unique.
    pub fn check(&self) -> Result<(), String> {
        if let Some(x) = self.ioc_max_cross
            && !(1..=PM_IOC_CROSS_MAX).contains(&x)
        {
            return Err(format!(
                "ioc_max_cross {x}: must be within 1..={PM_IOC_CROSS_MAX} (1e-9)"
            ));
        }
        if let Some(q) = &self.presign_grid {
            if !self.presign {
                return Err("presign_grid needs presign".into());
            }
            if presign_grid_e6(q).is_none() {
                return Err(format!(
                    "presign_grid `{q}`: shares must be a decimal > 0 (at most 6 places)"
                ));
            }
        }
        if self.extra_accounts.len() > MAX_EXTRA_ACCOUNTS {
            return Err(format!(
                "at most {} accounts per venue ({MAX_EXTRA_ACCOUNTS} besides {PM_LIVE_LABEL})",
                KillBits::MAX_ACCOUNTS_PER_GW
            ));
        }
        let labels = self.labels();
        for (i, l) in labels.iter().enumerate() {
            let chars = l
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
            if l.is_empty() || !chars {
                return Err(format!("account label `{l}`: expected [A-Za-z0-9._-]+"));
            }
            if l.starts_with("sim-") {
                return Err(format!(
                    "account label `{l}`: a Live account's label must not start with `sim-`"
                ));
            }
            if labels[..i].contains(l) {
                return Err(format!("account label `{l}` is used twice"));
            }
        }
        let uids: Vec<&str> = std::iter::once(self.uid.as_str())
            .chain(self.extra_accounts.iter().map(|a| a.uid.as_str()))
            .collect();
        for (i, u) in uids.iter().enumerate() {
            if u.is_empty() {
                return Err(format!("account {}: empty UID", labels[i]));
            }
            if uids[..i].contains(u) {
                return Err(format!("UID `{u}` is used by two accounts"));
            }
        }
        Ok(())
    }
}

/// SHA-256 of a blob (`core.toml` `blobs.*.sha256`).
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).into()
}

/// Lower-case hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `--dry-run` configuration whose only venue is `polymarket-clob` with `live_feed = true`: 16 slot
/// insts (InstId 1–16 in the order coin, window, up/dn; tick 0.001), Sim account `sim-pm` bound to
/// a book-observer instance with one single-leg group per slot. `user` is the Admin cli principal;
/// `pipeline` (the discovery process's system user) is the pipeline principal that sends `BindSlot`.
pub fn write_polymarket_sim(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
) -> Result<PathBuf, String> {
    write_pm(
        &Target {
            root,
            hot,
            user,
            pipeline,
            src_ip,
            bind_device,
        },
        &PM_COINS,
        2,
        PmStrategy::Observer,
        None,
        &[],
        &Refs::default(),
    )
}

/// The maker simulation (`--dry-run`, `live_feed` venues): `polymarket-clob` with three BTC slot
/// pairs (InstId 1–6: up-0, dn-0, up-1, dn-1, up-2, dn-2) and `binance-spot` BTCUSDT (InstId 7) as
/// the reference; Sim account `sim-pm` (10 000 USDC) bound to a `binary-maker` instance (enabled,
/// other parameters at their defaults) with one single-leg group per slot plus the reference group;
/// `[sim] maker_fill = "queue"`. Discovery: `bind_slots.py --assets btc --slots 3 --settle`.
pub fn write_polymarket_maker_sim(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
) -> Result<PathBuf, String> {
    write_polymarket_maker_sim_with(root, hot, user, pipeline, src_ip, bind_device, &[])
}

/// `write_polymarket_maker_sim` with binary-maker parameter values `params` (field name, raw value;
/// `--param`, checked by `check_maker_params`) set after the defaults.
pub fn write_polymarket_maker_sim_with(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    params: &[(String, i64)],
) -> Result<PathBuf, String> {
    write_polymarket_maker_sim_opts(root, hot, user, pipeline, src_ip, bind_device, params, true)
}

/// `write_polymarket_maker_sim_with`; `coinbase = false` (`--maker --no-coinbase`) omits
/// `[venues.cb]` and its refdata. With the perpetual reference, without Chainlink.
#[allow(clippy::too_many_arguments)]
pub fn write_polymarket_maker_sim_opts(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    params: &[(String, i64)],
    coinbase: bool,
) -> Result<PathBuf, String> {
    let refs = Refs {
        coinbase,
        perp: true,
        chainlink: None,
        cb_perp: false,
    };
    write_polymarket_maker_sim_refs(root, hot, user, pipeline, src_ip, bind_device, params, &refs)
}

/// The maker simulation with the reference markets `refs` (`--no-coinbase`, `--no-perp`,
/// `--cl-keys DIR`).
#[allow(clippy::too_many_arguments)]
pub fn write_polymarket_maker_sim_refs(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    params: &[(String, i64)],
    refs: &Refs,
) -> Result<PathBuf, String> {
    write_pm(
        &Target {
            root,
            hot,
            user,
            pipeline,
            src_ip,
            bind_device,
        },
        &["btc"],
        3,
        PmStrategy::Maker,
        None,
        params,
        refs,
    )
}

/// `NAME=RAW` of `--param` → (name, raw i64).
pub fn parse_param(s: &str) -> Result<(String, i64), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("--param {s}: expected NAME=RAW"))?;
    let v = v
        .trim()
        .parse::<i64>()
        .map_err(|_| format!("--param {s}: RAW must be an integer (schema raw units)"))?;
    Ok((k.trim().to_string(), v))
}

/// Every `(name, raw)` names a field of the maker's parameter schema and lies within its
/// `[min, max]`.
pub fn check_maker_params(params: &[(String, i64)]) -> Result<(), String> {
    check_params(PmStrategy::Maker, params)
}

/// Every `(name, raw)` names a field of strategy `s`'s parameter schema and lies within its
/// `[min, max]`.
pub fn check_params(s: PmStrategy, params: &[(String, i64)]) -> Result<(), String> {
    let kind = s.kind_name();
    let desc = Strategies::kinds()
        .iter()
        .find(|d| d.name == kind)
        .ok_or_else(|| format!("{kind} is not registered"))?;
    for (name, v) in params {
        let f = hft_strategy_sdk::params::field_ix(desc.schema, name)
            .and_then(|i| desc.schema.field(i))
            .ok_or_else(|| format!("--param {name}: not a {kind} parameter"))?;
        if *v < f.min || *v > f.max {
            return Err(format!("--param {name}={v}: outside [{}, {}]", f.min, f.max));
        }
    }
    Ok(())
}

/// The live maker (`order_env = "prod"`, no `--dry-run`): the maker simulation's venues, slots and
/// strategy with a Live account `pm-live` (`is_prod`, UID `live.uid`) on Polymarket, its keys in
/// `live.keys_dir`, `PM_LIVE_PARAMS`, the micro risk limits (order 10 USDC, account and venue
/// 55, slot group 25, daily loss `live.daily_loss_abs` or 15, resumes `live.daily_loss_max_resumes`
/// or the default), OMS `ack_timeout_ms[polymarket] = 5000` and the REST pools'
/// `venues.pm.net` (`rest_timeout_ms = 5000`, `rest_max_idle_ms = 25000`, `rest_keepalive_ms = 5000`); every Polymarket
/// config sets OMS `expire_window_ms[polymarket]` to the GTD lifetime (`stale_guard()`).
/// `live.extra_accounts` are accounts 2, 3, … of the same instance with `pm-live`'s limits.
pub fn write_polymarket_maker_live(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    live: &PmLive,
) -> Result<PathBuf, String> {
    write_polymarket_maker_live_with(root, hot, user, pipeline, src_ip, bind_device, live, &[])
}

/// `write_polymarket_maker_live` with binary-maker parameter values `params` set after
/// `PM_LIVE_PARAMS` (`--live … --param`).
#[allow(clippy::too_many_arguments)]
pub fn write_polymarket_maker_live_with(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    live: &PmLive,
    params: &[(String, i64)],
) -> Result<PathBuf, String> {
    live.check()?;
    write_pm(
        &Target {
            root,
            hot,
            user,
            pipeline,
            src_ip,
            bind_device,
        },
        &["btc"],
        3,
        PmStrategy::Maker,
        Some(live),
        params,
        &live.refs(),
    )
}

/// The live taker probe (`--live UID KEYS_DIR --taker-probe`): everything of
/// `write_polymarket_maker_live_with` — venues, slots, accounts, keys, risk limits, presign, OMS
/// and REST settings — except the strategy: a `pm-taker-probe` instance (`pm-taker-probe`,
/// StrategyId 1) with one group (slot `btc-up-0`, InstId 1, open allowed), its schema defaults
/// (`enabled = 0`) and `params` (checked against its schema), and the price band's
/// `rr_max_cross = PM_PROBE_RR_MAX_CROSS` so that its sell-back at the best bid passes.
#[allow(clippy::too_many_arguments)]
pub fn write_polymarket_taker_probe_live(
    root: &Path,
    hot: &Path,
    user: Option<&str>,
    pipeline: Option<&str>,
    src_ip: &str,
    bind_device: &str,
    live: &PmLive,
    params: &[(String, i64)],
) -> Result<PathBuf, String> {
    live.check()?;
    write_pm(
        &Target {
            root,
            hot,
            user,
            pipeline,
            src_ip,
            bind_device,
        },
        &["btc"],
        3,
        PmStrategy::TakerProbe,
        Some(live),
        params,
        &live.refs(),
    )
}

/// The sequencer parameters of the generated configurations (the SeqRig's: generous ring limits,
/// keyframes every 5 min), before the per-config changes in `write_pm`.
pub fn seq_params() -> SeqParamsRow {
    let mut rings = [RingLimit::default(); 8];
    for (i, r) in rings.iter_mut().enumerate().skip(1) {
        *r = RingLimit {
            per_ring: 64,
            total: if i == 4 { 0 } else { 256 },
        };
    }
    SeqParamsRow {
        rings,
        md_round_ns: 50_000,
        ctl_bytes_per_round: 65_536,
        keyframe_period_ms: 300_000,
        keyframe_max_events: 1_500_000,
        advisor_mem_budget_mib: 256,
        advisor_ckpt_max_mib: 1_024,
        sim_max_per_round: 64,
        conflate_backlog: 64,
        notify_per_continue: 64,
        replay_per_round: 1_024,
        auto_resume_after_clean_drain: 1,
        allow_missing_standby: 0,
        _pad: [0; 6],
    }
}

/// The engine setup the generated configurations start from (equal to hft-testkit's
/// `seq_rig::engine_setup` for the same inputs, PMC-006): OMS epoch 1 at 1 GHz with 16 outputs per
/// event, 16 links per account and 64 slots; one exec account per refdata account; a gw and a feed
/// per venue of `venues`; no keys, not prod, no drain timeout.
pub fn engine_base(
    rd: &RefData,
    caps: &Capacities,
    ledger: LedgerConfig,
    venues: Vec<VenueId>,
) -> EngineSetup {
    let tsc_hz_milli = 1_000_000_000_000;
    let mut oms = OmsConfig::new(1, tsc_hz_milli);
    oms.max_out_per_event = 16;
    oms.links_per_account = 16;
    oms.slots = hft_oms::PerTable::new(64);
    let accounts = (1..=254u8)
        .filter(|a| rd.account(AccountId(*a)).id == AccountId(*a))
        .map(|a| ExecAccount {
            account: AccountId(a),
            sim: matches!(rd.account(AccountId(a)).kind, AccountKind::Sim(_)),
            params: None,
        })
        .collect();
    let feeds = venues.iter().fold(0u64, |m, v| m | 1u64 << v.0);
    EngineSetup {
        caps: CapacitiesRow {
            max_insts: caps.max_insts,
            max_cells_total: caps.max_cells_total,
            max_timers: caps.max_timers,
            recent_cmd_ids: caps.recent_cmd_ids,
            max_subs_per_inst: caps.max_subs_per_inst,
            max_groups_per_instance: caps.max_groups_per_instance,
            max_instances: caps.max_instances,
            _pad: 0,
        },
        seq: seq_params(),
        oms,
        ledger,
        exec: ExecConfig {
            default: ExecParams::default(),
            accounts,
        },
        gws: GwMap { venues },
        feeds: VenueSet(feeds),
        keys_required: Vec::new(),
        role_prod: false,
        advisor_max_lag_ms: 5_000,
        drain_timeout_ms: 0,
        day_pnl_carry: Vec::new(),
        risk_decode: None,
        clock: SegClock {
            tsc_hz_milli,
            calib_tsc: 0,
            calib_realtime_ns: 0,
        },
        base_engine_ns: 0,
    }
}

/// Where and for whom a configuration is written.
struct Target<'a> {
    root: &'a Path,
    hot: &'a Path,
    user: Option<&'a str>,
    pipeline: Option<&'a str>,
    src_ip: &'a str,
    bind_device: &'a str,
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

fn write_pm(
    t: &Target<'_>,
    coins: &[&str],
    pairs: usize,
    strategy: PmStrategy,
    live: Option<&PmLive>,
    params: &[(String, i64)],
    refs: &Refs,
) -> Result<PathBuf, String> {
    use hft_venue_proto::VenueProtocol;
    let (root, src_ip, bind_device) = (t.root, t.src_ip, t.bind_device);
    // The venues, slots and reference market of the maker (also under the taker probe).
    let maker = strategy != PmStrategy::Observer;
    let probe = strategy == PmStrategy::TakerProbe;
    if strategy == PmStrategy::Observer && !params.is_empty() {
        return Err("parameter values need the maker".into());
    }
    if probe && live.is_none() {
        return Err("the taker probe needs a live account".into());
    }
    check_params(strategy, params)?;
    std::fs::create_dir_all(root).map_err(io(root))?;
    let v = VenueId::POLYMARKET_CLOB;
    let bn = VenueId::BINANCE_SPOT;
    let cb = VenueId::COINBASE_SPOT;
    let bp = VenueId::BINANCE_USDM;
    let clv = VenueId::POLYMARKET_PRICE;
    let cbp = VenueId::COINBASE_ADV;
    // The further reference markets ride with the maker's Binance reference.
    let coinbase = maker && refs.coinbase;
    let perp = maker && refs.perp;
    let cl_keys = if maker { refs.chainlink.clone() } else { None };
    let chainlink = cl_keys.is_some();
    let cb_perp = maker && refs.cb_perp;
    let a = AccountId(1);
    let usdc = AssetId(1);
    let desc = Strategies::kinds()
        .iter()
        .find(|d| d.name == strategy.kind_name())
        .ok_or("strategy not registered")?;
    let mut slots = Vec::new();
    for coin in coins {
        for k in 0..pairs {
            for side in ["up", "dn"] {
                slots.push(format!("{coin}-{side}-{k}"));
            }
        }
    }
    // Reference market of the maker: BTC and USDT assets after the token assets.
    let btc = AssetId(slots.len() as u16 + 2);
    let usdt = AssetId(slots.len() as u16 + 3);
    // Binance spot's fee rule names BNB (fee discount asset).
    let bnb = AssetId(slots.len() as u16 + 4);
    let reference = InstId(slots.len() as u32 + 1);
    // USD after BNB (Coinbase and Chainlink quote it); the further references after the Binance
    // one, in the order Coinbase, perpetual, Chainlink, Coinbase perpetual (each only when
    // present).
    let usd = AssetId(slots.len() as u16 + 5);
    let mut next_inst = slots.len() as u32 + 2;
    let mut take = |on: bool| {
        on.then(|| {
            next_inst += 1;
            InstId(next_inst - 1)
        })
    };
    let cb_ref = take(coinbase).unwrap_or(InstId(0));
    let perp_ref = take(perp);
    let cl_ref = take(chainlink);
    let cbp_ref = take(cb_perp);
    // Reference groups after the slots: (asset, venue) in group order.
    let mut ref_groups = vec![(btc, bn)];
    if coinbase {
        ref_groups.push((btc, cb));
    }
    if perp {
        ref_groups.push((btc, bp));
    }
    if chainlink {
        ref_groups.push((btc, clv));
    }
    if cb_perp {
        ref_groups.push((btc, cbp));
    }
    let token = |i: usize| AssetId(i as u16 + 2);
    let mut b = RdbBuilder::new(1);
    b.asset(AssetSpec {
        id: usdc,
        name: "USDC".into(),
        class: AssetClass::Stable,
        exp: AssetExp {
            px_exp: -6,
            qty_exp: -6,
        },
        px_max: Px(1_000_000_000_000),
        qty_max: Qty(1_000_000_000_000_000),
        dust: Qty(1),
    });
    for (i, s) in slots.iter().enumerate() {
        b.asset(AssetSpec {
            id: token(i),
            name: format!("PM{}", s.replace('-', "").to_ascii_uppercase()).into(),
            class: AssetClass::Crypto,
            exp: AssetExp {
                px_exp: -3,
                qty_exp: -6,
            },
            px_max: Px(1_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    if maker {
        for (id, name, class, px_exp, qty_exp) in [
            (btc, "BTC", AssetClass::Crypto, -4, -8),
            (usdt, "USDT", AssetClass::Stable, -6, -6),
            (bnb, "BNB", AssetClass::Crypto, -4, -8),
        ] {
            b.asset(AssetSpec {
                id,
                name: name.into(),
                class,
                exp: AssetExp { px_exp, qty_exp },
                px_max: Px(1_000_000_000_000),
                qty_max: Qty(1_000_000_000_000_000),
                dust: Qty(1),
            });
        }
        if coinbase || chainlink || cb_perp {
            b.asset(AssetSpec {
                id: usd,
                name: "USD".into(),
                class: AssetClass::Stable,
                exp: AssetExp {
                    px_exp: -6,
                    qty_exp: -6,
                },
                px_max: Px(1_000_000_000_000),
                qty_max: Qty(1_000_000_000_000_000),
                dust: Qty(1),
            });
        }
        if perp {
            // USDT-M perpetual, market data only (venue rows in ascending id order: 1 before 2).
            b.venue(VenueSpec {
                id: bp,
                exchange: Exchange::Binance,
                product: InstKind::Perp,
                ws_max_streams_per_conn: 1024,
                ws_max_in_msgs_per_s: 10,
                ws_max_conn_age_s: 86400,
                ws_new_conn_per_5min_ip: 300,
                app_ping_interval_s: 0,
                ts_unit: TsUnit::Ms,
                client_id_max_len: 36,
                client_id_prefix: "".into(),
                max_batch: 5,
                max_inflight_amend: 1,
                crossed_tolerance_default: 0,
                stale_guard: StaleGuardKind::RecvWindow,
            });
            for (role, url) in [
                // The protocol appends /public/ws (bookTicker, depth5) and /market/ws (aggTrade).
                (EndpointRole::MdPublic, "wss://fstream.binance.com"),
                (EndpointRole::MdMarket, "wss://fstream.binance.com"),
                (EndpointRole::Rest, "https://fapi.binance.com"),
            ] {
                b.endpoint(
                    bp,
                    Endpoint {
                        env: EndpointEnv::Prod,
                        role,
                        url: url.into(),
                        conn_group: 0,
                    },
                );
            }
            b.caps(venue_binance::Usdm::default_caps(|s| {
                (s == "USDT").then_some(usdt)
            }));
            b.fee(FeeSpec {
                venue: bp,
                tier: 0,
                fee: FeePair {
                    maker: Rate(200_000),
                    taker: Rate(500_000),
                },
            });
            for r in venue_binance::Usdm::rate_rules() {
                b.limit(
                    bp,
                    RateLimit {
                        scope: r.scope,
                        bucket: r.bucket,
                        limit: r.limit,
                        window_ms: r.window_ms,
                        default_cost: 1,
                    },
                );
            }
        }
        b.venue(VenueSpec {
            id: bn,
            exchange: Exchange::Binance,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 1024,
            ws_max_in_msgs_per_s: 5,
            ws_max_conn_age_s: 86400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Us,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 5,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::RecvWindow,
        });
        for (role, url) in [
            // Port 9443: ~3.6 ms earlier than 443 at eu-west-1 (2026-11-06, 181k trades on both).
            (EndpointRole::MdPublic, "wss://stream.binance.com:9443/ws"),
            (EndpointRole::MdMarket, "wss://stream.binance.com:9443/ws"),
            (EndpointRole::Rest, "https://api.binance.com"),
        ] {
            b.endpoint(
                bn,
                Endpoint {
                    env: EndpointEnv::Prod,
                    role,
                    url: url.into(),
                    conn_group: 0,
                },
            );
        }
        b.caps(venue_binance::Spot::default_caps(|s| match s {
            "USDT" => Some(usdt),
            "BNB" => Some(bnb),
            _ => None,
        }));
        b.fee(FeeSpec {
            venue: bn,
            tier: 0,
            fee: FeePair {
                maker: Rate(1_000_000),
                taker: Rate(1_000_000),
            },
        });
        b.limit(
            bn,
            RateLimit {
                scope: LimitScope::Account,
                bucket: LimitBucket::Orders,
                limit: 100,
                window_ms: 1000,
                default_cost: 1,
            },
        );
    }
    b.venue(VenueSpec {
        id: v,
        exchange: Exchange::Polymarket,
        product: InstKind::Spot,
        ws_max_streams_per_conn: 2,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 10,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 32,
        client_id_prefix: "".into(),
        max_batch: 1,
        max_inflight_amend: 1,
        crossed_tolerance_default: PM_CROSSED_TOLERANCE,
        stale_guard: StaleGuardKind::ExpireAt,
    });
    let market = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
    let mut pm_endpoints = vec![
        (EndpointRole::MdPublic, market),
        (EndpointRole::MdMarket, market),
        (EndpointRole::Rest, "https://clob.polymarket.com"),
    ];
    if live.is_some() {
        pm_endpoints.push((
            EndpointRole::OrderWs,
            "wss://ws-subscriptions-clob.polymarket.com/ws/user",
        ));
    }
    for (role, url) in pm_endpoints {
        b.endpoint(
            v,
            Endpoint {
                env: EndpointEnv::Prod,
                role,
                url: url.into(),
                conn_group: 0,
            },
        );
    }
    b.caps(venue_polymarket::PolymarketClob::default_caps(|s| {
        (s == "USDC").then_some(usdc)
    }));
    b.fee(FeeSpec {
        venue: v,
        tier: 0,
        fee: FeePair {
            maker: Rate(0),
            taker: Rate(0),
        },
    });
    for r in venue_polymarket::PolymarketClob::rate_rules() {
        b.limit(
            v,
            RateLimit {
                scope: r.scope,
                bucket: r.bucket,
                limit: r.limit,
                window_ms: r.window_ms,
                default_cost: 1,
            },
        );
    }
    if coinbase {
        b.venue(VenueSpec {
            id: cb,
            exchange: Exchange::Coinbase,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 64,
            ws_max_in_msgs_per_s: 8,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Us,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::None,
        });
        for (role, url) in [
            // Coinbase Exchange feed, not Advanced Trade (~60 ms later on the same trades,
            // eu-west-1 2026-11-06; venue-coinbase README "端点与路由").
            (EndpointRole::MdPublic, "wss://ws-feed.exchange.coinbase.com"),
            (EndpointRole::MdMarket, "wss://ws-feed.exchange.coinbase.com"),
            (EndpointRole::Rest, "https://api.exchange.coinbase.com"),
        ] {
            b.endpoint(
                cb,
                Endpoint {
                    env: EndpointEnv::Prod,
                    role,
                    url: url.into(),
                    conn_group: 0,
                },
            );
        }
        b.caps(venue_coinbase::CoinbaseSpot::default_caps(|s| {
            (s == "USD").then_some(usd)
        }));
        b.fee(FeeSpec {
            venue: cb,
            tier: 0,
            fee: FeePair {
                maker: Rate(4_000_000),
                taker: Rate(6_000_000),
            },
        });
        for r in venue_coinbase::CoinbaseSpot::rate_rules() {
            b.limit(
                cb,
                RateLimit {
                    scope: r.scope,
                    bucket: r.bucket,
                    limit: r.limit,
                    window_ms: r.window_ms,
                    default_cost: 1,
                },
            );
        }
    }
    if chainlink {
        b.venue(VenueSpec {
            id: clv,
            exchange: Exchange::Polymarket,
            product: InstKind::Spot,
            ws_max_streams_per_conn: 16,
            ws_max_in_msgs_per_s: 10,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 5,
            ts_unit: TsUnit::Ms,
            client_id_max_len: 32,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::None,
        });
        for (role, url) in [
            (EndpointRole::MdPublic, "wss://ws-live-v2.polymarket.com"),
            (EndpointRole::MdMarket, "wss://ws-live-v2.polymarket.com"),
            // No REST use; the assembly requires a public REST endpoint per venue.
            (EndpointRole::Rest, "https://ws-live-v2.polymarket.com"),
        ] {
            b.endpoint(
                clv,
                Endpoint {
                    env: EndpointEnv::Prod,
                    role,
                    url: url.into(),
                    conn_group: 0,
                },
            );
        }
        b.caps(venue_pmprice::PolymarketPrice::default_caps(|s| {
            (s == "USD").then_some(usd)
        }));
        b.fee(FeeSpec {
            venue: clv,
            tier: 0,
            fee: FeePair {
                maker: Rate(0),
                taker: Rate(0),
            },
        });
        for r in venue_pmprice::PolymarketPrice::rate_rules() {
            b.limit(
                clv,
                RateLimit {
                    scope: r.scope,
                    bucket: r.bucket,
                    limit: r.limit,
                    window_ms: r.window_ms,
                    default_cost: 1,
                },
            );
        }
    }
    if cb_perp {
        // Coinbase Advanced Trade, market data only (venue rows in ascending id order: 18 last).
        b.venue(VenueSpec {
            id: cbp,
            exchange: Exchange::Coinbase,
            product: InstKind::Perp,
            ws_max_streams_per_conn: 2,
            ws_max_in_msgs_per_s: 8,
            ws_max_conn_age_s: 86_400,
            ws_new_conn_per_5min_ip: 300,
            app_ping_interval_s: 0,
            ts_unit: TsUnit::Us,
            client_id_max_len: 36,
            client_id_prefix: "".into(),
            max_batch: 1,
            max_inflight_amend: 1,
            crossed_tolerance_default: 0,
            stale_guard: StaleGuardKind::None,
        });
        for (role, url) in [
            (EndpointRole::MdPublic, "wss://advanced-trade-ws.coinbase.com"),
            (EndpointRole::MdMarket, "wss://advanced-trade-ws.coinbase.com"),
            // No REST use; the assembly requires a public REST endpoint per venue.
            (EndpointRole::Rest, "https://api.coinbase.com"),
        ] {
            b.endpoint(
                cbp,
                Endpoint {
                    env: EndpointEnv::Prod,
                    role,
                    url: url.into(),
                    conn_group: 0,
                },
            );
        }
        b.caps(venue_coinbase::CoinbaseAdv::default_caps(|s| {
            (s == "USD").then_some(usd)
        }));
        b.fee(FeeSpec {
            venue: cbp,
            tier: 0,
            fee: FeePair {
                maker: Rate(0),
                taker: Rate(0),
            },
        });
        for r in venue_coinbase::CoinbaseAdv::rate_rules() {
            b.limit(
                cbp,
                RateLimit {
                    scope: r.scope,
                    bucket: r.bucket,
                    limit: r.limit,
                    window_ms: r.window_ms,
                    default_cost: 1,
                },
            );
        }
    }
    for (i, s) in slots.iter().enumerate() {
        b.inst(InstSpec {
            id: InstId(i as u32 + 1),
            venue: v,
            symbol: s.as_str().into(),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: token(i),
            quote: usdc,
            tick: 1,
            // Live: the exchange takes sizes in 0.01 shares (taker amount of a buy at most 2
            // decimals) and at least 5 shares; the sim keeps one unit.
            lot: if live.is_some() { 10_000 } else { 1 },
            min_qty: Qty(if live.is_some() { 5_000_000 } else { 1 }),
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 128,
            l2_depth: 100,
            crossed_tolerance: PM_CROSSED_TOLERANCE,
            tick_str: "0.001".into(),
            lot_str: if live.is_some() { "0.01" } else { "0.000001" }.into(),
            min_qty_str: if live.is_some() { "5" } else { "0.000001" }.into(),
            ..InstSpec::default()
        });
    }
    if maker {
        b.inst(InstSpec {
            id: reference,
            venue: bn,
            symbol: "BTCUSDT".into(),
            kind: InstKind::Spot,
            asset: btc,
            quote: usdt,
            tick: 100,
            lot: 1000,
            min_qty: Qty(1000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 4096,
            tick_str: "0.01".into(),
            lot_str: "0.00001".into(),
            min_qty_str: "0.00001".into(),
            ..InstSpec::default()
        });
    }
    if coinbase {
        // Market data only: no account; the maker's second reference group (InstId 8).
        b.inst(InstSpec {
            id: cb_ref,
            venue: cb,
            symbol: "BTC-USD".into(),
            kind: InstKind::Spot,
            asset: btc,
            quote: usd,
            tick: 100,
            lot: 1,
            min_qty: Qty(1),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 4096,
            tick_str: "0.01".into(),
            lot_str: "0.00000001".into(),
            min_qty_str: "0.00000001".into(),
            ..InstSpec::default()
        });
    }
    if let Some(id) = perp_ref {
        // Market data only: no account; tick 0.10 USDT, lot 0.001 BTC (exchangeInfo 2026-11).
        b.inst(InstSpec {
            id,
            venue: bp,
            symbol: "BTCUSDT".into(),
            kind: InstKind::Perp,
            asset: btc,
            quote: usdt,
            tick: 1_000,
            lot: 100_000,
            min_qty: Qty(100_000),
            max_qty: Qty(100_000_000_000),
            // Never traded: the maker's dust floor (5 USDT, as the spot reference), not the
            // exchange's 100 USDT.
            min_notional: Notional(5_000_000),
            book_window: 4096,
            tick_str: "0.10".into(),
            lot_str: "0.001".into(),
            min_qty_str: "0.001".into(),
            funding_interval_s: 28_800,
            max_leverage: 20,
            ..InstSpec::default()
        });
    }
    if let Some(id) = cl_ref {
        // Chainlink BTC/USD (price → IndexPrice, 60 s TWAP → MarkPrice); prices at BTC's px_exp.
        b.inst(InstSpec {
            id,
            venue: clv,
            symbol: "btcusd".into(),
            kind: InstKind::Spot,
            asset: btc,
            quote: usd,
            tick: 1,
            lot: 1,
            min_qty: Qty(1),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 4096,
            tick_str: "0.0001".into(),
            lot_str: "0.00000001".into(),
            min_qty_str: "0.00000001".into(),
            ..InstSpec::default()
        });
    }
    if let Some(id) = cbp_ref {
        // Market data only: tick 5 USD; quantities are contracts of 0.01 BTC.
        b.inst(InstSpec {
            id,
            venue: cbp,
            symbol: PM_CB_PERP_SYMBOL.into(),
            kind: InstKind::Perp,
            asset: btc,
            quote: usd,
            qty_unit_shift: -2,
            tick: 50_000,
            lot: 1_000_000,
            min_qty: Qty(1_000_000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(1_000_000),
            book_window: 4096,
            tick_str: "5".into(),
            lot_str: "1".into(),
            min_qty_str: "1".into(),
            funding_interval_s: 3_600,
            max_leverage: 10,
            ..InstSpec::default()
        });
    }
    b.instance(InstanceSpec {
        id: StrategyId(1),
        kind_id: desc.kind_id,
        name: match strategy {
            PmStrategy::Observer => "pm-observer",
            PmStrategy::Maker => "pm-maker",
            PmStrategy::TakerProbe => "pm-taker-probe",
        }
        .into(),
    });
    b.account(AccountSpec {
        id: a,
        sim: live.is_none(),
        bound_instance: StrategyId(1),
        is_prod: live.is_some(),
        venues: 1 << v.0,
        label: if live.is_some() { PM_LIVE_LABEL } else { "sim-pm" }.into(),
        uid_policy: UidPolicy::Verified,
    });
    if let Some(l) = live {
        b.account_venue(AccountVenueSpec {
            account: a,
            venue: v,
            uid: l.uid.as_str().into(),
        });
        let labels = l.labels();
        for (i, x) in l.extra_accounts.iter().enumerate() {
            let id = AccountId(a.0 + 1 + i as u8);
            b.account(AccountSpec {
                id,
                sim: false,
                bound_instance: StrategyId(1),
                is_prod: true,
                venues: 1 << v.0,
                label: labels[i + 1].as_str().into(),
                uid_policy: UidPolicy::Verified,
            });
            b.account_venue(AccountVenueSpec {
                account: id,
                venue: v,
                uid: x.uid.as_str().into(),
            });
        }
    } else {
        b.sim(SimSpec {
            account: a,
            fee_tier: 0,
            latency_model: 0,
            margin_model: 0,
        });
        b.sim_balance(SimBalanceSpec {
            account: a,
            venue: v,
            asset: usdc,
            qty: Qty(if maker { 10_000_000_000 } else { 1_000_000_000 }),
        });
    }
    let rdb = b.finish();
    let rd = RefData::load(&rdb, None).map_err(|e| format!("refdata: {e:?}"))?;
    let schema = desc.schema;
    // Groups: one per slot (and the reference, then the Coinbase reference); the probe has one,
    // slot InstId 1.
    let n = if probe {
        1
    } else {
        slots.len() as u16 + if maker { ref_groups.len() as u16 } else { 0 }
    };
    let field =
        |name: &str| (0..schema.n_fields()).position(|i| schema.field(i).is_some_and(|f| f.name == name));
    let mut defaults: Vec<_> = (0..schema.n_fields())
        .map(|i| schema.field(i).map_or(0, |f| f.default))
        .collect();
    if strategy == PmStrategy::Maker {
        defaults[field("enabled").ok_or("binary-maker has no `enabled`")?] = 1;
    }
    if live.is_some() && strategy == PmStrategy::Maker {
        for (name, value) in PM_LIVE_PARAMS {
            defaults[field(name).ok_or_else(|| format!("binary-maker has no `{name}`"))?] = value;
        }
    }
    if strategy == PmStrategy::Maker {
        // The auxiliary groups (binary-maker `ref_aux`): bit 0 perpetual, bit 1 Chainlink, bit 2
        // Coinbase perpetual.
        defaults[field("ref_aux").ok_or("binary-maker has no `ref_aux`")?] =
            i64::from(perp) | i64::from(chainlink) << 1 | i64::from(cb_perp) << 2;
    }
    for (name, value) in params {
        if let Some(k) = field(name) {
            defaults[k] = *value;
        }
    }
    let body = encode_body(
        schema,
        n,
        &RawParams {
            defaults,
            overrides: vec![],
        },
    )
    .map_err(|e| format!("strategy parameters: {e:?}"))?;
    let h = StrategyParamsHdr {
        strategy: StrategyId(1),
        account: a,
        kind_id: desc.kind_id,
        schema_id: schema.id,
        schema_ver: schema.version,
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: n,
        _pad2: 0,
        version: 1,
        generated_at_ns: 0,
        body_len: body.len() as u32,
        group_ctr_hwm: u32::from(n),
        schema_digest: hft_strategy_sdk::params::schema_digest(desc, schema.version),
    };
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut params = h.as_bytes().to_vec();
    for i in 0..usize::from(n) {
        let (asset, venue) = if i < slots.len() {
            (token(i), v)
        } else {
            ref_groups[i - slots.len()]
        };
        let g = GroupSpec {
            id: GroupId::compose(StrategyId(1), i as u32 + 1).ok_or("group id")?,
            asset,
            n_legs: 1,
            open_allowed: u8::from((maker && i < slots.len()) || probe),
            legs: [
                LegDef {
                    inst: InstId(i as u32 + 1),
                    venue,
                    kind: LegDef::CEX,
                    ..none
                },
                none,
                none,
                none,
            ],
            _rsv: 0,
        };
        params.extend_from_slice(g.as_bytes());
    }
    params.extend(body);
    let mut risk = hft_risk::ParamsBuilder::new(&rd);
    risk.rate_headroom = Rate(100_000_000);
    risk.classes.push(hft_risk::BandSpec {
        // Outcome-token spreads are wide relative to their price (0.01 at 0.10 is 10 %).
        max_px_dev: Rate(if maker { 500_000_000 } else { 30_000_000 }),
        ioc_max_cross: Rate(live.and_then(|l| l.ioc_max_cross).unwrap_or(PM_IOC_CROSS)),
        rr_max_cross: Some(Rate(if probe { PM_PROBE_RR_MAX_CROSS } else { 10_000_000 })),
    });
    // Live micro limits (USDC, 1e-6): account and venue 55 (two overlapping windows plus resting
    // quotes, below the 82 deposited), one slot group 25, daily loss 15 unless chosen.
    let (acct_cap, group_cap, loss_cap) = match (live, maker) {
        (Some(l), _) => (
            l.account_cap.unwrap_or(55_000_000),
            l.group_cap.unwrap_or(25_000_000),
            l.daily_loss_abs.unwrap_or(PM_LIVE_DAILY_LOSS),
        ),
        (None, true) => (10_000_000_000, 1_000_000_000, 1_000_000_000),
        (None, false) => (100_000_000, 10_000_000, 10_000_000),
    };
    risk.accounts.push((
        a,
        hft_risk::AcctSpec {
            max_order_notional: Notional(10_000_000),
            max_account_notional: Notional(acct_cap),
            account_notional_from_equity: Rate(1_000_000_000),
            // A slot group is one leg held to settlement: its whole position is "naked" (ratio 1),
            // so the exposure gate is off for the maker (naked_gate = 1).
            default_group: if maker {
                hft_risk::GroupLimits {
                    max_group_notional: Notional(group_cap),
                    max_pos_qty: Qty(1_000_000_000),
                    naked_gate: Rate(1_000_000_000),
                    naked_gate_min: Notional(1_000_000_000),
                }
            } else {
                hft_risk::GroupLimits {
                    max_group_notional: Notional(10_000_000),
                    max_pos_qty: Qty(100_000_000),
                    naked_gate: Rate(500_000_000),
                    naked_gate_min: Notional(10_000_000),
                }
            },
            max_hedge_notional: Some(Notional(10_000_000)),
            max_reduce_notional: Some(Notional(10_000_000)),
            // The absolute line binds: the quota is min(daily_loss_abs, equity × daily_loss_kill).
            daily_loss_kill: Rate(if live.is_some() {
                1_000_000_000
            } else {
                100_000_000
            }),
            daily_loss_abs: Notional(loss_cap),
            daily_loss_max_resumes: live.and_then(|l| l.daily_loss_max_resumes),
            recon_closeonly_notional: Notional(5_000_000),
            ..hft_risk::AcctSpec::default()
        },
    ));
    risk.venues.push((
        v,
        a,
        hft_risk::VenueSpec {
            max_venue_notional: Notional(acct_cap),
            inst_bucket_per_s: 10,
            backoff_base_ms: 500,
            backoff_max_ms: 8000,
            rr_reserve: None,
        },
    ));
    // Further live accounts: `pm-live`'s account and venue limits each.
    let extra: Vec<AccountId> = live.map_or(Vec::new(), |l| {
        (0..l.extra_accounts.len())
            .map(|i| AccountId(a.0 + 1 + i as u8))
            .collect()
    });
    for id in &extra {
        let (acct, venue) = (risk.accounts[0].1, risk.venues[0].2);
        risk.accounts.push((*id, acct));
        risk.venues.push((v, *id, venue));
    }
    let risk = risk.build();
    let rpb = risk.encode_blob().map_err(|e| format!("risk params: {e:?}"))?;
    // Polymarket settles in USDC (pUSD, 1:1) and lists no USDC/USDT book: USDC is valued at par,
    // otherwise every USDC amount (P&L, held notional, equity) converts to 0 USDT and the daily
    // loss never fires (hft-ledger refuses such a config: NoUsdtRate).
    let mut ledger = LedgerConfig::default();
    ledger.stable_par_fallback.clear();
    ledger.par_assets.push(hft_ledger::AssetName::new("USDC"));
    // Coinbase and Chainlink quote BTC in USD: valued at par (no position is ever held on them).
    if coinbase || chainlink || cb_perp {
        ledger.par_assets.push(hft_ledger::AssetName::new("USD"));
    }
    let caps = Capacities {
        max_insts: 16,
        max_cells_total: 32,
        max_timers: 1024,
        recent_cmd_ids: 256,
        max_subs_per_inst: 4,
        max_groups_per_instance: 16,
        max_instances: 4,
    };
    let mut feed_venues = vec![v];
    if maker {
        feed_venues.push(bn);
    }
    if coinbase {
        feed_venues.push(cb);
    }
    if perp {
        feed_venues.push(bp);
    }
    if chainlink {
        feed_venues.push(clv);
    }
    if cb_perp {
        feed_venues.push(cbp);
    }
    let mut engine = engine_base(&rd, &caps, ledger, feed_venues.clone());
    engine.drain_timeout_ms = 3000;
    engine.gws.venues.clear();
    engine.feeds = VenueSet(feed_venues.iter().fold(0u64, |m, x| m | 1u64 << x.0));
    engine.seq.rings[7] = RingLimit::default();
    engine.seq.allow_missing_standby = 1;
    // Every place is GTD: its lifetime is the expiry window, a NotFound past it + 500 ms ends the
    // slot (hft-oms "Unknown 收敛流程" 4; venue-polymarket "VenueCaps 默认值").
    let hft_venue_proto::StaleGuard::ExpireAt { default_ms } =
        venue_polymarket::PolymarketClob::stale_guard()
    else {
        return Err("Polymarket declares ExpireAt".into());
    };
    engine.oms.expire_window_ms.set(v, Some(default_ms));
    if let Some(l) = live {
        engine.role_prod = true;
        engine.gws.venues = vec![v];
        engine.keys_required = vec![(a, v, 3)];
        engine.keys_required.extend(extra.iter().map(|id| (*id, v, 3)));
        // A SmokeOrder bids half the ask (an outcome token at 0.50 → 0.25): it must never fill.
        engine.oms.smoke_far_bp = Rate(500_000_000);
        // A REST place answer can take over 2 s (2026-10-23, round trips otherwise up to 0.76 s);
        // a slot that goes Unknown on time-out only resolves by query. The REST pool waits as long
        // (`venues.pm.net.rest_timeout_ms`, at most this value).
        engine.oms.ack_timeout_ms.set(v, PM_REST_TIMEOUT_MS);
        if let Some((day, pnl)) = l.day_loss_carry {
            engine.day_pnl_carry = vec![(a, day, Notional(pnl))];
        }
    }
    engine.exec.default.force_slice.max_chunk_notional = Notional(10_000_000);
    let ecf = engine.encode_blob(1, HostClass::Dev2, 0);
    let mut uids = String::new();
    let mut issuers = String::new();
    if let Some(u) = t.user {
        uids += &format!("{u:?} = \"cli\"\n");
        issuers += &format!("\"cli:{u}\" = [\"Admin\"]\n");
    }
    if let Some(p) = t.pipeline {
        uids += &format!("{p:?} = \"pipeline\"\n");
    }
    let principals = root.join("principals.toml");
    std::fs::write(&principals, format!("[uids]\n{uids}[issuers]\n{issuers}")).map_err(io(&principals))?;
    let mut text = format!(
        r#"schema = 1
[instance]
name = "sim"
feed_env = "prod"
config_source = "file"
[paths]
journal_ram = '{}'
journal_cold = 'cold'
ctl_socket = 'core.sock'
ctl_principals = 'principals.toml'
[shutdown]
cancel_open_orders = true
drain_timeout_ms = 3000
[journal]
segment_bytes = 8388608
hot_keep = 2
[clock]
step_threshold_ns = 50000
interval_ms = 1000
[watchdog]
hb_stale_ms = 1000
[rings]
elements = 4096
bytes = 4194304
[venues.pm]
protocol = 'polymarket-clob'
id = {}
max_sub_insts = {n_slots}
stale_guard_ms = 1500
src_ip = '{src_ip}'
bind_device = '{bind_device}'
live_feed = true
[venues.pm.feed]
depth_conns = 2
{bn_text}{cb_text}[sim]
{maker_fill}seed = 123
entry_us = 100
resp_us = 100
leverage = 1
mmr_raw = 5000000
liq_fee_raw = 1000000
liq_slippage_bp = 10
quote_interval_ms = 1000
"#,
        t.hot.display(),
        v.0,
        n_slots = slots.len(),
        bn_text = if maker {
            format!(
                "[venues.bn]\nprotocol = 'binance-spot'\nid = {}\nmax_sub_insts = 1\nstale_guard_ms = 1500\n\
                 src_ip = '{src_ip}'\nbind_device = '{bind_device}'\nlive_feed = true\n\
                 [venues.bn.feed]\nrace_rotate_s = 600\n",
                bn.0
            )
        } else {
            String::new()
        },
        cb_text = if coinbase {
            format!(
                "[venues.cb]\nprotocol = 'coinbase-spot'\nid = {}\nmax_sub_insts = 1\nstale_guard_ms = 1500\n\
                 src_ip = '{src_ip}'\nbind_device = '{bind_device}'\nlive_feed = true\n\
                 [venues.cb.feed]\nrace_rotate_s = 600\n",
                cb.0
            )
        } else {
            String::new()
        },
        maker_fill = if maker { "maker_fill = 'queue'\n" } else { "" },
    );
    if perp {
        // Inserted before `[sim]` with the other venue tables (TOML: a table header ends the
        // previous table, so appending after `[sim]` would be fine too; kept together for reading).
        let bp_text = format!(
            "[venues.bp]\nprotocol = 'binance-usdm'\nid = {}\nmax_sub_insts = 1\nstale_guard_ms = 1500\n\
             src_ip = '{src_ip}'\nbind_device = '{bind_device}'\nlive_feed = true\n\
             [venues.bp.feed]\nrace_rotate_s = 600\n",
            bp.0
        );
        text = text.replacen("[sim]\n", &format!("{bp_text}[sim]\n"), 1);
    }
    if let Some(dir) = &cl_keys {
        let k = |f: &str| dir.join(f).display().to_string();
        let cl_text = format!(
            "[venues.cl]\nprotocol = 'polymarket-price'\nid = {}\nmax_sub_insts = 1\nstale_guard_ms = 1500\n\
             src_ip = '{src_ip}'\nbind_device = '{bind_device}'\nlive_feed = true\n\
             [venues.cl.md_login]\napi_key_path = {:?}\nsecret_path = {:?}\npassphrase_path = {:?}\n",
            clv.0,
            k("api_key"),
            k("secret"),
            k("passphrase")
        );
        text = text.replacen("[sim]\n", &format!("{cl_text}[sim]\n"), 1);
    }
    if cb_perp {
        let cp_text = format!(
            "[venues.cp]\nprotocol = 'coinbase-adv'\nid = {}\nmax_sub_insts = 1\nstale_guard_ms = 1500\n\
             src_ip = '{src_ip}'\nbind_device = '{bind_device}'\nlive_feed = true\n\
             [venues.cp.feed]\nrace_rotate_s = 600\n",
            cbp.0
        );
        text = text.replacen("[sim]\n", &format!("{cp_text}[sim]\n"), 1);
    }
    for (name, task, nice) in [
        ("sequencer", "sequencer", 0),
        ("persist", "persist", 10),
        ("housekeeping", "housekeeping", 0),
        ("ctl-gateway", "ctl-gateway", 10),
        ("sim-gw", "sim-gw", 0),
        ("paper-feed", "paper-feed", 0),
        ("feed-pm", "feed:pm", 0),
        ("build-pm", "conn-builder:pm", 10),
    ] {
        text += &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
    }
    if maker {
        for (name, task, nice) in [("feed-bn", "feed:bn", 0), ("build-bn", "conn-builder:bn", 10)] {
            text +=
                &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
        }
    }
    if coinbase {
        for (name, task, nice) in [("feed-cb", "feed:cb", 0), ("build-cb", "conn-builder:cb", 10)] {
            text +=
                &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
        }
    }
    if perp {
        for (name, task, nice) in [("feed-bp", "feed:bp", 0), ("build-bp", "conn-builder:bp", 10)] {
            text +=
                &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
        }
    }
    if chainlink {
        for (name, task, nice) in [("feed-cl", "feed:cl", 0), ("build-cl", "conn-builder:cl", 10)] {
            text +=
                &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
        }
    }
    if cb_perp {
        for (name, task, nice) in [("feed-cp", "feed:cp", 0), ("build-cp", "conn-builder:cp", 10)] {
            text +=
                &format!("\n[[threads]]\nname = '{name}'\ntask = '{task}'\npark_us = 100\nnice = {nice}\n");
        }
    }
    for (name, bytes) in [
        ("refdata", rdb),
        ("engine", ecf),
        ("risk_params", rpb),
        ("strategy_params", params),
    ] {
        let file = format!("{name}.bin");
        let path = root.join(&file);
        std::fs::write(&path, &bytes).map_err(io(&path))?;
        let table = if name == "strategy_params" {
            format!("[[blobs.{name}]]")
        } else {
            format!("[blobs.{name}]")
        };
        text += &format!(
            "\n{table}\npath = '{file}'\nsha256 = '{}'\n",
            hex(&digest(&bytes))
        );
    }
    let path = root.join("core.toml");
    if let Some(l) = live {
        text = live_toml(&text, a, &extra, l)?;
    }
    std::fs::write(&path, text).map_err(io(&path))?;
    Ok(path)
}

/// The sim `core.toml` text → the live one: `instance` prod, no `[sim]`, no `live_feed`, the
/// accounts' key files, `account_io`, `net`, `presign`, the gw thread instead of sim-gw/paper-feed.
fn table(v: Option<&mut toml::Value>) -> Result<&mut toml::Table, String> {
    v.and_then(toml::Value::as_table_mut)
        .ok_or_else(|| "core.toml: missing table".to_string())
}

fn live_toml(text: &str, a: AccountId, extra: &[AccountId], l: &PmLive) -> Result<String, String> {
    let parse = |s: &str| toml::from_str::<toml::Value>(s).map_err(|e| format!("core.toml: {e}"));
    let mut config = parse(text)?;
    table(Some(&mut config))?.remove("sim");
    let instance = table(config.get_mut("instance"))?;
    instance.insert("name".into(), "prod".into());
    instance.insert("order_env".into(), "prod".into());
    for name in ["pm", "bn", "cb", "bp", "cl", "cp"] {
        if let Some(v) = config["venues"].get_mut(name).and_then(toml::Value::as_table_mut) {
            v.remove("live_feed");
        }
    }
    let keys = |account: u8, dir: &Path| {
        let k = |f: &str| dir.join(f).display().to_string();
        parse(&format!(
            "account={account}\napi_key_path={:?}\nsession_seed_path={:?}\nrest_api_key_path={:?}\nrest_secret_path={:?}\npassphrase_path={:?}",
            k("api_key"),
            k("secret"),
            k("api_key"),
            k("secret"),
            k("passphrase")
        ))
    };
    let mut accounts = vec![keys(a.0, &l.keys_dir)?];
    for (id, x) in extra.iter().zip(&l.extra_accounts) {
        accounts.push(keys(id.0, &x.keys_dir)?);
    }
    let pm = table(config["venues"].get_mut("pm"))?;
    pm.insert("accounts".into(), toml::Value::Array(accounts));
    pm.insert(
        "account_io".into(),
        parse("timeout_ms=5000\ninterval_ms=60000\nrequest_gap_ms=200\nmax_cancel_rounds=3\nlisten_keepalive_ms=600000")?,
    );
    // A place answer is read up to the OMS ack timeout (tail 0.8 s, multi-second at window
    // starts); an idle REST connection is rebuilt after 25 s, before a middlebox can drop it
    // silently (hft-net "9. REST 客户端"); the keep-alive request keeps every connection answered
    // and fresh, so that rebuild is only the fallback.
    pm.insert(
        "net".into(),
        parse(&format!(
            "rest_timeout_ms={PM_REST_TIMEOUT_MS}\nrest_max_idle_ms={PM_REST_MAX_IDLE_MS}\nrest_keepalive_ms={PM_REST_KEEPALIVE_MS}"
        ))?,
    );
    if l.presign {
        let mut presign = parse(PM_LIVE_PRESIGN)?;
        if let Some(q) = &l.presign_grid {
            table(Some(&mut presign))?.insert("grid_qty".into(), q.as_str().into());
        }
        pm.insert("presign".into(), presign);
    }
    if l.cancel_race {
        pm.insert("gw".into(), parse("cancel_race = true")?);
    }
    let threads = config
        .get_mut("threads")
        .and_then(toml::Value::as_array_mut)
        .ok_or("core.toml: missing threads")?;
    threads.retain(|t| !matches!(t["task"].as_str(), Some("sim-gw" | "paper-feed")));
    threads.push(parse("name='gw-pm'\ntask='gw:pm'\npark_us=100\nnice=0")?);
    toml::to_string(&config).map_err(|e| format!("core.toml: {e}"))
}

/// `--cpu-plan` (README "绑核"): thread placement of a generated `core.toml` on the 4-core
/// pm-strategy box (c8g.xlarge).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CpuPlan {
    /// No thread is pinned (every thread parks `park_us` 100 µs when idle).
    #[default]
    None,
    /// `sequencer` on core 1, every `feed:` thread on core 2, the `gw:` thread on core 3, all other
    /// threads on core 0; idle threads still park. Needs nothing from the host.
    Pin,
    /// As `Pin`, and `sequencer` and the `gw:` thread spin (`idle = "spin"`); the feed threads on
    /// core 2 park 20 µs. hft-core refuses to start unless cores 1–3 are isolated
    /// (`/sys/devices/system/cpu/isolated`, kernel `isolcpus=1-3`).
    Spin,
}

impl CpuPlan {
    pub fn parse(s: &str) -> Option<CpuPlan> {
        match s {
            "none" => Some(CpuPlan::None),
            "pin" => Some(CpuPlan::Pin),
            "spin" => Some(CpuPlan::Spin),
            _ => None,
        }
    }
}

/// `park_us` of the Chainlink feed thread under `pin`/`spin` (core 0).
pub const SLOW_FEED_PARK_US: i64 = 1_000;

/// Applies `plan` to the `[[threads]]` of the `core.toml` at `path` (rewritten in place); `None`
/// leaves the file untouched.
pub fn apply_cpu_plan(path: &Path, plan: CpuPlan) -> Result<(), String> {
    if plan == CpuPlan::None {
        return Ok(());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut config: toml::Value = toml::from_str(&text).map_err(|e| format!("core.toml: {e}"))?;
    let threads = config
        .get_mut("threads")
        .and_then(toml::Value::as_array_mut)
        .ok_or("core.toml: missing threads")?;
    for t in threads.iter_mut() {
        let t = t.as_table_mut().ok_or("core.toml: thread is not a table")?;
        let task = t
            .get("task")
            .and_then(toml::Value::as_str)
            .unwrap_or("")
            .to_string();
        // The Chainlink feed (two samples a second, themselves ~1.2 s late) parks on core 0.
        let slow_feed = task == "feed:cl";
        let (core, hot) = if task == "sequencer" {
            (1, true)
        } else if task.starts_with("gw:") {
            (3, true)
        } else if task.starts_with("feed:") && !slow_feed {
            (2, false)
        } else {
            (0, false)
        };
        t.insert("core".into(), toml::Value::Integer(core));
        if slow_feed {
            t.insert("park_us".into(), toml::Value::Integer(SLOW_FEED_PARK_US));
        }
        if plan == CpuPlan::Spin {
            if hot {
                t.insert("idle".into(), "spin".into());
            } else if core == 2 {
                t.insert("park_us".into(), toml::Value::Integer(20));
            }
        }
    }
    let out = toml::to_string(&config).map_err(|e| format!("core.toml: {e}"))?;
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}
