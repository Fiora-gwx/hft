//! Public types of hft-oms: slot layouts (`OrderSlot`, `SlotExt`, `SlotStamps`, `Tomb`),
//! `LinkEntry`, `LegState`, `OutItem`, the code tables (`OrderState`, `TransitionCause`,
//! `LinkState`, `LinkBroken`), private slot bits (`SlotFlags`), the output vectors (`OmsOut`) and
//! the request/result structs exchanged with the sequencer.
//!
//! The order-intent vocabulary (`LegLink`, `HedgePlan`, `LinkRole`, `OrderRef`, `LegView`, …) is
//! the SDK's; wire flag bits (`OrderFlags`) are hft-event's. Nothing here redefines them.
// zerocopy's derive helpers carry no Debug impl.
#![allow(missing_debug_implementations)]

use core::mem::{align_of, offset_of, size_of};

use hft_event::{LatencySample, Note, OrderDecision};
use hft_strategy_sdk::{
    HedgePlan, IntentClass, LegLink, LegLinkOpt, LegView, LinkRole, OrderIntent, OrderRef, OrderUpdate,
    ResidualPolicy,
};
use hft_types::{
    AccountId, AssetId, GroupId, InstId, Px, Qty, Rate, RejectBits, Side, StrategyId, Tif, Tsc, VenueId,
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes};

// ------------------------------------------------------------------------------------ codes

/// Slot state (snapshot section 2, `OutItem`, `Note{OmsTransition}` `v[0]`); append-only.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum OrderState {
    Free = 0,
    PendingNew = 1,
    Live = 2,
    PendingCancel = 3,
    PendingAmend = 4,
    Filled = 5,
    Cancelled = 6,
    Rejected = 7,
    Unknown = 8,
}

impl OrderState {
    /// Filled, Cancelled or Rejected.
    #[inline]
    pub const fn terminal(self) -> bool {
        matches!(
            self,
            OrderState::Filled | OrderState::Cancelled | OrderState::Rejected
        )
    }
    /// Neither Free nor terminal.
    #[inline]
    pub const fn open(self) -> bool {
        !matches!(
            self,
            OrderState::Free | OrderState::Filled | OrderState::Cancelled | OrderState::Rejected
        )
    }
}

impl TryFrom<u8> for OrderState {
    type Error = u8;
    fn try_from(v: u8) -> Result<OrderState, u8> {
        Ok(match v {
            0 => OrderState::Free,
            1 => OrderState::PendingNew,
            2 => OrderState::Live,
            3 => OrderState::PendingCancel,
            4 => OrderState::PendingAmend,
            5 => OrderState::Filled,
            6 => OrderState::Cancelled,
            7 => OrderState::Rejected,
            8 => OrderState::Unknown,
            _ => return Err(v),
        })
    }
}

/// Transition cause (`Note{OmsTransition}` and `OutItem.cause`); append-only.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TransitionCause {
    Place = 1,
    CancelReq = 2,
    AmendReq = 3,
    Ack = 4,
    Reject = 5,
    Fill = 6,
    CancelAck = 7,
    Expired = 8,
    CancelReject = 9,
    AmendAck = 10,
    AmendReject = 11,
    QueryResult = 12,
    Reconcile = 13,
    OutcomeUnknown = 14,
    ConnDown = 15,
    Timeout = 16,
    LocalSendFail = 17,
    WriteError = 18,
    ReportDecode = 19,
    RecoveryEnd = 20,
    ForceUnknown = 21,
    NotFoundExpire = 22,
    Recycle = 23,
    SimReset = 24,
    SmokeTtl = 25,
    FeeOnly = 26,
}

impl TryFrom<u8> for TransitionCause {
    type Error = u8;
    fn try_from(v: u8) -> Result<TransitionCause, u8> {
        use TransitionCause::*;
        Ok(match v {
            1 => Place,
            2 => CancelReq,
            3 => AmendReq,
            4 => Ack,
            5 => Reject,
            6 => Fill,
            7 => CancelAck,
            8 => Expired,
            9 => CancelReject,
            10 => AmendAck,
            11 => AmendReject,
            12 => QueryResult,
            13 => Reconcile,
            14 => OutcomeUnknown,
            15 => ConnDown,
            16 => Timeout,
            17 => LocalSendFail,
            18 => WriteError,
            19 => ReportDecode,
            20 => RecoveryEnd,
            21 => ForceUnknown,
            22 => NotFoundExpire,
            23 => Recycle,
            24 => SimReset,
            25 => SmokeTtl,
            26 => FeeOnly,
            _ => return Err(v),
        })
    }
}

/// Link state; `Free = 0` marks an unused `LinkTable` entry (never written to the snapshot).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub enum LinkState {
    Free = 0,
    Open = 1,
    Hedging = 2,
    Done = 3,
    Broken = 4,
}

/// Why a link is Broken (`LinkUpdate.broken_reason`); append-only.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum LinkBroken {
    None = 0,
    RetryExhausted = 1,
    Deadline = 2,
    HedgeLimit = 3,
    Overfill = 4,
    ReverseFailed = 5,
    ResumeTimeout = 6,
}

