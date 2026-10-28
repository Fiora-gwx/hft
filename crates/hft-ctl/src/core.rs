//! The gateway's request processing, independent of sockets: authentication (given the peer
//! uid), schema, permissions, confirmation fields, de-duplication, expiry, rate limits,
//! normalization, command checks, all-or-nothing ring writes with the audit Note, the Mailbox
//! hand-over and the `CmdResult` → response matching (README "gateway", "鉴权", "CmdResult 回传").

use std::collections::VecDeque;

use hft_ctl_proto::codes::detail;
use hft_ctl_proto::names;
use hft_ctl_proto::{
    CmdBody, CmdClass, CmdRequest, CmdResponse, EngineInfo, Normalized, Principal, ResultCode,
    TransportStatus, Uuid, check_confirm_fields, class_of, effective_role, needs_confirm_static, normalize,
    parse_request,
};
use hft_event::{
    CmdResult, EventKind, GroupSpec, Note, NoteLevel, NoteSource, ParamsErr, RecordFlags, note_code,
    parse_spb1,
};
use hft_refdata::{AccountKind, LegView, LoadErrCode, RefData};
use hft_rt::{MailReceiver, MailSender, UtcClock};
use hft_strategy_sdk::{GroupTable, ParamError, RefView, ResetPreview, StrategySet};
use hft_types::{AccountId, GROUP_NONE, InstId, StrategyId, Tsc, VenueId};
use sha2::{Digest, Sha256};
use zerocopy::IntoBytes;

use crate::config::{GatewayConfig, PENDING_CAPACITY};
use crate::parsed::{ParsedConfig, ParsedParams};
use crate::ring::{CtlRing, Rec, rec_len};

/// `n` of the `CtlAudit` Note code (`source << 24 | n`; the only CtlAudit name).
pub const CTL_AUDIT_N: u32 = 1;
/// Audit `chat_type` codes.
pub const CHAT_PRIVATE: u8 = 1;
pub const CHAT_GROUP: u8 = 2;
pub const CHAT_CLI: u8 = 3;
pub const CHAT_PIPELINE: u8 = 4;
/// Funding periods `VenueNotice{FundingInterval}` accepts (seconds).
pub const FUNDING_INTERVALS_S: [i64; 4] = [3_600, 7_200, 14_400, 28_800];

/// A connection handle chosen by the caller.
pub type ConnId = u64;

/// Assembly check of a new refdata blob against the current one (hft-core startup step 2);
/// `Err(item)` = `RefdataAssembly` arg (1 AccountNamePrefix … 5 DustBelowMinNotional).
pub type RefdataAssemblyCheck = fn(&[u8], &RefData) -> Result<(), u16>;
/// `RiskParams` decoding for `SetRisk` (encoding owned by hft-risk); `Err(detail)` = InvalidArgs
/// detail (`BadValue`, `OutOfScope`). The same function type as the sequencer's `RiskDecode`.
pub type RiskDecode = fn(&[u8], AccountId, &RefData) -> Result<hft_risk::RiskParams, u16>;

/// Commands of hft-core's SIGTERM path (`issuer = cli:systemd`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum InternalCmd {
    Drain { cancel_resting: bool },
    CancelAll,
}

/// Outcome of a request.
#[derive(Debug)]
pub enum Reply {
    /// Answer now and close.
    Now(Box<CmdResponse>),
    /// Written to the ring; the answer comes from `on_result` or `on_tick`.
    Wait,
}

/// TSC ↔ UTC on the published line (hft-rt `UtcClock`).
#[derive(Clone, Debug)]
pub struct Clock(pub UtcClock);

impl Clock {
    /// UTC of `t` (never less than this clock's previous result).
    pub fn utc(&self, t: Tsc) -> i64 {
        self.0.utc_ns(t)
    }
    /// The TSC at which the current line reads `utc_ns`.
    pub fn tsc_of(&self, utc_ns: i64) -> u64 {
        Self::tsc_on(&self.0.line(), utc_ns)
    }
    fn tsc_on(c: &hft_rt::TscUtc, utc_ns: i64) -> u64 {
        let d = (i128::from(utc_ns) - i128::from(c.base_utc_ns)) << c.shift.min(64);
        let t = i128::from(c.base_tsc.0) + d / i128::from(c.mult.max(1));
        t.clamp(0, i128::from(u64::MAX)) as u64
    }
    /// Ticks of `ms` milliseconds.
    pub fn ticks_ms(&self, ms: u64) -> u64 {
        let c = self.0.line();
        let base = Self::tsc_on(&c, c.base_utc_ns);
        Self::tsc_on(
            &c,
            c.base_utc_ns
                .saturating_add((ms as i64).saturating_mul(1_000_000)),
        ) - base
    }
}

type MailboxOut<S> = Result<Option<(ParsedConfig<S>, Option<Base<S>>)>, Fail>;

struct Base<S: StrategySet> {
    strategy: StrategyId,
    account: AccountId,
    params: S::Params,
    groups: GroupTable,
}

