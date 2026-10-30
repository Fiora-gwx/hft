//! Read-only view types (README "视图类型"). The SDK defines them; the engine component named in
//! each doc comment is their only writer. Amounts are `Notional` (USDT 1e-6), quantities `Qty`
//! (asset exponent), ratios `Rate` (1e-9).
// zerocopy's derive helpers carry no Debug impl.
#![allow(missing_debug_implementations)]

use core::mem::{align_of, offset_of, size_of};

use hft_types::{
    AccountId, AssetId, CapSource, GroupId, InstId, Notional, Px, Qty, Rate, Side, StrategyId, Tif, Tsc,
    VenueId,
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes};

pub use hft_event::ExternalCause;
pub use hft_refdata::FeePair as FeeView;

use crate::intent::{IntentClass, OrderRef};
use crate::target::TargetIntent;

// ------------------------------------------------------------------------------------------ BBO

/// Source of `BboView.hw_rx_ns` (projected from the Bbo record header's `HW_TS`/`SW_TS` bits).
#[repr(u8)]
#[derive(
    Copy, Clone, Eq, PartialEq, Hash, Debug, Default, TryFromBytes, IntoBytes, KnownLayout, Immutable,
)]
pub enum RxTsSrc {
    /// No receive stamp.
    #[default]
    None = 0,
    /// NIC PHC stamp.
    Hw = 1,
    /// Kernel software stamp (CLOCK_REALTIME).
    Sw = 2,
}

/// Latest quote of an inst plus the aggregated health of its BBO member connections (by value;
/// writer: sequencer). Quote availability is [`BboView::quote_live`], never the quote's age.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct BboView {
    pub bid_px: Px,
    pub bid_qty: Qty,
    pub ask_px: Px,
    pub ask_qty: Qty,
    pub update_id: u64,
    pub ex_ts_ns: i64,
    /// `MarketHdr.rx_tsc` of the latest applied Bbo; 0 = unknown (after a keyframe, before this
    /// run's first Bbo of the inst). Not an availability signal.
    pub rx_tsc: Tsc,
    /// `MarketHdr.hw_rx_ns` of the same Bbo (PHC domain, not UTC).
    pub hw_rx_ns: i64,
    pub rx_ts_src: RxTsSrc,
    /// Seq of that Bbo record.
    pub seq: u64,
    /// Latest liveness evidence (`rx_tsc`, this run's TSC) of any member connection; 0 = none.
    pub conn_live_tsc: Tsc,
    /// Some member connection is up.
    pub conn_up: bool,
    /// The quote is resyncing or has no healthy member; `quote_live` is then false.
    pub resync: bool,
}

impl BboView {
    /// All zero: no quote, no connection.
    pub const EMPTY: BboView = BboView {
        bid_px: Px(0),
        bid_qty: Qty(0),
        ask_px: Px(0),
        ask_qty: Qty(0),
        update_id: 0,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        hw_rx_ns: 0,
        rx_ts_src: RxTsSrc::None,
        seq: 0,
        conn_live_tsc: Tsc(0),
        conn_up: false,
        resync: false,
    };

    /// Quote health, the system's single definition: `conn_up && !resync &&
    /// now − conn_live_tsc ≤ max_silence` (TSC ticks; boundary inclusive). How long the quote has
    /// not changed does not matter (quiet ≠ stale). `conn_live_tsc = 0` (no evidence) or a
    /// `conn_live_tsc` later than `now` with no evidence is handled conservatively: 0 → false.
    #[inline]
    pub const fn quote_live(&self, now: Tsc, max_silence: u64) -> bool {
        if !self.conn_up || self.resync || self.conn_live_tsc.0 == 0 {
            return false;
        }
        match now.0.checked_sub(self.conn_live_tsc.0) {
            Some(age) => age <= max_silence,
            // Evidence stamped after `now` (same run, read order): silence is 0.
            None => true,
        }
    }
}

// -------------------------------------------------------------------------------- market views

