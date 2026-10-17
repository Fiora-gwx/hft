//! System bodies (0x05xx except Advice/StateSnapshot), `NoteLevel`/`NoteSource` and the engine
//! Note name registry (`NOTE_NAMES`).
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{AccountId, GroupId, InstId, SlotId, StrategyId, Tsc, VenueId};

use crate::EncodeErr;
use crate::EventKind;
use crate::codes;
use crate::macros::{layout, wire_enum};
use crate::schema::FieldTy;

wire_enum!(
    /// Connection state (append-only).
    ConnStateKind: u8 = "ConnStateKind" { Up = 1, Down = 2, Resync = 3, Rotated = 4, Degraded = 5, Alive = 6 }
);
wire_enum!(
    /// Connection role (sole owner of these codes; hft-net and hft-venue use this type).
    ConnRole: u8 = "ConnRole" { Md = 1, Order = 2, UserStream = 3, Rest = 4 }
);
wire_enum!(
    /// Alert level (SDK, ops_bot, hft-telemetry and watchtower use the same names and values).
    NoteLevel: u8 = "NoteLevel" { P0 = 1, P1 = 2, P2 = 3, Info = 4 }
);
wire_enum!(
    /// Note source (append-only).
    NoteSource: u8 = "NoteSource" {
        Sequencer = 1, Strategy = 2, Risk = 3, Oms = 4, Venue = 5, CtlAudit = 6, Measure = 7, Housekeeping = 8,
        Ledger = 9, Exec = 10, Boot = 11, Advisor = 12,
    }
);

crate::event_body! {
    /// Timer firing (0x0501, Input); `(owner, timer_id, group)` from `TimerCode`.
    pub struct Timer : fixed(EventKind::TIMER, 1), FromBytes {
        pub timer_id: u32,
        pub owner: u16,
        pub _pad: u16,
        /// `GROUP_NONE` = not applicable.
        pub group: GroupId,
        pub _pad2: u32,
        pub deadline_tsc: u64,
    }
}
layout!(Timer, 24, 8; timer_id @ 0, owner @ 4, _pad @ 6, group @ 8, _pad2 @ 12, deadline_tsc @ 16);

crate::event_body! {
    /// Connection state change (0x0502, Input; v2 appends `rx_tsc`).
    pub struct ConnState : fixed(EventKind::CONN_STATE, 2), TryFromBytes {
        pub venue: VenueId,
        pub conn: u8,
        pub state: ConnStateKind,
        pub role: ConnRole,
        /// 0 = whole connection.
        pub inst: InstId,
        /// Reason code (table in hft-venue).
        pub reason: u32,
        /// 0 = market-data connection.
        pub account: AccountId,
        pub _pad2: [u8; 3],
        /// v2; 0 = not provided.
        pub rx_tsc: u64,
    }
}
layout!(ConnState, 24, 8; venue @ 0, conn @ 1, state @ 2, role @ 3, inst @ 4, reason @ 8, account @ 12,
    _pad2 @ 13, rx_tsc @ 16);

crate::event_body! {
    /// Latency sample (0x0505, Meta; v3).
    pub struct LatencySample : fixed(EventKind::LATENCY_SAMPLE, 3), FromBytes {
        pub trigger_seq: u64,
        pub ack_seq: u64,
        pub slot: SlotId,
        pub r#gen: u32,
        pub venue: VenueId,
        pub account: AccountId,
        pub strategy: StrategyId,
        pub _pad: u8,
        /// 2 bits per point 0..7 (`codes::LATENCY_SRC`).
        pub src: u32,
        pub ex_ts_ns: i64,
        pub ex_send_ns: i64,
        pub hw_rx_ns: i64,
        pub rx_tsc: Tsc,
        pub proc_tsc: Tsc,
        pub decided_tsc: Tsc,
        pub sent_tsc: Tsc,
        pub ack_hw_rx_ns: i64,
        pub ack_rx_tsc: Tsc,
        pub ack_ex_ts_ns: i64,
        /// v2.
        pub sw_rx_ns: i64,
        /// v2.
        pub link_role: u8 = FieldTy::Enum(&codes::LINK_ROLE),
        /// 0 New, 1 Cancel, 2 Amend.
        pub action: u8 = FieldTy::Enum(&codes::LATENCY_ACTION),
        pub _pad2: [u8; 6],
        /// v3: `EventKind` of the trigger; 0 = not provided.
        pub trigger_kind: u16,
        pub _pad3: [u8; 6],
    }
}
layout!(LatencySample, 136, 8; trigger_seq @ 0, ack_seq @ 8, slot @ 16, r#gen @ 20, venue @ 24, account @ 25,
    strategy @ 26, _pad @ 27, src @ 28, ex_ts_ns @ 32, ex_send_ns @ 40, hw_rx_ns @ 48, rx_tsc @ 56, proc_tsc @ 64,
    decided_tsc @ 72, sent_tsc @ 80, ack_hw_rx_ns @ 88, ack_rx_tsc @ 96, ack_ex_ts_ns @ 104, sw_rx_ns @ 112,
    link_role @ 120, action @ 121, _pad2 @ 122, trigger_kind @ 128, _pad3 @ 130);

