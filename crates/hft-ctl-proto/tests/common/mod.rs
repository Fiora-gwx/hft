//! Baseline `P0` of crates/hft-ctl-proto/TESTS.md: refdata (`p0`), envelope `E0`, request
//! builders and the 13 principal fixtures.
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]

pub mod p0;

use std::path::PathBuf;
use std::sync::OnceLock;

use hft_ctl_proto::{
    ChatType, CmdBody, CmdRequest, NormErr, Normalized, Principal, RoleSet, normalize, parse_request,
};
use hft_refdata::RefData;
use serde_json::{Value, json};

pub const CMD_ID: &str = "00000000-0000-4000-8000-000000000001";
pub const ISSUED: i64 = 1_790_000_000_000_000_000;
pub const EXPIRES: i64 = 1_790_000_030_000_000_000;

/// P0 refdata, loaded once.
pub fn rd() -> &'static RefData {
    static RD: OnceLock<RefData> = OnceLock::new();
    RD.get_or_init(p0::p0)
}

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// `E0` + `cmd` (a JSON object literal) as a JSON value; `account = None` leaves the key out.
pub fn e0(account: Option<&str>, cmd: Value) -> Value {
    let mut v = json!({
        "v": 2,
        "cmd_id": CMD_ID,
        "issuer": "cli:alice",
        "issued_at_ns": ISSUED,
        "expires_at_ns": EXPIRES,
        "cmd": cmd,
    });
    if let Some(a) = account {
        v["account"] = json!(a);
    }
    v
}

/// Parses a request value (panics on a schema error).
pub fn parse(v: &Value) -> CmdRequest {
    let b = serde_json::to_vec(v).unwrap();
    match parse_request(&b) {
        Ok(r) => r,
        Err(e) => panic!("schema error {e:?} for {v}"),
    }
}

/// `E0` + `cmd` parsed.
pub fn req(account: Option<&str>, cmd: Value) -> CmdRequest {
    parse(&e0(account, cmd))
}

/// `E0` + `cmd` normalized against P0.
pub fn norm(account: Option<&str>, cmd: Value) -> Result<Normalized, NormErr> {
    normalize(&req(account, cmd), rd())
}

/// Normalization succeeds.
pub fn ok(account: Option<&str>, cmd: Value) -> Normalized {
    match norm(account, cmd.clone()) {
        Ok(n) => n,
        Err(e) => panic!("normalize failed {e:?} for {cmd}"),
    }
}

/// Normalization fails; returns the detail code.
pub fn fail(account: Option<&str>, cmd: Value) -> u16 {
    match norm(account, cmd.clone()) {
        Ok(_) => panic!("normalize unexpectedly succeeded for {cmd}"),
        Err(e) => e.detail,
    }
}

/// A command body parsed from its JSON object.
pub fn body(cmd: Value) -> CmdBody {
    req(Some("*"), cmd).cmd
}

/// Little-endian readers over a payload.
pub fn u8_at(b: &[u8], o: usize) -> u8 {
    b[o]
}
pub fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
pub fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
pub fn i64_at(b: &[u8], o: usize) -> i64 {
    u64_at(b, o) as i64
}

/// `CmdScope` at offset `o`: `(strategy, venue, account, group, inst)`.
pub fn scope_at(b: &[u8], o: usize) -> (u8, u8, u8, u32, u32) {
    (b[o], b[o + 1], b[o + 2], u32_at(b, o + 4), u32_at(b, o + 8))
}

/// One principal fixture of the baseline (13 in total).
#[derive(Copy, Clone, Debug)]
pub struct Who {
    pub tag: &'static str,
    pub issuer: &'static str,
    pub roles: RoleSet,
    pub principal: Principal,
    pub chat: ChatType,
}

const fn who(
    tag: &'static str,
    issuer: &'static str,
    roles: RoleSet,
    principal: Principal,
    chat: ChatType,
) -> Who {
    Who {
        tag,
        issuer,
        roles,
        principal,
        chat,
    }
}