/// Funding state of a perp inst (writer: sequencer, `Funding`/`MarkPrice` events).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FundingView {
    /// Current (predicted) rate per period.
    pub rate: Rate,
    /// Next settlement, UTC ns.
    pub next_settle_ns: i64,
    /// Actual settlement period (from the event, not the refdata default).
    pub interval_s: u32,
    /// Previous different period; `≠ interval_s` means the period just changed; never changed =
    /// `interval_s`.
    pub prev_interval_s: u32,
    /// `MarketHdr.rx_tsc` of the latest applied Funding; 0 = unknown (treat as no data).
    pub upd_tsc: Tsc,
}

impl FundingView {
    pub const EMPTY: FundingView = FundingView {
        rate: Rate(0),
        next_settle_ns: 0,
        interval_s: 0,
        prev_interval_s: 0,
        upd_tsc: Tsc(0),
    };
}

/// One public trade.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TradeView {
    pub px: Px,
    pub qty: Qty,
    pub side: Side,
    pub ex_ts_ns: i64,
}

/// Venue and receive stamps of one market-data message of an inst (its `MarketHdr`), kept by the
/// engine for `Views::md_stamp`: an L2 batch, an exchange or top-N L2 snapshot (also the exchange
/// snapshots that deliver no `EventRef::Book`), or a public trade; `Views::bbo_stamp` gives the
/// last applied Bbo in the same form.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MdStampView {
    /// `MD_BATCH`, `MD_SNAPSHOT`, `MD_TRADE`, or `MD_BBO` (`Views::bbo_stamp` only).
    pub kind: u8,
    /// `MarketHdr.ex_ts_ns`: the venue's message time (UTC ns; 0 = the venue gave none).
    pub ex_ts_ns: i64,
    /// `MarketHdr.sw_rx_ns`: the kernel software receive stamp (CLOCK_REALTIME ns; 0 = unavailable).
    pub sw_rx_ns: i64,
    /// `MarketHdr.rx_tsc`: the receiving thread's TSC (this run; `Now::utc_of` converts it).
    pub rx_tsc: Tsc,
}

impl MdStampView {
    /// No message (all zero).
    pub const NONE: MdStampView = MdStampView {
        kind: 0,
        ex_ts_ns: 0,
        sw_rx_ns: 0,
        rx_tsc: Tsc(0),
    };
    /// An `L2Batch`.
    pub const MD_BATCH: u8 = 1;
    /// An `L2Snapshot` of origin exchange or top-N (keyframes are engine state, not market data).
    pub const MD_SNAPSHOT: u8 = 2;
    /// A `Trade`.
    pub const MD_TRADE: u8 = 3;
    /// A `Bbo` that updated the quote (duplicates and older updates from another member
    /// connection are not recorded).
    pub const MD_BBO: u8 = 4;
}

/// The time stamps of the Input being delivered (`Views::stamps`): the venue's time, the local
/// receive stamps and the engine clock. Strategies decide on the engine clock and may use the
/// others as data (feed lag, venue-time phase).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Stamps {
    /// `EventKind` number of the Input.
    pub kind: u16,
    /// Venue time: `MarketHdr.ex_ts_ns` or `OrderEvent.ex_ts_ns` (UTC ns); 0 = none.
    pub venue_ns: i64,
    /// Kernel software receive stamp (CLOCK_REALTIME ns); 0 = unavailable.
    pub sw_rx_ns: i64,
    /// Hardware receive stamp (PHC domain, not UTC); 0 = unavailable.
    pub hw_rx_ns: i64,
    /// The receiving thread's TSC (this run); 0 = not a received message.
    pub rx_tsc: Tsc,
    /// `Now.engine_ns` of the Input.
    pub engine_ns: i64,
    /// `Now.utc_ns` of the Input.
    pub utc_ns: i64,
}

impl Stamps {
    /// All zero.
    pub const NONE: Stamps = Stamps {
        kind: 0,
        venue_ns: 0,
        sw_rx_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: Tsc(0),
        engine_ns: 0,
        utc_ns: 0,
    };
}

/// One public-market liquidation (not this account's).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LiquidationView {
    pub px: Px,
    pub qty: Qty,
    pub side: Side,
    pub ex_ts_ns: i64,
}

// ---------------------------------------------------------------------------------- portfolio

/// Set of `VenueId`s (bit v = venue v; same encoding as `AccountMeta.venues`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct VenueSet(pub u64);