impl TryFrom<u8> for LinkBroken {
    type Error = u8;
    fn try_from(v: u8) -> Result<LinkBroken, u8> {
        Ok(match v {
            0 => LinkBroken::None,
            1 => LinkBroken::RetryExhausted,
            2 => LinkBroken::Deadline,
            3 => LinkBroken::HedgeLimit,
            4 => LinkBroken::Overfill,
            5 => LinkBroken::ReverseFailed,
            6 => LinkBroken::ResumeTimeout,
            _ => return Err(v),
        })
    }
}

/// Private slot state bits (never on any wire record; in the snapshot).
#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SlotFlags(pub u16);

impl SlotFlags {
    pub const CANCEL_SENT: u16 = 1;
    pub const AMEND_SENT: u16 = 2;
    pub const FILL_BEFORE_ACK: u16 = 4;
    pub const VERIFY: u16 = 8;
    pub const QUEUED: u16 = 16;
    pub const EXEC: u16 = 32;
    pub const SMOKE_DUE: u16 = 64;
    pub const ACKED: u16 = 128;
    pub const HIST_WAIT: u16 = 256;
    pub const ALARMED: u16 = 512;
    pub const AGED: u16 = 1024;
    /// Union of every defined bit.
    pub const ALL: u16 = 0x07FF;

    #[inline]
    pub const fn has(self, b: u16) -> bool {
        self.0 & b != 0
    }
}

/// `LinkEntry.flags` bits.
pub mod link_flags {
    /// Recovered, before resumption: no deadline, bounded by `resume_hold_max_ms`.
    pub const RESUME_HOLD: u8 = 1;
    /// The resumption order is in flight: no deadline until it is terminal.
    pub const RESUME_RETRY: u8 = 2;
    pub const SLIP_NOTED: u8 = 4;
    pub const STALE_NOTED: u8 = 8;
    pub const BEYOND_NOTED: u8 = 16;
    /// A `HedgeRetry` timer is registered and not yet fired (`quiescent` `HEDGE_RETRY_PENDING`).
    pub const RETRY_PENDING: u8 = 32;
    /// A resumption `Hedge` item waits in `pending_out`.
    pub const HEDGE_QUEUED: u8 = 64;
    /// The failure handling switched to `Reverse` (`on_fail = Reverse` or Simultaneous mismatch).
    pub const REVERSING: u8 = 128;
}

/// `FillOut.flags` bits.
pub mod fill_flags {
    /// Price estimated from the order price (QueryResult / reconciliation without `avg_px`).
    pub const PX_EST: u8 = 1;
    /// Late fill matched through a tombstone.
    pub const LATE: u8 = 2;
    /// `cum_filled > qty`.
    pub const OVERFILL: u8 = 4;
    /// Part of the increment had already been booked as an external fill.
    pub const EXT_DEDUP: u8 = 8;
}

/// `quiescent()` reason bits (Drain readiness of one account).
pub mod quiesce {
    pub const LINKS_ACTIVE: u8 = 1;
    pub const IOC_NONTERMINAL: u8 = 2;
    pub const HEDGE_RETRY_PENDING: u8 = 4;
    pub const UNKNOWN: u8 = 8;
}
/// Bit set returned by `Oms::quiescent`.
pub type QuiesceBits = u8;

/// `SlotExt.origin` codes.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Origin {
    Strategy = 1,
    Exec = 2,
    Oms = 3,
    /// Smoke orders (`SmokeOrder` command).
    Command = 4,
}

/// `OmsAnomaly` kinds (`v[0]`).
pub mod anomaly {
    pub const TABLE: u64 = 1;
    pub const FOREIGN_EVICT: u64 = 2;
    pub const SMOKE_FILLED: u64 = 3;
    pub const RECON_IGNORED: u64 = 4;
}

// ------------------------------------------------------------------------------- layouts

/// One order slot (128 B, two cache lines; see README "数据布局").
#[repr(C, align(64))]
#[derive(Copy, Clone, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct OrderSlot {
    pub state: OrderState,
    pub side: Side,
    pub tif: Tif,
    pub prev_state: OrderState,
    /// hft-event `OrderFlags` bits (raw).
    pub oflags: u16,
    pub venue: VenueId,
    pub account: AccountId,
    pub r#gen: u32,
    pub slot_id: u32,
    pub inst: InstId,
    pub strategy: StrategyId,
    pub query_count: u8,
    pub epoch: u16,
    pub group: GroupId,
    pub link_id: u32,
    pub qty: Qty,
    pub filled: Qty,
    pub px: Px,
    pub exch_order_id: u64,
    pub filled_notional: i128,
    pub pending_px: Px,
    pub pending_qty: Qty,
    pub fee_accum: i64,
    pub fee_asset: AssetId,
    pub sflags: SlotFlags,
    pub reject_code: u32,
    pub t_state: Tsc,
    pub seq_last: u64,
}
const _: () = assert!(size_of::<OrderSlot>() == 128 && align_of::<OrderSlot>() == 64);
const _: () = assert!(offset_of!(OrderSlot, oflags) == 4 && offset_of!(OrderSlot, sflags) == 106);
const _: () = assert!(offset_of!(OrderSlot, filled_notional) == 64 && offset_of!(OrderSlot, t_state) == 112);

