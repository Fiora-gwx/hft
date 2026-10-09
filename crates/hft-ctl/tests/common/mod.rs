//! Baseline `G0` of crates/hft-ctl/TESTS.md: the gateway core over an in-memory ctl ring, a fake
//! sequencer (`FakeSeq`), P0 refdata and the hand-written test strategy set.
#![allow(
    dead_code,
    unused_imports,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]

#[path = "../../../hft-ctl-proto/tests/common/p0.rs"]
pub mod p0;
pub mod set;
#[cfg(target_os = "linux")]
pub mod sock;

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use hft_ctl::{
    Core, CtlRing, GatewayConfig, ParsedConfig, ParsedParams, PrincipalTable, Reply, UidKind, VecRing,
};
use hft_ctl_proto::{CmdResponse, EngineInfo, Role, RoleSet, Uuid};
use hft_event::{
    CmdHdr, CmdResult, ControlCmd, EventKind, GroupSpec, LegDef, Note, ParamSource, RecordFlags,
    StrategyParamsHdr,
};
use hft_refdata::RefData;
use hft_rt::{MailReceiver, MailSender, Mailbox, TscUtc};
use hft_strategy_sdk::{GroupTable, ResetPreview, StrategySet};
use hft_types::{AccountId, AssetId, GroupId, InstId, StrategyId, Tsc, VenueId};
use serde_json::{Value, json};
use zerocopy::{IntoBytes, TryFromBytes};

pub use set::{CtlTestSet, CtlTestSetMin, F_BAD, F_CKPT, F_MEM, F_THR, F_WIN, P};

pub const UTC0: i64 = 1_790_000_000_000_000_000;
pub const MS: u64 = 1_000_000;
pub const S: u64 = 1_000_000_000;
pub const G1: u32 = (1 << 24) | 101;
pub const G2: u32 = (2 << 24) | 101;
pub const G9: u32 = (3 << 24) | 101;

/// 1 GHz test clock, tsc 0 = `UTC0`.
pub fn clock() -> TscUtc {
    TscUtc {
        base_tsc: Tsc(0),
        base_utc_ns: UTC0,
        mult: 1,
        shift: 0,
        err_ns: 0,
    }
}

pub fn principals() -> PrincipalTable {
    let r = RoleSet::from_roles;
    PrincipalTable::from_parts(
        vec![
            (1100, UidKind::OpsBot),
            (1101, UidKind::Pipeline),
            (1102, UidKind::Reconciler),
            (
                1200,
                UidKind::Cli {
                    login: "alice".into(),
                },
            ),
            (1201, UidKind::Cli { login: "tom".into() }),
            (1202, UidKind::Cli { login: "vera".into() }),
        ],
        vec![
            ("tg:1001".into(), r(&[Role::Viewer])),
            ("tg:1002".into(), r(&[Role::Trader])),
            ("tg:1003".into(), r(&[Role::Admin])),
            ("tg:1004".into(), r(&[Role::Viewer, Role::Trader])),
            ("tg:1005".into(), r(&[Role::Viewer])),
            ("cli:alice".into(), r(&[Role::Admin])),
            ("cli:tom".into(), r(&[Role::Trader])),
            ("cli:vera".into(), r(&[Role::Viewer])),
        ],
    )
}

/// The uid an issuer connects with in G0.
pub fn uid_of(issuer: &str) -> u32 {
    match issuer {
        i if i.starts_with("tg:") || i == "ops_bot" => 1100,
        "pipeline" => 1101,
        "reconciler" => 1102,
        "cli:alice" => 1200,
        "cli:tom" => 1201,
        "cli:vera" => 1202,
        _ => 9999,
    }
}

pub fn config(socket: PathBuf) -> GatewayConfig {
    GatewayConfig::new(socket, principals(), 256, 1024, 8)
}

thread_local! {
    /// `refdata_check` result of the current test thread (0 = pass).
    pub static ASSEMBLY_FAIL: Cell<u16> = const { Cell::new(0) };
}

pub fn refdata_check(_: &[u8], _: &RefData) -> Result<(), u16> {
    match ASSEMBLY_FAIL.with(Cell::get) {
        0 => Ok(()),
        n => Err(n),
    }
}