impl VenueSet {
    pub const EMPTY: VenueSet = VenueSet(0);
    /// Whether `v` is in the set (`v ≥ 64` → false).
    pub const fn contains(self, v: VenueId) -> bool {
        v.0 < 64 && self.0 & (1u64 << v.0) != 0
    }
    /// The set with `v` added (`v ≥ 64` → unchanged).
    #[must_use]
    pub const fn with(self, v: VenueId) -> VenueSet {
        if v.0 < 64 {
            VenueSet(self.0 | (1u64 << v.0))
        } else {
            self
        }
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }
    /// Venues in ascending id order.
    pub fn iter(self) -> impl Iterator<Item = VenueId> {
        (0u8..64).filter(move |v| self.0 & (1u64 << v) != 0).map(VenueId)
    }
}

/// Per-venue equity summary row (56 B; writer: hft-ledger, maintained incrementally).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct VenueEquityView {
    pub venue: VenueId,
    pub _pad0: u8,
    /// [`VenueEquityView::RATE_STALE`] | [`VenueEquityView::MARGIN_STALE`] |
    /// [`VenueEquityView::BALANCE_UNRECONCILED`].
    pub flags: u16,
    pub _pad1: u32,
    /// Spot at bid, perps at mark (valuation rules are ledger parameters).
    pub equity: Notional,
    pub quote_cash: Notional,
    pub spot_value: Notional,
    pub perp_upnl: Notional,
    /// = wallet + Σ unrealised.
    pub margin_balance: Notional,
    /// Data time (engine_ns, comparable across restarts).
    pub as_of_ens: i64,
}
const _: () = assert!(size_of::<VenueEquityView>() == 56);
const _: () = assert!(align_of::<VenueEquityView>() == 8);
const _: () = assert!(offset_of!(VenueEquityView, flags) == 2);
const _: () = assert!(offset_of!(VenueEquityView, equity) == 8);
const _: () = assert!(offset_of!(VenueEquityView, margin_balance) == 40);
const _: () = assert!(offset_of!(VenueEquityView, as_of_ens) == 48);

impl VenueEquityView {
    pub const RATE_STALE: u16 = 1;
    pub const MARGIN_STALE: u16 = 2;
    pub const BALANCE_UNRECONCILED: u16 = 4;
    /// All zero for `venue`.
    pub const fn empty(venue: VenueId) -> VenueEquityView {
        VenueEquityView {
            venue,
            _pad0: 0,
            flags: 0,
            _pad1: 0,
            equity: Notional(0),
            quote_cash: Notional(0),
            spot_value: Notional(0),
            perp_upnl: Notional(0),
            margin_balance: Notional(0),
            as_of_ens: 0,
        }
    }
}

/// Cash of one asset on one venue (writer: hft-ledger).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CashView {
    pub free: Qty,
    pub locked: Qty,
}

/// Spot inventory of one asset on one venue (writer: hft-ledger).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InventoryView {
    pub total: Qty,
    /// Σ assigned to groups.
    pub assigned: Qty,
    pub unassigned: Qty,
    /// Non-strategy assets registered by `SetBalanceBaseline`.
    pub baseline: Qty,
}

/// One perp position (writer: hft-ledger).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PerpPosView {
    /// Signed.
    pub qty: Qty,
    pub entry_px: Px,
    /// At mark.
    pub upnl: Notional,
    /// From `MarginReport`; 0 = unknown.
    pub liq_px: Px,
    /// From `MarginReport`; 0 = unknown.
    pub adl_quantile: u8,
}

/// Margin of one venue (writer: hft-ledger; `stale` judged by the sequencer adapter).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarginView {
    pub margin_balance: Notional,
    pub maint_margin: Notional,
    pub avail_margin: Notional,
    /// Maintenance / margin balance (venue conversions live in `venue-*`).
    pub margin_ratio: Rate,
    /// Engine time of the source `MarginReport`.
    pub src_ens: i64,
    /// No report, report older than this run, or older than `margin_stale_ms`.
    pub stale: bool,
}