impl OrderSlot {
    pub(crate) const FREE: OrderSlot = OrderSlot {
        state: OrderState::Free,
        side: Side::Buy,
        tif: Tif::Gtc,
        prev_state: OrderState::Free,
        oflags: 0,
        venue: VenueId(0),
        account: AccountId(0),
        r#gen: 0,
        slot_id: 0,
        inst: InstId(0),
        strategy: StrategyId(0),
        query_count: 0,
        epoch: 0,
        group: GroupId(0),
        link_id: 0,
        qty: Qty(0),
        filled: Qty(0),
        px: Px(0),
        exch_order_id: 0,
        filled_notional: 0,
        pending_px: Px(0),
        pending_qty: Qty(0),
        fee_accum: 0,
        fee_asset: AssetId(0),
        sflags: SlotFlags(0),
        reject_code: 0,
        t_state: Tsc(0),
        seq_last: 0,
    };
    /// Remaining quantity `qty − filled` (≥ 0).
    #[inline]
    pub fn remaining(&self) -> Qty {
        Qty(self.qty.0.saturating_sub(self.filled.0).max(0))
    }
    /// Handle of this generation.
    #[inline]
    pub fn handle(&self) -> OrderRef {
        OrderRef::new(self.venue, self.account, self.slot_id, self.r#gen)
    }
}

/// Per-slot extension (64 B), parallel to the slots; in the snapshot.
#[repr(C, align(32))]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SlotExt {
    pub net_filled: Qty,
    pub tag: u32,
    /// Leg index of the slot in its group; `NO_LEG` = not tracked.
    pub leg_ix: u8,
    /// `LinkRole` code; 0 = no link.
    pub role: u8,
    pub origin: u8,
    /// `IntentClass` code from `classify()`.
    pub class: u8,
    pub lev_x100: u16,
    pub _pad1: u16,
    pub max_rest_ms: u32,
    /// `OrderDecision.reason` of the New decision (hft-exec `ExecOrderKind`; 0 otherwise).
    pub reason: u32,
    /// `OrderDecision.intent_no` of the New decision.
    pub intent_no: u32,
    /// `cum_filled` of the last `PartialFill`/`Fill` report applied: fees are booked up to here
    /// (README "回报匹配与成交增量" 9).
    pub fee_cum: Qty,
    /// Reserved (zero).
    pub _rsv: [u64; 3],
}
const _: () = assert!(size_of::<SlotExt>() == 64 && size_of::<SlotStamps>() == 80);
const _: () = assert!(
    offset_of!(SlotExt, leg_ix) == 12
        && offset_of!(SlotExt, max_rest_ms) == 20
        && offset_of!(SlotExt, fee_cum) == 32
);

impl SlotExt {
    /// `leg_ix` of a slot outside any group leg.
    pub const NO_LEG: u8 = 0xFF;
    pub(crate) const ZERO: SlotExt = SlotExt {
        net_filled: Qty(0),
        tag: 0,
        leg_ix: Self::NO_LEG,
        role: 0,
        origin: 0,
        class: 0,
        lev_x100: 0,
        _pad1: 0,
        max_rest_ms: 0,
        reason: 0,
        intent_no: 0,
        fee_cum: Qty(0),
        _rsv: [0; 3],
    };
}

/// Latency stamps (80 B), parallel to the slots; not in the snapshot.
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SlotStamps {
    pub trig_ex_ts_ns: i64,
    pub trig_ex_send_ns: i64,
    pub trig_hw_rx_ns: i64,
    pub trig_sw_rx_ns: i64,
    pub trig_rx_tsc: u64,
    pub trig_proc_tsc: u64,
    pub decided_tsc: u64,
    pub sent_tsc: u64,
    pub trigger_seq: u64,
    pub trig_kind: u16,
    /// Operation awaiting its sample: New 0, Cancel 1, Amend 2; `OP_NONE` = none.
    pub op: u8,
    pub _pad: [u8; 5],
}
const _: () = assert!(offset_of!(SlotStamps, trig_kind) == 72 && offset_of!(SlotStamps, op) == 74);

impl SlotStamps {
    pub const OP_NONE: u8 = 0xFF;
    pub const OP_NEW: u8 = 0;
    pub const OP_CANCEL: u8 = 1;
    pub const OP_AMEND: u8 = 2;
    pub(crate) const EMPTY: SlotStamps = SlotStamps {
        trig_ex_ts_ns: 0,
        trig_ex_send_ns: 0,
        trig_hw_rx_ns: 0,
        trig_sw_rx_ns: 0,
        trig_rx_tsc: 0,
        trig_proc_tsc: 0,
        decided_tsc: 0,
        sent_tsc: 0,
        trigger_seq: 0,
        trig_kind: 0,
        op: Self::OP_NONE,
        _pad: [0; 5],
    };
}