/// Test `RiskParams` encoding: `b"RSK1"`, then a flag byte (1 = changes another account's
/// fields, 2 = changes global fields), then anything.
pub fn risk_blob(flags: u8) -> Vec<u8> {
    let mut b = b"RSK1".to_vec();
    b.push(flags);
    b.extend(std::iter::repeat_n(7u8, 2_043));
    b
}

pub fn risk_decode(b: &[u8], a: AccountId, rd: &RefData) -> Result<hft_risk::RiskParams, u16> {
    use hft_ctl_proto::codes::detail;
    if b.get(..4) != Some(b"RSK1") {
        return Err(detail::BAD_VALUE);
    }
    let f = b.get(4).copied().unwrap_or(0);
    if f & 1 != 0 || (f & 2 != 0 && a.0 != 0) {
        return Err(detail::OUT_OF_SCOPE);
    }
    Ok(hft_risk::ParamsBuilder::new(rd).build())
}

/// One test leg.
pub fn leg(inst: u32, venue: u8) -> LegDef {
    LegDef {
        inst: InstId(inst),
        venue: VenueId(venue),
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }
}

pub fn group(id: u32, legs: &[LegDef]) -> GroupSpec {
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut l = [none; 4];
    l[..legs.len()].copy_from_slice(legs);
    GroupSpec {
        id: GroupId(id),
        asset: AssetId(2),
        n_legs: legs.len() as u8,
        open_allowed: 1,
        legs: l,
        _rsv: 0,
    }
}

/// `g1`-style group of `s1-main` (`binance-usdm:BTCUSDT` + `bitget-spot:BTCUSDT`).
pub fn g_s1(id: u32) -> GroupSpec {
    group(id, &[leg(1, 1), leg(4, 11)])
}

/// An SPB1 blob of a test kind.
#[derive(Clone, Debug)]
pub struct Spb {
    pub strategy: u8,
    pub account: u8,
    pub kind_id: u16,
    pub schema_ver: u16,
    pub version: u64,
    pub digest: u64,
    pub groups: Vec<GroupSpec>,
    pub values: [i64; 5],
}

impl Spb {
    pub fn s1(version: u64) -> Spb {
        Spb {
            strategy: 1,
            account: 1,
            kind_id: 0xFF01,
            schema_ver: 1,
            version,
            digest: set::digest(0xFF01),
            groups: vec![g_s1(G1)],
            values: [10, 5, 0, 1_000, 1_000],
        }
    }
    pub fn bytes(&self) -> Vec<u8> {
        let body = set::body(self.values, self.groups.len() as u16);
        let hwm = self
            .groups
            .iter()
            .map(|g| g.id.counter())
            .max()
            .unwrap_or(0)
            .max(1000);
        let h = StrategyParamsHdr {
            strategy: StrategyId(self.strategy),
            account: AccountId(self.account),
            kind_id: self.kind_id,
            schema_id: u32::from(self.kind_id),
            schema_ver: self.schema_ver,
            source: ParamSource::Manual,
            _pad: 0,
            n_groups: self.groups.len() as u16,
            _pad2: 0,
            version: self.version,
            generated_at_ns: 0,
            body_len: body.len() as u32,
            group_ctr_hwm: hwm,
            schema_digest: self.digest,
        };
        let mut b = h.as_bytes().to_vec();
        for g in &self.groups {
            b.extend_from_slice(g.as_bytes());
        }
        b.extend_from_slice(&body);
        b
    }
}

pub fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

/// The decoded starting params of `(strategy, account)` (the `active_params` of G0).
pub fn active(spb: &Spb) -> ParsedConfig<CtlTestSet> {
    let groups = GroupTable::new(&spb.groups, 8);
    let body = set::body(spb.values, spb.groups.len() as u16);
    let params = CtlTestSet::decode_params(spb.kind_id, &body, 1, &groups).unwrap();
    ParsedConfig::StrategyParams(Box::new(ParsedParams {
        sha256: [0; 32],
        strategy: StrategyId(spb.strategy),
        account: AccountId(spb.account),
        version: spb.version,
        kind_id: spb.kind_id,
        params,
        groups,
        model_mem: 0,
        model_ckpt: 0,
        structural_changed: false,
        reset_preview: ResetPreview::default(),
    }))
}