struct Pending {
    cmd_id: Uuid,
    conn: ConnId,
    deadline: u64,
}

struct Dedup {
    cmd_id: Uuid,
    cmd_hash: Option<[u8; 32]>,
    result: Option<CmdResponse>,
}

/// A rejection: code and raw detail (`code | arg << 16`).
#[derive(Copy, Clone, Debug)]
struct Fail {
    code: ResultCode,
    detail: u32,
}

impl Fail {
    fn args(d: u16) -> Fail {
        Fail {
            code: ResultCode::InvalidArgs,
            detail: u32::from(d),
        }
    }
    fn args_arg(d: u16, arg: u16) -> Fail {
        Fail {
            code: ResultCode::InvalidArgs,
            detail: u32::from(d) | (u32::from(arg) << 16),
        }
    }
}

/// Fields of the audit Note.
#[derive(Clone, Debug, Default)]
struct Audit {
    cmd_id: [u8; 16],
    cmd_kind: u16,
    role: u8,
    view_account: u8,
    chat_type: u8,
    target_account: u8,
    issuer: [u8; 8],
    cmd_hash: [u8; 24],
    peer_uid: u32,
}

fn audit_note(a: &Audit, result_code: u8) -> Vec<u8> {
    let le = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap_or([0; 8]));
    let mut v = [0u64; 8];
    v[0] = le(&a.cmd_id[..8]);
    v[1] = le(&a.cmd_id[8..]);
    v[2] = u64::from(a.cmd_kind)
        | (u64::from(result_code) << 16)
        | (u64::from(a.role) << 24)
        | (u64::from(a.view_account) << 32)
        | (u64::from(a.chat_type) << 40)
        | (u64::from(a.target_account) << 48);
    v[3] = le(&a.issuer);
    v[4] = le(&a.cmd_hash[..8]);
    v[5] = le(&a.cmd_hash[8..16]);
    v[6] = le(&a.cmd_hash[16..]);
    v[7] = u64::from(a.peer_uid);
    let n = Note {
        code: note_code(NoteSource::CtlAudit, CTL_AUDIT_N),
        level: NoteLevel::Info,
        source: NoteSource::CtlAudit,
        strategy: StrategyId(0),
        _pad: 0,
        group: GROUP_NONE,
        inst: InstId(0),
        v,
    };
    n.as_bytes().to_vec()
}

fn params_err_detail(e: ParamsErr) -> u16 {
    match e {
        ParamsErr::DupGroup => detail::GROUP_ID_DUP,
        _ => detail::BAD_GROUP,
    }
}

/// hft-event decode errors → detail (README "gateway 侧的命令校验").
pub fn detail_of_event_err(e: EventErr) -> (ResultCode, u16) {
    let d = match e {
        EventErr::Params(p) => params_err_detail(p),
        EventErr::UnknownKind | EventErr::InvalidEnum => detail::UNKNOWN_DISCRIMINANT,
    };
    (ResultCode::InvalidArgs, d)
}

/// The hft-event errors the gateway maps.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EventErr {
    Params(ParamsErr),
    UnknownKind,
    InvalidEnum,
}

fn param_error_field(e: &ParamError) -> u16 {
    match e {
        ParamError::OutOfRange { field_ix, .. }
        | ParamError::BadEnum { field_ix }
        | ParamError::NotPerGroup { field_ix }
        | ParamError::Invalid { field_ix, .. }
        | ParamError::BadText { field_ix } => *field_ix,
        _ => detail::PARAM_FIELD_NONE,
    }
}

fn strategy_param_fail(e: &ParamError) -> Fail {
    Fail::args_arg(
        detail::PARAM_INVALID,
        detail::param_arg(detail::PARAM_KIND_STRATEGY, param_error_field(e)),
    )
}

fn aligned(b: &[u8]) -> Vec<u64> {
    let mut v = vec![0u64; b.len().div_ceil(8)];
    v.as_mut_bytes()[..b.len()].copy_from_slice(b);
    v
}

/// The gateway core. `S` = the assembled strategy set, `R` = the ctl ring.
pub struct Core<S: StrategySet, R: CtlRing> {
    cfg: GatewayConfig,
    rd: RefData,
    ring: R,
    parsed: MailSender<ParsedConfig<S>>,
    retired: MailReceiver<ParsedConfig<S>>,
    refdata_check: RefdataAssemblyCheck,
    risk_decode: RiskDecode,
    clock: Clock,
    engine: EngineInfo,
    bases: Vec<Base<S>>,
    /// `SetParams` written whose result has not been read: (cmd_id, strategy, account, new base).
    awaiting: Vec<(Uuid, StrategyId, AccountId, Option<Base<S>>)>,
    write_rate: Vec<(String, VecDeque<u64>)>,
    warm_rate: Vec<(String, VecDeque<u64>)>,
    dedup: VecDeque<Dedup>,
    pending: Vec<Pending>,
    internal_seq: u64,
}