/// Equity view of one account (borrowing hft-ledger's per-venue books). The engine computes no
/// "available funds" or leverage: strategies compute those from here. Cash per venue is exact
/// (an account never holds two markets of one exchange).
///
/// A trait (like [`crate::BookRead`]) because the SDK cannot name hft-ledger's types and may not
/// use `dyn`; `Views::portfolio` returns the implementor `V::Portfolio<'_>`.
pub trait PortfolioRead {
    fn account(&self) -> AccountId;
    /// Venues of the account (`AccountMeta.venues`).
    fn venues(&self) -> VenueSet;
    /// Per-venue summary row (all zero for a venue outside the account).
    fn venue(&self, v: VenueId) -> &VenueEquityView;
    fn cash(&self, v: VenueId, asset: AssetId) -> CashView;
    fn inventory(&self, v: VenueId, asset: AssetId) -> InventoryView;
    /// Spot net quantity assigned to group `g` on venue `v`.
    fn inventory_of(&self, v: VenueId, g: GroupId) -> Qty;
    fn perp(&self, v: VenueId, i: InstId) -> PerpPosView;
    fn margin(&self, v: VenueId) -> MarginView;
    /// Σ `venue(v).equity` over `venues()`.
    fn equity_total(&self) -> Notional;
}

/// Cumulative PnL breakdown of one (account, group) (writer: hft-ledger).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PnlSummary {
    pub price_realized: Notional,
    pub funding: Notional,
    pub fees: Notional,
    pub unrealized: Notional,
}

/// One leg's position inside `GroupPosView`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LegPos {
    /// Net quantity after base-asset fees.
    pub qty: Qty,
    pub entry_px: Px,
    pub upnl: Notional,
}

/// Position of one (account, group) (writer: hft-ledger).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GroupPosView {
    pub legs: [LegPos; 4],
    pub n_legs: u8,
    /// [`GroupPosView::ENTRY_ADOPTED`].
    pub flags: u16,
    pub held_notional: Notional,
    pub pnl: PnlSummary,
    /// engine_ns (may precede this run).
    pub round_start_ens: i64,
    pub last_fill_ens: i64,
}

impl GroupPosView {
    /// Contains `AdoptExchange` quantity: entry prices are not fully from fills.
    pub const ENTRY_ADOPTED: u16 = 1;
    pub const EMPTY: GroupPosView = GroupPosView {
        legs: [LegPos {
            qty: Qty(0),
            entry_px: Px(0),
            upnl: Notional(0),
        }; 4],
        n_legs: 0,
        flags: 0,
        held_notional: Notional(0),
        pnl: PnlSummary {
            price_realized: Notional(0),
            funding: Notional(0),
            fees: Notional(0),
            unrealized: Notional(0),
        },
        round_start_ens: 0,
        last_fill_ens: 0,
    };
}

// ------------------------------------------------------------------------------- leg tracking

/// One leg of `LegView` (32 B). The split between the two in-flight kinds is defined once in
/// hft-oms "LegTracker"; a non-terminal slot's remainder counts in exactly one of them.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegTrack {
    pub net: Qty,
    /// IOC/FOK, GTC/PostOnly before Ack, Unknown slots; naked quantity and repair suppression
    /// only look at this.
    pub inflight_immediate: Qty,
    /// Acked GTC/PostOnly (also during PendingCancel/PendingAmend); never hedge coverage.
    pub inflight_resting: Qty,
    pub residual: Qty,
}
const _: () = assert!(size_of::<LegTrack>() == 32);
const _: () = assert!(align_of::<LegTrack>() == 8);
const _: () = assert!(offset_of!(LegTrack, inflight_immediate) == 8);
const _: () = assert!(offset_of!(LegTrack, inflight_resting) == 16);
const _: () = assert!(offset_of!(LegTrack, residual) == 24);

impl LegTrack {
    pub const ZERO: LegTrack = LegTrack {
        net: Qty(0),
        inflight_immediate: Qty(0),
        inflight_resting: Qty(0),
        residual: Qty(0),
    };
}

/// Legs of one (account, group) (160 B; writer: hft-oms LegTracker).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, TryFromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct LegView {
    pub leg: [LegTrack; 4],
    /// Includes immediate in-flight, excludes resting orders.
    pub naked_qty: Qty,
    pub naked_notional: Notional,
    pub naked_ratio: Rate,
    pub n_legs: u8,
    pub open_links: u8,
    /// The group has an Unknown slot.
    pub has_unknown: bool,
    /// The group has a non-terminal PendingNew/PendingCancel/PendingAmend slot.
    pub has_pending: bool,
    pub _pad: [u8; 4],
}
const _: () = assert!(size_of::<LegView>() == 160);
const _: () = assert!(align_of::<LegView>() == 8);
const _: () = assert!(offset_of!(LegView, naked_qty) == 128);
const _: () = assert!(offset_of!(LegView, naked_ratio) == 144);
const _: () = assert!(offset_of!(LegView, n_legs) == 152);
const _: () = assert!(offset_of!(LegView, has_unknown) == 154);
const _: () = assert!(offset_of!(LegView, has_pending) == 155);