pub const WHO: [Who; 13] = [
    who(
        "tgV",
        "tg:1001",
        RoleSet::VIEWER,
        Principal::Tg,
        ChatType::Private,
    ),
    who("tgVg", "tg:1001", RoleSet::VIEWER, Principal::Tg, ChatType::Group),
    who(
        "tgT",
        "tg:1002",
        RoleSet::TRADER,
        Principal::Tg,
        ChatType::Private,
    ),
    who("tgTg", "tg:1002", RoleSet::TRADER, Principal::Tg, ChatType::Group),
    who("tgA", "tg:1003", RoleSet::ADMIN, Principal::Tg, ChatType::Private),
    who("tgAg", "tg:1003", RoleSet::ADMIN, Principal::Tg, ChatType::Group),
    who(
        "cliV",
        "cli:vera",
        RoleSet::VIEWER,
        Principal::Cli,
        ChatType::Private,
    ),
    who(
        "cliT",
        "cli:tom",
        RoleSet::TRADER,
        Principal::Cli,
        ChatType::Private,
    ),
    who(
        "cliA",
        "cli:alice",
        RoleSet::ADMIN,
        Principal::Cli,
        ChatType::Private,
    ),
    who(
        "bot",
        "ops_bot",
        RoleSet::EMPTY,
        Principal::OpsBot,
        ChatType::Private,
    ),
    who(
        "pipe",
        "pipeline",
        RoleSet::EMPTY,
        Principal::Pipeline,
        ChatType::Private,
    ),
    who(
        "recon",
        "reconciler",
        RoleSet::EMPTY,
        Principal::Reconciler,
        ChatType::Private,
    ),
    who(
        "sysd",
        "cli:systemd",
        RoleSet::EMPTY,
        Principal::Systemd,
        ChatType::Private,
    ),
];

/// The 13 baseline principals by tag.
pub fn who_by(tag: &str) -> Who {
    *WHO.iter().find(|w| w.tag == tag).unwrap()
}

/// Valid command bodies (JSON) of every core command variant, with the account each needs.
pub fn core_cmds() -> Vec<(&'static str, Option<&'static str>, Value)> {
    vec![
        (
            "GetSchema",
            None,
            json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
        ),
        ("KillGlobal", None, json!({"type":"Kill","level":"Global"})),
        (
            "KillAccount",
            Some("acct-a"),
            json!({"type":"Kill","level":"Account"}),
        ),
        (
            "KillVenue",
            Some("*"),
            json!({"type":"Kill","level":"Venue","venue":"bitget-spot"}),
        ),
        (
            "SetModeCloseOnly",
            Some("acct-a"),
            json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"CloseOnly"}),
        ),
        (
            "SetModeShadow",
            Some("acct-a"),
            json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Shadow"}),
        ),
        (
            "SetModeLive",
            Some("acct-a"),
            json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
        ),
        ("Resume", Some("acct-a"), json!({"type":"Resume","source":"Kill"})),
        (
            "SetParams",
            Some("acct-a"),
            json!({"type":"SetParams","strategy":"s1-main","blob_b64":spb1_b64(1, 1, 42, 16)}),
        ),
        (
            "SetRisk",
            Some("acct-a"),
            json!({"type":"SetRisk","blob_b64":"AAAAAAAAAAA="}),
        ),
        (
            "ReplaceRefdata",
            Some("acct-a"),
            json!({"type":"ReplaceRefdata","blob_b64":"AAAAAAAAAAA=","version":"8"}),
        ),
        ("CancelAll", Some("acct-a"), json!({"type":"CancelAll"})),
        ("Drain", None, json!({"type":"Drain"})),
        (
            "HedgeFix",
            Some("acct-a"),
            json!({"type":"HedgeFix","group":"16777317"}),
        ),
        (
            "WriteOffResidual",
            Some("acct-a"),
            json!({"type":"WriteOffResidual","group":"16777317"}),
        ),
        (
            "SmokeOrder",
            Some("acct-a"),
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"}),
        ),
        (
            "StrategyCmd",
            Some("acct-a"),
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"16777317","reduce_only":false,"payload_b64":"AQ=="}),
        ),
        (
            "StrategyCmdRo",
            Some("acct-a"),
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"16777317","reduce_only":true,"payload_b64":"AQ=="}),
        ),
        (
            "InjectWarmup",
            None,
            json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","last":false,"payload_b64":"AQ=="}),
        ),
        (
            "AdoptExchange",
            Some("acct-a"),
            json!({"type":"AdoptExchange","venue":"binance-usdm","report_id":"7","item":"Position","inst":"binance-usdm:BTCUSDT"}),
        ),
        (
            "SetBalanceBaseline",
            Some("acct-a"),
            json!({"type":"SetBalanceBaseline","venue":"bitget-spot"}),
        ),
        (
            "VenueNoticeDelisting",
            None,
            json!({"type":"VenueNotice","venue":"binance-spot","kind":"Delisting","at_ns":"1790100000000000000"}),
        ),
        (
            "VenueNoticeFunding",
            Some("*"),
            json!({"type":"VenueNotice","venue":"binance-spot","kind":"FundingInterval","new_s":"14400"}),
        ),
        (
            "VenueNoticeMaint",
            Some("*"),
            json!({"type":"VenueNotice","venue":"binance-spot","kind":"Maintenance","from_ns":"1790100000000000000","to_ns":"1790103600000000000"}),
        ),
        ("SimReset", Some("sim-a"), json!({"type":"SimReset"})),
        (
            "SimTransfer",
            Some("sim-a"),
            json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"BTC","amount":"0.5"}),
        ),
        (
            "SimFault",
            Some("sim-a"),
            json!({"type":"SimFault","venue":"binance-usdm","fault":"RejectNew","delay_ms":"0"}),
        ),
        (
            "RecordTransfer",
            Some("acct-a"),
            json!({"type":"RecordTransfer","from":"binance-usdm","to":"bitget-spot","asset":"USDT","amount":"100","ext_ref":"TX123"}),
        ),
        (
            "AnnotateOpsNote",
            Some("*"),
            json!({"type":"Annotate","kind":"OpsNote","text":"x"}),
        ),
        (
            "AnnotateLocalWrite",
            Some("*"),
            json!({"type":"Annotate","kind":"LocalWrite","text":"x"}),
        ),
        (
            "AnnotateIncident",
            Some("*"),
            json!({"type":"Annotate","kind":"Incident","text":"x"}),
        ),
        (
            "ReconcileReport",
            None,
            json!({"type":"ReconcileReport","report_id":"7","items":[{"item":"Summary"}]}),
        ),
    ]
}

