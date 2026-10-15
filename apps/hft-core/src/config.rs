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
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub name: String,
    pub order_env: Option<String>,
    pub config_source: String,
    #[serde(default)]
    pub secrets_mode: SecretsMode,
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
    pub quote_interval_ms: u32,
    pub quotes: Vec<Quote>,
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
    if c.clock.step_threshold_ns == 0 || c.clock.interval_ms == 0 {
        bad!("BadValue", "clock threshold and interval must be positive".into());
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
    if (dry && (c.instance.name != "sim" || c.instance.order_env.is_some()))
        || (!dry && (c.instance.name != "prod" || c.instance.order_env.as_deref() != Some("testnet")))
    {
        bad!(
            "EnvCombo",
            "--dry-run requires sim without order_env; network mode requires prod/testnet".into()
        );
    }
    if dry != c.sim.is_some() {
        bad!(
            "SimConfig",
            "sim section must be present exactly in --dry-run mode".into()
        );
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
    if dry {
        tasks.push("sim-gw".into());
        tasks.push("paper-feed".into());
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
        if dry && !v.accounts.is_empty() {
            errors.push(Error::config("SecretsInSim", name));
        }
        if !dry {
            if v.accounts.len() > 1 {
                errors.push(Error::config(
                    "AccountCapacity",
                    "one live account per venue in this assembly",
                ));
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
        if c.instance.secrets_mode == SecretsMode::LocalDemo {
            let endpoints = rd.endpoints(VenueId(v.id), hft_refdata::EndpointEnv::Testnet);
            if endpoints.is_empty()
                || !rd
                    .endpoints(VenueId(v.id), hft_refdata::EndpointEnv::Prod)
                    .is_empty()
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
    if setup.role_prod == dry || setup.drain_timeout_ms != c.shutdown.drain_timeout_ms {
        errors.push(Error::config(
            "EngineMismatch",
            "role/drain timeout differs from ECF1",
        ));
    }
    if dry && setup.seq.allow_missing_standby != 1 {
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
        if matches!(m.kind, AccountKind::Sim(_)) != dry || m.is_prod {
            errors.push(Error::config("EnvAccountMismatch", format!("account {a}")));
        }
        let rows: Vec<_> = parsed
            .iter()
            .filter(|(_, p)| p.account == m.id && p.strategy == m.bound_instance)
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
                    Some(vc) if !dry && vc.accounts.iter().filter(|k| k.account == a).count() != 1 => errors
                        .push(Error::config(
                            "MissingCredential",
                            format!("account {a}, venue {v}"),
                        )),
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
    if let Some(paper) = &c.sim {
        if paper.quote_interval_ms == 0 || paper.quotes.is_empty() || paper.leverage == 0 {
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
    if !dry {
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
    Ok(Loaded {
        startup_trace,
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