/// One recycled generation of a slot (56 B); `gen = 0` = empty.
#[repr(C)]
#[derive(Copy, Clone, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct Tomb {
    pub r#gen: u32,
    pub epoch: u16,
    pub oflags: u16,
    pub inst: InstId,
    pub group: GroupId,
    pub side: Side,
    pub leg_ix: u8,
    pub strategy: StrategyId,
    pub role: u8,
    pub link_id: u32,
    pub filled: Qty,
    pub net_filled: Qty,
    pub exch_order_id: u64,
    pub freed_tsc: Tsc,
}
const _: () =
    assert!(size_of::<Tomb>() == 56 && offset_of!(Tomb, filled) == 24 && offset_of!(Tomb, freed_tsc) == 48);

impl Tomb {
    pub(crate) const EMPTY: Tomb = Tomb {
        r#gen: 0,
        epoch: 0,
        oflags: 0,
        inst: InstId(0),
        group: GroupId(0),
        side: Side::Buy,
        leg_ix: 0,
        strategy: StrategyId(0),
        role: 0,
        link_id: 0,
        filled: Qty(0),
        net_filled: Qty(0),
        exch_order_id: 0,
        freed_tsc: Tsc(0),
    };
}

/// `LinkTable` element (80 B; private to this crate; snapshot section 12 `Link` row).
#[repr(C, align(8))]
#[derive(Copy, Clone, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LinkEntry {
    pub link_id: u32,
    pub group: GroupId,
    pub account: AccountId,
    pub state: LinkState,
    pub retries: u8,
    /// [`link_flags`] bits.
    pub flags: u8,
    /// Plan table index (= link table index).
    pub plan: u32,
    pub first: OrderRef,
    pub held_ms: u32,
    pub first_net: Qty,
    pub hedge_req: Qty,
    pub hedge_filled: Qty,
    pub hedge_open: Qty,
    pub first_fill_tsc: Tsc,
    pub ref_px: Px,
}
const _: () = assert!(
    size_of::<LinkEntry>() == 80
        && offset_of!(LinkEntry, first) == 16
        && offset_of!(LinkEntry, first_net) == 32
);

impl LinkEntry {
    pub(crate) const FREE: LinkEntry = LinkEntry {
        link_id: 0,
        group: GroupId(0),
        account: AccountId(0),
        state: LinkState::Free,
        retries: 0,
        flags: 0,
        plan: 0,
        first: OrderRef::new(VenueId(0), AccountId(0), 0, 0),
        held_ms: 0,
        first_net: Qty(0),
        hedge_req: Qty(0),
        hedge_filled: Qty(0),
        hedge_open: Qty(0),
        first_fill_tsc: Tsc(0),
        ref_px: Px(0),
    };
    /// `hedge_req − hedge_filled − hedge_open`.
    #[inline]
    pub fn owed(&self) -> Qty {
        Qty(self
            .hedge_req
            .0
            .saturating_sub(self.hedge_filled.0)
            .saturating_sub(self.hedge_open.0))
    }
    #[inline]
    pub const fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }
    /// Open or Hedging.
    #[inline]
    pub const fn active(&self) -> bool {
        matches!(self.state, LinkState::Open | LinkState::Hedging)
    }
}

/// LegTracker state of one (account, group) (192 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegState {
    /// SDK `LegView` (`Views::legs` returns it); this crate is its only writer.
    pub view: LegView,
    pub tolerance: Rate,
    pub last_fill_tsc: Tsc,
    pub last_link: u32,
    pub broken_links: u16,
    pub pending_slots: u16,
    pub unknown_slots: u16,
    /// Price of the group's latest fill (48-bit signed raw, little-endian; `naked_notional`).
    pub last_px48: [u8; 6],
}
const _: () = assert!(size_of::<LegState>() == 192);
const _: () = assert!(offset_of!(LegState, tolerance) == 160 && offset_of!(LegState, unknown_slots) == 184);

impl LegState {
    pub const EMPTY: LegState = LegState {
        view: LegView::EMPTY,
        tolerance: Rate(0),
        last_fill_tsc: Tsc(0),
        last_link: 0,
        broken_links: 0,
        pending_slots: 0,
        unknown_slots: 0,
        last_px48: [0; 6],
    };
    /// Latest fill price stored in `last_px48`.
    pub fn last_px(&self) -> Px {
        let b = self.last_px48;
        let raw = i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], 0, 0]);
        // Sign-extend bit 47.
        Px((raw << 16) >> 16)
    }
    pub(crate) fn set_last_px(&mut self, px: Px) {
        let lim = (1i64 << 47) - 1;
        let v = px.0.clamp(-lim, lim).to_le_bytes();
        self.last_px48 = [v[0], v[1], v[2], v[3], v[4], v[5]];
    }
    /// Whether every field is zero (not written to the snapshot).
    pub fn is_zero(&self) -> bool {
        self.as_bytes().iter().all(|b| *b == 0)
    }
}

