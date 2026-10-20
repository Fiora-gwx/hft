//! `ControlCmd` (0x0401, Multi): `CmdHdr`, `CmdKind`, `CmdScope`, `BlobRef` and the typed command
//! view with its encoder/decoder. Semantics and permissions belong to control-plane / hft-ctl-proto.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{
    AccountId, AssetId, CapSource, GroupId, InstId, KillLevel, Px, Qty, RunMode, Side, StrategyId, Tsc,
    VenueId,
};
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

use crate::bytes::{put, round8, u8_at, u16_at, u32_at, u64_at};
use crate::codes;
use crate::macros::layout;
use crate::schema::FieldTy;
use crate::var::BlobKind;
use crate::{DecodeErr, EncodeErr, EventKind};

/// Command number (append-only, never reused; 0x0005 reserved, 0x000B–0x000D retired).
#[repr(transparent)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct CmdKind(pub u16);

impl CmdKind {
    pub const SET_MODE: Self = CmdKind(0x0001);
    pub const SET_PARAMS: Self = CmdKind(0x0002);
    pub const SET_RISK: Self = CmdKind(0x0003);
    pub const KILL: Self = CmdKind(0x0004);
    pub const ROTATE_CONN: Self = CmdKind(0x0006);
    pub const RESUBSCRIBE: Self = CmdKind(0x0007);
    pub const SNAPSHOT_NOW: Self = CmdKind(0x0008);
    pub const REPLACE_REFDATA: Self = CmdKind(0x0009);
    pub const RESUME: Self = CmdKind(0x000A);
    pub const HEDGE_FIX: Self = CmdKind(0x000E);
    pub const CANCEL_ALL: Self = CmdKind(0x000F);
    pub const DRAIN: Self = CmdKind(0x0010);
    pub const RECORD_TRANSFER: Self = CmdKind(0x0011);
    pub const ANNOTATE: Self = CmdKind(0x0012);
    pub const ADOPT_EXCHANGE: Self = CmdKind(0x0013);
    pub const SIM_RESET: Self = CmdKind(0x0014);
    pub const SIM_TRANSFER: Self = CmdKind(0x0015);
    pub const VENUE_NOTICE: Self = CmdKind(0x0016);
    pub const SET_BALANCE_BASELINE: Self = CmdKind(0x0017);
    pub const STRATEGY_CMD: Self = CmdKind(0x0018);
    pub const INJECT_WARMUP: Self = CmdKind(0x0019);
    pub const SMOKE_ORDER: Self = CmdKind(0x001A);
    pub const WRITE_OFF_RESIDUAL: Self = CmdKind(0x001B);
    pub const SIM_FAULT: Self = CmdKind(0x001C);
    pub const BIND_SLOT: Self = CmdKind(0x001D);

    /// Name of an assigned number; reserved, retired and unassigned → `None`.
    pub fn name(self) -> Option<&'static str> {
        codes::CMD_KIND
            .items
            .iter()
            .find(|i| i.value == u32::from(self.0))
            .map(|i| i.name)
    }
}

crate::event_body! {
    /// Command scope (16 B): strategy/venue/account/inst 0 = all; group `GROUP_ALL` = all,
    /// `GROUP_NONE` is illegal here.
    pub struct CmdScope : plain, FromBytes {
        pub strategy: StrategyId,
        pub venue: VenueId,
        pub account: AccountId,
        pub _pad: u8,
        pub group: GroupId,
        pub inst: InstId,
        pub _pad2: u32,
    }
}
layout!(CmdScope, 16, 4; strategy @ 0, venue @ 1, account @ 2, _pad @ 3, group @ 4, inst @ 8, _pad2 @ 12);

crate::event_body! {
    /// Reference to a `ConfigBlob` content (48 B).
    pub struct BlobRef : plain, TryFromBytes {
        pub kind: BlobKind,
        pub encoding: u16,
        pub len: u32,
        pub version: u64,
        pub sha256: [u8; 32],
    }
}
layout!(BlobRef, 48, 8; kind @ 0, encoding @ 2, len @ 4, version @ 8, sha256 @ 16);