crate::event_body! {
    /// The hot path's only "log" (0x0506); class decided by `source`.
    pub struct Note : fixed(EventKind::NOTE, 1), TryFromBytes {
        /// `source << 24 | n`.
        pub code: u32,
        pub level: NoteLevel,
        pub source: NoteSource,
        pub strategy: StrategyId,
        pub _pad: u8,
        pub group: GroupId,
        pub inst: InstId,
        pub v: [u64; 8],
    }
}
layout!(Note, 80, 8; code @ 0, level @ 4, source @ 5, strategy @ 6, _pad @ 7, group @ 8, inst @ 12, v @ 16);

crate::event_body! {
    /// Host profile (0x0507, Meta).
    pub struct HostProfile : fixed(EventKind::HOST_PROFILE, 1), FromBytes {
        pub boot_id: [u8; 16],
        pub build_git: [u8; 20],
        pub _pad: [u8; 4],
        pub profile_sha256: [u8; 32],
        pub release: [u8; 16],
        pub kernel: [u8; 32],
        pub tsc_hz_milli: u64,
        pub host_checks: u64,
        pub hugepages_1g_total: u32,
        pub hugepages_1g_free: u32,
        pub isolated_cpus: [u64; 4],
    }
}
layout!(HostProfile, 176, 8; boot_id @ 0, build_git @ 16, _pad @ 36, profile_sha256 @ 40, release @ 72,
    kernel @ 88, tsc_hz_milli @ 120, host_checks @ 128, hugepages_1g_total @ 136, hugepages_1g_free @ 140,
    isolated_cpus @ 144);

crate::event_body! {
    /// Clock calibration (0x0508, Meta).
    pub struct ClockStatus : fixed(EventKind::CLOCK_STATUS, 1), FromBytes {
        pub tsc: Tsc,
        pub realtime_ns: i64,
        pub phc_ns: i64,
        pub tsc_hz_milli: u64,
        pub chrony_offset_ns: i64,
        pub max_error_ns: u32,
        pub rms_ns: u32,
        pub calib_err_ns: u32,
        pub source: u8 = FieldTy::Enum(&codes::CLOCK_SOURCE),
        pub state: u8 = FieldTy::Enum(&codes::CLOCK_STATE),
        pub _pad: u16,
    }
}
layout!(ClockStatus, 56, 8; tsc @ 0, realtime_ns @ 8, phc_ns @ 16, tsc_hz_milli @ 24, chrony_offset_ns @ 32,
    max_error_ns @ 40, rms_ns @ 44, calib_err_ns @ 48, source @ 52, state @ 53, _pad @ 54);

crate::event_body! {
    /// Segment roll structure record (0x0509, Meta).
    pub struct SegmentRoll : fixed(EventKind::SEGMENT_ROLL, 1), FromBytes {
        pub segment_no: u64,
        pub other_segment_no: u64,
        pub seq_mark: u64,
        pub utc_ns: i64,
        pub mark: u8 = FieldTy::Enum(&codes::ROLL_MARK),
        pub reason: u8 = FieldTy::Enum(&codes::ROLL_REASON),
        pub journal_kind: u8 = FieldTy::Enum(&codes::JOURNAL_KIND),
        pub _pad: u8,
        pub boot_records: u32,
        pub _rsv: u64,
    }
}
layout!(SegmentRoll, 48, 8; segment_no @ 0, other_segment_no @ 8, seq_mark @ 16, utc_ns @ 24, mark @ 32,
    reason @ 33, journal_kind @ 34, _pad @ 35, boot_records @ 36, _rsv @ 40);