pub fn g0_active() -> Vec<ParsedConfig<CtlTestSet>> {
    let mut s2 = Spb::s1(7);
    s2.strategy = 2;
    s2.account = 2;
    s2.kind_id = 0xFF02;
    s2.groups = vec![group(G2, &[leg(2, 2), leg(3, 10)])];
    let mut s9 = Spb::s1(3);
    s9.strategy = 3;
    s9.account = 3;
    s9.groups = vec![group(G9, &[leg(1, 1), leg(4, 11)])];
    vec![active(&Spb::s1(41)), active(&s2), active(&s9)]
}

static CMD_SEQ: AtomicU64 = AtomicU64::new(1);

pub fn new_id() -> Uuid {
    let n = CMD_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut b = [0u8; 16];
    b[8..].copy_from_slice(&n.to_be_bytes());
    b[0] = 0x5b;
    b[6] = 0x40;
    b[8] |= 0x80;
    Uuid::from_bytes(b)
}

/// A decoded ring record.
#[derive(Clone, Debug)]
pub struct RingRec {
    pub kind: EventKind,
    pub flags: RecordFlags,
    pub body: Vec<u8>,
}

impl RingRec {
    pub fn is_cmd(&self) -> bool {
        self.kind == EventKind::CONTROL_CMD
    }
    pub fn is_note(&self) -> bool {
        self.kind == EventKind::NOTE
    }
    /// `ControlCmd` logical body (after the 16 B `PartHdr`).
    pub fn logical(&self) -> &[u8] {
        &self.body[16..]
    }
    pub fn cmd(&self) -> (CmdHdr, ControlCmd<'_>) {
        ControlCmd::decode(self.logical()).unwrap()
    }
    pub fn note(&self) -> Note {
        Note::try_read_from_bytes(&self.body).unwrap()
    }
}

/// Audit Note fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditV {
    pub cmd_id: [u8; 16],
    pub cmd_kind: u16,
    pub result_code: u8,
    pub role: u8,
    pub view_account: u8,
    pub chat_type: u8,
    pub target_account: u8,
    pub issuer: [u8; 8],
    pub cmd_hash: [u8; 24],
    pub peer_uid: u32,
}

pub fn audit_of(n: &Note) -> AuditV {
    let v = n.v;
    let mut cmd_id = [0u8; 16];
    cmd_id[..8].copy_from_slice(&v[0].to_le_bytes());
    cmd_id[8..].copy_from_slice(&v[1].to_le_bytes());
    let mut h = [0u8; 24];
    h[..8].copy_from_slice(&v[4].to_le_bytes());
    h[8..16].copy_from_slice(&v[5].to_le_bytes());
    h[16..].copy_from_slice(&v[6].to_le_bytes());
    AuditV {
        cmd_id,
        cmd_kind: v[2] as u16,
        result_code: (v[2] >> 16) as u8,
        role: (v[2] >> 24) as u8,
        view_account: (v[2] >> 32) as u8,
        chat_type: (v[2] >> 40) as u8,
        target_account: (v[2] >> 48) as u8,
        issuer: v[3].to_le_bytes(),
        cmd_hash: h,
        peer_uid: v[7] as u32,
    }
}

/// Script of the fake sequencer for one command.
#[derive(Copy, Clone, Debug)]
pub struct Outcome {
    pub status: u8,
    pub detail: u32,
    pub rflags: u32,
    pub ctl_state_version: u64,
}

impl Outcome {
    pub const APPLIED: Outcome = Outcome {
        status: 0,
        detail: 0,
        rflags: 0,
        ctl_state_version: 21,
    };
}

/// G0 harness: `Core` + in-memory ring + fake sequencer.
pub struct G0<St: StrategySet = CtlTestSet> {
    pub core: Core<St, VecRing>,
    pub now: u64,
    /// Sequencer end of the `parsed` Mailbox.
    pub mb_take: MailReceiver<ParsedConfig<St>>,
    /// Sequencer end of the `retired` Mailbox.
    pub retired_send: MailSender<ParsedConfig<St>>,
    /// Records the fake sequencer has consumed, in order.
    pub seen: Vec<RingRec>,
    /// `ControlCmd`s taken and not yet answered: (cmd_id, seq).
    pub unanswered: Vec<([u8; 16], u64)>,
    /// Duplicate table of the fake sequencer.
    pub seq_seen_ids: Vec<[u8; 16]>,
    pub next_seq: u64,
    pub paused: bool,
    pub script: Vec<(u64, Outcome)>,
    /// Responses delivered through `on_result`/`on_tick`: (conn, response).
    pub delivered: Vec<(u64, CmdResponse)>,
    pub next_conn: u64,
}