crate::event_body! {
    /// Command envelope (128 B) at the start of the `ControlCmd` logical body.
    pub struct CmdHdr : multi(EventKind::CONTROL_CMD, 1), FromBytes {
        /// UUID chosen by the client; retries reuse it.
        pub cmd_id: [u8; 16],
        /// ASCII, right-padded with 0.
        pub issuer: [u8; 24],
        pub issued_at_ns: i64,
        pub expires_at_ns: i64,
        pub expires_tsc: Tsc,
        pub peer_uid: u32,
        pub cmd: CmdKind,
        pub cmd_ver: u16,
        pub role: u8 = FieldTy::Enum(&codes::CMD_ROLE),
        /// 0 = "*".
        pub account: AccountId,
        pub confirmed: u8,
        pub hflags: u8 = FieldTy::Bits(&codes::CMD_HFLAGS),
        pub payload_len: u32,
        pub cmd_hash: [u8; 32],
        pub base_version: u64,
        pub expect_state_version: u64,
    }
}
layout!(CmdHdr, 128, 8; cmd_id @ 0, issuer @ 16, issued_at_ns @ 40, expires_at_ns @ 48, expires_tsc @ 56,
    peer_uid @ 64, cmd @ 68, cmd_ver @ 70, role @ 72, account @ 73, confirmed @ 74, hflags @ 75, payload_len @ 76,
    cmd_hash @ 80, base_version @ 112, expect_state_version @ 120);

impl CmdHdr {
    pub const ROLE_VIEWER: u8 = 1;
    pub const ROLE_TRADER: u8 = 2;
    pub const ROLE_ADMIN: u8 = 4;
    /// `hflags`: preview only.
    pub const PREVIEW: u8 = 1;

    /// All-zero envelope (callers fill the fields they use).
    pub fn zeroed() -> CmdHdr {
        <CmdHdr as zerocopy::FromZeros>::new_zeroed()
    }
}

/// Upper bound of `StrategyCmd.len`.
pub const STRATEGY_CMD_MAX_PAYLOAD: usize = 256;
/// Upper bound of `InjectWarmup.len` (the whole record stays single-part, ≤ 4 288 B).
pub const WARMUP_MAX_PAYLOAD: usize = 4096;

const HDR: usize = 128;

/// Typed command view; payload layout = fields in order + explicit padding to a multiple of 8
/// (README "控制命令编码"). Implicit padding bytes must be 0.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ControlCmd<'a> {
    SetMode {
        scope: CmdScope,
        mode: RunMode,
    },
    SetParams {
        scope: CmdScope,
        blob: BlobRef,
    },
    SetRisk {
        blob: BlobRef,
    },
    Kill {
        scope: CmdScope,
        level: KillLevel,
        flatten: bool,
        cancel_resting: bool,
        hard: bool,
    },
    RotateConn {
        venue: VenueId,
        account: AccountId,
    },
    Resubscribe {
        venue: VenueId,
        /// 0 = all.
        inst: InstId,
    },
    SnapshotNow,
    ReplaceRefdata {
        blob: BlobRef,
    },
    Resume {
        scope: CmdScope,
        source: CapSource,
    },
    HedgeFix {
        scope: CmdScope,
    },
    CancelAll {
        scope: CmdScope,
    },
    Drain {
        cancel_resting: bool,
    },
    RecordTransfer {
        account: AccountId,
        from: VenueId,
        to: VenueId,
        asset: AssetId,
        amount: Qty,
        reference: [u8; 64],
    },
    Annotate {
        kind: u16,
        text_len: u16,
        digest: [u8; 32],
        text: [u8; 256],
    },
    AdoptExchange {
        account: AccountId,
        venue: VenueId,
        inst: InstId,
        report_id: u64,
        item: u8,
        _pad: u8,
        asset: AssetId,
        group: GroupId,
        entry_px: Px,
    },
    SimReset {
        account: AccountId,
    },
    SimTransfer {
        account: AccountId,
        from: VenueId,
        to: VenueId,
        asset: AssetId,
        amount: Qty,
    },
    VenueNotice {
        venue: VenueId,
        kind: u8,
        _pad: u16,
        inst: InstId,
        a: i64,
        b: i64,
    },
    SetBalanceBaseline {
        account: AccountId,
        venue: VenueId,
    },
    StrategyCmd {
        instance: StrategyId,
        reduce_only: u8,
        len: u16,
        group: GroupId,
        payload: [u8; 256],
    },
    InjectWarmup {
        instance: StrategyId,
        last: u8,
        local_kind: u16,
        chunk_no: u32,
        len: u16,
        _pad: [u8; 6],
        payload: &'a [u8],
    },
    SmokeOrder {
        account: AccountId,
        venue: VenueId,
        _pad: u16,
        inst: InstId,
        side: Side,
        _pad2: [u8; 3],
        ttl_ms: u32,
    },
    WriteOffResidual {
        account: AccountId,
        _pad: [u8; 3],
        group: GroupId,
    },
    SimFault {
        account: AccountId,
        venue: VenueId,
        fault: u8,
        _pad: u8,
        delay_ms: u32,
    },
    /// Binds a slot inst of a run-time-bound venue to an exchange wire id (Polymarket outcome
    /// token: uint256, big-endian; all zero = unbind) for the window `[start_s, end_s)` (UTC s).
    BindSlot {
        venue: VenueId,
        _pad: [u8; 3],
        inst: InstId,
        start_s: i64,
        end_s: i64,
        wire_id: [u8; 32],
    },
}