crate::event_body! {
    /// Engine-time base of a run (0x050A, Input).
    pub struct EngineClock : fixed(EventKind::ENGINE_CLOCK, 1), FromBytes {
        pub base_engine_ns: i64,
        pub tsc_hz_milli: u64,
        pub prev_last_seq: u64,
        pub source: u8 = FieldTy::Enum(&codes::ENGINE_CLOCK_SOURCE),
        pub _pad: [u8; 7],
    }
}
layout!(EngineClock, 32, 8; base_engine_ns @ 0, tsc_hz_milli @ 8, prev_last_seq @ 16, source @ 24, _pad @ 25);

/// One `v[]` field of a Note: bits `[bit, bit + bits)` of the 512-bit `v` (bit = index × 64 + shift).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct VField {
    pub name: &'static str,
    pub bit: u16,
    pub bits: u16,
}

impl VField {
    /// Field value (only for `bits ≤ 64`; wider fields or out-of-range positions read 0).
    #[inline]
    pub const fn get(&self, v: &[u64; 8]) -> u64 {
        let ix = (self.bit / 64) as usize;
        let sh = self.bit % 64;
        if self.bits == 0 || self.bits > 64 || ix >= 8 {
            return 0;
        }
        let e = match ix {
            0 => v[0],
            1 => v[1],
            2 => v[2],
            3 => v[3],
            4 => v[4],
            5 => v[5],
            6 => v[6],
            _ => v[7],
        };
        let w = e >> sh;
        if self.bits == 64 {
            w
        } else {
            w & ((1u64 << self.bits) - 1)
        }
    }

    /// Writes `x` into this field only; `bits < 64` and `x ≥ 2^bits` → `TooLong` (nothing written).
    /// Fields wider than 64 bits (byte strings) are written by the source crate directly → `TooLong`.
    #[inline]
    pub fn put(&self, v: &mut [u64; 8], x: u64) -> Result<(), EncodeErr> {
        let ix = usize::from(self.bit / 64);
        let sh = self.bit % 64;
        if self.bits == 0 || self.bits > 64 || u32::from(sh) + u32::from(self.bits) > 64 {
            return Err(EncodeErr::TooLong);
        }
        let mask = if self.bits == 64 {
            u64::MAX
        } else {
            (1u64 << self.bits) - 1
        };
        if x & !mask != 0 {
            return Err(EncodeErr::TooLong);
        }
        let Some(e) = v.get_mut(ix) else {
            return Err(EncodeErr::TooLong);
        };
        *e = (*e & !(mask << sh)) | (x << sh);
        Ok(())
    }
}

/// Engine Note name registry row (README "引擎 Note 码登记表").
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct NoteName {
    pub name: &'static str,
    pub source: NoteSource,
    /// Fixed default level; `None` = level by condition (see the table).
    pub level: Option<NoteLevel>,
    /// `v[]` fields listed in the table (the rest belong to the source crate's const table).
    pub v: &'static [VField],
}

const fn f(name: &'static str, bit: u16, bits: u16) -> VField {
    VField { name, bit, bits }
}

macro_rules! notes {
    ($($name:ident : $src:ident, $lvl:tt, [$($v:expr),* $(,)?]);* $(;)?) => {
        &[$(NoteName {
            name: stringify!($name),
            source: NoteSource::$src,
            level: notes!(@lvl $lvl),
            v: &[$($v),*],
        }),*]
    };
    (@lvl None) => { None };
    (@lvl $l:ident) => { Some(NoteLevel::$l) };
}

