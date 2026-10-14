//! Normalization: a schema-valid `CmdRequest` + refdata → `ControlCmd` record bytes, `cmd_hash`,
//! `ConfigBlob` segments (or `ReconcileReport` records). Steps ①–⑥ of the README in order; the
//! first failure wins. Pure: no I/O, no clock, no state.

use base64::Engine as _;
use hft_event::{
    BlobHdr, BlobKind, BlobRef, CmdHdr, CmdScope, ControlCmd, EventKind, PartHdr, PartIter, ReconcileReport,
    RecordFlags, STRATEGY_CMD_MAX_PAYLOAD, WARMUP_MAX_PAYLOAD,
};
use hft_refdata::RefData;
use hft_types::{
    AccountId, AssetId, CapSource, GROUP_ALL, GROUP_NONE, GroupId, InstId, KillLevel, Notional, Px, Qty,
    SlotId, StrategyId, VenueId, parse_exact,
};
use sha2::{Digest, Sha256};
use zerocopy::IntoBytes;

use crate::MAX_BLOB;
use crate::codes::detail;
use crate::names;
use crate::schema::{AccountSel, AdoptItem, CmdBody, CmdRequest, NoticeKind, ReconItem, SimFaultKind};

/// `StrategyParamsHdr` size: shorter SPB1 blobs are `BadValue`.
pub const SPB1_HDR_LEN: usize = 48;
/// `SmokeOrder.ttl_ms` default and range (control-plane 命令集).
pub const SMOKE_TTL_DEFAULT_MS: u32 = 10_000;
pub const SMOKE_TTL_MAX_MS: u32 = 60_000;
/// `SimFault.delay_ms` range for `DelayReports`.
pub const SIM_FAULT_DELAY_MAX_MS: u32 = 60_000;
/// `Annotate.text` and `RecordTransfer.reference` limits.
pub const ANNOTATE_TEXT_MAX: usize = 256;
pub const TRANSFER_REF_MAX: usize = 64;
/// `Notional` fixed exponent.
const NOTIONAL_EXP: i8 = -6;

/// Normalization failure: always `InvalidArgs{detail}`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct NormErr {
    pub detail: u16,
    pub arg: u16,
}

impl NormErr {
    pub const fn new(detail: u16) -> NormErr {
        NormErr { detail, arg: 0 }
    }
    /// `CmdResult.detail` encoding `code | arg << 16`.
    pub fn raw(self) -> u32 {
        u32::from(self.detail) | (u32::from(self.arg) << 16)
    }
}

type R<T> = Result<T, NormErr>;

fn err<T>(d: u16) -> R<T> {
    Err(NormErr::new(d))
}

/// One `ConfigBlob` segment ready for the ctl ring: body = `PartHdr` + chunk of the logical body
/// (`BlobHdr` + blob content).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct BlobPart {
    pub flags: RecordFlags,
    pub body: Vec<u8>,
    content_off: usize,
}

impl BlobPart {
    /// `PartHdr` of the segment.
    pub fn part_hdr(&self) -> PartHdr {
        let b = |o: usize, n: usize| self.body.get(o..o + n).unwrap_or(&[0; 4]).to_vec();
        let u32_at = |o| u32::from_le_bytes(b(o, 4).try_into().unwrap_or([0; 4]));
        let u16_at = |o| u16::from_le_bytes(b(o, 2).try_into().unwrap_or([0; 2]));
        PartHdr {
            total_len: u32_at(0),
            chunk_len: u32_at(4),
            part: u16_at(8),
            parts: u16_at(10),
            _rsv: 0,
        }
    }
    /// The blob content carried by this segment (the `BlobHdr` prefix of segment 0 excluded);
    /// concatenating every segment's `content()` gives the blob.
    pub fn content(&self) -> &[u8] {
        self.body.get(16 + self.content_off..).unwrap_or(&[])
    }
}

/// A normalized request.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Normalized {
    /// `CONTROL_CMD`, or `RECONCILE_REPORT` / `RECONCILE_BATCH` for reconciliation.
    pub kind: EventKind,
    /// `CmdHdr` as written in `record` (gateway fields `expires_tsc`, `peer_uid`, `role` 0).
    pub hdr: CmdHdr,
    /// `ControlCmd` logical body = `CmdHdr` + payload; empty for `ReconcileReport`.
    pub record: Vec<u8>,
    /// SHA-256(account ‖ cmd ‖ cmd_ver ‖ payload); zero for `ReconcileReport`.
    pub cmd_hash: [u8; 32],
    /// `ConfigBlob` segments that must precede the command (≤ 64).
    pub blobs: Vec<BlobPart>,
    /// Decoded blob of `SetParams`/`SetRisk`/`ReplaceRefdata`.
    pub blob: Option<Vec<u8>>,
    /// `ReconcileReport` records in order (`idx` 0..count).
    pub reports: Vec<ReconcileReport>,
}