fn leak<T: Send + 'static>() -> &'static Mailbox<T> {
    Box::leak(Box::new(Mailbox::new()))
}

impl G0<CtlTestSet> {
    pub fn new() -> G0<CtlTestSet> {
        G0::with(p0::p0(), 262_144, g0_active())
    }
    /// G0 with `max_groups_per_instance = 4096` (for blobs of ~180 000 bytes).
    pub fn big() -> G0<CtlTestSet> {
        let mut cfg = config(std::env::temp_dir().join("ctl-g0-big.sock"));
        cfg.max_groups_per_instance = 4096;
        G0::with_cfg(p0::p0(), 262_144, g0_active(), cfg)
    }
}

/// The CTL-082 blob: version 42, legal, ≥ 180 000 bytes (2 200+ groups of `s1-main`).
pub fn big_spb() -> Spb {
    let mut s = Spb::s1(42);
    let mut n = 2u32;
    while s.bytes().len() < 180_000 {
        s.groups.push(g_s1((1 << 24) | (1000 + n)));
        n += 1;
    }
    s
}

impl<St: StrategySet> G0<St> {
    pub fn with(rd: RefData, ring_free: usize, active: Vec<ParsedConfig<St>>) -> G0<St> {
        G0::with_cfg(
            rd,
            ring_free,
            active,
            config(std::env::temp_dir().join("ctl-g0.sock")),
        )
    }

    pub fn with_cfg(
        rd: RefData,
        ring_free: usize,
        active: Vec<ParsedConfig<St>>,
        cfg: GatewayConfig,
    ) -> G0<St> {
        let (p_send, p_take) = leak::<ParsedConfig<St>>().split();
        let (r_send, r_take) = leak::<ParsedConfig<St>>().split();
        let core = Core::new(
            cfg,
            rd,
            VecRing::with_free(ring_free),
            p_send,
            r_take,
            refdata_check,
            risk_decode,
            active,
            clock(),
            EngineInfo {
                version: "0.4.0".into(),
                epoch: 17,
                refdata: "sha256:00".into(),
                params: Default::default(),
                risk: 7,
            },
        );
        G0 {
            core,
            now: 0,
            mb_take: p_take,
            retired_send: r_send,
            seen: Vec::new(),
            unanswered: Vec::new(),
            seq_seen_ids: Vec::new(),
            next_seq: 900,
            paused: false,
            script: Vec::new(),
            delivered: Vec::new(),
            next_conn: 1,
        }
    }

    pub fn utc(&self) -> i64 {
        UTC0 + self.now as i64
    }

    /// A request in G0 conventions: new cmd_id, issued now, expires +30 s; `tg:*` private.
    pub fn req(&self, issuer: &str, account: Option<&str>, cmd: Value) -> Value {
        let mut v = json!({
            "v": 2,
            "cmd_id": new_id().hyphenated().to_string(),
            "issuer": issuer,
            "issued_at_ns": self.utc(),
            "expires_at_ns": self.utc() + 30 * S as i64,
            "cmd": cmd,
        });
        if let Some(a) = account {
            v["account"] = json!(a);
        }
        if issuer.starts_with("tg:") {
            v["chat_type"] = json!("private");
        }
        v
    }

    /// Sends a request value from `uid` at `self.now`; returns the immediate reply, or `None`
    /// when it waits for a `CmdResult` (then `conn` is in `self.next_conn - 1`).
    pub fn send_raw(&mut self, uid: u32, body: &[u8]) -> Option<CmdResponse> {
        let conn = self.next_conn;
        self.next_conn += 1;
        if let Some(r) = self.core.check_uid(uid) {
            return Some(r);
        }
        match self.core.on_request(conn, uid, body, Tsc(self.now)) {
            Reply::Now(r) => Some(*r),
            Reply::Wait => None,
        }
    }

    pub fn send_v(&mut self, v: &Value) -> Option<CmdResponse> {
        let uid = uid_of(v["issuer"].as_str().unwrap());
        self.send_raw(uid, &serde_json::to_vec(v).unwrap())
    }

    /// Sends and, when written, lets the fake sequencer answer (unless paused); returns the
    /// response the client sees.
    pub fn send(&mut self, issuer: &str, account: Option<&str>, cmd: Value) -> CmdResponse {
        let v = self.req(issuer, account, cmd);
        self.send_and_wait(&v)
    }