impl LegView {
    pub const EMPTY: LegView = LegView {
        leg: [LegTrack::ZERO; 4],
        naked_qty: Qty(0),
        naked_notional: Notional(0),
        naked_ratio: Rate(0),
        n_legs: 0,
        open_links: 0,
        has_unknown: false,
        has_pending: false,
        _pad: [0; 4],
    };
}

// ------------------------------------------------------------------------------ target state

/// Reason a target is Stuck; discriminants equal hft-exec `ExecDecision.detail` table 2
/// (append-only). `Gated` carries the gate's `RejectBits` (`ExecDecision.gate`).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum StuckReason {
    Gated(hft_types::RejectBits) = 1,
    Shadow = 2,
    NoLiquidity = 3,
    StaleQuote = 4,
    SliceFails = 5,
    External = 6,
    EngineOverride = 7,
    MinOrder = 8,
}

impl StuckReason {
    /// Wire code (`detail` table 2 `code`).
    pub const fn code(&self) -> u8 {
        match self {
            StuckReason::Gated(_) => 1,
            StuckReason::Shadow => 2,
            StuckReason::NoLiquidity => 3,
            StuckReason::StaleQuote => 4,
            StuckReason::SliceFails => 5,
            StuckReason::External => 6,
            StuckReason::EngineOverride => 7,
            StuckReason::MinOrder => 8,
        }
    }
    /// From the wire code and `ExecDecision.gate` (used by `Gated` only); unknown → `None`.
    pub const fn from_code(code: u8, gate: hft_types::RejectBits) -> Option<StuckReason> {
        Some(match code {
            1 => StuckReason::Gated(gate),
            2 => StuckReason::Shadow,
            3 => StuckReason::NoLiquidity,
            4 => StuckReason::StaleQuote,
            5 => StuckReason::SliceFails,
            6 => StuckReason::External,
            7 => StuckReason::EngineOverride,
            8 => StuckReason::MinOrder,
            _ => return None,
        })
    }
}

/// Reason a cell is Broken; discriminants equal hft-exec `ExecDecision.detail` table 3
/// (append-only). Broken is not terminal (see README "目标意图").
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum BrokenReason {
    LinkBroken = 1,
    ReverseFailed = 2,
    FixExhausted = 3,
    HedgeVeto = 4,
}

impl BrokenReason {
    pub const fn code(self) -> u8 {
        self as u8
    }
    pub const fn from_code(code: u8) -> Option<BrokenReason> {
        Some(match code {
            1 => BrokenReason::LinkBroken,
            2 => BrokenReason::ReverseFailed,
            3 => BrokenReason::FixExhausted,
            4 => BrokenReason::HedgeVeto,
            _ => return None,
        })
    }
}

/// State of a target cell (writer: hft-exec); wire code = `ExecDecision.state_from/state_to`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TargetState {
    Idle,
    Working,
    Stuck(StuckReason),
    Broken(BrokenReason),
}

impl TargetState {
    /// `TargetState` wire code: Idle 1, Working 2, Stuck 3, Broken 4.
    pub const fn code(&self) -> u8 {
        match self {
            TargetState::Idle => 1,
            TargetState::Working => 2,
            TargetState::Stuck(_) => 3,
            TargetState::Broken(_) => 4,
        }
    }
}

/// Which engine path set a target.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum EngineOrigin {
    MarginStress,
    External,
    Command,
    KillFlatten,
    AutoFix,
}

/// Who set the current target / produced the order (writer: hft-exec). Wire form
/// [`hft_event::TargetOriginWire`] (`ExecDecision.origin`); the conversions are one-to-one.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TargetOrigin {
    Strategy,
    Engine(EngineOrigin),
}