impl Normalized {
    /// The raw SPB1 bytes of a `SetParams` (handed to the gateway's container and body checks).
    pub fn params_blob(&self) -> Option<&[u8]> {
        (self.hdr.cmd == hft_event::CmdKind::SET_PARAMS)
            .then_some(self.blob.as_deref())
            .flatten()
    }

    /// Command payload bytes (after `CmdHdr`).
    pub fn payload(&self) -> &[u8] {
        self.record.get(HDR..).unwrap_or(&[])
    }

    /// Fills the gateway-owned header fields in `hdr` and `record` (not covered by `cmd_hash`).
    pub fn set_gateway_fields(&mut self, expires_tsc: u64, peer_uid: u32, role: u8) {
        self.hdr.expires_tsc = hft_types::Tsc(expires_tsc);
        self.hdr.peer_uid = peer_uid;
        self.hdr.role = role;
        put(&mut self.record, 56, &expires_tsc.to_le_bytes());
        put(&mut self.record, 64, &peer_uid.to_le_bytes());
        put(&mut self.record, 72, &[role]);
    }

    /// Ring bodies in write order: `ConfigBlob` segments, then the `ControlCmd` as one Multi
    /// segment (`PartHdr` + logical body); reconciliation uses report bodies or one receipt batch.
    pub fn ring_records(&self) -> Vec<(EventKind, RecordFlags, Vec<u8>)> {
        if self.kind == EventKind::RECONCILE_BATCH {
            return vec![(self.kind, RecordFlags(0), self.record.clone())];
        }
        if self.kind == EventKind::RECONCILE_REPORT {
            return self
                .reports
                .iter()
                .map(|r| (EventKind::RECONCILE_REPORT, RecordFlags(0), r.as_bytes().to_vec()))
                .collect();
        }
        let mut out: Vec<_> = self
            .blobs
            .iter()
            .map(|p| (EventKind::CONFIG_BLOB, p.flags, p.body.clone()))
            .collect();
        let ph = PartHdr {
            total_len: self.record.len() as u32,
            chunk_len: self.record.len() as u32,
            part: 0,
            parts: 1,
            _rsv: 0,
        };
        let mut body = ph.as_bytes().to_vec();
        body.extend_from_slice(&self.record);
        out.push((EventKind::CONTROL_CMD, RecordFlags(0), body));
        out
    }
}

const HDR: usize = 128;

fn put(b: &mut [u8], o: usize, v: &[u8]) {
    if let Some(d) = b.get_mut(o..o + v.len()) {
        d.copy_from_slice(v);
    }
}

/// `cmd_hash` = SHA-256(account (1 B) ‖ cmd (u16 LE) ‖ cmd_ver (u16 LE) ‖ payload).
pub fn cmd_hash(account: u8, cmd: u16, cmd_ver: u16, payload: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([account]);
    h.update(cmd.to_le_bytes());
    h.update(cmd_ver.to_le_bytes());
    h.update(payload);
    h.finalize().into()
}

// ---------------------------------------------------------------- ② account semantics

/// Account selection after combining the envelope and the in-body account.
#[derive(Clone, Eq, PartialEq, Debug)]
enum Acct {
    Missing,
    All,
    Name(String),
}

/// Per-command account rule (control-plane 账户语义第 9 条).
#[derive(Copy, Clone)]
enum Rule {
    /// A concrete account; missing or `*` → `AccountRequired`.
    Concrete,
    /// Only `*`; a concrete account → `AccountNotAllowed`; missing allowed iff `missing_ok`.
    Star { missing_ok: bool },
    /// Name or `*`; missing allowed iff `missing_ok` (downgrade commands).
    Either { missing_ok: bool },
}

fn combine(req: &CmdRequest) -> R<Acct> {
    let env = match &req.account {
        AccountSel::Missing => Acct::Missing,
        AccountSel::All => Acct::All,
        AccountSel::Name(n) => Acct::Name(n.clone()),
    };
    let body = match req.cmd.body_account() {
        None => Acct::Missing,
        Some("*") => Acct::All,
        Some(n) => Acct::Name(n.to_owned()),
    };
    match (env, body) {
        (Acct::Missing, b) => Ok(b),
        (e, Acct::Missing) => Ok(e),
        (e, b) if e == b => Ok(e),
        _ => err(detail::ACCOUNT_MISMATCH),
    }
}

fn apply(a: Acct, rule: Rule) -> R<Acct> {
    match (rule, &a) {
        (Rule::Concrete, Acct::Missing | Acct::All) => err(detail::ACCOUNT_REQUIRED),
        (Rule::Star { .. }, Acct::Name(_)) => err(detail::ACCOUNT_NOT_ALLOWED),
        (Rule::Star { missing_ok: false } | Rule::Either { missing_ok: false }, Acct::Missing) => {
            err(detail::ACCOUNT_REQUIRED)
        }
        _ => Ok(a),
    }
}

fn kill_combination(level: KillLevel, venue: Option<&str>, flatten: bool, hard: bool) -> R<()> {
    let bad = match level {
        KillLevel::Venue => hard || flatten || matches!(venue, None | Some("*")),
        KillLevel::Global => flatten || venue.is_some(),
        KillLevel::Account => (flatten && hard) || venue.is_some(),
    };
    if bad { err(detail::BAD_COMBINATION) } else { Ok(()) }
}