    pub fn send_and_wait(&mut self, v: &Value) -> CmdResponse {
        match self.send_v(v) {
            Some(r) => r,
            None => {
                let conn = self.next_conn - 1;
                self.step();
                self.delivered
                    .iter()
                    .rev()
                    .find(|(c, _)| *c == conn)
                    .map(|(_, r)| r.clone())
                    .unwrap_or_else(|| {
                        assert!(self.paused, "no response for conn {conn}");
                        // Written and waiting: what the client would see so far.
                        CmdResponse::new(
                            Uuid::nil(),
                            hft_ctl_proto::TransportStatus::Accepted,
                            None,
                            EngineInfo::default(),
                        )
                    })
            }
        }
    }

    /// Takes everything written to the ring since the last call.
    pub fn take_ring(&mut self) -> Vec<RingRec> {
        let out: Vec<RingRec> = self
            .core
            .ring_mut()
            .out
            .drain(..)
            .map(|(kind, flags, body)| RingRec { kind, flags, body })
            .collect();
        self.seen.extend(out.iter().cloned());
        out
    }

    /// Fake sequencer round: consume the ring, answer every command (unless paused), feed results.
    pub fn step(&mut self) {
        let recs = self.take_ring();
        for r in &recs {
            if r.is_cmd() {
                let (h, _) = r.cmd();
                self.unanswered.push((h.cmd_id, self.next_seq));
                self.next_seq += 1;
            }
        }
        if !self.paused {
            self.answer_all();
        }
        for (c, r) in self.core.on_tick(Tsc(self.now)) {
            self.delivered.push((c, r));
        }
    }

    /// Answers every taken command (duplicates → `Duplicate`).
    pub fn answer_all(&mut self) {
        let pend = std::mem::take(&mut self.unanswered);
        for (id, seq) in pend {
            let o = self
                .script
                .iter()
                .find(|(s, _)| *s == seq)
                .map(|(_, o)| *o)
                .unwrap_or(Outcome::APPLIED);
            let o = if self.seq_seen_ids.contains(&id) {
                Outcome {
                    status: 2,
                    ..Outcome::APPLIED
                }
            } else {
                o
            };
            self.seq_seen_ids.push(id);
            self.result(id, seq, o);
        }
    }

    pub fn result(&mut self, id: [u8; 16], seq: u64, o: Outcome) {
        let mut r = <CmdResult as zerocopy::FromZeros>::new_zeroed();
        r.cmd_id = id;
        r.cmd_seq = seq;
        r.status = o.status;
        r.detail = o.detail;
        r.engine_epoch = 17;
        r.ctl_state_version = o.ctl_state_version;
        r.rflags = o.rflags;
        if let Some(d) = self.core.on_result(&r) {
            self.delivered.push(d);
        }
    }

    /// Advance the clock.
    pub fn at(&mut self, ns: u64) {
        self.now = ns;
    }

    /// The commands in records `recs`.
    pub fn cmds(recs: &[RingRec]) -> Vec<&RingRec> {
        recs.iter().filter(|r| r.is_cmd()).collect()
    }

    pub fn notes(recs: &[RingRec]) -> Vec<AuditV> {
        recs.iter()
            .filter(|r| r.is_note())
            .map(|r| audit_of(&r.note()))
            .collect()
    }
}

/// `result_code` of the single audit Note of `recs`.
pub fn only_audit(recs: &[RingRec]) -> AuditV {
    let n = G0::<CtlTestSet>::notes(recs);
    assert_eq!(n.len(), 1, "{recs:?}");
    n[0].clone()
}

pub fn code(r: &CmdResponse) -> &'static str {
    r.code.map_or("", |c| c.name())
}

pub fn status(r: &CmdResponse) -> String {
    serde_json::to_value(r.status)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

pub fn detail_name(r: &CmdResponse) -> String {
    r.detail.as_ref().map_or(String::new(), |d| d.name.clone())
}

pub fn detail_arg(r: &CmdResponse) -> String {
    r.detail.as_ref().and_then(|d| d.arg.clone()).unwrap_or_default()
}

/// Busy/written helper: whether the response means "waiting/answered by the sequencer".
pub fn applied(r: &CmdResponse) -> bool {
    r.code == Some(hft_ctl_proto::ResultCode::Applied)
}