impl From<TargetOrigin> for hft_event::TargetOriginWire {
    fn from(o: TargetOrigin) -> Self {
        use hft_event::TargetOriginWire as W;
        match o {
            TargetOrigin::Strategy => W::Strategy,
            TargetOrigin::Engine(EngineOrigin::MarginStress) => W::EngineMarginStress,
            TargetOrigin::Engine(EngineOrigin::External) => W::EngineExternal,
            TargetOrigin::Engine(EngineOrigin::Command) => W::EngineCommand,
            TargetOrigin::Engine(EngineOrigin::KillFlatten) => W::EngineKillFlatten,
            TargetOrigin::Engine(EngineOrigin::AutoFix) => W::EngineAutoFix,
        }
    }
}

impl From<hft_event::TargetOriginWire> for TargetOrigin {
    fn from(w: hft_event::TargetOriginWire) -> Self {
        use hft_event::TargetOriginWire as W;
        match w {
            W::Strategy => TargetOrigin::Strategy,
            W::EngineMarginStress => TargetOrigin::Engine(EngineOrigin::MarginStress),
            W::EngineExternal => TargetOrigin::Engine(EngineOrigin::External),
            W::EngineCommand => TargetOrigin::Engine(EngineOrigin::Command),
            W::EngineKillFlatten => TargetOrigin::Engine(EngineOrigin::KillFlatten),
            W::EngineAutoFix => TargetOrigin::Engine(EngineOrigin::AutoFix),
        }
    }
}

/// Projection of one target cell (writer: hft-exec).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TargetView {
    /// Current target (copy).
    pub intent: TargetIntent,
    pub origin: TargetOrigin,
    pub state: TargetState,
    pub set_seq: u64,
    pub set_tsc: Tsc,
    pub last_slice_tsc: Tsc,
    pub n_slices: u32,
    /// Net fill per leg since this target took effect (signed).
    pub filled: [Qty; 4],
}

// ------------------------------------------------------------------------------------- orders

/// State of one order slot as seen by the strategy.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum OrderStateView {
    PendingNew,
    Live,
    PendingCancel,
    PendingAmend,
    Filled,
    Cancelled,
    Rejected,
    Unknown,
}

/// One order this instance placed (writer: hft-oms).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct OrderView {
    pub r: OrderRef,
    pub group: GroupId,
    pub inst: InstId,
    pub side: Side,
    pub tif: Tif,
    pub class: IntentClass,
    pub px: Px,
    pub qty: Qty,
    pub filled: Qty,
    pub state: OrderStateView,
    pub tag: u32,
}

/// One external fill of this account (from hft-event `ExternalFill`): liquidation, ADL, manual,
/// delivery, or auto-booked missing fills (`Unknown`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExternalFillView {
    pub venue: VenueId,
    pub inst: InstId,
    /// `GROUP_NONE` when it cannot be attributed.
    pub group: GroupId,
    pub leg_ix: u8,
    pub side: Side,
    pub qty: Qty,
    pub px: Px,
    pub fee: i64,
    pub fee_asset: AssetId,
    pub cause: ExternalCause,
    pub ex_ts_ns: i64,
    /// This run's TSC; handle the group before it or hft-exec applies its default (nominal; 0 = the
    /// engine applies no default, e.g. `cause = Manual`). Save with `Now::ens_of`.
    pub deadline_tsc: Tsc,
}

// --------------------------------------------------------------------------------------- risk

/// Margin stress level of a venue or account (levels by distance to liquidation, hft-risk).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub enum MarginLevel {
    #[default]
    None = 0,
    L1 = 1,
    L2 = 2,
    L3 = 3,
}

/// Scope of a cap (same fields as hft-event `CmdScope`; 0 = any).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CapScopeView {
    pub strategy: StrategyId,
    pub venue: VenueId,
    pub account: AccountId,
    pub group: GroupId,
    pub inst: InstId,
}

/// Bitmap of active cap sources (bit n = `CapSource(n)`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct CapBits(pub u32);

impl CapBits {
    pub const fn contains(self, s: CapSource) -> bool {
        s.0 < 32 && self.0 & (1u32 << s.0) != 0
    }
    #[must_use]
    pub const fn with(self, s: CapSource) -> CapBits {
        if s.0 < 32 {
            CapBits(self.0 | (1u32 << s.0))
        } else {
            self
        }
    }
}