/// SPB1 test blob: `StrategyParamsHdr{strategy, account, version, kind_id 0xFF01}` + one
/// `GroupSpec{id g1}` + `body_len` zero bytes (container-valid).
pub fn spb1(strategy: u8, account: u8, version: u64, body_len: u32) -> Vec<u8> {
    spb1_groups(
        strategy,
        account,
        version,
        body_len,
        &[(u32::from(strategy) << 24) | 101],
    )
}

pub fn spb1_groups(strategy: u8, account: u8, version: u64, body_len: u32, groups: &[u32]) -> Vec<u8> {
    let mut b = Vec::new();
    b.push(strategy);
    b.push(account);
    b.extend_from_slice(&0xFF01u16.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes()); // schema_id
    b.extend_from_slice(&1u16.to_le_bytes()); // schema_ver
    b.push(2); // source Manual
    b.push(0);
    b.extend_from_slice(&(groups.len() as u16).to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&version.to_le_bytes());
    b.extend_from_slice(&0i64.to_le_bytes());
    b.extend_from_slice(&body_len.to_le_bytes());
    b.extend_from_slice(&1000u32.to_le_bytes()); // group_ctr_hwm
    b.extend_from_slice(&0u64.to_le_bytes()); // schema_digest
    assert_eq!(b.len(), 48);
    for g in groups {
        let mut gs = [0u8; 80];
        gs[0..4].copy_from_slice(&g.to_le_bytes());
        gs[4..6].copy_from_slice(&2u16.to_le_bytes()); // asset BTC
        gs[6] = 2;
        gs[7] = 1;
        let leg = |o: usize, inst: u32, venue: u8, gs: &mut [u8; 80]| {
            gs[o..o + 4].copy_from_slice(&inst.to_le_bytes());
            gs[o + 4] = venue;
            gs[o + 5] = 1;
        };
        leg(8, 1, 1, &mut gs);
        leg(24, 4, 11, &mut gs);
        b.extend_from_slice(&gs);
    }
    b.extend(std::iter::repeat_n(0u8, body_len as usize));
    b
}

pub fn b64(b: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(b)
}

pub fn spb1_b64(strategy: u8, account: u8, version: u64, body_len: u32) -> String {
    b64(&spb1(strategy, account, version, body_len))
}

/// Deterministic pseudo-random bytes.
pub fn bytes(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}