/// `pending_out` element (32 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct OutItem {
    /// [`out_kind`] code.
    pub kind: u8,
    /// Decision: `OrderAction`; Notify: to-state; Hedge: `LinkRole`; Link: [`link_note`] code.
    pub action: u8,
    /// Notify: from-state; Link with `RESUME_TIMEOUT_*`: timeout reason (GATE 1, UNKNOWN 2).
    pub from: u8,
    /// Notify: `TransitionCause`.
    pub cause: u8,
    pub link_id: u32,
    /// Seq of the event that caused the item (Timer items: the timer's `deadline_tsc`).
    pub cause_seq: u64,
    pub slot: OrderRef,
    pub _pad: u32,
}
const _: () = assert!(size_of::<OutItem>() == 32);

/// `OutItem.kind` codes.
pub mod out_kind {
    pub const DECISION: u8 = 1;
    /// Transition Note (+ `OmsUnknown` when entering Unknown) and the owner's `OrderUpdate`.
    pub const NOTIFY: u8 = 2;
    pub const HEDGE: u8 = 3;
    pub const LINK: u8 = 4;
    /// `Timer{Oms, UnknownQuery}` registration of `slot`; `cause_seq` = deadline tsc.
    pub const TIMER: u8 = 5;
    /// `OmsUnknown` P0 escalation of `slot`.
    pub const ALARM: u8 = 6;
    /// `OrderUpdate{Aged}` of `slot`.
    pub const AGED: u8 = 7;
}

/// Notes that ride on a `Link` item (`OutItem.action`).
pub mod link_note {
    pub const NONE: u8 = 0;
    pub const HEDGE_BROKEN: u8 = 1;
    pub const RESUME_TIMEOUT_P0: u8 = 2;
    pub const RESUME_TIMEOUT_P1: u8 = 3;
}

// ------------------------------------------------------------------------- snapshot rows

/// Section 3 header row (16 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct OmsCountersHdr {
    pub engine_epoch: u32,
    pub n_tables: u32,
    pub resume_hold_until: Tsc,
}

/// Section 3 per-table row (32 B), followed by its variable tail.
#[repr(C)]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct TableRow {
    pub venue: VenueId,
    pub account: AccountId,
    pub conn_up: u8,
    pub hedge_gate: u8,
    pub n_slots: u32,
    pub free_len: u32,
    pub sweep_cursor: u32,
    pub unknown_count: u32,
    pub n_foreign: u32,
    pub n_ext_held: u32,
    pub smoke_slot: u32,
}

/// Section 12 row header (8 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LinkRowHdr {
    /// Link 1, Plan 2, Leg 3, Tomb 4, Out 5.
    pub kind: u8,
    pub _pad: [u8; 3],
    pub len: u32,
}
const _: () =
    assert!(size_of::<TableRow>() == 32 && size_of::<LinkRowHdr>() == 8 && size_of::<OmsCountersHdr>() == 16);
const _: () =
    assert!(offset_of!(TableRow, smoke_slot) == 28 && offset_of!(OmsCountersHdr, resume_hold_until) == 8);

/// Section 2 row length: `OrderSlot` (128 B) ‖ `SlotExt` (64 B).
pub const SLOT_ROW_LEN: usize = 192;
/// Section 2 row length of row version 1 (`SlotExt` without `fee_cum`, 32 B).
pub const SLOT_ROW_LEN_V1: usize = 160;
/// Snapshot section numbers (journal-format) and their row versions (registered here).
pub const SECTION_ORDER_SLOTS: u16 = 2;
pub const SECTION_OMS_COUNTERS: u16 = 3;
pub const SECTION_OMS_LINKS: u16 = 12;
pub const ROW_VERSION_SLOTS: u16 = 2;
pub const ROW_VERSION_COUNTERS: u16 = 1;
pub const ROW_VERSION_LINKS: u16 = 1;

/// Section 12 `Plan` row (64 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct PlanRowBody {
    pub link_id: u32,
    pub _pad: u32,
    pub tolerance: Rate,
    pub plan: HedgePlan,
}
/// Section 12 `Leg` row (200 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegRowBody {
    pub account: AccountId,
    pub _pad: [u8; 3],
    pub group: GroupId,
    pub state: LegState,
}
/// Section 12 `Tomb` row (64 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct TombRowBody {
    pub venue: VenueId,
    pub account: AccountId,
    pub _pad: u16,
    pub slot: u32,
    pub tomb: Tomb,
}
const _: () = assert!(
    size_of::<PlanRowBody>() == 64 && size_of::<LegRowBody>() == 200 && size_of::<TombRowBody>() == 64
);

// ---------------------------------------------------------------------------- outputs

/// Fixed-capacity output vector: allocated once (`new`), never grows past its capacity; the
/// sequencer clears it after each input event. `unbounded()` is the reference used by tests.
#[derive(Debug, Clone)]
pub struct FixedVec<T, const N: usize> {
    v: Vec<T>,
    cap: usize,
}