/// Fixed payload size of `(cmd, cmd_ver)`; `None` = unknown command or version.
/// `InjectWarmup` reports its 16 B prefix (the real size is `16 + round8(len)`).
fn payload_size(cmd: CmdKind, ver: u16) -> Option<usize> {
    let n = match (cmd, ver) {
        (CmdKind::SET_MODE, 1) => 24,
        (CmdKind::SET_PARAMS, 1) => 64,
        (CmdKind::SET_RISK | CmdKind::REPLACE_REFDATA, 1) => 48,
        (CmdKind::KILL, 1 | 2) => 24,
        (CmdKind::ROTATE_CONN | CmdKind::RESUBSCRIBE, 1) => 8,
        (CmdKind::SNAPSHOT_NOW, 1) => 0,
        (CmdKind::RESUME, 1) => 24,
        (CmdKind::HEDGE_FIX | CmdKind::CANCEL_ALL, 1) => 16,
        (CmdKind::DRAIN, 1) => 8,
        (CmdKind::RECORD_TRANSFER, 1) => 80,
        (CmdKind::ANNOTATE, 1) => 296,
        (CmdKind::ADOPT_EXCHANGE, 1) => 16,
        (CmdKind::ADOPT_EXCHANGE, 2) => 32,
        (CmdKind::SIM_RESET, 1) => 8,
        (CmdKind::SIM_TRANSFER, 1) => 16,
        (CmdKind::VENUE_NOTICE, 1) => 24,
        (CmdKind::SET_BALANCE_BASELINE, 1) => 8,
        (CmdKind::STRATEGY_CMD, 1) => 264,
        (CmdKind::INJECT_WARMUP, 1) => 16,
        (CmdKind::SMOKE_ORDER, 1) => 16,
        (CmdKind::WRITE_OFF_RESIDUAL | CmdKind::SIM_FAULT, 1) => 8,
        (CmdKind::BIND_SLOT, 1) => 56,
        _ => return None,
    };
    Some(n)
}

/// Payload writer over a zeroed payload slice (out-of-range writes are dropped; sizes are
/// checked before writing).
struct W<'b>(&'b mut [u8]);

impl W<'_> {
    fn b(&mut self, o: usize, v: &[u8]) {
        put(self.0, o, v);
    }
    fn u8(&mut self, o: usize, v: u8) {
        self.b(o, &[v]);
    }
    fn u16(&mut self, o: usize, v: u16) {
        self.b(o, &v.to_le_bytes());
    }
    fn u32(&mut self, o: usize, v: u32) {
        self.b(o, &v.to_le_bytes());
    }
    fn u64(&mut self, o: usize, v: u64) {
        self.b(o, &v.to_le_bytes());
    }
    fn i64(&mut self, o: usize, v: i64) {
        self.b(o, &v.to_le_bytes());
    }
}

