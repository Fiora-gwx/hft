//! Events delivered to strategies (README "EventRef"): `EventRef`, `OrderUpdate`, `ExecUpdate`,
//! `GIX_NONE`, `MODEL_TICK`. Which part receives which variant is the delivery table in the
//! README (the advisor builds Model input from it).

use hft_event::ConnStateKind;
use hft_types::{AccountId, AssetId, GroupId, InstId, Px, Qty, RejectBits, RunMode, Tsc, VenueId};

use crate::intent::OrderRef;
use crate::view_types::{
    BboView, BrokenReason, ExternalFillView, FundingView, LiquidationView, RiskEvent, StuckReason,
    TargetOrigin, TradeView, VenueNoticeView,
};

/// The event does not target a group (`GROUP_NONE`, unattributable external fills).
pub const GIX_NONE: u16 = u16::MAX;

/// Timer tag of the model tick (`SubsBuilder::model_tick`); reserved, strategies do not register
/// timers with it.
pub const MODEL_TICK: u32 = u32::MAX;

/// One event for `Strategy::on_event` / `Model::on_event`. Non-exhaustive: every `match` needs a
/// fallback arm.
#[derive(Copy, Clone, Debug)]
#[non_exhaustive]
pub enum EventRef<'a> {
    /// By value; equals `v.bbo(inst)` after this event.
    Bbo {
        inst: InstId,
        gix: u16,
        b: BboView,
    },
    /// After one `L2Batch` or exchange `L2Snapshot` is applied; read through `v.book()`.
    Book {
        inst: InstId,
        gix: u16,
    },
    Trade {
        inst: InstId,
        gix: u16,
        t: &'a TradeView,
    },
    Funding {
        inst: InstId,
        gix: u16,
        f: &'a FundingView,
    },
    Mark {
        inst: InstId,
        gix: u16,
        px: Px,
    },
    /// `IndexPrice` (subscribe `EvMask::INDEX`).
    Index {
        inst: InstId,
        gix: u16,
        px: Px,
    },
    /// Public-market liquidations (subscribe `EvMask::LIQUIDATION`).
    Liquidation {
        inst: InstId,
        gix: u16,
        l: &'a LiquidationView,
    },
    Order {
        gix: u16,
        u: &'a OrderUpdate,
    },
    Exec {
        gix: u16,
        u: &'a ExecUpdate,
    },
    /// `gix = GIX_NONE` when the fill cannot be attributed to a group.
    External {
        gix: u16,
        f: &'a ExternalFillView,
    },
    Risk {
        a: AccountId,
        e: &'a RiskEvent,
    },
    Notice {
        inst: InstId,
        gix: u16,
        n: &'a VenueNoticeView,
    },
    /// Registered by `GroupId`; `gix` looked up at delivery. `tag = MODEL_TICK` is the model tick.
    Timer {
        tag: u32,
        group: GroupId,
        gix: u16,
    },
    /// Output of this instance's Model; `group = GROUP_NONE` → `gix = GIX_NONE`.
    Advice {
        gix: u16,
        local_kind: u16,
        payload: &'a [u8],
        /// Journal seq the Model had consumed when producing it (`AdviceHdr.basis_seq`).
        basis_seq: u64,
        /// Record-header tsc of that record; expiry is judged by it, not by arrival.
        basis_tsc: Tsc,
    },
    Mode {
        gix: u16,
        old: RunMode,
        new: RunMode,
    },
    Conn {
        venue: VenueId,
        state: ConnStateKind,
    },
    /// `StrategyCmd` (≤ 256 B, parse with `CmdView::parse`); `GROUP_NONE` → `GIX_NONE`.
    Manual {
        gix: u16,
        payload: &'a [u8],
        reduce_only: bool,
    },
    /// `InjectWarmup` payload (≤ 4096 B, strategy-defined).
    Warmup {
        local_kind: u16,
        chunk_no: u32,
        last: bool,
        payload: &'a [u8],
    },
}

/// What happened to an order of this instance.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum OrderUpdateKind {
    Acked,
    Filled {
        /// Net quantity of this fill.
        d: Qty,
        px: Px,
        fee: i64,
        fee_asset: AssetId,
        cum: Qty,
        done: bool,
    },
    Cancelled,
    Rejected {
        bits: RejectBits,
        /// Exchange rejects: `OrderEvent.reject_code`; local send failures (`LOCAL_SEND_FAIL`):
        /// the hft-event `SendStatus` code.
        venue_code: u32,
    },
    Unknown,
    /// Rested longer than `max_rest_ms`.
    Aged,
}

/// `EventRef::Order` payload.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct OrderUpdate {
    pub r: OrderRef,
    pub group: GroupId,
    pub tag: u32,
    pub kind: OrderUpdateKind,
}

/// Progress of a target.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum ExecUpdateKind {
    /// Once per slice reaching a terminal state with a fill (engine orders too).
    Progress { leg_ix: u8, d: Qty },
    /// Target reached, or cleared by the engine; also when Broken is released back to Idle.
    Done { cleared_by_engine: bool },
    /// On entering Stuck and on each reason change.
    Stuck(StuckReason),
    /// On entering Broken (not terminal).
    Broken(BrokenReason),
}

/// `EventRef::Exec` payload.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExecUpdate {
    pub group: GroupId,
    pub tag: u32,
    pub origin: TargetOrigin,
    pub kind: ExecUpdateKind,
    pub target: [Qty; 4],
    pub filled: [Qty; 4],
    /// Immediate in-flight per leg (`LegTrack.inflight_immediate`, excludes acked resting orders).
    pub inflight: [Qty; 4],
}