fn account_rule(cmd: &CmdBody) -> Rule {
    use CmdBody as B;
    match cmd {
        B::SetParams { .. }
        | B::HedgeFix { .. }
        | B::WriteOffResidual { .. }
        | B::SmokeOrder { .. }
        | B::AdoptExchange { .. }
        | B::SetBalanceBaseline { .. }
        | B::SimReset { .. }
        | B::SimTransfer { .. }
        | B::SimFault { .. }
        | B::RecordTransfer { .. }
        | B::StrategyCmd { .. }
        | B::RotateConn { .. } => Rule::Concrete,
        B::Kill { level, flatten, .. } => match level {
            KillLevel::Account => Rule::Concrete,
            KillLevel::Global => Rule::Star { missing_ok: true },
            KillLevel::Venue => Rule::Either {
                missing_ok: !flatten.unwrap_or(false),
            },
        },
        B::InjectWarmup { .. } | B::Drain { .. } | B::ReconcileReport { .. } => {
            Rule::Star { missing_ok: true }
        }
        B::VenueNotice { kind, .. } => Rule::Star {
            missing_ok: matches!(kind, NoticeKind::Delisting { .. }),
        },
        B::SetMode { mode, .. } => Rule::Either {
            missing_ok: *mode != hft_types::RunMode::Live,
        },
        B::CancelAll { .. } => Rule::Either { missing_ok: true },
        B::SetRisk { .. }
        | B::Resume { .. }
        | B::ReplaceRefdata { .. }
        | B::Annotate { .. }
        | B::Resubscribe { .. }
        | B::SnapshotNow { .. }
        | B::GetSchema { .. } => Rule::Either { missing_ok: false },
    }
}

// ---------------------------------------------------------------- ③ names

struct Rd<'a>(&'a RefData);

impl Rd<'_> {
    fn account(&self, a: &Acct) -> R<AccountId> {
        match a {
            Acct::Name(n) => names::account(self.0, n).ok_or(NormErr::new(detail::UNKNOWN_NAME)),
            _ => Ok(AccountId(0)),
        }
    }
    /// `None`/`"*"` → 0.
    fn strategy(&self, s: Option<&str>) -> R<StrategyId> {
        match s {
            None | Some("*") => Ok(StrategyId(0)),
            Some(n) => names::instance(self.0, n).ok_or(NormErr::new(detail::UNKNOWN_NAME)),
        }
    }
    fn venue_opt(&self, s: Option<&str>) -> R<VenueId> {
        match s {
            None | Some("*") => Ok(VenueId(0)),
            Some(n) => self.venue(n),
        }
    }
    fn venue(&self, s: &str) -> R<VenueId> {
        names::venue(self.0, s).ok_or(NormErr::new(detail::UNKNOWN_NAME))
    }
    fn inst_opt(&self, s: Option<&str>) -> R<InstId> {
        match s {
            None | Some("*") => Ok(InstId(0)),
            Some(n) => self.inst(n),
        }
    }
    fn inst(&self, s: &str) -> R<InstId> {
        names::inst(self.0, s).ok_or(NormErr::new(detail::UNKNOWN_NAME))
    }
    fn asset(&self, s: &str) -> R<AssetId> {
        names::asset(self.0, s).ok_or(NormErr::new(detail::UNKNOWN_NAME))
    }
    /// Account bound to the strategy when both are concrete.
    fn bound(&self, s: StrategyId, a: AccountId) -> R<()> {
        if s.0 != 0 && a.0 != 0 && self.0.account(a).bound_instance != s {
            return err(detail::ACCOUNT_BOUND);
        }
        Ok(())
    }

    /// A concrete group value: construction rule, instance present, and the instance equals
    /// `strategy` when that is concrete.
    fn concrete_group(&self, s: &str, strategy: StrategyId) -> R<GroupId> {
        let v: u32 = uint(s)?;
        let g = GroupId(v);
        let inst = g.strategy();
        if GroupId::compose(inst, g.counter()) != Some(g) || self.0.instance(inst).is_none() {
            return err(detail::BAD_GROUP);
        }
        if strategy.0 != 0 && strategy != inst {
            return err(detail::BAD_GROUP);
        }
        Ok(g)
    }
}

/// Group spelling rules per command.
#[derive(Copy, Clone)]
enum GroupRule {
    /// Missing/`*` → `GROUP_ALL`; `none` → `BadGroup`.
    AllOk,
    /// Must be concrete.
    Concrete,
    /// `none` → `GROUP_NONE`; missing/`*` → `BadGroup`.
    NoneOk,
    /// Missing/`none` → `GROUP_NONE`; `*` → `BadGroup`.
    NoneDefault,
}

