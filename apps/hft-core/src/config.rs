//! Cold configuration loading. Blob bytes, not paths, become the boot journal inputs.
use crate::{Error, Strategies};
use hft_refdata::{AccountKind, RefData};
use hft_sequencer::{ConfigSet, EngineSetup, SegClock};
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, VenueId};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    pub instance: Instance,
    pub paths: Paths,
    pub blobs: Blobs,
    pub shutdown: Shutdown,
    pub journal: Journal,
    pub clock: Clock,
    pub watchdog: Watchdog,
    pub threads: Vec<Thread>,
    pub venues: BTreeMap<String, Venue>,
    pub rings: Rings,
    pub sim: Option<Paper>,
    #[serde(default)]
    pub startup: crate::startup::Config,
}
/// The run assembly (README "实例角色(prod 与 sim)" 运行装配): resolved once at start-up from
/// `--dry-run`, `instance.name` and `instance.order_env`; every mode-dependent branch of hft-core
/// reads it. Replay assemblies are replayer's drivers (`verify`, `research`), not hft-core runs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Assembly {
    /// Network mode: instance `prod`, `order_env` testnet or prod; Live accounts, gw per venue, and
    /// optionally Sim accounts (Shadow variants) on the `sim-venue` task when `[sim]` is present.
    Live { prod_orders: bool },
    /// `--dry-run`: instance `sim` without `order_env`; Sim accounts, sim-gw, paper or
    /// `live_feed` venues.
    Sim,
}
impl Assembly {
    /// The assembly of `--dry-run` and `[instance]`; any other combination → `None` (`EnvCombo`).
    pub fn resolve(dry: bool, i: &Instance) -> Option<Assembly> {
        match (dry, i.name.as_str(), i.order_env.as_deref()) {
            (true, "sim", None) => Some(Assembly::Sim),
            (false, "prod", Some("testnet")) => Some(Assembly::Live { prod_orders: false }),
            (false, "prod", Some("prod")) => Some(Assembly::Live { prod_orders: true }),
            _ => None,
        }
    }
    pub const fn is_sim(self) -> bool {
        matches!(self, Assembly::Sim)
    }
    pub const fn prod_orders(self) -> bool {
        matches!(self, Assembly::Live { prod_orders: true })
    }
}
impl Config {
    /// The assembly of a loaded configuration (load accepts `sim` only without `order_env` and
    /// `prod` only with it).
    pub fn assembly(&self) -> Assembly {
        if self.instance.order_env.is_none() {
            Assembly::Sim
        } else {
            Assembly::Live {
                prod_orders: self.instance.prod_orders(),
            }
        }
    }
    /// The task of the in-process simulated exchange: the sim instance's `sim-gw`, or the
    /// `sim-venue` of a prod instance with Sim accounts (`[sim]` present; README "实例角色").
    pub fn sim_task(&self) -> Option<&'static str> {
        match (self.assembly(), &self.sim) {
            (Assembly::Sim, _) => Some(SIM_GW),
            (Assembly::Live { .. }, Some(_)) => Some(SIM_VENUE),
            (Assembly::Live { .. }, None) => None,
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub name: String,
    pub order_env: Option<String>,
    #[serde(default)]
    pub feed_env: FeedEnv,
    pub config_source: String,
    #[serde(default)]
    pub secrets_mode: SecretsMode,
}
impl Instance {
    /// `order_env = "prod"`: orders and account queries go to production endpoints. Allowed only
    /// when every venue with accounts is Polymarket (`ProdOrderVenue`; README "运行模式").
    pub fn prod_orders(&self) -> bool {
        self.order_env.as_deref() == Some("prod")
    }
    pub fn order_env(&self) -> FeedEnv {
        if self.prod_orders() {
            FeedEnv::Prod
        } else {
            FeedEnv::Testnet
        }
    }
}

/// The sim instance's simulated-exchange task.
pub const SIM_GW: &str = "sim-gw";
/// The simulated-exchange task of a prod instance's Sim accounts (Shadow variants): warm, on a
/// non-isolated core, at the lowest priority of the plan (README "线程计划").
pub const SIM_VENUE: &str = "sim-venue";

/// Protocols whose accounts may trade on production endpoints (`order_env = "prod"`).
pub const PROD_ORDER_PROTOCOLS: &[&str] = &["polymarket-clob"];

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedEnv {
    Prod,
    #[default]
    Testnet,
}
impl FeedEnv {
    pub fn endpoint_env(self) -> hft_refdata::EndpointEnv {
        match self {
            Self::Prod => hft_refdata::EndpointEnv::Prod,
            Self::Testnet => hft_refdata::EndpointEnv::Testnet,
        }
    }
    pub fn venue_env(self) -> hft_venue_proto::VenueEnv {
        match self {
            Self::Prod => hft_venue_proto::VenueEnv::Prod,
            Self::Testnet => hft_venue_proto::VenueEnv::Testnet,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretsMode {
    #[default]
    AwsSecretsManager,
    LocalDemo,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paths {
    pub journal_ram: PathBuf,
    pub journal_cold: PathBuf,
    pub ctl_socket: PathBuf,
    pub ctl_principals: PathBuf,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blob {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blobs {
    pub refdata: Blob,
    pub engine: Blob,
    pub risk_params: Blob,
    pub strategy_params: Vec<Blob>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shutdown {
    pub cancel_open_orders: bool,
    pub drain_timeout_ms: u32,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    /// Optional bounded per-worker diagnostic transport, drained by persist.
    pub latency_trace_capacity: Option<usize>,
    pub segment_bytes: u64,
    pub hot_keep: u8,
    pub raw_feed: Option<JournalSize>,
    pub raw_gw: Option<JournalSize>,
}
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalSize {
    pub segment_bytes: u64,
    pub hot_keep: u8,
}
impl Journal {
    /// Names are allocated by the assembly; omitted raw layouts retain the original config.
    pub fn layout(&self, name: &str) -> JournalSize {
        let raw = if name.starts_with("raw-feed-") {
            self.raw_feed
        } else if name.starts_with("raw-gw-") {
            self.raw_gw
        } else {
            None
        };
        raw.unwrap_or(JournalSize {
            segment_bytes: self.segment_bytes,
            hot_keep: self.hot_keep,
        })
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub step_threshold_ns: u32,
    pub interval_ms: u32,
    /// Period of the `UtcAnchor` Input (engine UTC re-anchoring); rounded up to whole
    /// `interval_ms` ticks.
    #[serde(default = "default_anchor_interval_ms")]
    pub anchor_interval_ms: u32,
}
const fn default_anchor_interval_ms() -> u32 {
    10_000
}
impl Clock {
    /// Calibration ticks between two `UtcAnchor` Inputs (≥ 1).
    pub fn anchor_every(&self) -> u32 {
        self.anchor_interval_ms.div_ceil(self.interval_ms.max(1)).max(1)
    }
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Watchdog {
    pub hb_stale_ms: u32,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thread {
    pub name: String,
    pub task: String,
    pub core: Option<u16>,
    pub park_us: u64,
    pub nice: u8,
    #[serde(default)]
    pub idle: Idle,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Idle {
    #[default]
    Park,
    Spin,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rings {
    pub elements: usize,
    pub bytes: usize,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Venue {
    pub protocol: String,
    pub id: u8,
    pub max_sub_insts: u32,
    pub stale_guard_ms: u32,
    pub src_ip: Option<std::net::Ipv4Addr>,
    pub bind_device: Option<String>,
    pub account_io: Option<AccountIo>,
    #[serde(default)]
    pub accounts: Vec<Keys>,
    /// `--dry-run` only: this venue's market data comes from the exchange's public feed instead of
    /// `sim.quotes` (orders stay in sim-gw; no account, no key).
    #[serde(default)]
    pub live_feed: bool,
    /// hft-venue feed keys (`venues.<v>.feed.*`); `feed.raw` and `feed.depth_conns` in this
    /// assembly.
    #[serde(default)]
    pub feed: FeedOpts,
    /// hft-net `NetConfig` keys of this venue's connections (`venues.<v>.net.*`).
    #[serde(default)]
    pub net: NetOpts,
    /// `venues.<v>.presign` (`polymarket-clob` only): pre-sign orders in the gw's idle time;
    /// absent = off.
    #[serde(default)]
    pub presign: Option<PresignOpts>,
    /// hft-venue gw keys (`venues.<v>.gw.*`); `gw.rest_conns` and `gw.rest_cancel_reserve` in
    /// this assembly.
    #[serde(default)]
    pub gw: GwOpts,
}
/// `venues.<v>.presign` (venue-polymarket "预签名"): prices `below`/`above` the last place's in
/// `step`s (decimal USDC text), each order used within `max_age_ms` of its signing.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresignOpts {
    pub below: u8,
    pub above: u8,
    pub step: String,
    pub max_age_ms: u32,
}
impl PresignOpts {
    /// `step` in 1e-6 USDC: positive, at most six decimals.
    pub fn step_e6(&self) -> Option<u32> {
        let (int, frac) = self.step.split_once('.').unwrap_or((self.step.as_str(), ""));
        if int.is_empty() && frac.is_empty()
            || frac.len() > 6
            || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let int: u32 = if int.is_empty() { 0 } else { int.parse().ok()? };
        let frac: u32 = if frac.is_empty() {
            0
        } else {
            format!("{frac:0<6}").parse().ok()?
        };
        int.checked_mul(1_000_000)?.checked_add(frac).filter(|v| *v > 0)
    }
}
/// `venues.<v>.gw.{rest_conns,rest_cancel_reserve}` (hft-venue "配置项"): absent = the protocol's
/// `GW_REST_CONNS` / `GW_REST_CANCEL_RESERVE`.
#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GwOpts {
    pub rest_conns: Option<u8>,
    pub rest_cancel_reserve: Option<u8>,
}
impl GwOpts {
    /// `(rest_conns, rest_cancel_reserve)` in effect over the protocol defaults `(conns, reserve)`.
    pub fn resolve(&self, (conns, reserve): (u8, u8)) -> (u8, u8) {
        (
            self.rest_conns.unwrap_or(conns),
            self.rest_cancel_reserve.unwrap_or(reserve),
        )
    }
    /// The `BadValue` text when the effective pair is out of range (1..=`GW_REST_CONNS_MAX`
    /// connections, reserve below them).
    pub fn check(&self, name: &str, defaults: (u8, u8)) -> Option<String> {
        let (conns, reserve) = self.resolve(defaults);
        if conns == 0 || usize::from(conns) > hft_venue::GW_REST_CONNS_MAX {
            Some(format!(
                "venues.{name}.gw.rest_conns must be 1..={}",
                hft_venue::GW_REST_CONNS_MAX
            ))
        } else if reserve >= conns {
            Some(format!(
                "venues.{name}.gw.rest_cancel_reserve must be below rest_conns ({conns})"
            ))
        } else {
            None
        }
    }
}
/// `venues.<v>.net.{rest_timeout_ms,rest_max_idle_ms}` (hft-net "配置项"): absent = the
/// `NetConfig` default; applies to every REST pool of the venue (gw, snap-fetch).
#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetOpts {
    pub rest_timeout_ms: Option<u32>,
    pub rest_max_idle_ms: Option<u32>,
}
impl NetOpts {
    /// Overrides the configured keys in `net`.
    pub fn apply(&self, net: &mut hft_net::NetConfig) {
        if let Some(ms) = self.rest_timeout_ms {
            net.rest_timeout_ms = ms;
        }
        if let Some(ms) = self.rest_max_idle_ms {
            net.rest_max_idle_ms = ms;
        }
    }
    /// The REST request timeout in effect.
    pub fn rest_timeout_ms(&self) -> u32 {
        let mut net = hft_net::NetConfig::default();
        self.apply(&mut net);
        net.rest_timeout_ms
    }
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedOpts {
    #[serde(default)]
    pub raw: RawOpts,
    /// `venues.<v>.feed.depth_conns` (hft-venue): connections per DepthTopN group; absent = 1, or
    /// 2 for a protocol without a Bbo route (Polymarket, whose depth stream is the whole book).
    pub depth_conns: Option<u8>,
}
/// `venues.<v>.feed.raw.{bbo,md}` (hft-venue): absent = the instance default, `on` for network
/// mode and `off` for `--dry-run` (the sim instance of the memory table).
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawOpts {
    pub bbo: Option<RawKind>,
    pub md: Option<RawKind>,
}
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RawKind {
    Off,
    Sampled,
    On,
}
impl RawKind {
    /// The configured mode, else `on` in network mode and `off` in `--dry-run`.
    pub fn resolve(v: Option<RawKind>, dry: bool) -> hft_venue::RawMode {
        match v.unwrap_or(if dry { RawKind::Off } else { RawKind::On }) {
            RawKind::Off => hft_venue::RawMode::Off,
            RawKind::Sampled => hft_venue::RawMode::Sampled,
            RawKind::On => hft_venue::RawMode::On,
        }
    }
}
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountIo {
    pub timeout_ms: u32,
    pub interval_ms: u32,
    pub request_gap_ms: u32,
    pub max_cancel_rounds: u32,
    pub listen_keepalive_ms: u32,
}
// Never contains secret values. Contents are read only when constructing a real session.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    pub account: u8,
    pub api_key_path: PathBuf,
    #[serde(alias = "session_secret_path")]
    pub session_seed_path: PathBuf,
    pub rest_api_key_path: PathBuf,
    pub rest_secret_path: PathBuf,
    pub rest_sign_alg: Option<String>,
    pub passphrase_path: Option<PathBuf>,
    /// This account's egress IP (README "多账户"); absent = the venue's `src_ip`. Bound to the
    /// venue's `bind_device`.
    #[serde(default)]
    pub src_ip: Option<std::net::Ipv4Addr>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paper {
    pub seed: u64,
    pub entry_us: u32,
    pub resp_us: u32,
    pub leverage: u8,
    pub mmr_raw: i64,
    pub liq_fee_raw: i64,
    pub liq_slippage_bp: u16,
    /// Synthetic quote period of the `--dry-run` paper venues (positive there); absent or 0 in a
    /// prod instance, which has no paper venue.
    #[serde(default)]
    pub quote_interval_ms: u32,
    /// Required while any venue is a paper venue (not `live_feed`); empty in a prod instance.
    #[serde(default)]
    pub quotes: Vec<Quote>,
    /// hft-sim `maker_fill` (resting-order fill model); absent = `trade_through`.
    #[serde(default)]
    pub maker_fill: MakerFillKind,
    /// Calibration table (hft-sim `calib`, README "[sim] 校准表"): venue-behaviour options and
    /// conditional latency pools; its `maker_fill` overrides the one above.
    #[serde(default)]
    pub calib: Option<PathBuf>,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MakerFillKind {
    #[default]
    TradeThrough,
    Queue,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub inst: u32,
    pub bid_raw: i64,
    pub ask_raw: i64,
    pub qty_raw: i64,
}

pub struct Loaded {
    pub startup_trace: crate::startup::Serial,
    pub cfg: Config,
    pub assembly: Assembly,
    pub rd: RefData,
    pub setup: EngineSetup,
    pub configs: ConfigSet<Strategies>,
    pub principals: hft_ctl::PrincipalTable,
}
impl std::fmt::Debug for Loaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loaded").finish_non_exhaustive()
    }
}
pub fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn resolve(root: &Path, p: &mut PathBuf) {
    if !p.as_os_str().is_empty() && p.is_relative() {
        *p = root.join(&*p);
    }
}
fn read_blob(b: &Blob) -> Result<Vec<u8>, Error> {
    let bytes =
        std::fs::read(&b.path).map_err(|_| Error::config("FileRead", b.path.display().to_string()))?;
    if b.sha256 != hex(&digest(&bytes)) {
        return Err(Error::config("BlobHash", b.path.display().to_string()));
    }
    Ok(bytes)
}
pub fn parse(path: &Path) -> Result<Config, Vec<Error>> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| vec![Error::config("FileRead", path.display().to_string())])?;
    crate::config_shape::check(&text)?;
    // Do not echo TOML source: an accidentally inline key must not be printed on parse failure.
    let mut c: Config = toml::from_str(&text).map_err(|_| {
        vec![Error::config(
            "ConfigParse",
            "invalid TOML, unknown field, or missing required field",
        )]
    })?;
    if c.instance.secrets_mode == SecretsMode::LocalDemo
        && (c.instance.name != "prod" || c.instance.order_env.as_deref() != Some("testnet"))
    {
        return Err(vec![Error::config(
            "LocalSecretEnvironment",
            "local secrets require demo/testnet",
        )]);
    }
    let root = path.parent().unwrap_or(Path::new("."));
    for p in [
        &mut c.paths.journal_ram,
        &mut c.paths.journal_cold,
        &mut c.paths.ctl_socket,
        &mut c.paths.ctl_principals,
    ] {
        resolve(root, p);
    }
    for b in [
        &mut c.blobs.refdata,
        &mut c.blobs.engine,
        &mut c.blobs.risk_params,
    ] {
        resolve(root, &mut b.path);
    }
    for b in &mut c.blobs.strategy_params {
        resolve(root, &mut b.path);
    }
    for v in c.venues.values_mut() {
        for k in &mut v.accounts {
            for p in [
                &mut k.api_key_path,
                &mut k.session_seed_path,
                &mut k.rest_api_key_path,
                &mut k.rest_secret_path,
            ] {
                resolve(root, p);
            }
            if let Some(p) = &mut k.passphrase_path {
                resolve(root, p);
            }
        }
    }
    Ok(c)
}

/// `sim-venue` placement (README "线程计划", rule 14): the lowest priority of the plan (`nice` ≥
/// every other thread's) and no core shared with a hot task (`sequencer`, `feed:`/`feed-`, `gw:`,
/// `ext:`) or a spinning thread. Whether its core is isolated is checked when the thread starts.
pub fn sim_venue_placement(threads: &[Thread]) -> Option<Error> {
    let (ix, t) = threads.iter().enumerate().find(|(_, t)| t.task == SIM_VENUE)?;
    let hot = |o: &Thread| {
        o.idle == Idle::Spin
            || o.task == "sequencer"
            || ["feed:", "feed-bbo:", "feed-md:", "gw:", "ext:"]
                .iter()
                .any(|p| o.task.starts_with(p))
    };
    threads
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != ix)
        .any(|(_, o)| o.nice > t.nice || (t.core.is_some() && o.core == t.core && hot(o)))
        .then(|| {
            Error::config(
                "ThreadPlan",
                format!("{}: sim-venue takes the lowest nice and no hot core", t.name),
            )
        })
}

/// `SharedAccountIp` (README "多账户"): two Live accounts of venue `venue` on one egress IP,
/// unless its protocol declares `SHARED_EGRESS_IP` (`shared_ok`). `accounts` = (account, src_ip).
pub fn shared_account_ip(
    venue: &str,
    shared_ok: bool,
    accounts: &[(u8, std::net::Ipv4Addr)],
) -> Option<Error> {
    if shared_ok {
        return None;
    }
    accounts.iter().enumerate().find_map(|(i, (a, ip))| {
        accounts[..i].iter().find(|(_, other)| other == ip).map(|(b, _)| {
            Error::config(
                "SharedAccountIp",
                format!("venues.{venue}: accounts {b} and {a} share one src_ip"),
            )
        })
    })
}

pub fn load(path: &Path, dry: bool) -> Result<Loaded, Vec<Error>> {
    load_timed(path, dry, crate::startup::Serial::default())
}

pub fn load_timed(
    path: &Path,
    dry: bool,
    mut startup_trace: crate::startup::Serial,
) -> Result<Loaded, Vec<Error>> {
    let c = parse(path)?;
    let mut errors = Vec::new();
    macro_rules! bad {
        ($code:expr, $s:expr) => {{
            let detail: String = $s;
            errors.push(Error::config($code, detail));
        }};
    }
    if c.schema != 1 {
        bad!("BadValue", "schema must be 1".into());
    }
    if c.startup.budget_s == 0 || c.startup.gate_timeout_s == 0 {
        bad!("BadValue", "startup budgets must be positive".into());
    }
    if c.clock.step_threshold_ns == 0 || c.clock.interval_ms == 0 || c.clock.anchor_interval_ms == 0 {
        bad!(
            "BadValue",
            "clock threshold and intervals must be positive".into()
        );
    }
    if c.watchdog.hb_stale_ms == 0 {
        bad!("BadValue", "watchdog.hb_stale_ms must be positive".into());
    }
    if c.instance.config_source != "file" {
        bad!(
            "BadValue",
            "this assembly requires config_source=file and a fresh journal".into()
        );
    }
    let assembly = Assembly::resolve(dry, &c.instance);
    // From here on the mode is the assembly's (the flag where the combination is refused).
    let sim = assembly.map_or(dry, Assembly::is_sim);
    if assembly.is_none() {
        bad!(
            "EnvCombo",
            "--dry-run requires sim without order_env; network mode requires prod with order_env testnet or prod"
                .into()
        );
    }
    if c.instance.prod_orders() {
        if c.instance.feed_env != FeedEnv::Prod {
            bad!("EnvCombo", "order_env prod requires feed_env prod".into());
        }
        for (name, v) in &c.venues {
            if !v.accounts.is_empty() && !PROD_ORDER_PROTOCOLS.contains(&v.protocol.as_str()) {
                bad!(
                    "ProdOrderVenue",
                    format!("venue {name}: production orders are enabled for Polymarket only")
                );
            }
        }
    }
    // A prod instance has `[sim]` exactly when its refdata has Sim accounts (checked below).
    if sim && c.sim.is_none() {
        bad!("SimConfig", "--dry-run requires the sim section".into());
    }
    if c.rings.elements < 256
        || c.rings.elements > 65536
        || !c.rings.elements.is_power_of_two()
        || c.rings.bytes < 262144
        || c.rings.bytes > 4194304
        || !c.rings.bytes.is_power_of_two()
    {
        bad!(
            "EngineMismatch",
            "ring elements 256..65536 and bytes 262144..4194304 must be powers of two".into()
        );
    }
    if let Some(cap) = c.journal.latency_trace_capacity
        && (!(256..=65536).contains(&cap)
            || !cap.is_power_of_two()
            || (cap as u128) * (c.threads.len() as u128) * 120 > 32 << 20)
    {
        bad!(
            "BadValue",
            "latency trace: power-of-two capacity 256..65536 and total ring storage <=32 MiB required".into()
        );
    }
    if !(8 << 20..=256 << 20).contains(&c.journal.segment_bytes)
        || !c.journal.segment_bytes.is_multiple_of(4096)
        || c.journal.hot_keep == 0
    {
        bad!(
            "BadValue",
            "journal segment_bytes must be page aligned, 8..256 MiB; hot_keep > 0".into()
        );
    }
    if c.shutdown.drain_timeout_ms == 0 {
        bad!("BadValue", "drain_timeout_ms must be nonzero".into());
    }
    for raw in [c.journal.raw_feed, c.journal.raw_gw].into_iter().flatten() {
        if !(2 << 20..=256 << 20).contains(&raw.segment_bytes) || !raw.segment_bytes.is_multiple_of(4096) {
            bad!(
                "BadValue",
                "raw journal segment_bytes must be page aligned, 2..256 MiB".into()
            );
        }
    }
    let mut tasks = vec![
        "sequencer".to_string(),
        "persist".into(),
        "housekeeping".into(),
        "ctl-gateway".into(),
    ];
    if sim {
        tasks.push(SIM_GW.into());
        tasks.push("paper-feed".into());
    } else if c.sim.is_some() {
        tasks.push(SIM_VENUE.into());
    }
    let mut vids = Vec::new();
    for (name, v) in &c.venues {
        if !(1..64).contains(&v.id) || vids.contains(&v.id) {
            bad!("UnknownVenue", name.clone());
        }
        vids.push(v.id);
        if let Err(e) = crate::venues::registered(&v.protocol, VenueId(v.id)) {
            errors.push(e);
        }
        if v.max_sub_insts == 0 || v.stale_guard_ms == 0 {
            errors.push(Error::config("SubCapacity", name));
        }
        if let Some(e) = crate::venues::presign_error(name, v) {
            errors.push(e);
        }
        if v.net.rest_timeout_ms == Some(0) {
            errors.push(Error::config(
                "BadValue",
                format!("venues.{name}.net.rest_timeout_ms must be positive"),
            ));
        }
        if let Some(why) = crate::venues::gw_rest_defaults(&v.protocol).and_then(|d| v.gw.check(name, d)) {
            errors.push(Error::config("BadValue", why));
        }
        if sim && !v.accounts.is_empty() {
            errors.push(Error::config("SecretsInSim", name));
        }
        if v.live_feed && !sim {
            errors.push(Error::config("LiveFeed", "live_feed is a --dry-run venue option"));
        }
        if sim && v.live_feed {
            tasks.push(format!("feed:{name}"));
            tasks.push(format!("conn-builder:{name}"));
            if v.src_ip.is_none() || v.bind_device.as_ref().is_none_or(|s| s.is_empty()) {
                errors.push(Error::config(
                    "MissingField",
                    format!("venues.{name}.src_ip/bind_device"),
                ));
            }
        }
        if !sim {
            // One gw polls every Live account of the venue: its kill word has one bit per account.
            if v.accounts.len() > usize::from(hft_types::KillBits::MAX_ACCOUNTS_PER_GW) {
                errors.push(Error::config(
                    "AccountCapacity",
                    format!(
                        "venues.{name}: at most {} live accounts per venue",
                        hft_types::KillBits::MAX_ACCOUNTS_PER_GW
                    ),
                ));
            }
            // Each account's egress IP: its own `src_ip`, else the venue's.
            if let Some(ip) = v.src_ip {
                let rows: Vec<(u8, std::net::Ipv4Addr)> = v
                    .accounts
                    .iter()
                    .map(|k| (k.account, k.src_ip.unwrap_or(ip)))
                    .collect();
                if let Some(e) = shared_account_ip(name, crate::venues::shared_egress_ip(&v.protocol), &rows)
                {
                    errors.push(e);
                }
            }
            for task in [format!("feed:{name}"), format!("conn-builder:{name}")] {
                tasks.push(task);
            }
            if !v.accounts.is_empty() {
                tasks.push(format!("gw:{name}"));
                if v.account_io.is_none_or(|c| {
                    c.timeout_ms == 0
                        || c.interval_ms == 0
                        || c.request_gap_ms == 0
                        || c.max_cancel_rounds == 0
                        || c.listen_keepalive_ms == 0
                        || c.listen_keepalive_ms >= 3_600_000
                }) {
                    errors.push(Error::config(
                        "AccountIo",
                        "explicit nonzero account polling settings required",
                    ));
                }
            }
            if v.src_ip.is_none() || v.bind_device.as_ref().is_none_or(|s| s.is_empty()) {
                errors.push(Error::config(
                    "MissingField",
                    format!("venues.{name}.src_ip/bind_device"),
                ));
            }
            for k in &v.accounts {
                if k.rest_sign_alg
                    .as_deref()
                    .is_some_and(|s| s != "ed25519" && s != "hmac-sha256")
                {
                    errors.push(Error::config("CredentialAlgorithm", name));
                }
                if k.api_key_path.as_os_str().is_empty()
                    || k.session_seed_path.as_os_str().is_empty()
                    || k.rest_api_key_path.as_os_str().is_empty()
                    || k.rest_secret_path.as_os_str().is_empty()
                    || (v.protocol.starts_with("bitget-")
                        && k.passphrase_path
                            .as_ref()
                            .is_none_or(|p| p.as_os_str().is_empty()))
                {
                    errors.push(Error::config("MissingCredential", name));
                }
            }
        }
    }
    let mut names = Vec::new();
    if c.threads.len() > 30 {
        errors.push(Error::config("ThreadPlan", "heartbeat capacity is 30 threads"));
    }
    for t in &c.threads {
        if names.contains(&t.name)
            || t.name.is_empty()
            || t.name.len() > 15
            || t.name.as_bytes().contains(&0)
            || t.core.is_some_and(|n| n >= 256)
            || t.nice > 19
            || (t.task == "housekeeping" && t.nice != 0)
            || !(1..=10000).contains(&t.park_us)
            || u128::from(t.park_us) * 4 >= u128::from(c.watchdog.hb_stale_ms) * 1000
            || !tasks.contains(&t.task)
            || (t.idle == Idle::Spin
                && (t.core.is_none()
                    || !(t.task == "sequencer" || t.task.starts_with("feed:") || t.task.starts_with("gw:"))
                    || c.threads.iter().filter(|other| other.core == t.core).count() != 1))
        {
            errors.push(Error::config("ThreadPlan", &t.name));
        }
        names.push(t.name.clone());
    }
    for task in tasks {
        if c.threads.iter().filter(|t| t.task == task).count() != 1 {
            errors.push(Error::config("ThreadPlan", task));
        }
    }
    if let Some(e) = sim_venue_placement(&c.threads) {
        errors.push(e);
    }
    // Independent file failures are reported together.
    let mut read = |b: &Blob| match read_blob(b) {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push(e);
            None
        }
    };
    let rd_blob = read(&c.blobs.refdata);
    let ec_blob = read(&c.blobs.engine);
    let risk_blob = read(&c.blobs.risk_params);
    let params: Vec<_> = c.blobs.strategy_params.iter().filter_map(&mut read).collect();
    let principals = match hft_ctl::PrincipalTable::load(&c.paths.ctl_principals) {
        Ok(p) => Some(p),
        Err(e) => {
            errors.push(Error::config("CtlPrincipals", format!("{e:?}")));
            None
        }
    };
    if !errors.is_empty() {
        return Err(errors);
    }
    let one = |code, detail: String| vec![Error::config(code, detail)];
    startup_trace.mark(1, true);
    startup_trace.mark(2, false);
    let rd_blob = rd_blob.ok_or_else(|| one("FileRead", "refdata".into()))?;
    let ec_blob = ec_blob.ok_or_else(|| one("FileRead", "engine".into()))?;
    let risk_blob = risk_blob.ok_or_else(|| one("FileRead", "risk".into()))?;
    let rd = RefData::load(&rd_blob, None).map_err(|report| {
        report
            .errors
            .iter()
            .map(|e| Error::config(e.code.name(), format!("section={} row={}", e.section, e.row)))
            .collect::<Vec<_>>()
    })?;
    let setup = EngineSetup::decode_blob(
        &ec_blob,
        &rd,
        SegClock {
            tsc_hz_milli: 1_000_000_000_000,
            calib_tsc: 0,
            calib_realtime_ns: 0,
        },
        1,
        0,
    )
    .map_err(|e| one("EngineMismatch", format!("{e:?}")))?;
    for (name, v) in &c.venues {
        // A REST order answer must still be read when the OMS gives up on its acknowledgement.
        let ack = setup.oms.ack_timeout_ms.get(VenueId(v.id));
        if !v.accounts.is_empty() && v.net.rest_timeout_ms() > ack {
            errors.push(Error::config(
                "RestTimeoutAboveAck",
                format!(
                    "venues.{name}.net.rest_timeout_ms {} > OMS ack_timeout_ms {ack}",
                    v.net.rest_timeout_ms()
                ),
            ));
        }
    }
    for (name, v) in &c.venues {
        if !v.accounts.is_empty()
            && let Err(e) = crate::venues::check_kill_caps(&v.protocol, rd.caps(VenueId(v.id)))
        {
            errors.push(e);
        }
        if c.instance.secrets_mode == SecretsMode::LocalDemo {
            let endpoints = rd.endpoints(VenueId(v.id), hft_refdata::EndpointEnv::Testnet);
            if endpoints.is_empty()
                || endpoints
                    .iter()
                    .any(|e| !crate::venues::local_demo_endpoint(&v.protocol, e.role, &e.url))
            {
                errors.push(Error::config(
                    "LocalSecretEndpoint",
                    "local demo mode requires explicit demo hosts only",
                ));
            }
        }
        let public = rd.endpoints(VenueId(v.id), hft_refdata::EndpointEnv::Prod);
        if (c.instance.feed_env == FeedEnv::Testnet && !public.is_empty())
            || (c.instance.feed_env == FeedEnv::Prod
                && (public.iter().any(|e| {
                    !crate::venues::public_market_endpoint(&v.protocol, e.role, &e.url)
                        && !(c.instance.prod_orders()
                            && crate::venues::prod_order_endpoint(&v.protocol, e.role, &e.url))
                }) || [
                    hft_refdata::EndpointRole::MdPublic,
                    hft_refdata::EndpointRole::MdMarket,
                    hft_refdata::EndpointRole::Rest,
                ]
                .iter()
                .any(|role| !public.iter().any(|e| e.role == *role))))
        {
            errors.push(Error::config(
                "PublicFeedEndpoint",
                "production feed requires only explicit public market endpoints",
            ));
        }
        for k in &v.accounts {
            let account = rd.account(AccountId(k.account));
            if k.account == 0
                || account.id != AccountId(k.account)
                || account.venues & (1u64 << v.id) == 0
                || !matches!(account.kind, AccountKind::Live)
            {
                errors.push(Error::config(
                    "AccountMismatch",
                    format!("venue {name}, account {}", k.account),
                ));
            }
        }
    }
    if setup.role_prod == sim || setup.drain_timeout_ms != c.shutdown.drain_timeout_ms {
        errors.push(Error::config(
            "EngineMismatch",
            "role/drain timeout differs from ECF1",
        ));
    }
    if sim && setup.seq.allow_missing_standby != 1 {
        errors.push(Error::config(
            "EngineMismatch",
            "paper mode has one virtual order connection; ECF1 must allow missing standby",
        ));
    }
    let risk = hft_risk::RiskParams::decode_blob(&risk_blob, &rd)
        .map_err(|e| one("RiskParams", format!("{e:?}")))?;
    let mut parsed = Vec::new();
    for b in params {
        // Report the stable startup diagnostics before the generic parameter decoder.
        // SPB1 parsing requires an aligned slice; a byte Vec has no alignment contract.
        use zerocopy::IntoBytes;
        let mut aligned = vec![0u64; b.len().div_ceil(8)];
        aligned.as_mut_slice().as_mut_bytes()[..b.len()].copy_from_slice(&b);
        if let Ok(view) = hft_event::parse_spb1(&aligned.as_slice().as_bytes()[..b.len()]) {
            match Strategies::kinds().iter().find(|d| d.kind_id == view.hdr.kind_id) {
                None => {
                    errors.push(Error::config(
                        "UnknownStrategyKind",
                        format!("kind {}", view.hdr.kind_id),
                    ));
                    continue;
                }
                Some(desc) if desc.non_prod_only && rd.account(view.hdr.account).is_prod => {
                    errors.push(Error::config(
                        "ProbeOnProdAccount",
                        format!("account {}", view.hdr.account.0),
                    ));
                    continue;
                }
                _ => {}
            }
        }
        match hft_sequencer::parsed::parse_params::<Strategies>(
            &b,
            digest(&b),
            &rd,
            setup.caps.max_groups_per_instance,
            None,
        ) {
            Ok(p) => {
                // The same group rule as SetParams (ctl-gateway and sequencer).
                if let Err(e) = hft_sequencer::parsed::check_groups(&rd, p.account, &p.groups) {
                    errors.push(Error::config("StrategyParams", format!("detail={e}")));
                }
                if p.model_mem != 0 || p.model_ckpt != 0 {
                    errors.push(Error::config(
                        "AdvisorUnavailable",
                        "this assembly supports registered NoModel strategies only",
                    ));
                }
                parsed.push((b, p));
            }
            Err(e) => errors.push(Error::config("StrategyParams", format!("detail={e}"))),
        }
    }
    for a in 1..=u8::MAX {
        let m = rd.account(AccountId(a));
        if m.id.0 == 0 {
            continue;
        }
        let sim_account = matches!(m.kind, AccountKind::Sim(_));
        // The sim instance has Sim accounts only; a prod instance has Live accounts whose
        // `is_prod` follows `order_env`, and may have Sim accounts (Shadow variants; refdata
        // refuses `is_prod` on them, `SimIsProd`).
        if (sim && !sim_account) || (!sim_account && m.is_prod != c.instance.prod_orders()) {
            errors.push(Error::config("EnvAccountMismatch", format!("account {a}")));
        }
        if !sim && sim_account {
            // AccountNamePrefix: a prod instance's Sim account is named `sim-…`, as hft-cfg
            // requires of every Sim account.
            if !m.label.starts_with("sim-") {
                errors.push(Error::config("AccountNamePrefix", format!("account {a}")));
            }
            // A strategy instance trades Live accounts or Sim accounts, never both.
            if rd
                .accounts_of(m.bound_instance)
                .iter()
                .any(|x| !matches!(rd.account(*x).kind, AccountKind::Sim(_)))
            {
                errors.push(Error::config(
                    "EnvAccountMismatch",
                    format!(
                        "account {a}: instance {} mixes Live and Sim accounts",
                        m.bound_instance.0
                    ),
                ));
            }
        }
        // One parameter blob per instance, under its primary account (the lowest id of its account
        // set; with one account per instance, the account itself).
        let primary = rd.account_of(m.bound_instance).unwrap_or(m.id);
        let rows: Vec<_> = parsed
            .iter()
            .filter(|(_, p)| p.account == primary && p.strategy == m.bound_instance)
            .collect();
        if rows.len() != 1 {
            errors.push(Error::config("BindingMismatch", format!("account {a}")));
        }
        if rows.iter().any(|(_, p)| {
            Strategies::kinds()
                .iter()
                .find(|d| d.kind_id == p.kind_id)
                .is_none()
        }) {
            errors.push(Error::config("UnknownStrategyKind", format!("account {a}")));
        }
        for v in 1..64 {
            if m.venues & (1u64 << v) != 0 {
                match c.venues.values().find(|x| x.id == v) {
                    None => errors.push(Error::config("UnknownVenue", format!("account {a}, venue {v}"))),
                    Some(vc)
                        if !sim
                            && !sim_account
                            && vc.accounts.iter().filter(|k| k.account == a).count() != 1 =>
                    {
                        errors.push(Error::config(
                            "MissingCredential",
                            format!("account {a}, venue {v}"),
                        ))
                    }
                    _ => {}
                }
            }
        }
    }
    for (_, p) in &parsed {
        if parsed
            .iter()
            .filter(|(_, other)| other.strategy == p.strategy)
            .count()
            != 1
        {
            errors.push(Error::config(
                "BindingMismatch",
                format!("instance {}", p.strategy.0),
            ));
        }
        let exec = setup
            .exec
            .accounts
            .iter()
            .find(|a| a.account == p.account)
            .and_then(|a| a.params)
            .unwrap_or(setup.exec.default);
        let min_notional = p
            .groups
            .specs()
            .iter()
            .flat_map(|g| g.legs.iter().take(usize::from(g.n_legs)))
            .map(|leg| rd.inst(leg.inst).min_notional)
            .max()
            .unwrap_or(hft_types::Notional(0));
        if exec.dust_notional < min_notional {
            errors.push(Error::config(
                "DustBelowMinNotional",
                format!("account {}", p.account.0),
            ));
        }
    }
    if !sim {
        // Shadow variants: the sim-venue settings (`[sim]`) exactly with Sim accounts.
        let sim_accounts = (1..=u8::MAX)
            .map(|a| rd.account(AccountId(a)))
            .any(|m| m.id.0 != 0 && matches!(m.kind, AccountKind::Sim(_)));
        if sim_accounts != c.sim.is_some() {
            errors.push(Error::config(
                "SimConfig",
                "a prod instance has the sim section exactly when its refdata has Sim accounts",
            ));
        }
        if let Some(paper) = &c.sim {
            if paper.quote_interval_ms != 0 || !paper.quotes.is_empty() || paper.leverage == 0 {
                errors.push(Error::config(
                    "SimConfig",
                    "prod instance: leverage required, no quotes or quote_interval_ms",
                ));
            }
            // The SimExchange the sim-venue task builds (README 启动步 2: dry validation).
            if sim_accounts && let Err(e) = crate::paper::exchange(paper, &rd, 1_000_000_000_000) {
                errors.push(e);
            }
        }
    }
    if let Some(paper) = c.sim.as_ref().filter(|_| sim) {
        let paper_venues = c.venues.values().any(|v| !v.live_feed);
        if paper.quote_interval_ms == 0 || (paper_venues && paper.quotes.is_empty()) || paper.leverage == 0 {
            errors.push(Error::config(
                "SimConfig",
                "quotes, quote_interval_ms and leverage required",
            ));
        }
        for q in &paper.quotes {
            if q.inst == 0
                || q.inst as usize > rd.inst_count()
                || q.bid_raw <= 0
                || q.ask_raw <= q.bid_raw
                || q.qty_raw <= 0
                || c.venues
                    .values()
                    .any(|v| v.live_feed && VenueId(v.id) == rd.inst(hft_types::InstId(q.inst)).venue)
            {
                errors.push(Error::config("SimConfig", format!("quote {}", q.inst)));
            }
        }
    }
    let feed_bits = c.venues.values().fold(0u64, |bits, v| bits | (1u64 << v.id));
    let subscriptions: u64 = c.venues.values().map(|v| u64::from(v.max_sub_insts)).sum();
    if subscriptions > u64::from(setup.caps.max_insts) {
        errors.push(Error::config("SubCapacity", "subscription sum exceeds max_insts"));
    }
    if parsed.iter().map(|(_, p)| u64::from(p.groups.len())).sum::<u64>()
        > u64::from(setup.caps.max_cells_total)
    {
        errors.push(Error::config(
            "CellCapacity",
            "group count exceeds max_cells_total",
        ));
    }
    if c.venues
        .values()
        .any(|v| u64::from(v.max_sub_insts) * 2 > c.rings.elements as u64)
    {
        errors.push(Error::config(
            "FeedCtlCapacity",
            "feed control ring requires twice max_sub_insts",
        ));
    }
    if risk.conn_liveness_ms < 2 * hft_venue::FeedCfg::default().bbo_ping_ms {
        errors.push(Error::config(
            "ConnLiveness",
            "connection liveness requires two BBO ping intervals",
        ));
    }
    if !sim {
        for v in c.venues.values() {
            for k in &v.accounts {
                if rd.account(AccountId(k.account)).venues & (1u64 << v.id) == 0 {
                    errors.push(Error::config(
                        "BindingMismatch",
                        "credential account not bound to venue",
                    ));
                }
                if !setup
                    .keys_required
                    .iter()
                    .any(|(a, venue, mask)| a.0 == k.account && venue.0 == v.id && *mask == 3)
                {
                    errors.push(Error::config(
                        "EngineMismatch",
                        "both session and REST identity gates must be required",
                    ));
                }
            }
        }
    }
    let gw_bits = c
        .venues
        .values()
        .filter(|v| !v.accounts.is_empty())
        .fold(0u64, |m, v| m | (1u64 << v.id));
    let ecf_gw_bits = setup.gws.venues.iter().fold(0u64, |m, v| m | (1u64 << v.0));
    if gw_bits != ecf_gw_bits {
        errors.push(Error::config("EngineMismatch", "gateway set differs from ECF1"));
    }
    if setup.feeds.0 != feed_bits {
        errors.push(Error::config("EngineMismatch", "feed venues differ from ECF1"));
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // Exercise the risk owner's consistency check before any journal, key or network I/O.
    let mut bounds = vec![hft_risk::ExecBounds::default(); 256];
    for a in &setup.exec.accounts {
        let p = a.params.unwrap_or(setup.exec.default);
        bounds[usize::from(a.account.0)] = hft_risk::ExecBounds {
            force_max_cross: p.force_slice.max_cross,
            force_max_chunk: p.force_slice.max_chunk_notional,
            dust_notional: p.dust_notional,
        };
    }
    hft_risk::Risk::new(
        &risk,
        &rd,
        &setup.gws,
        &hft_risk::Capacities {
            max_insts: setup.caps.max_insts,
            max_cells_total: setup.caps.max_cells_total,
            max_caps: hft_risk::Capacities::MAX_CAPS,
        },
        &bounds,
        hft_risk::ModeCfg {
            auto_resume_after_clean_drain: setup.seq.auto_resume_after_clean_drain == 1,
            allow_missing_standby: setup.seq.allow_missing_standby == 1,
        },
    )
    .map_err(|e| {
        one(
            if e == hft_risk::RiskParamError::ExecRiskMismatch {
                "ExecRiskMismatch"
            } else {
                "RiskParams"
            },
            format!(
                "{e:?}; accounts {:?}",
                setup
                    .exec
                    .accounts
                    .iter()
                    .map(|a| a.account.0)
                    .collect::<Vec<_>>()
            ),
        )
    })?;
    startup_trace.mark(2, true);
    let Some(assembly) = assembly else {
        return Err(vec![Error::config("EnvCombo", "no run assembly")]);
    };
    Ok(Loaded {
        startup_trace,
        assembly,
        cfg: c,
        rd,
        setup,
        configs: ConfigSet {
            refdata_blob: rd_blob,
            engine_blob: ec_blob,
            risk_sha256: digest(&risk_blob),
            risk_blob,
            risk,
            params: parsed,
        },
        principals: principals.ok_or_else(|| one("CtlPrincipals", "missing".into()))?,
    })
}
