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
    pub threads: Vec<Thread>,
    pub venues: BTreeMap<String, Venue>,
    pub rings: Rings,
    pub sim: Option<Paper>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub name: String,
    pub order_env: Option<String>,
    pub config_source: String,
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
    pub segment_bytes: u64,
    pub hot_keep: u8,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thread {
    pub name: String,
    pub task: String,
    pub core: Option<u16>,
    pub park_us: u64,
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
    pub src_ip: Option<std::net::Ipv4Addr>,
    pub bind_device: Option<String>,
    #[serde(default)]
    pub accounts: Vec<Keys>,
}
// Never contains secret values. Contents are read only when constructing a real session.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    pub account: u8,
    pub api_key_path: PathBuf,
    pub session_seed_path: PathBuf,
    pub rest_api_key_path: PathBuf,
    pub rest_secret_path: PathBuf,
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
    if p.is_relative() {
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
pub fn load(path: &Path, dry: bool) -> Result<Loaded, Vec<Error>> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| vec![Error::config("FileRead", path.display().to_string())])?;
    // Do not echo TOML source: an accidentally inline key must not be printed on parse failure.
    let mut c: Config = toml::from_str(&text).map_err(|_| {
        vec![Error::config(
            "ConfigParse",
            "invalid TOML, unknown field, or missing required field",
        )]
    })?;
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
        }
    }
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
        if v.max_sub_insts == 0 {
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
            }
            if v.src_ip.is_none() || v.bind_device.as_ref().is_none_or(|s| s.is_empty()) {
                errors.push(Error::config(
                    "MissingField",
                    format!("venues.{name}.src_ip/bind_device"),
                ));
            }
            for k in &v.accounts {
                if k.api_key_path.as_os_str().is_empty()
                    || k.session_seed_path.as_os_str().is_empty()
                    || k.rest_secret_path.as_os_str().is_empty()
                {
                    errors.push(Error::config("MissingCredential", name));
                }
            }
        }
    }
    let mut names = Vec::new();
    for t in &c.threads {
        if names.contains(&t.name)
            || t.name.is_empty()
            || t.core.is_some_and(|n| n >= 256)
            || !(1..=10000).contains(&t.park_us)
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
    let rd_blob = rd_blob.ok_or_else(|| one("FileRead", "refdata".into()))?;
    let ec_blob = ec_blob.ok_or_else(|| one("FileRead", "engine".into()))?;
    let risk_blob = risk_blob.ok_or_else(|| one("FileRead", "risk".into()))?;
    let rd = RefData::load(&rd_blob, None).map_err(|e| one("Refdata", format!("{e:?}")))?;
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
    if setup.role_prod == dry || setup.drain_timeout_ms != c.shutdown.drain_timeout_ms {
        errors.push(Error::config(
            "EngineMismatch",
            "role/drain timeout differs from ECF1",
        ));
    }
    let risk = hft_risk::RiskParams::decode_blob(&risk_blob, &rd)
        .map_err(|e| one("RiskParams", format!("{e:?}")))?;
    let mut parsed = Vec::new();
    for b in params {
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
    Ok(Loaded {
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