/// Payload reader (lengths are checked before reading).
struct R<'b>(&'b [u8]);

impl<'b> R<'b> {
    fn u8(&self, o: usize) -> u8 {
        u8_at(self.0, o)
    }
    fn u16(&self, o: usize) -> u16 {
        u16_at(self.0, o)
    }
    fn u32(&self, o: usize) -> u32 {
        u32_at(self.0, o)
    }
    fn u64(&self, o: usize) -> u64 {
        u64_at(self.0, o)
    }
    fn i64(&self, o: usize) -> i64 {
        u64_at(self.0, o) as i64
    }
    fn slice(&self, o: usize, n: usize) -> &'b [u8] {
        self.0.get(o..o.saturating_add(n)).unwrap_or(&[])
    }
    fn arr<const N: usize>(&self, o: usize) -> [u8; N] {
        self.slice(o, N).try_into().unwrap_or([0; N])
    }
    fn bool(&self, o: usize) -> Result<bool, DecodeErr> {
        match self.u8(o) {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeErr::InvalidEnum),
        }
    }
    fn flag01(&self, o: usize) -> Result<u8, DecodeErr> {
        self.bool(o).map(u8::from)
    }
    fn code(&self, o: usize, legal: core::ops::RangeInclusive<u8>) -> Result<u8, DecodeErr> {
        let v = self.u8(o);
        if legal.contains(&v) {
            Ok(v)
        } else {
            Err(DecodeErr::InvalidEnum)
        }
    }
    fn zero(&self, from: usize, to: usize) -> Result<(), DecodeErr> {
        if self.0.get(from..to).is_some_and(|s| s.iter().all(|&b| b == 0)) {
            Ok(())
        } else {
            Err(DecodeErr::InvalidEnum)
        }
    }
    fn scope(&self, o: usize) -> CmdScope {
        CmdScope::read_from_bytes(self.slice(o, 16))
            .unwrap_or_else(|_| <CmdScope as zerocopy::FromZeros>::new_zeroed())
    }
    fn blob(&self, o: usize) -> Result<BlobRef, DecodeErr> {
        BlobRef::try_read_from_bytes(self.slice(o, 48)).map_err(|_| DecodeErr::InvalidEnum)
    }
}

impl<'a> ControlCmd<'a> {
    /// Command number of this variant.
    pub fn kind(&self) -> CmdKind {
        match self {
            ControlCmd::SetMode { .. } => CmdKind::SET_MODE,
            ControlCmd::SetParams { .. } => CmdKind::SET_PARAMS,
            ControlCmd::SetRisk { .. } => CmdKind::SET_RISK,
            ControlCmd::Kill { .. } => CmdKind::KILL,
            ControlCmd::RotateConn { .. } => CmdKind::ROTATE_CONN,
            ControlCmd::Resubscribe { .. } => CmdKind::RESUBSCRIBE,
            ControlCmd::SnapshotNow => CmdKind::SNAPSHOT_NOW,
            ControlCmd::ReplaceRefdata { .. } => CmdKind::REPLACE_REFDATA,
            ControlCmd::Resume { .. } => CmdKind::RESUME,
            ControlCmd::HedgeFix { .. } => CmdKind::HEDGE_FIX,
            ControlCmd::CancelAll { .. } => CmdKind::CANCEL_ALL,
            ControlCmd::Drain { .. } => CmdKind::DRAIN,
            ControlCmd::RecordTransfer { .. } => CmdKind::RECORD_TRANSFER,
            ControlCmd::Annotate { .. } => CmdKind::ANNOTATE,
            ControlCmd::AdoptExchange { .. } => CmdKind::ADOPT_EXCHANGE,
            ControlCmd::SimReset { .. } => CmdKind::SIM_RESET,
            ControlCmd::SimTransfer { .. } => CmdKind::SIM_TRANSFER,
            ControlCmd::VenueNotice { .. } => CmdKind::VENUE_NOTICE,
            ControlCmd::SetBalanceBaseline { .. } => CmdKind::SET_BALANCE_BASELINE,
            ControlCmd::StrategyCmd { .. } => CmdKind::STRATEGY_CMD,
            ControlCmd::InjectWarmup { .. } => CmdKind::INJECT_WARMUP,
            ControlCmd::SmokeOrder { .. } => CmdKind::SMOKE_ORDER,
            ControlCmd::WriteOffResidual { .. } => CmdKind::WRITE_OFF_RESIDUAL,
            ControlCmd::SimFault { .. } => CmdKind::SIM_FAULT,
            ControlCmd::BindSlot { .. } => CmdKind::BIND_SLOT,
        }
    }