impl<T, const N: usize> FixedVec<T, N> {
    pub fn new() -> Self {
        FixedVec {
            v: Vec::with_capacity(N),
            cap: N,
        }
    }
    /// A vector with capacity `cap` (≤ N keeps the no-allocation guarantee).
    pub fn with_cap(cap: usize) -> Self {
        FixedVec {
            v: Vec::with_capacity(cap.min(N)),
            cap,
        }
    }
    /// Unlimited capacity (reference implementation in tests; may allocate).
    pub fn unbounded() -> Self {
        FixedVec {
            v: Vec::with_capacity(N),
            cap: usize::MAX,
        }
    }
    #[inline]
    pub fn push(&mut self, x: T) -> Result<(), T> {
        if self.v.len() >= self.cap {
            return Err(x);
        }
        self.v.push(x);
        Ok(())
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.v.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }
    #[inline]
    pub fn room(&self) -> usize {
        self.cap.saturating_sub(self.v.len())
    }
    #[inline]
    pub fn is_full(&self) -> bool {
        self.v.len() >= self.cap
    }
    #[inline]
    pub fn clear(&mut self) {
        self.v.clear();
    }
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.v
    }
}

impl<T, const N: usize> Default for FixedVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> core::ops::Deref for FixedVec<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.v
    }
}

/// Timer kinds this crate registers (`Timer{owner = Oms}`; encoding in hft-event).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum OmsTimerKind {
    UnknownQuery,
    HedgeRetry,
    SmokeTtl,
}

/// Register (`cancel = false`) or cancel a `Timer{Oms, …}`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct OmsTimerReq {
    pub kind: OmsTimerKind,
    /// UnknownQuery: `table_ix | slot_id << 8`; HedgeRetry: `link_id`; SmokeTtl: `account | venue << 8`.
    pub id: u32,
    pub deadline_tsc: Tsc,
    pub cancel: bool,
}

impl OmsTimerReq {
    /// `UnknownQuery` id.
    #[inline]
    pub const fn unknown_query_id(table_ix: u16, slot: u32) -> u32 {
        (table_ix as u32 & 0xFF) | slot << 8
    }
    /// Inverse of [`OmsTimerReq::unknown_query_id`].
    #[inline]
    pub const fn split_unknown_query_id(id: u32) -> (u16, u32) {
        ((id & 0xFF) as u16, id >> 8)
    }
    /// `SmokeTtl` id.
    #[inline]
    pub const fn smoke_id(a: AccountId, v: VenueId) -> u32 {
        a.0 as u32 | (v.0 as u32) << 8
    }
    /// Inverse of [`OmsTimerReq::smoke_id`].
    #[inline]
    pub const fn split_smoke_id(id: u32) -> (AccountId, VenueId) {
        (AccountId(id as u8), VenueId((id >> 8) as u8))
    }
}

/// Second leg / retry / reverse request (→ `price_hedge` → risk → `place`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct HedgeRequest {
    pub link_id: u32,
    pub role: LinkRole,
    pub venue: VenueId,
    pub account: AccountId,
    pub side: Side,
    pub reduce_only: bool,
    pub inst: InstId,
    pub group: GroupId,
    pub strategy: StrategyId,
    pub qty: Qty,
    pub basis: Qty,
    pub hedge_cap_qty: Qty,
    pub ref_px: Px,
    pub peer_venue: VenueId,
    pub peer_account: AccountId,
    pub trigger_seq: u64,
}

/// One fill increment (→ hft-ledger `FillDelta`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FillOut {
    pub slot: OrderRef,
    pub account: AccountId,
    pub venue: VenueId,
    pub inst: InstId,
    pub group: GroupId,
    pub leg_ix: u8,
    pub side: Side,
    pub class: u8,
    /// [`fill_flags`] bits.
    pub flags: u8,
    pub qty: Qty,
    /// Signed net quantity (buy +).
    pub net_qty: Qty,
    pub px: Px,
    pub fee: i64,
    pub fee_asset: AssetId,
    pub seq: u64,
}

/// Fee-only report (→ hft-ledger `apply_fee`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FeeOut {
    pub account: AccountId,
    pub venue: VenueId,
    pub inst: InstId,
    pub group: GroupId,
    /// `Side` code; 0 = unknown (unattributed).
    pub side: u8,
    pub fee: i64,
    pub fee_asset: AssetId,
}

/// Link state change (→ hft-exec; Broken → `Broken` cap).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LinkUpdate {
    pub link_id: u32,
    pub account: AccountId,
    pub group: GroupId,
    pub first: OrderRef,
    pub state: LinkState,
    pub broken_reason: LinkBroken,
    /// `RESUME_HOLD` only.
    pub flags: u8,
    pub first_net: Qty,
    pub hedge_filled: Qty,
    pub owed: Qty,
    pub tolerance: Rate,
    pub residual_policy: ResidualPolicy,
}

/// External fill increment from the defensive path or `StaleGen` (→ `ExternalFill{cause: Unknown}`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ForeignFill {
    pub venue: VenueId,
    pub account: AccountId,
    pub inst: InstId,
    pub side: Side,
    pub qty: Qty,
    pub px: Px,
    pub fee: i64,
    pub fee_asset: AssetId,
    pub exch_order_id: u64,
    pub seq: u64,
    pub trade_id: u64,
    pub cum_filled: Qty,
}