fn group(rd: &Rd<'_>, s: Option<&str>, strategy: StrategyId, rule: GroupRule) -> R<GroupId> {
    match (rule, s) {
        (GroupRule::AllOk, None | Some("*")) => Ok(GROUP_ALL),
        (GroupRule::NoneOk, Some("none")) | (GroupRule::NoneDefault, None | Some("none")) => Ok(GROUP_NONE),
        (_, None | Some("*" | "none")) => err(detail::BAD_GROUP),
        (_, Some(v)) => rd.concrete_group(v, strategy),
    }
}

// ---------------------------------------------------------------- ④ numbers

/// Unsigned decimal integer of type `T`; fraction, sign or overflow → `BadValue`.
fn uint<T: TryFrom<u64>>(s: &str) -> R<T> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return err(detail::BAD_VALUE);
    }
    s.parse::<u64>()
        .ok()
        .and_then(|v| T::try_from(v).ok())
        .ok_or(NormErr::new(detail::BAD_VALUE))
}

fn int64(s: &str) -> R<i64> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return err(detail::BAD_VALUE);
    }
    s.parse::<i64>().map_err(|_| NormErr::new(detail::BAD_VALUE))
}

fn fixed(s: &str, exp: i8) -> R<i64> {
    parse_exact(s.as_bytes(), exp).map_err(|_| NormErr::new(detail::BAD_VALUE))
}

fn opt_fixed(s: &Option<String>, exp: i8) -> R<i64> {
    s.as_deref().map_or(Ok(0), |s| fixed(s, exp))
}

fn opt_uint<T: TryFrom<u64> + Default>(s: &Option<String>) -> R<T> {
    s.as_deref().map_or(Ok(T::default()), uint)
}

// ---------------------------------------------------------------- ⑤ payloads and blobs

fn b64(s: &str) -> R<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| NormErr::new(detail::BAD_VALUE))
}

fn blob(s: &str) -> R<Vec<u8>> {
    let b = b64(s)?;
    if b.len() > MAX_BLOB {
        return err(detail::TOO_LONG);
    }
    Ok(b)
}

fn sha(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

fn blob_parts(r: &BlobRef, scope: CmdScope, content: &[u8]) -> Vec<BlobPart> {
    let bh = BlobHdr {
        blob: *r,
        scope,
        boot: 0,
        _pad: [0; 7],
    };
    let mut logical = bh.as_bytes().to_vec();
    let prefix = logical.len();
    logical.extend_from_slice(content);
    let Ok(it) = PartIter::new(&logical) else {
        return Vec::new();
    };
    let mut skip = prefix;
    it.map(|(ph, chunk, flags)| {
        let off = skip.min(chunk.len());
        skip -= off;
        let mut body = ph.as_bytes().to_vec();
        body.extend_from_slice(chunk);
        BlobPart {
            flags,
            body,
            content_off: off,
        }
    })
    .collect()
}

fn scope(strategy: StrategyId, venue: VenueId, account: AccountId, group: GroupId, inst: InstId) -> CmdScope {
    CmdScope {
        strategy,
        venue,
        account,
        _pad: 0,
        group,
        inst,
        _pad2: 0,
    }
}

fn hex32(s: &str) -> R<[u8; 32]> {
    let b = s.as_bytes();
    if b.len() != 64 {
        return err(detail::BAD_VALUE);
    }
    let nib = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        let hi = b.get(2 * i).copied().and_then(nib);
        let lo = b.get(2 * i + 1).copied().and_then(nib);
        match (hi, lo) {
            (Some(h), Some(l)) => *o = (h << 4) | l,
            _ => return err(detail::BAD_VALUE),
        }
    }
    Ok(out)
}

struct Built<'p> {
    cmd: ControlCmd<'p>,
    account: AccountId,
    base_version: u64,
    blob: Option<(BlobRef, CmdScope, Vec<u8>)>,
}