/// Risk event delivered to the fast part (source: hft-risk cap changes).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum RiskEvent {
    /// One per (account, venue) when that venue's level changes (back to L0: `level = None`).
    MarginStress {
        venue: VenueId,
        level: MarginLevel,
        /// min over the venue's perp positions of |liq_px − mark| / mark; unknown = `Rate::MAX`.
        liq_dist: Rate,
        /// Maintenance / margin balance (cross-check).
        ratio: Rate,
        /// L2 reduce deadline (this run's TSC); still ≥ L2 then → engine forced reduction.
        deadline_tsc: Tsc,
    },
    HedgeBroken {
        group: GroupId,
    },
    CapSet {
        source: CapSource,
        scope: CapScopeView,
    },
    /// MarginStress release also arrives here.
    CapCleared {
        source: CapSource,
        scope: CapScopeView,
    },
}

/// Risk state of one account (writer: hft-risk).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct RiskStateView {
    /// Max over the account's venues.
    pub margin_level: MarginLevel,
    /// Min over the account's venues; unknown = `Rate::MAX`.
    pub liq_dist: Rate,
    pub margin_ratio: Rate,
    pub deadline_tsc: Tsc,
    /// Some active cap blocks this account's Open.
    pub open_blocked: bool,
    pub caps: CapBits,
    pub hard: bool,
    /// Flatten lock (`Kill{flatten}`): only targets closer to 0 and Reduce/Flatten are accepted.
    pub flatten_lock: bool,
    pub max_hedge_notional: Notional,
    pub max_reduce_notional: Notional,
    /// Per-order Open cap (check #6; notional = qty × limit).
    pub max_order_notional: Notional,
    /// Connection silence limit of quote health (hft-risk `RiskParams`).
    pub conn_liveness_ms: u32,
    /// The account's dust line (single definition hft-exec `ExecConfig.dust_notional`).
    pub dust_notional: Notional,
}

impl RiskStateView {
    /// Neutral state (no caps, no stress, zero limits).
    pub const EMPTY: RiskStateView = RiskStateView {
        margin_level: MarginLevel::None,
        liq_dist: Rate::MAX,
        margin_ratio: Rate(0),
        deadline_tsc: Tsc(0),
        open_blocked: false,
        caps: CapBits(0),
        hard: false,
        flatten_lock: false,
        max_hedge_notional: Notional(0),
        max_reduce_notional: Notional(0),
        max_order_notional: Notional(0),
        conn_liveness_ms: 0,
        dust_notional: Notional(0),
    };
}

/// Per-inst risk limits (writer: hft-risk; the inst's price-band class and `RiskParams`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InstRiskView {
    /// Open price band around mid (check #5).
    pub max_px_dev: Rate,
    /// Open IOC cross cap against the touch (check #5).
    pub ioc_max_cross: Rate,
    /// Risk-reducing cross cap around mid (engine orders and hedge legs via `cap_cross_px`).
    pub rr_max_cross: Rate,
    /// Optional wider L3 cap (global; 0 = disabled).
    pub l3_max_cross: Rate,
    /// Quote asset → USDT rate (hft-ledger; same basis as check #6).
    pub quote_rate: Rate,
    /// The inst's (venue, account) is in rate-limit backoff.
    pub rate_backoff: bool,
}

impl InstRiskView {
    pub const EMPTY: InstRiskView = InstRiskView {
        max_px_dev: Rate(0),
        ioc_max_cross: Rate(0),
        rr_max_cross: Rate(0),
        l3_max_cross: Rate(0),
        quote_rate: Rate(0),
        rate_backoff: false,
    };
}

// ------------------------------------------------------------------------------------ notices

/// Kind of a venue notice (`VenueNotice` command; wire codes Delisting 1, FundingInterval 2,
/// Maintenance 3).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum NoticeKind {
    Delisting { at_ns: i64 },
    FundingInterval { new_s: u32 },
    Maintenance { from_ns: i64, to_ns: i64 },
}

/// One venue notice (source `ControlCmd{VenueNotice}`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueNoticeView {
    pub venue: VenueId,
    /// `INST_ALL` = the whole venue.
    pub inst: InstId,
    pub kind: NoticeKind,
}