/// One gw mirror entry rebuilt after a restore (`GwCmd{MirrorRestore}`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MirrorRestore {
    pub slot: u32,
    pub r#gen: u32,
    pub epoch: u16,
    pub inst: InstId,
    pub exch_order_id: u64,
}

/// Output of one OMS call; capacities are per-call limits, never dropped (see "输出扇出").
#[derive(Debug, Clone)]
pub struct OmsOut {
    pub decisions: FixedVec<OrderDecision, 32>,
    pub hedges: FixedVec<HedgeRequest, 8>,
    pub fills: FixedVec<FillOut, 16>,
    pub fees: FixedVec<FeeOut, 8>,
    pub links: FixedVec<LinkUpdate, 8>,
    pub foreign_fills: FixedVec<ForeignFill, 4>,
    pub notes: FixedVec<Note, 32>,
    pub latency: FixedVec<LatencySample, 4>,
    pub updates: FixedVec<OrderUpdate, 16>,
    pub timers: FixedVec<OmsTimerReq, 8>,
}

impl OmsOut {
    pub fn new() -> OmsOut {
        OmsOut {
            decisions: FixedVec::new(),
            hedges: FixedVec::new(),
            fills: FixedVec::new(),
            fees: FixedVec::new(),
            links: FixedVec::new(),
            foreign_fills: FixedVec::new(),
            notes: FixedVec::new(),
            latency: FixedVec::new(),
            updates: FixedVec::new(),
            timers: FixedVec::new(),
        }
    }
    /// Every vector without a capacity limit (reference implementation; may allocate).
    pub fn unbounded() -> OmsOut {
        OmsOut {
            decisions: FixedVec::unbounded(),
            hedges: FixedVec::unbounded(),
            fills: FixedVec::unbounded(),
            fees: FixedVec::unbounded(),
            links: FixedVec::unbounded(),
            foreign_fills: FixedVec::unbounded(),
            notes: FixedVec::unbounded(),
            latency: FixedVec::unbounded(),
            updates: FixedVec::unbounded(),
            timers: FixedVec::unbounded(),
        }
    }
    pub fn clear(&mut self) {
        self.decisions.clear();
        self.hedges.clear();
        self.fills.clear();
        self.fees.clear();
        self.links.clear();
        self.foreign_fills.clear();
        self.notes.clear();
        self.latency.clear();
        self.updates.clear();
        self.timers.clear();
    }
    /// Whether every vector is empty.
    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty()
            && self.hedges.is_empty()
            && self.fills.is_empty()
            && self.fees.is_empty()
            && self.links.is_empty()
            && self.foreign_fills.is_empty()
            && self.notes.is_empty()
            && self.latency.is_empty()
            && self.updates.is_empty()
            && self.timers.is_empty()
    }
}

impl Default for OmsOut {
    fn default() -> Self {
        Self::new()
    }
}

// ------------------------------------------------------------------------------- inputs

/// Timestamps of the triggering event (written into `SlotStamps`; sequencer fills them from the
/// trigger record's header, `MarketHdr` or report header; 0 = missing).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TrigStamps {
    pub ex_ts_ns: i64,
    pub ex_send_ns: i64,
    pub hw_rx_ns: i64,
    pub sw_rx_ns: i64,
    pub rx_tsc: Tsc,
    pub proc_tsc: Tsc,
    pub trigger_seq: u64,
    /// `EventKind` number of the trigger.
    pub kind: u16,
}

impl Default for TrigStamps {
    fn default() -> Self {
        TrigStamps {
            ex_ts_ns: 0,
            ex_send_ns: 0,
            hw_rx_ns: 0,
            sw_rx_ns: 0,
            rx_tsc: Tsc(0),
            proc_tsc: Tsc(0),
            trigger_seq: 0,
            kind: 0,
        }
    }
}

/// Result of `price_hedge` (copied by the sequencer into `ApprovedIntent.hedge_px`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct HedgePx {
    pub px: Px,
    /// Limited by the cross cap, or priced at the cap because of slippage.
    pub capped: bool,
    /// Cross/Mid priced at the cap because `dev > max_slip`.
    pub slipped: bool,
    pub touch: Px,
    pub cap: Px,
}

impl Default for HedgePx {
    fn default() -> Self {
        HedgePx {
            px: Px(0),
            capped: false,
            slipped: false,
            touch: Px(0),
            cap: Px(0),
        }
    }
}

/// `price_hedge` gating outcome.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum HedgePxErr {
    /// The quote connection is not healthy or one side is missing (a quiet book is fine).
    StaleQuote,
    BeyondCross,
}

/// Why a hedge request did not reach `place` (not counted as a retry).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum HedgeGate {
    Px(HedgePxErr),
    Risk(RejectBits),
}