fn simple(cmd: ControlCmd<'static>, account: AccountId) -> Built<'static> {
    Built {
        cmd,
        account,
        base_version: 0,
        blob: None,
    }
}

/// JSON command → `ControlCmd` (steps ②–⑤). `payload` is scratch for variable-length payloads.
fn build<'p>(req: &CmdRequest, rd: &Rd<'_>, payload: &'p mut Vec<u8>) -> R<Built<'p>> {
    use CmdBody as B;
    // ② account semantics (Kill's parameter combination first).
    if let B::Kill {
        level,
        venue,
        flatten,
        hard,
        ..
    } = &req.cmd
    {
        kill_combination(
            *level,
            venue.as_deref(),
            flatten.unwrap_or(false),
            hard.unwrap_or(false),
        )?;
    }
    let acct = apply(combine(req)?, account_rule(&req.cmd))?;
    // ③ the account name (other names per command below, before ④).
    let account = rd.account(&acct)?;
    let all = GROUP_ALL;
    Ok(match &req.cmd {
        B::SetMode {
            strategy,
            group: g,
            mode,
            ..
        } => {
            let s = rd.strategy(strategy.as_deref())?;
            rd.bound(s, account)?;
            let g = group(rd, g.as_deref(), s, GroupRule::AllOk)?;
            simple(
                ControlCmd::SetMode {
                    scope: scope(s, VenueId(0), account, g, InstId(0)),
                    mode: *mode,
                },
                account,
            )
        }
        B::SetParams {
            strategy,
            blob_b64,
            base_version,
            ..
        } => {
            let s = rd.strategy(Some(strategy))?;
            if s.0 == 0 {
                return err(detail::UNKNOWN_NAME);
            }
            rd.bound(s, account)?;
            let base_version = opt_uint::<u64>(base_version)?;
            let b = blob(blob_b64)?;
            if b.len() < SPB1_HDR_LEN {
                return err(detail::BAD_VALUE);
            }
            if b.first() != Some(&s.0) || b.get(1) != Some(&account.0) {
                return err(detail::SCOPE_MISMATCH);
            }
            let version = b
                .get(16..24)
                .and_then(|v| v.try_into().ok())
                .map_or(0, u64::from_le_bytes);
            let sc = scope(s, VenueId(0), account, all, InstId(0));
            let r = BlobRef {
                kind: BlobKind::StrategyParams,
                encoding: 1,
                len: b.len() as u32,
                version,
                sha256: sha(&b),
            };
            Built {
                cmd: ControlCmd::SetParams { scope: sc, blob: r },
                account,
                base_version,
                blob: Some((r, sc, b)),
            }
        }
        B::SetRisk {
            blob_b64,
            base_version,
            ..
        } => {
            let base_version = opt_uint::<u64>(base_version)?;
            let b = blob(blob_b64)?;
            let r = BlobRef {
                kind: BlobKind::RiskParams,
                encoding: 1,
                len: b.len() as u32,
                version: 0,
                sha256: sha(&b),
            };
            Built {
                cmd: ControlCmd::SetRisk { blob: r },
                account,
                base_version,
                blob: Some((r, scope(StrategyId(0), VenueId(0), account, all, InstId(0)), b)),
            }
        }
        B::ReplaceRefdata {
            blob_b64, version, ..
        } => {
            let version: u64 = uint(version)?;
            let b = blob(blob_b64)?;
            let r = BlobRef {
                kind: BlobKind::Refdata,
                encoding: 1,
                len: b.len() as u32,
                version,
                sha256: sha(&b),
            };
            Built {
                cmd: ControlCmd::ReplaceRefdata { blob: r },
                account,
                base_version: 0,
                blob: Some((r, scope(StrategyId(0), VenueId(0), account, all, InstId(0)), b)),
            }
        }
        B::Kill {
            level,
            venue,
            cancel_resting,
            flatten,
            hard,
            ..
        } => {
            let v = rd.venue_opt(venue.as_deref())?;
            simple(
                ControlCmd::Kill {
                    scope: scope(StrategyId(0), v, account, all, InstId(0)),
                    level: *level,
                    flatten: flatten.unwrap_or(false),
                    cancel_resting: cancel_resting.unwrap_or(true),
                    hard: hard.unwrap_or(false),
                },
                account,
            )
        }
        B::Resume {
            source,
            strategy,
            venue,
            group: g,
            inst,
            ..
        } => {
            let s = rd.strategy(strategy.as_deref())?;
            rd.bound(s, account)?;
            let v = rd.venue_opt(venue.as_deref())?;
            let i = rd.inst_opt(inst.as_deref())?;
            let g = group(rd, g.as_deref(), s, GroupRule::AllOk)?;
            simple(
                ControlCmd::Resume {
                    scope: scope(s, v, account, g, i),
                    source: CapSource(source.0),
                },
                account,
            )
        }
        B::CancelAll { venue, inst, .. } => {
            let v = rd.venue_opt(venue.as_deref())?;
            let i = rd.inst_opt(inst.as_deref())?;
            simple(
                ControlCmd::CancelAll {
                    scope: scope(StrategyId(0), v, account, all, i),
                },
                account,
            )
        }
        B::Drain { cancel_resting, .. } => simple(
            ControlCmd::Drain {
                cancel_resting: cancel_resting.unwrap_or(true),
            },
            account,
        ),
        B::HedgeFix { group: g, .. } => {
            let g = group(rd, g.as_deref(), StrategyId(0), GroupRule::Concrete)?;
            simple(
                ControlCmd::HedgeFix {
                    scope: scope(StrategyId(0), VenueId(0), account, g, InstId(0)),
                },
                account,
            )
        }
        B::WriteOffResidual { group: g, .. } => {
            let g = group(rd, g.as_deref(), StrategyId(0), GroupRule::Concrete)?;
            simple(
                ControlCmd::WriteOffResidual {
                    account,
                    _pad: [0; 3],
                    group: g,
                },
                account,
            )
        }
        B::SetBalanceBaseline { venue, .. } => {
            let v = rd.venue(venue)?;
            simple(ControlCmd::SetBalanceBaseline { account, venue: v }, account)
        }
        B::SimReset { .. } => simple(ControlCmd::SimReset { account }, account),
        B::SimTransfer {
            from_venue,
            to_venue,
            asset,
            amount,
            ..
        } => {
            let from = rd.venue(from_venue)?;
            let to = rd.venue(to_venue)?;
            let a = rd.asset(asset)?;
            let amount = Qty(fixed(amount, rd.0.asset(a).exp.qty_exp)?);
            simple(
                ControlCmd::SimTransfer {
                    account,
                    from,
                    to,
                    asset: a,
                    amount,
                },
                account,
            )
        }
        B::SimFault {
            venue,
            fault,
            delay_ms,
            ..
        } => {
            let v = rd.venue(venue)?;
            let delay: u32 = opt_uint(delay_ms)?;
            let ok = match fault {
                SimFaultKind::DelayReports => (1..=SIM_FAULT_DELAY_MAX_MS).contains(&delay),
                _ => delay == 0,
            };
            if !ok {
                return err(detail::BAD_VALUE);
            }
            simple(
                ControlCmd::SimFault {
                    account,
                    venue: v,
                    fault: *fault as u8,
                    _pad: 0,
                    delay_ms: delay,
                },
                account,
            )
        }
        B::RecordTransfer {
            from,
            to,
            asset,
            amount,
            ext_ref,
            ..
        } => {
            let from = rd.venue(from)?;
            let to = rd.venue(to)?;
            let a = rd.asset(asset)?;
            let amount = Qty(fixed(amount, rd.0.asset(a).exp.qty_exp)?);
            if ext_ref.len() > TRANSFER_REF_MAX {
                return err(detail::TOO_LONG);
            }
            let mut reference = [0u8; 64];
            put(&mut reference, 0, ext_ref.as_bytes());
            simple(
                ControlCmd::RecordTransfer {
                    account,
                    from,
                    to,
                    asset: a,
                    amount,
                    reference,
                },
                account,
            )
        }
        B::Annotate {
            kind, text, digest, ..
        } => {
            let digest = match digest {
                Some(d) => hex32(d)?,
                None => [0; 32],
            };
            if text.len() > ANNOTATE_TEXT_MAX {
                return err(detail::TOO_LONG);
            }
            let mut t = [0u8; 256];
            put(&mut t, 0, text.as_bytes());
            simple(
                ControlCmd::Annotate {
                    kind: *kind as u16,
                    text_len: text.len() as u16,
                    digest,
                    text: t,
                },
                account,
            )
        }
        B::AdoptExchange {
            venue,
            report_id,
            item,
            inst,
            asset,
            group: g,
            entry_px,
            ..
        } => {
            let v = rd.venue(venue)?;
            let (i, a) = match item {
                AdoptItem::Position => (rd.inst(inst.as_deref().unwrap_or(""))?, AssetId(0)),
                AdoptItem::Balance => (InstId(0), rd.asset(asset.as_deref().unwrap_or(""))?),
            };
            let g = group(rd, g.as_deref(), StrategyId(0), GroupRule::NoneDefault)?;
            let report_id: u64 = uint(report_id)?;
            let px_asset = if i.0 != 0 { rd.0.inst(i).asset } else { a };
            let entry_px = Px(opt_fixed(entry_px, rd.0.asset(px_asset).exp.px_exp)?);
            simple(
                ControlCmd::AdoptExchange {
                    account,
                    venue: v,
                    inst: i,
                    report_id,
                    item: *item as u8,
                    _pad: 0,
                    asset: a,
                    group: g,
                    entry_px,
                },
                account,
            )
        }
        B::VenueNotice {
            venue, inst, kind, ..
        } => {
            let v = rd.venue(venue)?;
            let i = rd.inst_opt(inst.as_deref())?;
            let (k, a, b) = match kind {
                NoticeKind::Delisting { at_ns } => (1, int64(at_ns)?, 0),
                NoticeKind::FundingInterval { new_s } => (2, int64(new_s)?, 0),
                NoticeKind::Maintenance { from_ns, to_ns } => (3, int64(from_ns)?, int64(to_ns)?),
            };
            simple(
                ControlCmd::VenueNotice {
                    venue: v,
                    kind: k,
                    _pad: 0,
                    inst: i,
                    a,
                    b,
                },
                account,
            )
        }
        B::SmokeOrder {
            venue,
            inst,
            side,
            ttl_ms,
            ..
        } => {
            let v = rd.venue(venue)?;
            let i = rd.inst(inst)?;
            let ttl: u32 = match ttl_ms {
                Some(t) => uint(t)?,
                None => SMOKE_TTL_DEFAULT_MS,
            };
            if !(1..=SMOKE_TTL_MAX_MS).contains(&ttl) {
                return err(detail::BAD_VALUE);
            }
            simple(
                ControlCmd::SmokeOrder {
                    account,
                    venue: v,
                    _pad: 0,
                    inst: i,
                    side: *side,
                    _pad2: [0; 3],
                    ttl_ms: ttl,
                },
                account,
            )
        }
        B::StrategyCmd {
            strategy,
            group: g,
            reduce_only,
            payload_b64,
            ..
        } => {
            let s = rd.strategy(Some(strategy))?;
            if s.0 == 0 {
                return err(detail::UNKNOWN_NAME);
            }
            rd.bound(s, account)?;
            let g = group(rd, g.as_deref(), s, GroupRule::NoneOk)?;
            let p = b64(payload_b64)?;
            if p.len() > STRATEGY_CMD_MAX_PAYLOAD {
                return err(detail::TOO_LONG);
            }
            let mut buf = [0u8; 256];
            put(&mut buf, 0, &p);
            simple(
                ControlCmd::StrategyCmd {
                    instance: s,
                    reduce_only: u8::from(reduce_only.unwrap_or(false)),
                    len: p.len() as u16,
                    group: g,
                    payload: buf,
                },
                account,
            )
        }
        B::InjectWarmup {
            strategy,
            local_kind,
            chunk_no,
            last,
            payload_b64,
            ..
        } => {
            let s = rd.strategy(Some(strategy))?;
            if s.0 == 0 {
                return err(detail::UNKNOWN_NAME);
            }
            let local_kind: u16 = uint(local_kind)?;
            let chunk_no: u32 = uint(chunk_no)?;
            *payload = b64(payload_b64)?;
            if payload.len() > WARMUP_MAX_PAYLOAD {
                return err(detail::TOO_LONG);
            }
            let len = payload.len() as u16;
            Built {
                cmd: ControlCmd::InjectWarmup {
                    instance: s,
                    last: u8::from(last.unwrap_or(false)),
                    local_kind,
                    chunk_no,
                    len,
                    _pad: [0; 6],
                    payload: payload.as_slice(),
                },
                account,
                base_version: 0,
                blob: None,
            }
        }
        B::RotateConn { venue, .. } => {
            let v = rd.venue(venue)?;
            simple(ControlCmd::RotateConn { venue: v, account }, account)
        }
        B::Resubscribe { venue, inst, .. } => {
            let v = rd.venue(venue)?;
            let i = rd.inst_opt(inst.as_deref())?;
            simple(ControlCmd::Resubscribe { venue: v, inst: i }, account)
        }
        B::SnapshotNow { .. } => simple(ControlCmd::SnapshotNow, account),
        B::GetSchema { .. } | B::ReconcileReport { .. } => return err(detail::UNKNOWN_DISCRIMINANT),
    })
}

fn issuer_bytes(s: &str) -> [u8; 24] {
    let mut b = [0u8; 24];
    put(&mut b, 0, s.as_bytes());
    b
}

fn report_item(rd: &Rd<'_>, report_id: u64, idx: u16, count: u16, it: &ReconItem) -> R<ReconcileReport> {
    let venue = rd.venue_opt(it.venue.as_deref())?;
    let account = match it.account.as_deref() {
        None | Some("*") => AccountId(0),
        Some(n) => rd.account(&Acct::Name(n.to_owned()))?,
    };
    let inst = rd.inst_opt(it.inst.as_deref())?;
    let asset = match it.asset.as_deref() {
        None => AssetId(0),
        Some(a) => rd.asset(a)?,
    };
    let qty_asset = if inst.0 != 0 { rd.0.inst(inst).asset } else { asset };
    let qexp = if qty_asset.0 != 0 {
        rd.0.asset(qty_asset).exp.qty_exp
    } else {
        0
    };
    let mut sflags = 0u16;
    for n in it.sflags.iter().flatten() {
        let bit = hft_event::codes::RECON_SFLAGS
            .items
            .iter()
            .find(|i| i.name == n)
            .map_or(0, |i| i.value);
        sflags |= bit as u16;
    }
    Ok(ReconcileReport {
        report_id,
        venue,
        account,
        item: it.item,
        severity: it.severity.unwrap_or(0),
        inst,
        asset,
        idx,
        count,
        _pad: 0,
        engine_qty: Qty(opt_fixed(&it.engine_qty, qexp)?),
        exch_qty: Qty(opt_fixed(&it.exch_qty, qexp)?),
        diff: Notional(opt_fixed(&it.diff, NOTIONAL_EXP)?),
        exch_ts_ns: it.exch_ts_ns.as_deref().map_or(Ok(0), int64)?,
        slot: SlotId(opt_uint(&it.slot)?),
        order_status: it.order_status.unwrap_or(0),
        history_checked: u8::from(it.history_checked.unwrap_or(false)),
        _pad2: [0; 2],
        view_seq: opt_uint(&it.view_seq)?,
        r#gen: opt_uint(&it.r#gen)?,
        epoch: opt_uint(&it.epoch)?,
        _pad3: 0,
        exch_amount: Notional(opt_fixed(&it.exch_amount, NOTIONAL_EXP)?),
        engine_amount: Notional(opt_fixed(&it.engine_amount, NOTIONAL_EXP)?),
        sflags,
        skipped_inflight: opt_uint(&it.skipped_inflight)?,
        _pad4: 0,
    })
}

fn normalize_reports(
    req: &CmdRequest,
    rd: &Rd<'_>,
    report_id: &str,
    items: &[ReconItem],
    receipts: Option<&str>,
) -> R<Normalized> {
    apply(combine(req)?, Rule::Star { missing_ok: true })?;
    let report_id: u64 = uint(report_id)?;
    let count = u16::try_from(items.len()).map_err(|_| NormErr::new(detail::TOO_LONG))?;
    let mut reports = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        reports.push(report_item(rd, report_id, i as u16, count, it)?);
    }
    let mut record = Vec::new();
    if let Some(receipts) = receipts {
        let receipts = b64(receipts)?;
        if receipts.is_empty()
            || !receipts.len().is_multiple_of(96)
            || receipts.len() / 96 > hft_event::RECONCILE_BATCH_MAX_ITEMS
            || reports.len() > hft_event::RECONCILE_BATCH_MAX_ITEMS
        {
            return err(detail::BAD_VALUE);
        }
        let first = reports.first().ok_or_else(|| NormErr::new(detail::BAD_VALUE))?;
        let h = hft_event::ReconcileBatchHdr {
            report_id,
            n_reports: count,
            n_fills: (receipts.len() / 96) as u16,
            venue: first.venue,
            account: first.account,
            _pad: 0,
        };
        record.extend_from_slice(h.as_bytes());
        for r in &reports {
            record.extend_from_slice(r.as_bytes());
        }
        record.extend_from_slice(&receipts);
        let batch =
            hft_event::ReconcileBatchView::read(&record).map_err(|_| NormErr::new(detail::BAD_VALUE))?;
        for e in batch.fills() {
            // Fill semantics the engine's settlement preflight would reject anyway: refuse them
            // at the control plane so a malformed receipt never becomes an Input.
            let identity = if e.trade_id != 0 {
                e.fill_seq == 0
            } else {
                e.exch_order_id != 0 && e.fill_seq != 0
            };
            if e.qty.0 <= 0
                || e.px.0 <= 0
                || e.cash_qty != e.qty
                || e._pad != [0; 6]
                || e.account != h.account
                || e.venue != h.venue
                || !identity
                || !matches!(
                    e.cause,
                    hft_event::ExternalCause::Adl
                        | hft_event::ExternalCause::Liquidation
                        | hft_event::ExternalCause::Unknown
                )
            {
                return err(detail::BAD_VALUE);
            }
            if e.inst.0 == 0
                || rd.0.inst(e.inst).venue != e.venue
                || !rd
                    .0
                    .caps(e.venue)
                    .flags
                    .contains(hft_refdata::CapFlags::PERP_CASH)
            {
                return err(detail::BAD_VALUE);
            }
        }
    }
    Ok(Normalized {
        kind: if receipts.is_some() {
            EventKind::RECONCILE_BATCH
        } else {
            EventKind::RECONCILE_REPORT
        },
        hdr: CmdHdr::zeroed(),
        record,
        cmd_hash: [0; 32],
        blobs: Vec::new(),
        blob: None,
        reports,
    })
}