    /// Current payload version written by `encode` (`Kill` 2, `AdoptExchange` 2, others 1).
    pub fn version(&self) -> u16 {
        match self {
            ControlCmd::Kill { .. } | ControlCmd::AdoptExchange { .. } => 2,
            _ => 1,
        }
    }

    fn payload_len(&self) -> Result<usize, EncodeErr> {
        match self {
            ControlCmd::StrategyCmd { len, .. } if usize::from(*len) > STRATEGY_CMD_MAX_PAYLOAD => {
                Err(EncodeErr::TooLong)
            }
            ControlCmd::Annotate { text_len, .. } if usize::from(*text_len) > 256 => Err(EncodeErr::TooLong),
            ControlCmd::InjectWarmup { len, payload, .. } => {
                if usize::from(*len) > WARMUP_MAX_PAYLOAD || payload.len() != usize::from(*len) {
                    Err(EncodeErr::TooLong)
                } else {
                    Ok(16 + round8(payload.len()))
                }
            }
            _ => payload_size(self.kind(), self.version()).ok_or(EncodeErr::Count),
        }
    }

    /// Writes `CmdHdr` + payload (the logical body). `cmd`, `cmd_ver` and `payload_len` come from
    /// `self` (ignored in `hdr`). Returns the logical body length `128 + payload_len`.
    pub fn encode(&self, hdr: &CmdHdr, out: &mut [u8]) -> Result<usize, EncodeErr> {
        let n = self.payload_len()?;
        let total = HDR + n;
        let Some(dst) = out.get_mut(..total) else {
            return Err(EncodeErr::BufTooSmall);
        };
        let mut h = *hdr;
        h.cmd = self.kind();
        h.cmd_ver = self.version();
        h.payload_len = n as u32;
        let (hd, pd) = dst.split_at_mut(HDR);
        hd.copy_from_slice(h.as_bytes());
        pd.fill(0);
        let mut w = W(pd);
        match self {
            ControlCmd::SetMode { scope, mode } => {
                w.b(0, scope.as_bytes());
                w.u8(16, *mode as u8);
            }
            ControlCmd::SetParams { scope, blob } => {
                w.b(0, scope.as_bytes());
                w.b(16, blob.as_bytes());
            }
            ControlCmd::SetRisk { blob } | ControlCmd::ReplaceRefdata { blob } => w.b(0, blob.as_bytes()),
            ControlCmd::Kill {
                scope,
                level,
                flatten,
                cancel_resting,
                hard,
            } => {
                w.b(0, scope.as_bytes());
                w.u8(16, *level as u8);
                w.u8(17, u8::from(*flatten));
                w.u8(18, u8::from(*cancel_resting));
                w.u8(19, u8::from(*hard));
            }
            ControlCmd::RotateConn { venue, account } => {
                w.u8(0, venue.0);
                w.u8(1, account.0);
            }
            ControlCmd::Resubscribe { venue, inst } => {
                w.u8(0, venue.0);
                w.u32(4, inst.0);
            }
            ControlCmd::SnapshotNow => {}
            ControlCmd::Resume { scope, source } => {
                w.b(0, scope.as_bytes());
                w.u8(16, source.0);
            }
            ControlCmd::HedgeFix { scope } | ControlCmd::CancelAll { scope } => w.b(0, scope.as_bytes()),
            ControlCmd::Drain { cancel_resting } => w.u8(0, u8::from(*cancel_resting)),
            ControlCmd::RecordTransfer {
                account,
                from,
                to,
                asset,
                amount,
                reference,
            } => {
                w.u8(0, account.0);
                w.u8(1, from.0);
                w.u8(2, to.0);
                w.u16(4, asset.0);
                w.i64(8, amount.0);
                w.b(16, reference);
            }
            ControlCmd::Annotate {
                kind,
                text_len,
                digest,
                text,
            } => {
                w.u16(0, *kind);
                w.u16(2, *text_len);
                w.b(4, digest);
                w.b(36, text);
            }
            ControlCmd::AdoptExchange {
                account,
                venue,
                inst,
                report_id,
                item,
                _pad,
                asset,
                group,
                entry_px,
            } => {
                w.u8(0, account.0);
                w.u8(1, venue.0);
                w.u32(4, inst.0);
                w.u64(8, *report_id);
                w.u8(16, *item);
                w.u8(17, *_pad);
                w.u16(18, asset.0);
                w.u32(20, group.0);
                w.i64(24, entry_px.0);
            }
            ControlCmd::SimReset { account } => w.u8(0, account.0),
            ControlCmd::SimTransfer {
                account,
                from,
                to,
                asset,
                amount,
            } => {
                w.u8(0, account.0);
                w.u8(1, from.0);
                w.u8(2, to.0);
                w.u16(4, asset.0);
                w.i64(8, amount.0);
            }
            ControlCmd::VenueNotice {
                venue,
                kind,
                _pad,
                inst,
                a,
                b,
            } => {
                w.u8(0, venue.0);
                w.u8(1, *kind);
                w.u16(2, *_pad);
                w.u32(4, inst.0);
                w.i64(8, *a);
                w.i64(16, *b);
            }
            ControlCmd::SetBalanceBaseline { account, venue } => {
                w.u8(0, account.0);
                w.u8(1, venue.0);
            }
            ControlCmd::StrategyCmd {
                instance,
                reduce_only,
                len,
                group,
                payload,
            } => {
                w.u8(0, instance.0);
                w.u8(1, *reduce_only);
                w.u16(2, *len);
                w.u32(4, group.0);
                w.b(8, payload.get(..usize::from(*len)).unwrap_or(&[]));
            }
            ControlCmd::InjectWarmup {
                instance,
                last,
                local_kind,
                chunk_no,
                len,
                _pad,
                payload,
            } => {
                w.u8(0, instance.0);
                w.u8(1, *last);
                w.u16(2, *local_kind);
                w.u32(4, *chunk_no);
                w.u16(8, *len);
                w.b(10, _pad);
                w.b(16, payload);
            }
            ControlCmd::SmokeOrder {
                account,
                venue,
                _pad,
                inst,
                side,
                _pad2,
                ttl_ms,
            } => {
                w.u8(0, account.0);
                w.u8(1, venue.0);
                w.u16(2, *_pad);
                w.u32(4, inst.0);
                w.u8(8, *side as u8);
                w.b(9, _pad2);
                w.u32(12, *ttl_ms);
            }
            ControlCmd::WriteOffResidual { account, _pad, group } => {
                w.u8(0, account.0);
                w.b(1, _pad);
                w.u32(4, group.0);
            }
            ControlCmd::SimFault {
                account,
                venue,
                fault,
                _pad,
                delay_ms,
            } => {
                w.u8(0, account.0);
                w.u8(1, venue.0);
                w.u8(2, *fault);
                w.u8(3, *_pad);
                w.u32(4, *delay_ms);
            }
            ControlCmd::BindSlot {
                venue,
                _pad,
                inst,
                start_s,
                end_s,
                wire_id,
            } => {
                w.u8(0, venue.0);
                w.b(1, _pad);
                w.u32(4, inst.0);
                w.i64(8, *start_s);
                w.i64(16, *end_s);
                w.b(24, wire_id);
            }
        }
        Ok(total)
    }