/// An intent that passed risk (and the mode gate where applicable).
#[derive(Copy, Clone, Debug)]
pub struct ApprovedIntent {
    pub intent: OrderIntent,
    pub origin: Origin,
    /// Owner instance (exec orders: the instance bound to the account).
    pub strategy: StrategyId,
    /// hft-risk `classify()` results.
    pub class: IntentClass,
    pub risk_reducing: bool,
    pub reduce_only: bool,
    /// Leverage (×100) risk used for a perp Open; 0 otherwise.
    pub lev_x100: u16,
    /// Smoke orders only; 0 otherwise.
    pub smoke_ttl_ms: u32,
    /// Pricing result of this crate's hedge requests; zero otherwise.
    pub hedge_px: HedgePx,
    /// First leg of a link: second leg's touch when placing; 0 otherwise.
    pub hedge_ref_px: Px,
    pub trig: TrigStamps,
    /// This crate's hedge requests: the link; 0 otherwise.
    pub link_id: u32,
    /// `OrderDecision.reason` (hft-exec `ExecOrderKind`; 0 otherwise).
    pub reason: u32,
    /// `OrderDecision.intent_no`.
    pub intent_no: u32,
}

impl ApprovedIntent {
    /// A strategy intent with the given classification and no extras.
    pub fn new(
        intent: OrderIntent,
        origin: Origin,
        strategy: StrategyId,
        class: IntentClass,
    ) -> ApprovedIntent {
        ApprovedIntent {
            intent,
            origin,
            strategy,
            class,
            risk_reducing: false,
            reduce_only: false,
            lev_x100: 0,
            smoke_ttl_ms: 0,
            hedge_px: HedgePx::default(),
            hedge_ref_px: Px(0),
            trig: TrigStamps::default(),
            link_id: 0,
            reason: 0,
            intent_no: 0,
        }
    }

    /// The intent of one of this crate's hedge requests priced by `price_hedge` (`hp.px` is the
    /// limit price; tif = the plan's for Second/SecondRetry, Ioc for Reverse).
    pub fn hedge(req: &HedgeRequest, plan: &HedgePlan, hp: HedgePx, class: IntentClass) -> ApprovedIntent {
        let tif = if req.role == LinkRole::Reverse {
            Tif::Ioc
        } else {
            plan.tif
        };
        let link = LegLink {
            group: req.group,
            role: req.role,
            _pad: [0; 3],
            tolerance: Rate(0),
            hedge: *plan,
        };
        let intent = OrderIntent {
            venue: req.venue,
            account: req.account,
            side: req.side,
            tif,
            class,
            flags: if req.reduce_only {
                OrderIntent::REDUCE_ONLY
            } else {
                0
            },
            _pad: [0; 2],
            inst: req.inst,
            group: req.group,
            px: hp.px,
            qty: req.qty,
            link: LegLinkOpt::some(link),
            tag: 0,
            max_rest_ms: 0,
        };
        let mut a = ApprovedIntent::new(intent, Origin::Oms, req.strategy, class);
        a.risk_reducing = true;
        a.reduce_only = req.reduce_only;
        a.hedge_px = hp;
        a.link_id = req.link_id;
        a.trig.trigger_seq = req.trigger_seq;
        a
    }
}

/// Kill / Drain / CancelAll scope (same variants as hft-risk `CancelScope`; the sequencer maps
/// one onto the other).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum CancelScope {
    Global,
    Account(AccountId),
    /// Every account on the venue.
    Venue(VenueId),
    VenueAccount(VenueId, AccountId),
    /// Cell (fields 0 / `GROUP_ALL` = any).
    Cell(CellKey),
    Inst(VenueId, InstId),
}

/// One (strategy, account, group) cell (hft-risk's mode cell).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CellKey {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub group: GroupId,
}

/// `SmokeOrder` command payload.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SmokeReq {
    pub account: AccountId,
    pub venue: VenueId,
    pub inst: InstId,
    pub side: Side,
    pub ttl_ms: u32,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SmokeErr {
    /// The (account, venue) already has a non-terminal smoke order.
    Busy,
    /// No usable quote or the far side is missing.
    NoQuote,
    /// The inst is not on that venue.
    BadInst,
}

/// Result of `write_off_residual` (→ hft-ledger `AdjustDelta::WriteOff`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct WriteOff {
    pub venue: VenueId,
    pub inst: InstId,
    pub leg_ix: u8,
    /// Signed net quantity removed from the excess leg; 0 = no excess leg.
    pub qty: Qty,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PlaceError {
    NoTable,
    NoFreeSlot,
    LinkTableFull,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum OpError {
    StaleRef,
    NotAllowed,
    NotQuiescent,
    NotIdle,
    AmendUnsupported,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum OmsConfigError {
    SlotsOutOfRange { venue: VenueId, account: AccountId },
    TooManyTables,
    TombGens,
}

/// Counters without Notes (dup reports, unmatched generations, recycles…).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct OmsStats {
    pub dup: u64,
    pub stale_gen: u64,
    pub foreign_orders: u64,
    pub recycled: u64,
    pub unmatched_table: u64,
}

/// In-flight quantities of one (account, inst).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InflightPos {
    pub open_buy: Qty,
    pub open_sell: Qty,
    pub imm_buy: Qty,
    pub imm_sell: Qty,
}

impl InflightPos {
    pub const ZERO: InflightPos = InflightPos {
        open_buy: Qty(0),
        open_sell: Qty(0),
        imm_buy: Qty(0),
        imm_sell: Qty(0),
    };
}