/// Normalizes a schema-valid request (README "实现说明" steps ①–⑥). `GetSchema` is not a
/// command (`UnknownDiscriminant`).
pub fn normalize(req: &CmdRequest, rd: &RefData) -> Result<Normalized, NormErr> {
    let rd = Rd(rd);
    if let CmdBody::ReconcileReport {
        report_id,
        items,
        receipts_b64,
    } = &req.cmd
    {
        return normalize_reports(req, &rd, report_id, items, receipts_b64.as_deref());
    }
    let mut scratch = Vec::new();
    let b = build(req, &rd, &mut scratch)?;
    // ⑥ envelope.
    let mut hdr = CmdHdr::zeroed();
    hdr.cmd_id = *req.cmd_id.as_bytes();
    hdr.issuer = issuer_bytes(&req.issuer);
    hdr.issued_at_ns = req.issued_at_ns;
    hdr.expires_at_ns = req.expires_at_ns;
    hdr.account = b.account;
    hdr.confirmed = u8::from(req.confirmed);
    hdr.hflags = if req.preview { CmdHdr::PREVIEW } else { 0 };
    hdr.base_version = b.base_version;
    hdr.expect_state_version = req.expect_state_version.unwrap_or(0);
    let mut buf = vec![0u8; HDR + 16 + WARMUP_MAX_PAYLOAD + 8];
    let n = b
        .cmd
        .encode(&hdr, &mut buf)
        .map_err(|_| NormErr::new(detail::TOO_LONG))?;
    buf.truncate(n);
    let cmd = b.cmd.kind().0;
    let ver = b.cmd.version();
    let h = cmd_hash(b.account.0, cmd, ver, buf.get(HDR..).unwrap_or(&[]));
    put(&mut buf, 80, &h);
    hdr.cmd = b.cmd.kind();
    hdr.cmd_ver = ver;
    hdr.payload_len = (n - HDR) as u32;
    hdr.cmd_hash = h;
    let (blobs, blob) = match b.blob {
        Some((r, sc, content)) => (blob_parts(&r, sc, &content), Some(content)),
        None => (Vec::new(), None),
    };
    Ok(Normalized {
        kind: EventKind::CONTROL_CMD,
        hdr,
        record: buf,
        cmd_hash: h,
        blobs,
        blob,
        reports: Vec::new(),
    })
}