/// Engine Note names in README table order (one entry per name).
pub static NOTE_NAMES: &[NoteName] = notes![
    EpochStart: Sequencer, Info, [f("engine_epoch", 0, 64), f("base_engine_ns", 64, 64), f("clock_seq", 128, 64)];
    Conflated: Sequencer, Info, [];
    DrainTimeout: Sequencer, None, [];
    RecoverMismatch: Sequencer, P0, [f("venue", 0, 8), f("account", 8, 8), f("seq", 64, 64)];
    AdviceMisrouted: Sequencer, P2, [];
    AdviceStale: Sequencer, P2, [];
    StartupGate: Sequencer, Info, [f("account", 0, 64), f("old_gate_bits", 64, 64), f("new_gate_bits", 128, 64),
        f("released", 192, 64)];
    ShadowDeferred: Sequencer, Info, [f("strategy", 0, 8), f("account", 8, 8), f("group", 64, 64), f("state", 128, 64)];
    IdentityMismatch: Sequencer, P0, [f("venue", 0, 8), f("account", 8, 8)];
    RefdataRecheckFailed: Sequencer, P1, [];
    TimersCancelled: Sequencer, Info, [];
    TargetCleared: Sequencer, Info, [];
    PositionDiff: Sequencer, P1, [f("venue", 0, 8), f("account", 8, 8), f("ledger_qty", 64, 64), f("report_qty", 128, 64)];
    ReconcileCash: Sequencer, Info, [f("report_id", 0, 64), f("venue", 64, 8), f("account", 72, 8), f("n_reports", 128, 64), f("n_fills", 192, 64), f("applied", 256, 64)];
    ReconcileRejected: Sequencer, P0, [f("report_id", 0, 64), f("venue", 64, 8), f("account", 72, 8), f("reason", 128, 64), f("detail", 192, 64)];
    RateLimitHit: Risk, P2, [];
    CapSet: Risk, Info, [f("cap", 0, 384)];
    CapCleared: Risk, Info, [f("cap", 0, 384)];
    MarginLevel: Risk, None, [f("account", 0, 8), f("venue", 8, 8), f("level", 64, 64), f("liq_dist", 128, 64),
        f("ratio", 192, 64), f("deadline_ens", 256, 64)];
    RecoveryHeldSim: Risk, P1, [];
    IdentityUnverified: Risk, P1, [f("venue", 0, 8), f("account", 8, 8)];
    OmsTransition: Oms, Info, [];
    OmsUnknown: Oms, None, [];
    OmsUnknownStuck: Oms, P0, [];
    OmsVenueReady: Oms, Info, [];
    OmsStaleGenFill: Oms, P0, [];
    OmsLateFill: Oms, P0, [];
    OmsOverfill: Oms, P0, [];
    OmsForeignOrder: Oms, P1, [];
    OmsForeignFillViaOrderEvent: Oms, P1, [];
    OmsExtDedup: Oms, P1, [];
    OmsForeignFill: Oms, Info, [];
    OmsHedgeBroken: Oms, P0, [];
    OmsHedgePxStale: Oms, P1, [];
    OmsHedgePxGated: Oms, P1, [];
    OmsHedgeSlipCapped: Oms, P1, [];
    OmsHedgeNakedCapped: Oms, P1, [];
    OmsResumeRetry: Oms, P1, [];
    OmsResumeTimeout: Oms, None, [];
    OmsFillPxEstimated: Oms, P1, [];
    OmsFeeUnattributed: Oms, P1, [];
    OmsCancelNotSent: Oms, P2, [];
    OmsNeedsRestQuery: Oms, Info, [];
    OmsAnomaly: Oms, None, [];
    OmsHedgeResidual: Oms, P1, [];
    OmsLinkCapacity: Oms, P1, [];
    RoundClosed: Ledger, Info, [];
    Adjusted: Ledger, Info, [f("account", 0, 8), f("venue", 8, 8), f("inst_or_asset", 64, 64), f("report_id", 128, 64),
        f("qty", 192, 64), f("px", 256, 64), f("realized", 320, 64)];
    BaselineSet: Ledger, Info, [];
    BalanceInit: Ledger, Info, [];
    SimReset: Ledger, Info, [];
    ResidualWrittenOff: Ledger, P1, [];
    ExternalFill: Ledger, None, [];
    LedgerAutoHeldOffset: Ledger, P0, [];
    BalanceDrift: Ledger, P1, [];
    InvShortfall: Ledger, P1, [];
    NetQtyMismatch: Ledger, P1, [];
    FundingMismatch: Ledger, P1, [];
    FundingCorrected: Ledger, P1, [];
    ChainReorg: Ledger, P1, [];
    ChainReorgNaked: Ledger, P0, [];
    BalanceCheck: Ledger, Info, [f("venue", 0, 8), f("account", 8, 8), f("asset", 64, 64),
        f("free", 128, 64), f("locked", 192, 64), f("ex_ts_ns", 256, 64)];
    ReceiptMismatch: Ledger, P0, [f("venue", 0, 8), f("account", 8, 8), f("inst", 64, 64), f("trade_id", 128, 64),
        f("exch_order_id", 192, 64), f("fill_seq", 256, 64), f("fields", 320, 64)];
    ExecBroken: Exec, P0, [];
    ExecExternalDefault: Exec, None, [];
    ExecAutoFixFailed: Exec, P0, [];
    ExecStuck: Exec, P1, [];
    ExecForceStep: Exec, P1, [];
    ExecAutoFixDone: Exec, P1, [];
    ExecSerialFallback: Exec, P0, [];
    ExecEngineOrderRejected: Exec, P0, [];
    ExecManualExternal: Exec, None, [];
    ExecManualHold: Exec, None, [];
    ExecUngroupedReduce: Exec, P0, [];
    ExecGateHeldNaked: Exec, P0, [];
    ExecChainLegExcess: Exec, P0, [];
    CtlAudit: CtlAudit, Info, [f("cmd_id", 0, 128), f("cmd_kind", 128, 16), f("result_code", 144, 8),
        f("role", 152, 8), f("view_account", 160, 8), f("chat_type", 168, 8), f("target_account", 176, 8),
        f("issuer", 192, 64), f("cmd_hash", 256, 192), f("peer_uid", 448, 32)];
    PartialEventDropped: Boot, P1, [];
    TailSkipped: Boot, P0, [f("seq_from", 0, 64), f("seq_to", 64, 64), f("reason", 128, 64)];
    RecoverFromSnapshot: Boot, P1, [f("snapshot_seq", 0, 64), f("skip_from", 64, 64), f("skip_to", 128, 64),
        f("reason", 192, 64), f("issuer", 256, 64)];
    KillSelfTest: Boot, Info, [f("account", 0, 64), f("result", 64, 64)];
    InputQuarantined: Boot, P0, [f("seq", 0, 64), f("crashes", 64, 64)];
    StartupStep: Boot, Info, [];
    HostDegraded: Boot, P1, [];
    JournalTailLost: Boot, P1, [];
    RefdataWarning: Boot, P1, [f("code", 0, 64), f("section", 64, 64), f("row", 128, 64)];
    StopMarkerCorrupt: Boot, P0, [];
    EndpointOverride: Boot, P1, [f("venue", 0, 8)];
    ModelCheckpointDiscarded: Advisor, P1, [f("strategy", 0, 64), f("reason", 64, 64)];
    ModelPathReset: Advisor, P2, [f("reason", 0, 64), f("last_seq", 64, 64), f("path_settle_ms", 128, 64),
        f("ckpt_path_version", 192, 64), f("bin_path_version", 256, 64)];
    AdvisorLag: Advisor, P1, [f("strategy", 0, 64), f("lag_ms", 64, 64), f("lag_records", 128, 64),
        f("consumed_seq", 192, 64)];
    ModelGroupReset: Advisor, P2, [f("reason", 0, 64), f("params_version", 64, 64)];
    AdvisorStats: Advisor, Info, [];
    ModelCkptSkipped: Advisor, P1, [];
];

/// Registry row of an engine Note name.
pub fn note_name(name: &str) -> Option<&'static NoteName> {
    NOTE_NAMES.iter().find(|n| n.name == name)
}

/// `(source as u32) << 24 | n`; `n = 0` (reserved) or `n ≥ 2^24` → debug_assert.
#[inline]
pub const fn note_code(source: NoteSource, n: u32) -> u32 {
    debug_assert!(n != 0 && n < (1 << 24), "Note n must be in 1..2^24");
    ((source as u32) << 24) | (n & 0xFF_FFFF)
}