    /// Decodes a logical body (README "控制命令编码" for the error rules).
    pub fn decode(logical_body: &'a [u8]) -> Result<(CmdHdr, ControlCmd<'a>), DecodeErr> {
        let (hdr, payload) = CmdHdr::read_from_prefix(logical_body).map_err(|_| DecodeErr::Short)?;
        let fixed = payload_size(hdr.cmd, hdr.cmd_ver).ok_or(DecodeErr::UnknownKind)?;
        let plen = hdr.payload_len as usize;
        if payload.len() < plen {
            return Err(DecodeErr::Short);
        }
        if payload.len() > plen {
            return Err(DecodeErr::TooLong);
        }
        let r = R(payload);
        let expected = if hdr.cmd == CmdKind::INJECT_WARMUP {
            if plen < fixed {
                return Err(DecodeErr::Short);
            }
            let len = usize::from(r.u16(8));
            if len > WARMUP_MAX_PAYLOAD {
                return Err(DecodeErr::TooLong);
            }
            16 + round8(len)
        } else {
            fixed
        };
        if plen < expected {
            return Err(DecodeErr::Short);
        }
        if plen > expected {
            return Err(DecodeErr::TooLong);
        }
        let venue = |o| VenueId(r.u8(o));
        let account = |o| AccountId(r.u8(o));
        let cmd = match hdr.cmd {
            CmdKind::SET_MODE => {
                r.zero(17, 24)?;
                ControlCmd::SetMode {
                    scope: r.scope(0),
                    mode: RunMode::try_read_from_bytes(r.slice(16, 1)).map_err(|_| DecodeErr::InvalidEnum)?,
                }
            }
            CmdKind::SET_PARAMS => ControlCmd::SetParams {
                scope: r.scope(0),
                blob: r.blob(16)?,
            },
            CmdKind::SET_RISK => ControlCmd::SetRisk { blob: r.blob(0)? },
            CmdKind::REPLACE_REFDATA => ControlCmd::ReplaceRefdata { blob: r.blob(0)? },
            CmdKind::KILL => {
                r.zero(20, 24)?;
                ControlCmd::Kill {
                    scope: r.scope(0),
                    level: KillLevel::try_read_from_bytes(r.slice(16, 1))
                        .map_err(|_| DecodeErr::InvalidEnum)?,
                    flatten: r.bool(17)?,
                    cancel_resting: r.bool(18)?,
                    // v1: byte 19 was padding; reads as false.
                    hard: if hdr.cmd_ver >= 2 { r.bool(19)? } else { false },
                }
            }
            CmdKind::ROTATE_CONN => {
                r.zero(2, 8)?;
                ControlCmd::RotateConn {
                    venue: venue(0),
                    account: account(1),
                }
            }
            CmdKind::RESUBSCRIBE => {
                r.zero(1, 4)?;
                ControlCmd::Resubscribe {
                    venue: venue(0),
                    inst: InstId(r.u32(4)),
                }
            }
            CmdKind::SNAPSHOT_NOW => ControlCmd::SnapshotNow,
            CmdKind::RESUME => {
                r.zero(17, 24)?;
                ControlCmd::Resume {
                    scope: r.scope(0),
                    source: CapSource(r.code(16, 1..=15)?),
                }
            }
            CmdKind::HEDGE_FIX => ControlCmd::HedgeFix { scope: r.scope(0) },
            CmdKind::CANCEL_ALL => ControlCmd::CancelAll { scope: r.scope(0) },
            CmdKind::DRAIN => {
                r.zero(1, 8)?;
                ControlCmd::Drain {
                    cancel_resting: r.bool(0)?,
                }
            }
            CmdKind::RECORD_TRANSFER => {
                r.zero(3, 4)?;
                ControlCmd::RecordTransfer {
                    account: account(0),
                    from: venue(1),
                    to: venue(2),
                    asset: AssetId(r.u16(4)),
                    amount: Qty(r.i64(8)),
                    reference: r.arr(16),
                }
            }
            CmdKind::ANNOTATE => {
                r.zero(292, 296)?;
                let kind = r.u16(0);
                if !(1..=3).contains(&kind) {
                    return Err(DecodeErr::InvalidEnum);
                }
                let text_len = r.u16(2);
                if usize::from(text_len) > 256 {
                    return Err(DecodeErr::TooLong);
                }
                ControlCmd::Annotate {
                    kind,
                    text_len,
                    digest: r.arr(4),
                    text: r.arr(36),
                }
            }
            CmdKind::ADOPT_EXCHANGE => {
                r.zero(2, 4)?;
                let v2 = hdr.cmd_ver >= 2;
                ControlCmd::AdoptExchange {
                    account: account(0),
                    venue: venue(1),
                    inst: InstId(r.u32(4)),
                    report_id: r.u64(8),
                    item: if v2 { r.code(16, 0..=1)? } else { 0 },
                    _pad: if v2 { r.u8(17) } else { 0 },
                    asset: AssetId(if v2 { r.u16(18) } else { 0 }),
                    group: GroupId(if v2 { r.u32(20) } else { 0 }),
                    entry_px: Px(if v2 { r.i64(24) } else { 0 }),
                }
            }
            CmdKind::SIM_RESET => {
                r.zero(1, 8)?;
                ControlCmd::SimReset { account: account(0) }
            }
            CmdKind::SIM_TRANSFER => {
                r.zero(3, 4)?;
                r.zero(6, 8)?;
                ControlCmd::SimTransfer {
                    account: account(0),
                    from: venue(1),
                    to: venue(2),
                    asset: AssetId(r.u16(4)),
                    amount: Qty(r.i64(8)),
                }
            }
            CmdKind::VENUE_NOTICE => ControlCmd::VenueNotice {
                venue: venue(0),
                kind: r.code(1, 1..=3)?,
                _pad: r.u16(2),
                inst: InstId(r.u32(4)),
                a: r.i64(8),
                b: r.i64(16),
            },
            CmdKind::SET_BALANCE_BASELINE => {
                r.zero(2, 8)?;
                ControlCmd::SetBalanceBaseline {
                    account: account(0),
                    venue: venue(1),
                }
            }
            CmdKind::STRATEGY_CMD => {
                let len = r.u16(2);
                let n = usize::from(len);
                if n > STRATEGY_CMD_MAX_PAYLOAD || r.zero(8 + n, 264).is_err() {
                    return Err(DecodeErr::TooLong);
                }
                ControlCmd::StrategyCmd {
                    instance: StrategyId(r.u8(0)),
                    reduce_only: r.flag01(1)?,
                    len,
                    group: GroupId(r.u32(4)),
                    payload: r.arr(8),
                }
            }
            CmdKind::INJECT_WARMUP => {
                let len = r.u16(8);
                let n = usize::from(len);
                r.zero(16 + n, 16 + round8(n))?;
                ControlCmd::InjectWarmup {
                    instance: StrategyId(r.u8(0)),
                    last: r.flag01(1)?,
                    local_kind: r.u16(2),
                    chunk_no: r.u32(4),
                    len,
                    _pad: r.arr(10),
                    payload: r.slice(16, n),
                }
            }
            CmdKind::SMOKE_ORDER => ControlCmd::SmokeOrder {
                account: account(0),
                venue: venue(1),
                _pad: r.u16(2),
                inst: InstId(r.u32(4)),
                side: Side::try_read_from_bytes(r.slice(8, 1)).map_err(|_| DecodeErr::InvalidEnum)?,
                _pad2: r.arr(9),
                ttl_ms: r.u32(12),
            },
            CmdKind::WRITE_OFF_RESIDUAL => ControlCmd::WriteOffResidual {
                account: account(0),
                _pad: r.arr(1),
                group: GroupId(r.u32(4)),
            },
            CmdKind::SIM_FAULT => ControlCmd::SimFault {
                account: account(0),
                venue: venue(1),
                fault: r.code(2, 1..=4)?,
                _pad: r.u8(3),
                delay_ms: r.u32(4),
            },
            CmdKind::BIND_SLOT => {
                r.zero(1, 4)?;
                ControlCmd::BindSlot {
                    venue: venue(0),
                    _pad: [0; 3],
                    inst: InstId(r.u32(4)),
                    start_s: r.i64(8),
                    end_s: r.i64(16),
                    wire_id: r.arr(24),
                }
            }
            _ => return Err(DecodeErr::UnknownKind),
        };
        Ok((hdr, cmd))
    }
}
