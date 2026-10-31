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

/// Live `daily_loss_abs` without `PmLive.daily_loss_abs` (15 USDC).
pub const PM_LIVE_DAILY_LOSS: i64 = 15_000_000;

/// Binary-maker parameters of the first live run (`micro_s2_c5_cap10_mc20`, research reply
/// 2026-10-22): 5 shares per quote, net 10, no-chase 1 s, fair-value cooldown 1 s, streak pause
/// (2 fills within 10 s → 10 s), at most 20 USDC bought per window.
pub const PM_LIVE_PARAMS: [(&str, i64); 9] = [
    ("enabled", 1),
    ("clip_shares", 5),
    ("inv_max_shares", 10),
    ("fv_cool_ms", 1_000),
    ("nochase_ms", 1_000),
    ("streak_k", 2),
    ("streak_window_ms", 10_000),
    ("streak_pause_ms", 10_000),
    ("max_window_cost", 20_000_000),
];

/// Live Polymarket: OMS `ack_timeout_ms` and the REST request timeout (equal: the answer is read
/// as long as the OMS waits for it), and the REST connections' maximum idle age.
pub const PM_REST_TIMEOUT_MS: u32 = 5000;
pub const PM_REST_MAX_IDLE_MS: u32 = 25_000;

/// Label of the first Live account (`--live`).
pub const PM_LIVE_LABEL: &str = "pm-live";

/// `rr_max_cross` of the taker probe's price band (`--live --taker-probe`): its sell-back is a
/// risk-reducing Ioc at the best bid, half a spread below mid — 0.005 of 0.105 is 4.8 % — which
/// the maker's 1 % would refuse; 50 % as `max_px_dev`.
pub const PM_PROBE_RR_MAX_CROSS: i64 = 500_000_000;

/// The strategy of a generated configuration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PmStrategy {
    /// book-observer: one group per slot, never trades (the sim without a mode flag).
    Observer,
    /// binary-maker: one group per slot plus the reference group.
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
    /// `venues.pm.presign` = `PM_LIVE_PRESIGN`; false (`--no-presign`) = no table (off).
    pub presign: bool,
    /// Further Live accounts 2, 3, … (`--live-account`, `--account`), bound to the same
    /// binary-maker instance with `pm-live`'s risk limits; the strategy parameters stay one blob
    /// under `pm-live`.
    pub extra_accounts: Vec<LiveAccount>,
}

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
/// `venues.pm.net` (`rest_timeout_ms = 5000`, `rest_max_idle_ms = 25000`); every Polymarket
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
            (EndpointRole::MdPublic, "wss://stream.binance.com/ws"),
            (EndpointRole::MdMarket, "wss://stream.binance.com/ws"),
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
    // Groups: one per slot (and the reference); the probe has one, slot InstId 1.
    let n = if probe {
        1
    } else {
        slots.len() as u16 + u16::from(maker)
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
        let (asset, venue) = if i < slots.len() { (token(i), v) } else { (btc, bn) };
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
        ioc_max_cross: Rate(5_000_000),
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
    let caps = Capacities {
        max_insts: 16,
        max_cells_total: 32,
        max_timers: 1024,
        recent_cmd_ids: 256,
        max_subs_per_inst: 4,
        max_groups_per_instance: 16,
        max_instances: 4,
    };
    let mut engine = engine_base(&rd, &caps, ledger, if maker { vec![v, bn] } else { vec![v] });
    engine.drain_timeout_ms = 3000;
    engine.gws.venues.clear();
    engine.feeds = VenueSet((1 << v.0) | if maker { 1 << bn.0 } else { 0 });
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
{bn_text}[sim]
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
                 src_ip = '{src_ip}'\nbind_device = '{bind_device}'\nlive_feed = true\n",
                bn.0
            )
        } else {
            String::new()
        },
        maker_fill = if maker { "maker_fill = 'queue'\n" } else { "" },
    );
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
    for name in ["pm", "bn"] {
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
    // silently (hft-net "9. REST 客户端").
    pm.insert(
        "net".into(),
        parse(&format!(
            "rest_timeout_ms={PM_REST_TIMEOUT_MS}\nrest_max_idle_ms={PM_REST_MAX_IDLE_MS}"
        ))?,
    );
    if l.presign {
        pm.insert("presign".into(), parse(PM_LIVE_PRESIGN)?);
    }
    let threads = config
        .get_mut("threads")
        .and_then(toml::Value::as_array_mut)
        .ok_or("core.toml: missing threads")?;
    threads.retain(|t| !matches!(t["task"].as_str(), Some("sim-gw" | "paper-feed")));
    threads.push(parse("name='gw-pm'\ntask='gw:pm'\npark_us=100\nnice=0")?);
    toml::to_string(&config).map_err(|e| format!("core.toml: {e}"))
}
