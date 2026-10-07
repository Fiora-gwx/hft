//! Configuration (README "配置文件"), the order-host safety rule (README "安全约束" 1) and the
//! key sources (README "密钥").

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Deserialize;

/// Configuration error (exit code 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CfgErr {
    /// `orders.host` is a mainnet (or non-testnet) host.
    MainnetOrderHost(String),
    /// Key material in the configuration, a missing key source, a readable key file.
    Key(String),
    Invalid(String),
    Io(String),
}

impl std::fmt::Display for CfgErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CfgErr::MainnetOrderHost(h) => write!(
                f,
                "orders.host {h:?} is a mainnet (or non-testnet) host: only demo/testnet order endpoints are allowed"
            ),
            CfgErr::Key(m) => write!(f, "keys: {m}"),
            CfgErr::Invalid(m) => write!(f, "config: {m}"),
            CfgErr::Io(m) => write!(f, "config: {m}"),
        }
    }
}

fn inv(m: impl Into<String>) -> CfgErr {
    CfgErr::Invalid(m.into())
}

/// Product line.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Product {
    Usdm,
    Spot,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuoteSide {
    Buy,
    Sell,
    Alternate,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Idle {
    Spin,
    Park,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RawOpt {
    On,
    Off,
}

// ------------------------------------------------------------------------------------ raw TOML

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawSlice {
    product: Option<Product>,
    minutes: Option<u32>,
    out_dir: Option<PathBuf>,
    epoch: Option<u16>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawFeed {
    host: Option<String>,
    port: Option<u16>,
    ips: Option<Vec<Ipv4Addr>>,
    bbo_conns: Option<u8>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawOrders {
    enabled: Option<bool>,
    host: Option<String>,
    port: Option<u16>,
    ips: Option<Vec<Ipv4Addr>>,
    api_key_env: Option<String>,
    secret_env: Option<String>,
    secret_file: Option<PathBuf>,
    max_open_orders: Option<u16>,
    max_order_notional: Option<String>,
    max_orders: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawNet {
    src_ip: Option<Ipv4Addr>,
    bind_device: Option<String>,
    ts_expect_hw: Option<bool>,
}

#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct RawInst {
    symbol: String,
    base: String,
    tick: String,
    lot: String,
    min_qty: String,
    min_notional: String,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawProbe {
    probe_period_ms: Option<u32>,
    burst_period_ms: Option<u32>,
    far_bp: Option<u32>,
    quote_side: Option<QuoteSide>,
    max_spread_bp: Option<u32>,
    size_margin_pct: Option<u32>,
    quote_rest_ms: Option<u32>,
    max_consec_fail: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawBallast {
    conns: Option<u8>,
    symbols: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawJournal {
    hot_dir: Option<PathBuf>,
    cold_dir: Option<PathBuf>,
    segment_mib: Option<u32>,
    raw: Option<RawOpt>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawClock {
    phc: Option<PathBuf>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawReport {
    burst_gap_ns: Option<u32>,
    cal_max_err_ns: Option<u32>,
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(deny_unknown_fields)]
struct RawThread {
    core: Option<i32>,
    idle: Option<Idle>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawThreads {
    hk: Option<RawThread>,
    persist: Option<RawThread>,
    warm: Option<RawThread>,
    seq: Option<RawThread>,
    #[serde(rename = "feed-bbo")]
    feed_bbo: Option<RawThread>,
    #[serde(rename = "feed-md")]
    feed_md: Option<RawThread>,
    gw: Option<RawThread>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RawCfg {
    #[serde(default)]
    slice: RawSlice,
    #[serde(default)]
    feed: RawFeed,
    #[serde(default)]
    orders: RawOrders,
    #[serde(default)]
    net: RawNet,
    #[serde(default)]
    insts: Vec<RawInst>,
    #[serde(default)]
    probe: RawProbe,
    #[serde(default)]
    ballast: RawBallast,
    #[serde(default)]
    journal: RawJournal,
    #[serde(default)]
    clock: RawClock,
    #[serde(default)]
    report: RawReport,
    #[serde(default)]
    threads: RawThreads,
}

// ------------------------------------------------------------------------------------ typed

/// A decimal value of the exchange filters, kept as text and as `(mantissa, decimals)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dec {
    pub text: String,
    pub mantissa: i64,
    pub decimals: u8,
}

impl Dec {
    pub fn parse(s: &str) -> Option<Dec> {
        let (int, frac) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        if int.is_empty()
            || !int.bytes().all(|c| c.is_ascii_digit())
            || !frac.bytes().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let frac = frac.trim_end_matches('0');
        let decimals = u8::try_from(frac.len()).ok().filter(|d| *d <= 18)?;
        let mantissa: i64 = format!("{int}{frac}").parse().ok()?;
        Some(Dec {
            text: s.to_string(),
            mantissa,
            decimals,
        })
    }

    /// The value as an integer at exponent `exp` (`None` when not exact or out of range).
    pub fn raw(&self, exp: i8) -> Option<i64> {
        let shift = -i32::from(exp) - i32::from(self.decimals);
        if shift < 0 {
            return None;
        }
        self.mantissa
            .checked_mul(10i64.checked_pow(u32::try_from(shift).ok()?)?)
    }
}

/// One probed inst.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstCfg {
    pub symbol: String,
    pub base: String,
    pub tick: Dec,
    pub lot: Dec,
    pub min_qty: Dec,
    pub min_notional: Dec,
}

/// One thread's placement.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ThreadCfg {
    /// `None` = not pinned.
    pub core: Option<u16>,
    pub idle: Idle,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Threads {
    pub hk: ThreadCfg,
    pub persist: ThreadCfg,
    pub warm: ThreadCfg,
    pub seq: ThreadCfg,
    pub feed_bbo: ThreadCfg,
    pub feed_md: ThreadCfg,
    pub gw: ThreadCfg,
}

/// The loaded configuration (README "配置文件"; defaults applied).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub product: Product,
    pub minutes: u32,
    pub out_dir: PathBuf,
    pub epoch: u16,
    pub feed_host: String,
    pub feed_port: u16,
    pub feed_ips: Vec<Ipv4Addr>,
    pub bbo_conns: u8,
    pub orders_enabled: bool,
    /// Empty only when orders are disabled.
    pub orders_host: String,
    pub orders_port: u16,
    pub orders_ips: Vec<Ipv4Addr>,
    pub api_key_env: String,
    pub secret_env: Option<String>,
    pub secret_file: Option<PathBuf>,
    pub max_open_orders: u16,
    pub max_order_notional: Dec,
    pub max_orders: u32,
    pub src_ip: Option<Ipv4Addr>,
    pub bind_device: Option<String>,
    pub ts_expect_hw: bool,
    pub insts: Vec<InstCfg>,
    pub probe_period_ms: u32,
    pub burst_period_ms: u32,
    pub far_bp: u32,
    pub quote_side: QuoteSide,
    pub max_spread_bp: u32,
    pub size_margin_pct: u32,
    pub quote_rest_ms: u32,
    pub max_consec_fail: u32,
    pub ballast_conns: u8,
    pub ballast_symbols: Vec<String>,
    pub hot_dir: PathBuf,
    pub cold_dir: PathBuf,
    pub segment_mib: u32,
    pub raw: RawOpt,
    pub phc: Option<PathBuf>,
    pub burst_gap_ns: u32,
    pub cal_max_err_ns: u32,
    pub threads: Threads,
}

/// Keys that would put key material into the configuration (README "密钥").
const KEY_KEYS: &[&str] = &[
    "api_key",
    "apikey",
    "secret",
    "secret_key",
    "api_secret",
    "private_key",
    "key",
    "passphrase",
];

fn find_key_material(v: &toml::Value, path: &str) -> Option<String> {
    let t = v.as_table()?;
    for (k, x) in t {
        let p = if path.is_empty() {
            k.clone()
        } else {
            format!("{path}.{k}")
        };
        if KEY_KEYS.contains(&k.to_ascii_lowercase().as_str()) {
            return Some(p);
        }
        if let Some(found) = find_key_material(x, &p) {
            return Some(found);
        }
        if let Some(arr) = x.as_array() {
            for e in arr {
                if let Some(found) = find_key_material(e, &p) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// Mainnet hosts that never take orders from this program (README "安全约束" 1).
const MAINNET: &[&str] = &[
    "ws-fapi.binance.com",
    "ws-api.binance.com",
    "fapi.binance.com",
    "api.binance.com",
    "api1.binance.com",
    "api2.binance.com",
    "api3.binance.com",
    "api4.binance.com",
    "stream.binance.com",
    "fstream.binance.com",
];

/// README "安全约束" 1: only demo/testnet order hosts.
pub fn check_order_host(host: &str) -> Result<(), CfgErr> {
    let h = host.trim().to_ascii_lowercase();
    let testnet = h.contains("demo") || h.contains("testnet");
    if MAINNET.contains(&h.as_str()) || (h.ends_with(".binance.com") && !testnet) || !testnet {
        return Err(CfgErr::MainnetOrderHost(host.to_string()));
    }
    Ok(())
}

fn thread(r: Option<RawThread>, core: u16, idle: Idle) -> ThreadCfg {
    let r = r.unwrap_or_default();
    ThreadCfg {
        core: match r.core {
            None => Some(core),
            Some(c) if c < 0 => None,
            Some(c) => u16::try_from(c).ok(),
        },
        idle: r.idle.unwrap_or(idle),
    }
}

fn dec(field: &str, s: &str) -> Result<Dec, CfgErr> {
    Dec::parse(s).ok_or_else(|| inv(format!("{field}: not a decimal: {s:?}")))
}

/// Parses and validates the configuration text.
pub fn parse(text: &str) -> Result<Config, CfgErr> {
    let v: toml::Value = toml::from_str(text).map_err(|e| inv(e.to_string()))?;
    if let Some(p) = find_key_material(&v, "") {
        return Err(CfgErr::Key(format!(
            "`{p}` puts key material into the configuration; keys come only from environment variables or a local file"
        )));
    }
    let r: RawCfg = v.try_into().map_err(|e: toml::de::Error| inv(e.to_string()))?;
    let product = r.slice.product.ok_or_else(|| inv("slice.product is required"))?;
    let orders_enabled = r.orders.enabled.unwrap_or(true);
    let orders_host = match (r.orders.host, product) {
        (Some(h), _) => h,
        (None, Product::Spot) => "demo-ws-api.binance.com".to_string(),
        (None, Product::Usdm) if !orders_enabled => String::new(),
        (None, Product::Usdm) => {
            return Err(inv(
                "orders.host is required for usdm (the demo ws-fapi endpoint is unverified: venue-binance 开发备忘 14)",
            ));
        }
    };
    if orders_enabled {
        check_order_host(&orders_host)?;
    }
    if r.insts.is_empty() {
        return Err(inv("at least one [[insts]] is required"));
    }
    let mut insts = Vec::with_capacity(r.insts.len());
    for i in r.insts {
        insts.push(InstCfg {
            tick: dec("tick", &i.tick)?,
            lot: dec("lot", &i.lot)?,
            min_qty: dec("min_qty", &i.min_qty)?,
            min_notional: dec("min_notional", &i.min_notional)?,
            symbol: i.symbol,
            base: i.base,
        });
    }
    let probe_period_ms = r.probe.probe_period_ms.unwrap_or(1_000);
    let burst_period_ms = r.probe.burst_period_ms.unwrap_or(0);
    if !(100..=600_000).contains(&probe_period_ms) {
        return Err(inv("probe.probe_period_ms must be in 100..=600000"));
    }
    if burst_period_ms > probe_period_ms {
        return Err(inv("probe.burst_period_ms must be 0 or <= probe.probe_period_ms"));
    }
    let far_bp = r.probe.far_bp.unwrap_or(300);
    if !(50..=1_000).contains(&far_bp) {
        return Err(inv("probe.far_bp must be in 50..=1000"));
    }
    let quote_side = r.probe.quote_side.unwrap_or(QuoteSide::Buy);
    if product == Product::Spot && quote_side != QuoteSide::Buy {
        return Err(inv("probe.quote_side: spot quotes buy only"));
    }
    let max_open_orders = r.orders.max_open_orders.unwrap_or(2);
    if max_open_orders == 0 || max_open_orders > 64 {
        return Err(inv("orders.max_open_orders must be in 1..=64"));
    }
    let max_order_notional = dec(
        "orders.max_order_notional",
        r.orders.max_order_notional.as_deref().unwrap_or("50"),
    )?;
    let segment_mib = r.journal.segment_mib.unwrap_or(64);
    if segment_mib == 0 {
        return Err(inv("journal.segment_mib must be > 0"));
    }
    let out_dir = r.slice.out_dir.unwrap_or_else(|| PathBuf::from("./slice-out"));
    let (feed_host, feed_port) = match product {
        Product::Usdm => ("fstream.binance.com", 443),
        Product::Spot => ("stream.binance.com", 9443),
    };
    if r.orders.secret_env.is_some() && r.orders.secret_file.is_some() {
        return Err(CfgErr::Key(
            "give exactly one of orders.secret_env and orders.secret_file".into(),
        ));
    }
    let secret_env = match (&r.orders.secret_env, &r.orders.secret_file) {
        (Some(e), _) => Some(e.clone()),
        (None, None) => Some("HFT_SLICE_ED25519".to_string()),
        (None, Some(_)) => None,
    };
    let t = r.threads;
    Ok(Config {
        product,
        minutes: r.slice.minutes.unwrap_or(60),
        cold_dir: r.journal.cold_dir.unwrap_or_else(|| out_dir.join("journal")),
        out_dir,
        epoch: r.slice.epoch.unwrap_or(0) & 0x7FFF,
        feed_host: r.feed.host.unwrap_or_else(|| feed_host.to_string()),
        feed_port: r.feed.port.unwrap_or(feed_port),
        feed_ips: r.feed.ips.unwrap_or_default(),
        bbo_conns: r.feed.bbo_conns.unwrap_or(2),
        orders_enabled,
        orders_host,
        orders_port: r.orders.port.unwrap_or(443),
        orders_ips: r.orders.ips.unwrap_or_default(),
        api_key_env: r
            .orders
            .api_key_env
            .unwrap_or_else(|| "HFT_SLICE_API_KEY".to_string()),
        secret_env,
        secret_file: r.orders.secret_file,
        max_open_orders,
        max_order_notional,
        max_orders: r.orders.max_orders.unwrap_or(20_000),
        src_ip: r.net.src_ip,
        bind_device: r.net.bind_device,
        ts_expect_hw: r.net.ts_expect_hw.unwrap_or(false),
        insts,
        probe_period_ms,
        burst_period_ms,
        far_bp,
        quote_side,
        max_spread_bp: r.probe.max_spread_bp.unwrap_or(100),
        size_margin_pct: r.probe.size_margin_pct.unwrap_or(10),
        quote_rest_ms: r.probe.quote_rest_ms.unwrap_or(2_000).max(1),
        max_consec_fail: r.probe.max_consec_fail.unwrap_or(5).max(1),
        ballast_conns: r.ballast.conns.unwrap_or(0),
        ballast_symbols: r.ballast.symbols.unwrap_or_default(),
        hot_dir: r
            .journal
            .hot_dir
            .unwrap_or_else(|| PathBuf::from("/dev/shm/hft-slice")),
        segment_mib,
        raw: r.journal.raw.unwrap_or(RawOpt::On),
        phc: r.clock.phc,
        burst_gap_ns: r.report.burst_gap_ns.unwrap_or(20_000),
        cal_max_err_ns: r.report.cal_max_err_ns.unwrap_or(1_000),
        threads: Threads {
            hk: thread(t.hk, 0, Idle::Park),
            persist: thread(t.persist, 2, Idle::Park),
            warm: thread(t.warm, 3, Idle::Park),
            seq: thread(t.seq, 4, Idle::Spin),
            feed_bbo: thread(t.feed_bbo, 5, Idle::Spin),
            feed_md: thread(t.feed_md, 7, Idle::Spin),
            gw: thread(t.gw, 8, Idle::Spin),
        },
    })
}

/// Reads and parses a configuration file.
pub fn load(path: &Path) -> Result<Config, CfgErr> {
    let text = std::fs::read_to_string(path).map_err(|e| CfgErr::Io(format!("{}: {e}", path.display())))?;
    parse(&text)
}

// ------------------------------------------------------------------------------------ keys

/// The order credentials (README "密钥"); `Debug` never prints them.
#[derive(Clone, PartialEq, Eq)]
pub struct Keys {
    pub api_key: String,
    pub seed: [u8; 32],
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Keys(<redacted>)")
    }
}

/// PKCS#8 v1 prefix of an Ed25519 private key (RFC 8410); the 32-byte seed follows.
const PKCS8_ED25519: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// The seed of a PKCS#8 PEM Ed25519 private key.
pub fn pem_seed(pem: &str) -> Result<[u8; 32], CfgErr> {
    let begin = "-----BEGIN PRIVATE KEY-----";
    let end = "-----END PRIVATE KEY-----";
    let (Some(a), Some(b)) = (pem.find(begin), pem.find(end)) else {
        return Err(CfgErr::Key("not a PKCS#8 private key PEM".into()));
    };
    let body: String = pem[a + begin.len()..b]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(body.as_bytes())
        .map_err(|_| CfgErr::Key("PEM body is not base64".into()))?;
    if der.len() != 48 || der[..16] != PKCS8_ED25519 {
        return Err(CfgErr::Key("PEM is not an Ed25519 PKCS#8 key".into()));
    }
    let mut s = [0u8; 32];
    s.copy_from_slice(&der[16..]);
    Ok(s)
}

fn hex_seed(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

/// A seed from text: a PKCS#8 PEM or 64 hex digits.
fn seed_text(t: &str) -> Result<[u8; 32], CfgErr> {
    if t.contains("-----BEGIN") {
        return pem_seed(t);
    }
    hex_seed(t).ok_or_else(|| CfgErr::Key("Ed25519 key is neither PKCS#8 PEM nor 64 hex digits".into()))
}

#[cfg(unix)]
fn file_private(p: &Path) -> Result<(), CfgErr> {
    use std::os::unix::fs::PermissionsExt;
    let m = std::fs::metadata(p).map_err(|e| CfgErr::Key(format!("{}: {e}", p.display())))?;
    if m.permissions().mode() & 0o077 != 0 {
        return Err(CfgErr::Key(format!(
            "{} is readable by group or others (chmod 600)",
            p.display()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn file_private(_p: &Path) -> Result<(), CfgErr> {
    Ok(())
}

/// Reads the order credentials (`None` when orders are disabled).
pub fn load_keys(cfg: &Config, env: &dyn Fn(&str) -> Option<String>) -> Result<Option<Keys>, CfgErr> {
    if !cfg.orders_enabled {
        return Ok(None);
    }
    let api_key = env(&cfg.api_key_env)
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| CfgErr::Key(format!("environment variable {} is not set", cfg.api_key_env)))?;
    let seed = match (&cfg.secret_env, &cfg.secret_file) {
        (Some(e), None) => {
            let t = env(e).ok_or_else(|| CfgErr::Key(format!("environment variable {e} is not set")))?;
            seed_text(&t)?
        }
        (None, Some(f)) => {
            file_private(f)?;
            let t = std::fs::read_to_string(f).map_err(|e| CfgErr::Key(format!("{}: {e}", f.display())))?;
            seed_text(&t)?
        }
        _ => {
            return Err(CfgErr::Key(
                "give exactly one of orders.secret_env and orders.secret_file".into(),
            ));
        }
    };
    Ok(Some(Keys {
        api_key: api_key.trim().to_string(),
        seed,
    }))
}