impl<S: StrategySet, R: CtlRing> core::fmt::Debug for Core<S, R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Core")
            .field("pending", &self.pending.len())
            .field("dedup", &self.dedup.len())
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet, R: CtlRing> Core<S, R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: GatewayConfig,
        rd: RefData,
        ring: R,
        parsed: MailSender<ParsedConfig<S>>,
        retired: MailReceiver<ParsedConfig<S>>,
        refdata_check: RefdataAssemblyCheck,
        risk_decode: RiskDecode,
        active_params: Vec<ParsedConfig<S>>,
        clock: impl Into<UtcClock>,
        engine: EngineInfo,
    ) -> Core<S, R> {
        let bases = active_params
            .into_iter()
            .filter_map(|p| match p {
                ParsedConfig::StrategyParams(p) => Some(Base {
                    strategy: p.strategy,
                    account: p.account,
                    params: p.params,
                    groups: p.groups,
                }),
                _ => None,
            })
            .collect();
        Core {
            cfg,
            rd,
            ring,
            parsed,
            retired,
            refdata_check,
            risk_decode,
            clock: Clock(clock.into()),
            engine,
            bases,
            awaiting: Vec::new(),
            write_rate: Vec::new(),
            warm_rate: Vec::new(),
            dedup: VecDeque::new(),
            pending: Vec::new(),
            internal_seq: 0,
        }
    }

    pub fn ring(&self) -> &R {
        &self.ring
    }
    pub fn ring_mut(&mut self) -> &mut R {
        &mut self.ring
    }
    pub fn config(&self) -> &GatewayConfig {
        &self.cfg
    }
    pub fn refdata(&self) -> &RefData {
        &self.rd
    }
    pub fn clock(&self) -> &Clock {
        &self.clock
    }
    /// Number of commands waiting for their `CmdResult`.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    /// Earliest pending deadline.
    pub fn next_deadline(&self) -> Option<Tsc> {
        self.pending.iter().map(|p| p.deadline).min().map(Tsc)
    }
    /// Frees the objects the sequencer handed back (in this, the gateway's, thread).
    pub fn drain_retired(&mut self) -> usize {
        let mut n = 0;
        while let Some(o) = self.retired.try_take() {
            drop(o);
            n += 1;
        }
        n
    }

    fn response(&self, cmd_id: Uuid, status: TransportStatus, code: Option<ResultCode>) -> CmdResponse {
        CmdResponse::new(cmd_id, status, code, self.engine.clone())
    }

    fn render_fail(&self, cmd_id: Uuid, f: Fail, cmd_hash: Option<[u8; 32]>) -> CmdResponse {
        let status = match f.code {
            ResultCode::Unauthorized => TransportStatus::Unauthorized,
            ResultCode::Busy => TransportStatus::Busy,
            ResultCode::Internal => TransportStatus::InternalError,
            _ => TransportStatus::Ok,
        };
        let mut r = self.response(cmd_id, status, Some(f.code));
        r.detail = hft_ctl_proto::render_detail(f.detail);
        r.cmd_hash = cmd_hash.map(hft_ctl_proto::Hex32);
        r
    }

    /// Writes an audit-only record for a rejected request (best effort: a full ring drops it).
    fn audit_only(&mut self, a: &Audit, code: ResultCode) {
        let rec: Rec = (EventKind::NOTE, RecordFlags(0), audit_note(a, code as u8));
        let _ = self.ring.write_all(std::slice::from_ref(&rec));
    }

    fn reject(&mut self, a: &Audit, cmd_id: Uuid, f: Fail, cmd_hash: Option<[u8; 32]>) -> Reply {
        self.audit_only(a, f.code);
        Reply::Now(Box::new(self.render_fail(cmd_id, f, cmd_hash)))
    }

    /// Authentication step 1: an unknown uid gets `unauthorized` (cmd_id all zero) without the
    /// request being read; `None` = known uid.
    pub fn check_uid(&mut self, peer_uid: u32) -> Option<CmdResponse> {
        if self.cfg.principals.uid(peer_uid).is_some() {
            return None;
        }
        let a = Audit {
            peer_uid,
            ..Audit::default()
        };
        self.audit_only(&a, ResultCode::Unauthorized);
        Some(self.response(Uuid::nil(), TransportStatus::Unauthorized, None))
    }

    fn count(
        window: &mut Vec<(String, VecDeque<u64>)>,
        issuer: &str,
        now: u64,
        span: u64,
        limit: usize,
    ) -> bool {
        let ix = match window.iter().position(|(i, _)| i == issuer) {
            Some(i) => i,
            None => {
                window.push((issuer.to_owned(), VecDeque::new()));
                window.len() - 1
            }
        };
        let q = &mut window[ix].1;
        while q.front().is_some_and(|&t| t + span <= now) {
            q.pop_front();
        }
        if q.len() >= limit {
            return false;
        }
        q.push_back(now);
        true
    }

    fn dedup_find(&self, id: Uuid) -> Option<&Dedup> {
        self.dedup.iter().find(|d| d.cmd_id == id)
    }

    fn dedup_insert(&mut self, id: Uuid, cmd_hash: Option<[u8; 32]>) {
        while self.dedup.len() >= self.cfg.dedup_capacity.max(1) as usize {
            self.dedup.pop_front();
        }
        self.dedup.push_back(Dedup {
            cmd_id: id,
            cmd_hash,
            result: None,
        });
    }

    /// Processes one request frame body from a known `peer_uid` (steps ②–⑪).
    pub fn on_request(&mut self, conn: ConnId, peer_uid: u32, body: &[u8], now: Tsc) -> Reply {
        let mut a = Audit {
            peer_uid,
            ..Audit::default()
        };
        // ② schema.
        let req = match parse_request(body) {
            Ok(r) => r,
            Err(e) => {
                let id = e.cmd_id.unwrap_or(Uuid::nil());
                a.cmd_id = *id.as_bytes();
                self.audit_only(&a, ResultCode::InvalidArgs);
                let mut r = self.response(id, TransportStatus::InvalidRequest, None);
                r.detail = e.detail.map(hft_ctl_proto::codes::Detail::code);
                return Reply::Now(Box::new(r));
            }
        };
        a.cmd_id = *req.cmd_id.as_bytes();
        a.cmd_kind = req.cmd.cmd_kind();
        let mut iss = [0u8; 8];
        let n = req.issuer.len().min(8);
        iss[..n].copy_from_slice(&req.issuer.as_bytes()[..n]);
        a.issuer = iss;
        a.view_account = req
            .view_account
            .as_deref()
            .and_then(|v| names::account(&self.rd, v))
            .map_or(0, |x| x.0);
        // ③ issuer.
        let Some(who) = self.cfg.principals.resolve(peer_uid, &req.issuer) else {
            a.issuer = [0; 8];
            self.audit_only(&a, ResultCode::Unauthorized);
            return Reply::Now(Box::new(self.response(
                req.cmd_id,
                TransportStatus::Unauthorized,
                None,
            )));
        };
        a.issuer = iss;
        let chat = req.chat_type.unwrap_or(hft_ctl_proto::ChatType::Private);
        a.chat_type = match who.principal {
            Principal::Tg => match chat {
                hft_ctl_proto::ChatType::Private => CHAT_PRIVATE,
                hft_ctl_proto::ChatType::Group => CHAT_GROUP,
            },
            Principal::Cli => CHAT_CLI,
            Principal::Pipeline => CHAT_PIPELINE,
            _ => 0,
        };
        // ④ permissions.
        if hft_ctl_proto::allowed(who.roles, who.principal, chat, &req.cmd).is_err() {
            a.role = who.roles.highest().map_or(0, |r| r as u8);
            return self.reject(
                &a,
                req.cmd_id,
                Fail {
                    code: ResultCode::Forbidden,
                    detail: 0,
                },
                None,
            );
        }
        let role = effective_role(who.roles, who.principal, chat, &req.cmd);
        a.role = role.map_or(0, |r| r as u8);
        // ⑤ confirmation fields.
        let rule = needs_confirm_static(role, who.principal, &req.cmd);
        if check_confirm_fields(rule, &req).is_err() {
            self.audit_only(&a, ResultCode::InvalidArgs);
            return Reply::Now(Box::new(self.response(
                req.cmd_id,
                TransportStatus::InvalidRequest,
                None,
            )));
        }
        if let CmdBody::GetSchema {
            strategy_kind,
            kind_id,
        } = &req.cmd
        {
            return self.get_schema(&a, req.cmd_id, strategy_kind.as_deref(), kind_id.as_deref());
        }
        let recon = matches!(req.cmd, CmdBody::ReconcileReport { .. });
        // ⑥ duplicate cmd_id.
        if !recon && let Some(d) = self.dedup_find(req.cmd_id) {
            let r = match &d.result {
                Some(r) => r.clone(),
                None => {
                    let mut r = self.response(req.cmd_id, TransportStatus::Accepted, None);
                    r.cmd_hash = d.cmd_hash.map(hft_ctl_proto::Hex32);
                    r
                }
            };
            return Reply::Now(Box::new(r));
        }
        // ⑦ expiry.
        if self.clock.utc(now) > req.expires_at_ns {
            return self.reject(
                &a,
                req.cmd_id,
                Fail {
                    code: ResultCode::Expired,
                    detail: 0,
                },
                None,
            );
        }
        // ⑧ rate limits.
        let warm = matches!(req.cmd, CmdBody::InjectWarmup { .. });
        let counted = !recon && matches!(class_of(&req.cmd), CmdClass::Upgrade | CmdClass::Neutral);
        if counted {
            let ok = if warm {
                let span = self.clock.ticks_ms(1_000);
                Self::count(
                    &mut self.warm_rate,
                    &req.issuer,
                    now.0,
                    span,
                    usize::from(self.cfg.warmup_rate_per_s),
                )
            } else {
                let span = self.clock.ticks_ms(60_000);
                Self::count(
                    &mut self.write_rate,
                    &req.issuer,
                    now.0,
                    span,
                    usize::from(self.cfg.write_rate_per_min),
                )
            };
            if !ok {
                return self.reject(
                    &a,
                    req.cmd_id,
                    Fail {
                        code: ResultCode::RateLimited,
                        detail: 0,
                    },
                    None,
                );
            }
        }
        // ⑨ normalize.
        let mut n = match normalize(&req, &self.rd) {
            Ok(n) => n,
            Err(e) => return self.reject(&a, req.cmd_id, Fail::args_arg(e.detail, e.arg), None),
        };
        a.target_account = n.hdr.account.0;
        if recon {
            return self.write_reports(&a, &req, &n);
        }
        let hash = n.cmd_hash;
        a.cmd_hash.copy_from_slice(&hash[..24]);
        // ⑩ command checks.
        let mailbox = match self.validate(&req, &n) {
            Ok(m) => m,
            Err(f) => return self.reject(&a, req.cmd_id, f, Some(hash)),
        };
        // ⑪ write.
        if self.pending.len() >= PENDING_CAPACITY {
            return self.reject(
                &a,
                req.cmd_id,
                Fail {
                    code: ResultCode::Busy,
                    detail: 0,
                },
                Some(hash),
            );
        }
        let expires_tsc = self.clock.tsc_of(req.expires_at_ns);
        n.set_gateway_fields(expires_tsc, peer_uid, a.role);
        let mut recs: Vec<Rec> = n.ring_records();
        recs.push((EventKind::NOTE, RecordFlags(0), audit_note(&a, 0)));
        let lens: Vec<usize> = recs.iter().map(rec_len).collect();
        if !self.ring.fits(&lens) {
            return Reply::Now(Box::new(self.render_fail(
                req.cmd_id,
                Fail {
                    code: ResultCode::Busy,
                    detail: 0,
                },
                Some(hash),
            )));
        }
        let mut base = None;
        if let Some((obj, b)) = mailbox {
            if let Err(_back) = self.parsed.try_send(obj) {
                return self.reject(
                    &a,
                    req.cmd_id,
                    Fail {
                        code: ResultCode::Busy,
                        detail: 0,
                    },
                    Some(hash),
                );
            }
            base = b;
        }
        if !self.ring.write_all(&recs) {
            return Reply::Now(Box::new(self.render_fail(
                req.cmd_id,
                Fail {
                    code: ResultCode::Busy,
                    detail: 0,
                },
                Some(hash),
            )));
        }
        if let CmdBody::SetParams { .. } = &req.cmd {
            let strategy = match hft_event::ControlCmd::decode(&n.record) {
                Ok((_, hft_event::ControlCmd::SetParams { scope, .. })) => scope.strategy,
                _ => StrategyId(0),
            };
            self.awaiting.push((req.cmd_id, strategy, n.hdr.account, base));
        }
        self.dedup_insert(req.cmd_id, Some(hash));
        let deadline = now.0 + self.clock.ticks_ms(u64::from(self.cfg.result_wait_ms));
        self.pending.push(Pending {
            cmd_id: req.cmd_id,
            conn,
            deadline,
        });
        Reply::Wait
    }

    fn write_reports(&mut self, a: &Audit, req: &CmdRequest, n: &Normalized) -> Reply {
        let mut recs = n.ring_records();
        recs.push((EventKind::NOTE, RecordFlags(0), audit_note(a, 0)));
        if !self.ring.write_all(&recs) {
            return Reply::Now(Box::new(self.render_fail(
                req.cmd_id,
                Fail {
                    code: ResultCode::Busy,
                    detail: 0,
                },
                None,
            )));
        }
        Reply::Now(Box::new(self.response(
            req.cmd_id,
            TransportStatus::Ok,
            Some(ResultCode::Applied),
        )))
    }

    fn get_schema(&mut self, a: &Audit, cmd_id: Uuid, name: Option<&str>, kind_id: Option<&str>) -> Reply {
        let kind = match (name, kind_id) {
            (Some(n), _) => S::kinds().iter().find(|d| d.name == n).map(|d| d.kind_id),
            (_, Some(k)) => k
                .parse::<u16>()
                .ok()
                .filter(|k| S::kinds().iter().any(|d| d.kind_id == *k)),
            _ => None,
        };
        let Some(json) = kind.and_then(S::schema_json) else {
            return self.reject(a, cmd_id, Fail::args(detail::UNKNOWN_NAME), None);
        };
        let Ok(body) = serde_json::value::RawValue::from_string(json) else {
            return self.reject(
                a,
                cmd_id,
                Fail {
                    code: ResultCode::Internal,
                    detail: 0,
                },
                None,
            );
        };
        self.audit_only(a, ResultCode::Applied);
        let mut r = self.response(cmd_id, TransportStatus::Ok, Some(ResultCode::Applied));
        r.body = body;
        Reply::Now(Box::new(r))
    }

    fn account_venue(&self, a: AccountId, v: VenueId) -> bool {
        v.0 < 64 && self.rd.account(a).venues & (1u64 << v.0) != 0
    }

    /// Step ⑩; returns the Mailbox object (and the new comparison base for `SetParams`).
    #[allow(clippy::type_complexity)]
    fn validate(
        &self,
        req: &CmdRequest,
        n: &Normalized,
    ) -> Result<Option<(ParsedConfig<S>, Option<Base<S>>)>, Fail> {
        use hft_event::ControlCmd as C;
        let account = n.hdr.account;
        let meta = self.rd.account(account);
        let Ok((_, cmd)) = C::decode(&n.record) else {
            return Err(Fail::args(detail::UNKNOWN_DISCRIMINANT));
        };
        match cmd {
            C::SimReset { .. } if !matches!(meta.kind, AccountKind::Sim(_)) => {
                Err(Fail::args(detail::NOT_SIM))
            }
            C::SimTransfer { from, to, .. } => {
                if !matches!(meta.kind, AccountKind::Sim(_)) {
                    Err(Fail::args(detail::NOT_SIM))
                } else if !self.account_venue(account, from) || !self.account_venue(account, to) {
                    Err(Fail::args(detail::UNKNOWN_NAME))
                } else {
                    Ok(None)
                }
            }
            C::SimFault { venue, .. } => {
                if !matches!(meta.kind, AccountKind::Sim(_)) {
                    Err(Fail::args(detail::NOT_SIM))
                } else if !self.account_venue(account, venue) {
                    Err(Fail::args(detail::UNKNOWN_NAME))
                } else {
                    Ok(None)
                }
            }
            C::VenueNotice {
                venue,
                kind,
                inst,
                a,
                b,
                ..
            } => {
                if inst.0 != 0 && self.rd.inst(inst).venue != venue {
                    Err(Fail::args(detail::UNKNOWN_NAME))
                } else if (kind == 2 && !FUNDING_INTERVALS_S.contains(&a)) || (kind == 3 && a >= b) {
                    Err(Fail::args(detail::BAD_VALUE))
                } else {
                    Ok(None)
                }
            }
            C::BindSlot {
                venue,
                inst,
                start_s,
                end_s,
                ..
            } => {
                if inst.0 == 0 || self.rd.inst(inst).venue != venue {
                    Err(Fail::args(detail::UNKNOWN_NAME))
                } else if start_s >= end_s {
                    Err(Fail::args(detail::BAD_VALUE))
                } else {
                    Ok(None)
                }
            }
            C::Settle {
                venue, inst, payout, ..
            } => {
                if inst.0 == 0 || self.rd.inst(inst).venue != venue {
                    Err(Fail::args(detail::UNKNOWN_NAME))
                } else if payout.0 < 0 {
                    Err(Fail::args(detail::BAD_VALUE))
                } else {
                    Ok(None)
                }
            }
            C::AdoptExchange { item: 1, group, .. }
                if group != GROUP_NONE && group.strategy() != meta.bound_instance =>
            {
                Err(Fail::args(detail::BAD_GROUP))
            }
            C::SmokeOrder { venue, inst, .. } => {
                if !matches!(meta.kind, AccountKind::Live) {
                    Err(Fail::args(detail::NOT_LIVE))
                } else if !self.account_venue(account, venue) || self.rd.inst(inst).venue != venue {
                    Err(Fail::args(detail::UNKNOWN_NAME))
                } else {
                    Ok(None)
                }
            }
            C::WriteOffResidual { group, .. } if group.strategy() != meta.bound_instance => {
                Err(Fail::args(detail::BAD_GROUP))
            }
            C::StrategyCmd {
                instance,
                reduce_only,
                len,
                payload,
                ..
            } => self.validate_strategy_cmd(instance, reduce_only, &payload[..usize::from(len).min(256)]),
            C::ReplaceRefdata { blob } => {
                let content = n.blob.as_deref().unwrap_or(&[]);
                if let Err(item) = (self.refdata_check)(content, &self.rd) {
                    return Err(Fail {
                        code: ResultCode::Rejected,
                        detail: u32::from(detail::REFDATA_ASSEMBLY) | (u32::from(item) << 16),
                    });
                }
                let result = Box::new(RefData::load(content, Some(&self.rd)));
                Ok(Some((
                    ParsedConfig::Refdata {
                        sha256: blob.sha256,
                        result,
                    },
                    None,
                )))
            }
            C::SetRisk { blob } => {
                let content = n.blob.as_deref().unwrap_or(&[]);
                let p = (self.risk_decode)(content, account, &self.rd).map_err(Fail::args)?;
                Ok(Some((
                    ParsedConfig::Risk {
                        sha256: blob.sha256,
                        params: Box::new(p),
                    },
                    None,
                )))
            }
            C::SetParams { scope, .. } => {
                let blob = n.params_blob().unwrap_or(&[]);
                self.validate_set_params(scope.strategy, account, blob)
                    .map(|(p, b)| Some((ParsedConfig::StrategyParams(Box::new(p)), Some(b))))
            }
            _ => {
                let _ = req;
                Ok(None)
            }
        }
    }

    fn validate_strategy_cmd(&self, instance: StrategyId, reduce_only: u8, payload: &[u8]) -> MailboxOut<S> {
        let kind = self.rd.instance(instance).map_or(0, |i| i.kind_id);
        let cmds = S::cmds(kind);
        let none = Fail::args_arg(
            detail::PARAM_INVALID,
            detail::param_arg(detail::PARAM_KIND_STRATEGY, detail::PARAM_FIELD_NONE),
        );
        let (op, _) =
            hft_strategy_sdk::params::decode_cmd(cmds, payload).map_err(|e| strategy_param_fail(&e))?;
        let desc = cmds.iter().find(|c| c.op == op).ok_or(none)?;
        if desc.reduce_only != (reduce_only == 1) {
            return Err(none);
        }
        Ok(None)
    }

    /// `SetParams` checks ①–⑧ (README table); returns the Mailbox object and the new base.
    fn validate_set_params(
        &self,
        strategy: StrategyId,
        account: AccountId,
        blob: &[u8],
    ) -> Result<(ParsedParams<S>, Base<S>), Fail> {
        let buf = aligned(blob);
        let bytes = &buf.as_bytes()[..blob.len()];
        let v = parse_spb1(bytes).map_err(|e| Fail::args(params_err_detail(e)))?;
        let kind = v.hdr.kind_id;
        let desc = S::kinds()
            .iter()
            .find(|d| d.kind_id == kind)
            .ok_or(Fail::args(detail::SCHEMA_UNSUPPORTED))?;
        if let Err(e) = self.rd.check_binding(account, strategy, kind) {
            return Err(Fail::args(if e.code == LoadErrCode::KindMismatch {
                detail::KIND_MISMATCH
            } else {
                detail::ACCOUNT_BOUND
            }));
        }
        for g in v.groups {
            let mut legs = [LegView {
                inst: InstId(0),
                venue: VenueId(0),
            }; 4];
            if let Err(e) = self.rd.check_group(account, &g.view(&mut legs)) {
                return Err(if e.code == LoadErrCode::SameExchangeLegs {
                    Fail::args(detail::SAME_EXCHANGE_LEGS)
                } else {
                    Fail::args_arg(detail::GROUP_CHECK, e.code as u16)
                });
            }
        }
        let ver = v.hdr.schema_ver;
        if ver != desc.schema.version || v.hdr.schema_id != desc.schema.id {
            return Err(Fail::args(detail::SCHEMA_UNSUPPORTED));
        }
        if v.hdr.schema_digest != hft_strategy_sdk::params::schema_digest(desc, ver) {
            return Err(Fail::args(detail::SCHEMA_MISMATCH));
        }
        let decode = || -> Result<(S::Params, GroupTable), Fail> {
            let gt = GroupTable::new(v.groups, self.cfg.max_groups_per_instance);
            let p = S::decode_params(kind, v.body, ver, &gt).map_err(|e| strategy_param_fail(&e))?;
            S::validate_params(&p, &gt, &RefView::new(&self.rd, account))
                .map_err(|e| strategy_param_fail(&e))?;
            Ok((p, gt))
        };
        let (params, groups) = decode()?;
        if desc.non_prod_only && self.rd.account(account).is_prod {
            return Err(Fail::args_arg(
                detail::PARAM_INVALID,
                detail::param_arg(detail::PARAM_KIND_NON_PROD_ONLY, 0),
            ));
        }
        let n_groups = v.groups.len() as u16;
        let model_mem = S::model_mem_bytes(&params, n_groups);
        let model_ckpt = S::model_snapshot_bytes(&params, n_groups);
        let mib = |m: u32| u64::from(m) << 20;
        if model_mem > mib(self.cfg.advisor_mem_budget_mib)
            || model_ckpt.saturating_mul(2) > mib(self.cfg.advisor_ckpt_max_mib)
        {
            return Err(Fail::args_arg(
                detail::PARAM_INVALID,
                detail::param_arg(detail::PARAM_KIND_MODEL_MEM_BUDGET, 0),
            ));
        }
        if self
            .awaiting
            .iter()
            .any(|(_, s, a, _)| *s == strategy && *a == account)
        {
            return Err(Fail {
                code: ResultCode::Busy,
                detail: 0,
            });
        }
        let reset_preview = match self
            .bases
            .iter()
            .find(|b| b.strategy == strategy && b.account == account)
        {
            Some(b) => S::reset_preview(&b.params, &b.groups, &params, &groups),
            None => ResetPreview {
                reset_groups: 0,
                no_open_ms: 0,
            },
        };
        // Structural change = some group present in both versions restarts.
        let structural_changed = match self
            .bases
            .iter()
            .find(|b| b.strategy == strategy && b.account == account)
        {
            Some(b) => {
                let kept: Vec<GroupSpec> = v
                    .groups
                    .iter()
                    .filter(|g| b.groups.gix_of(g.id).is_some())
                    .copied()
                    .collect();
                let gt_kept = GroupTable::new(&kept, self.cfg.max_groups_per_instance);
                match S::decode_params(kind, v.body, ver, &gt_kept) {
                    Ok(pk) => S::reset_preview(&b.params, &b.groups, &pk, &gt_kept).reset_groups > 0,
                    Err(_) => reset_preview.reset_groups > 0,
                }
            }
            None => false,
        };
        let (base_params, base_groups) = decode()?;
        let sha256: [u8; 32] = Sha256::digest(blob).into();
        Ok((
            ParsedParams {
                sha256,
                strategy,
                account,
                version: v.hdr.version,
                kind_id: kind,
                params,
                groups,
                model_mem,
                model_ckpt,
                structural_changed,
                reset_preview,
            },
            Base {
                strategy,
                account,
                params: base_params,
                groups: base_groups,
            },
        ))
    }

    /// A `CmdResult` read from the journal tail: fills the de-duplication cache, updates the
    /// `SetParams` base and returns the waiting connection's answer.
    pub fn on_result(&mut self, res: &CmdResult) -> Option<(ConnId, CmdResponse)> {
        let id = Uuid::from_bytes(res.cmd_id);
        if let Some(i) = self.awaiting.iter().position(|(c, ..)| *c == id) {
            let (_, s, a, base) = self.awaiting.remove(i);
            if res.status == ResultCode::Applied as u8
                && let Some(b) = base
            {
                self.bases.retain(|x| !(x.strategy == s && x.account == a));
                self.bases.push(b);
            }
        }
        self.engine.epoch = res.engine_epoch;
        self.engine.risk = res.risk_version;
        let hash = self.dedup_find(id).and_then(|d| d.cmd_hash);
        let r = CmdResponse::from_result(res, hash, &self.rd, self.engine.clone());
        if let Some(d) = self.dedup.iter_mut().find(|d| d.cmd_id == id) {
            d.result = Some(r.clone());
        }
        let i = self.pending.iter().position(|p| p.cmd_id == id)?;
        let p = self.pending.remove(i);
        Some((p.conn, r))
    }

    /// Pending commands whose wait expired at `now` answer `accepted`.
    pub fn on_tick(&mut self, now: Tsc) -> Vec<(ConnId, CmdResponse)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.pending.len() {
            if now.0 >= self.pending[i].deadline {
                let p = self.pending.remove(i);
                let hash = self.dedup_find(p.cmd_id).and_then(|d| d.cmd_hash);
                let mut r = self.response(p.cmd_id, TransportStatus::Accepted, None);
                r.cmd_hash = hash.map(hft_ctl_proto::Hex32);
                out.push((p.conn, r));
            } else {
                i += 1;
            }
        }
        out
    }

    /// A connection went away: forget its pending entry (the result still lands in the cache).
    pub fn forget_conn(&mut self, conn: ConnId) {
        self.pending.retain(|p| p.conn != conn);
    }

    /// The SIGTERM path (`issuer = cli:systemd`, peer uid 0): written with its audit Note.
    pub fn on_internal(&mut self, cmd: InternalCmd, now: Tsc) -> bool {
        self.internal_seq += 1;
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&now.0.to_be_bytes());
        id[8..].copy_from_slice(&self.internal_seq.to_be_bytes());
        let utc = self.clock.utc(now);
        let body = match cmd {
            InternalCmd::Drain { cancel_resting } => CmdBody::Drain {
                cancel_resting: Some(cancel_resting),
                account: None,
            },
            InternalCmd::CancelAll => CmdBody::CancelAll {
                venue: None,
                inst: None,
                account: Some("*".into()),
            },
        };
        let req = CmdRequest {
            v: hft_ctl_proto::PROTO_VERSION,
            cmd_id: Uuid::from_bytes(id),
            issuer: "cli:systemd".into(),
            issued_at_ns: utc,
            expires_at_ns: utc.saturating_add(hft_ctl_proto::MAX_EXPIRY_NS),
            account: hft_ctl_proto::AccountSel::Missing,
            preview: false,
            confirmed: false,
            expect_state_version: None,
            view_account: None,
            chat_type: None,
            cmd: body,
        };
        let Ok(mut n) = normalize(&req, &self.rd) else {
            return false;
        };
        n.set_gateway_fields(self.clock.tsc_of(req.expires_at_ns), 0, 0);
        let mut a = Audit {
            cmd_id: id,
            cmd_kind: req.cmd.cmd_kind(),
            target_account: n.hdr.account.0,
            ..Audit::default()
        };
        a.issuer.copy_from_slice(b"cli:syst");
        a.cmd_hash.copy_from_slice(&n.cmd_hash[..24]);
        let mut recs = n.ring_records();
        recs.push((EventKind::NOTE, RecordFlags(0), audit_note(&a, 0)));
        let ok = self.ring.write_all(&recs);
        if ok {
            self.dedup_insert(req.cmd_id, Some(n.cmd_hash));
        }
        ok
    }
}
