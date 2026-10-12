//! Public value types of the ledger: deltas in (`FillDelta`, `FeeDelta`, `AdjustDelta`,
//! `TransferDelta`), results out, and the books read through the `Ledger` accessors.

use hft_event::ExternalCause;
use hft_types::{AccountId, AssetId, GroupId, InstId, Notional, Px, Qty, Rate, Side, VenueId};

use crate::fixed::FixedVec;

/// Per-venue cash entries (compile-time constant, README "数据结构").
pub const MAX_ASSETS_PER_VENUE: usize = 64;
/// Per-venue spot inventory attributions (compile-time constant).
pub const MAX_INV_PER_VENUE: usize = 128;
/// Margin report items kept per venue.
pub const MAX_MARGIN_ITEMS: usize = 64;

/// `FillDelta.class` codes.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FillClass {
    Open = 1,
    Reduce = 2,
    Hedge = 3,
    Flatten = 4,
    Reversal = 5,
    Adjust = 6,
    External = 7,
}

impl FillClass {
    pub const ALL: [FillClass; 7] = [
        FillClass::Open,
        FillClass::Reduce,
        FillClass::Hedge,
        FillClass::Flatten,
        FillClass::Reversal,
        FillClass::Adjust,
        FillClass::External,
    ];
}

impl TryFrom<u8> for FillClass {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        FillClass::ALL
            .get(usize::from(v).wrapping_sub(1))
            .copied()
            .ok_or(v)
    }
}

/// `FillDelta.flags` bits (copied from OMS `FillOut`).
pub mod fill_flags {
    pub const PX_EST: u8 = 1;
    pub const LATE: u8 = 2;
    pub const OVERFILL: u8 = 4;
    pub const EXT_DEDUP: u8 = 8;
}

/// CEX fill increment (sequencer builds it from OMS `FillOut`; the cold reader from `OrderEvent`).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FillDelta {
    pub account: AccountId,
    pub venue: VenueId,
    pub side: Side,
    /// `FillClass` code.
    pub class: u8,
    /// `fill_flags` bits.
    pub flags: u8,
    pub leg_ix: u8,
    pub _pad: [u8; 2],
    pub inst: InstId,
    pub group: GroupId,
    /// Gross quantity of this fill (> 0).
    pub qty: Qty,
    /// Absolute change of the base-asset holding (net-quantity rule; perp = `qty`).
    pub net_qty: Qty,
    /// Average price of the increment.
    pub px: Px,
    /// In `fee_asset` units, positive = paid.
    pub fee: i64,
    /// `ASSET_NONE`: the report carried no fee (a `FeeOnly` follows).
    pub fee_asset: AssetId,
    pub _pad2: u16,
    pub slot: u32,
    pub r#gen: u32,
    pub _pad3: u32,
    pub seq: u64,
}
const _: () = assert!(size_of::<FillDelta>() == 72);
const _: () = assert!(align_of::<FillDelta>() == 8);
const _: () = assert!(core::mem::offset_of!(FillDelta, inst) == 8);
const _: () = assert!(core::mem::offset_of!(FillDelta, qty) == 16);
const _: () = assert!(core::mem::offset_of!(FillDelta, fee_asset) == 48);
const _: () = assert!(core::mem::offset_of!(FillDelta, seq) == 64);

/// Realized cash of this exact perpetual fill, as reported by the execution source.
/// Amount is signed (received positive) in `asset` units, excluding fees and funding.
/// A known zero is a settlement fact; it is not an absent report. The caller must match
/// the complete order/execution identity and de-duplicate before calling the ledger.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PerpCashSettlement {
    pub asset: AssetId,
    pub amount: Qty,
}

/// A journal consumer cannot prove a cash booking. The failed record is never silently skipped.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum JournalApplyError {
    Identity,
    /// A BalanceCheck has no matching, initialized report in its own event.
    BalanceEvidence,
    Unmapped,
    /// Missing, duplicate, mismatched or cross-event OMS foreign-fill evidence.
    ForeignEvidence,
    ForeignCapacity,
    ReconcileEvidence,
    MissingCash,
    CashCoverage,
    CashConflict,
    CashOverflow,
    UnsupportedExternalCash,
    Settlement(SettlementError),
}

/// Invalid settlement/fill pairing. Validation happens before any ledger state changes.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SettlementError {
    UnknownInstrument,
    NotPerpetual,
    VenueMismatch,
    AccountVenue,
    WrongAsset,
    InvalidQuantity,
    InvalidPrice,
    MissingCash,
    CashCoverage,
    ExternalOverlap,
    ReceiptConflict,
    ReceiptCapacity,
    ReceiptIdentity,
}

/// Late fee (OMS `FeeOut`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeeDelta {
    pub account: AccountId,
    pub venue: VenueId,
    pub inst: InstId,
    pub group: GroupId,
    pub side: Side,
    pub fee: i64,
    pub fee_asset: AssetId,
    pub seq: u64,
}

/// `AdjustDelta.kind` codes.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AdjustKind {
    Position = 1,
    Balance = 2,
    Funding = 3,
    WriteOff = 4,
}

impl TryFrom<u8> for AdjustKind {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            1 => Ok(AdjustKind::Position),
            2 => Ok(AdjustKind::Balance),
            3 => Ok(AdjustKind::Funding),
            4 => Ok(AdjustKind::WriteOff),
            _ => Err(v),
        }
    }
}

/// `AdjustDelta.px_src` codes (Funding and WriteOff: 0).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PxSrc {
    Cmd = 1,
    Mark = 2,
    Bid = 3,
}

impl TryFrom<u8> for PxSrc {
    type Error = u8;
    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            1 => Ok(PxSrc::Cmd),
            2 => Ok(PxSrc::Mark),
            3 => Ok(PxSrc::Bid),
            _ => Err(v),
        }
    }
}

/// Adoption, funding correction or residual write-off (never through OMS).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AdjustDelta {
    pub account: AccountId,
    pub venue: VenueId,
    /// `AdjustKind` code.
    pub kind: u8,
    /// `PxSrc` code; 0 for Funding and WriteOff.
    pub px_src: u8,
    pub inst: InstId,
    pub asset: AssetId,
    pub _pad: u16,
    pub group: GroupId,
    /// Signed: exchange − engine (Funding: amount difference in the settlement asset;
    /// WriteOff: the naked filled quantity moved out of the group).
    pub qty: Qty,
    pub px: Px,
    pub report_id: u64,
    pub cmd_seq: u64,
    pub settle_ts_ns: i64,
}
const _: () = assert!(size_of::<AdjustDelta>() == 56);
const _: () = assert!(align_of::<AdjustDelta>() == 8);
const _: () = assert!(core::mem::offset_of!(AdjustDelta, inst) == 4);
const _: () = assert!(core::mem::offset_of!(AdjustDelta, asset) == 8);
const _: () = assert!(core::mem::offset_of!(AdjustDelta, group) == 12);
const _: () = assert!(core::mem::offset_of!(AdjustDelta, qty) == 16);
const _: () = assert!(core::mem::offset_of!(AdjustDelta, settle_ts_ns) == 48);

/// `TransferDelta.kind`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TransferKind {
    Record,
    Sim,
}

/// `RecordTransfer` / `SimTransfer`: moves cash between two venues of one account.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TransferDelta {
    pub account: AccountId,
    pub from_venue: VenueId,
    pub to_venue: VenueId,
    pub asset: AssetId,
    pub amount: Qty,
    pub kind: TransferKind,
}

/// PnL decomposition, all USDT `Notional` (1e-6).
#[repr(C)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct PnlBreakdown {
    pub price_realized: Notional,
    pub funding: Notional,
    pub fee_cex: Notional,
    pub gas: Notional,
    pub bribe: Notional,
    pub unrealized: Notional,
    pub adjust: Notional,
    pub external: Notional,
}
const _: () = assert!(size_of::<PnlBreakdown>() == 64);

impl PnlBreakdown {
    /// Every realized column (price, funding, fees, gas, bribe, adjust, external).
    pub fn net_realized(&self) -> Notional {
        Notional(
            self.price_realized
                .0
                .saturating_add(self.funding.0)
                .saturating_add(self.fee_cex.0)
                .saturating_add(self.gas.0)
                .saturating_add(self.bribe.0)
                .saturating_add(self.adjust.0)
                .saturating_add(self.external.0),
        )
    }

    /// `net_realized + unrealized`.
    pub fn net(&self) -> Notional {
        Notional(self.net_realized().0.saturating_add(self.unrealized.0))
    }

    pub(crate) fn add(&mut self, d: &PnlBreakdown) {
        let f = |a: &mut Notional, b: Notional| a.0 = a.0.saturating_add(b.0);
        f(&mut self.price_realized, d.price_realized);
        f(&mut self.funding, d.funding);
        f(&mut self.fee_cex, d.fee_cex);
        f(&mut self.gas, d.gas);
        f(&mut self.bribe, d.bribe);
        f(&mut self.unrealized, d.unrealized);
        f(&mut self.adjust, d.adjust);
        f(&mut self.external, d.external);
    }

    pub const fn zero() -> PnlBreakdown {
        PnlBreakdown {
            price_realized: Notional(0),
            funding: Notional(0),
            fee_cex: Notional(0),
            gas: Notional(0),
            bribe: Notional(0),
            unrealized: Notional(0),
            adjust: Notional(0),
            external: Notional(0),
        }
    }
}

/// Position of one (account, inst): signed quantity, average cost, realized.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct InstPos {
    /// Signed; spot = net-quantity holding.
    pub qty: Qty,
    /// Absolute cost (quote asset, exponent `px_exp + qty_exp`).
    pub cost: i128,
    /// Realized (quote asset, same exponent).
    pub realized_quote: i128,
    /// Fees in quote-asset units.
    pub fees_quote: i64,
    pub last_fill_ens: i64,
}

impl InstPos {
    pub const ZERO: InstPos = InstPos {
        qty: Qty(0),
        cost: 0,
        realized_quote: 0,
        fees_quote: 0,
        last_fill_ens: 0,
    };
}

/// One leg of a round: opening and closing quantities and USDT notionals, fees, gas.
#[repr(C)]
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Debug,
    zerocopy::FromBytes,
    zerocopy::IntoBytes,
    zerocopy::KnownLayout,
    zerocopy::Immutable,
)]
pub struct LegAgg {
    pub open_qty: Qty,
    pub open_notional: Notional,
    pub close_qty: Qty,
    pub close_notional: Notional,
    pub fee: Notional,
    pub gas: Notional,
}
const _: () = assert!(size_of::<LegAgg>() == 48);

impl LegAgg {
    pub const ZERO: LegAgg = LegAgg {
        open_qty: Qty(0),
        open_notional: Notional(0),
        close_qty: Qty(0),
        close_notional: Notional(0),
        fee: Notional(0),
        gas: Notional(0),
    };
}

/// The current round of a group.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RoundBook {
    pub round_id: u32,
    pub open_seq: u64,
    /// engine_ns of the round start (0 = no round open).
    pub open_ens: i64,
    /// Opaque strategy tag of the first opening intent (0 when unknown).
    pub tag: u32,
    pub legs: [LegAgg; 4],
    pub pnl: PnlBreakdown,
    pub peak_notional: Notional,
}

impl RoundBook {
    pub const ZERO: RoundBook = RoundBook {
        round_id: 0,
        open_seq: 0,
        open_ens: 0,
        tag: 0,
        legs: [LegAgg::ZERO; 4],
        pnl: PnlBreakdown::zero(),
        peak_notional: Notional(0),
    };
}

/// Book of one (account, group).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct GroupBook {
    /// Since the account was enabled.
    pub pnl: PnlBreakdown,
    /// Current UTC day.
    pub day: PnlBreakdown,
    pub round: RoundBook,
    /// Σ |leg notional| at the valuation prices.
    pub held_notional: Notional,
}

impl GroupBook {
    pub const ZERO: GroupBook = GroupBook {
        pnl: PnlBreakdown::zero(),
        day: PnlBreakdown::zero(),
        round: RoundBook::ZERO,
        held_notional: Notional(0),
    };
}

/// Account-level book.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AccountBook {
    pub day: PnlBreakdown,
    pub held_notional: Notional,
    pub upnl: Notional,
    /// Funding booked into the wallet − funding attributed to groups.
    pub funding_residual: Notional,
}

impl AccountBook {
    pub const ZERO: AccountBook = AccountBook {
        day: PnlBreakdown::zero(),
        held_notional: Notional(0),
        upnl: Notional(0),
        funding_residual: Notional(0),
    };
}

/// `CashEntry.bflags` bits.
pub mod bflags {
    pub const UNINIT: u8 = 1;
    pub const NON_STRATEGY: u8 = 2;
}

/// One asset of one (account, venue).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct CashEntry {
    pub asset: AssetId,
    pub bflags: u8,
    pub total: Qty,
    pub baseline: Qty,
    pub unassigned: Qty,
    pub rep_free: Qty,
    pub rep_locked: Qty,
    pub rep_ex_ts_ns: i64,
    pub rep_ens: i64,
}

impl CashEntry {
    pub(crate) fn new(asset: AssetId) -> CashEntry {
        CashEntry {
            asset,
            bflags: bflags::UNINIT,
            total: Qty(0),
            baseline: Qty(0),
            unassigned: Qty(0),
            rep_free: Qty(0),
            rep_locked: Qty(0),
            rep_ex_ts_ns: 0,
            rep_ens: 0,
        }
    }
}

/// Spot inventory of `asset` attributed to `group`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct InvEntry {
    pub asset: AssetId,
    pub group: GroupId,
    pub qty: Qty,
}

/// One `MarginReport` item kept by the ledger.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MarginItemRow {
    pub inst: InstId,
    pub liq_px: Px,
    pub adl_quantile: u8,
}

/// `MarginState.lflags` bits.
pub mod lflags {
    pub const LIQ_PX_MISSING: u8 = 1;
}

/// Margin data of one perpetual (account, venue).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MarginState {
    pub has_report: bool,
    pub rep_margin_balance: Notional,
    pub rep_maint: Notional,
    pub rep_avail: Notional,
    pub rep_ratio: Rate,
    pub rep_ens: i64,
    pub rep_perp_notional: Notional,
    pub items: FixedVec<MarginItemRow, MAX_MARGIN_ITEMS>,
    pub wallet: Notional,
    pub upnl: Notional,
    pub est_margin_balance: Notional,
    pub est_ratio: Rate,
    pub rep_liq_dist: Rate,
    pub est_liq_dist: Rate,
    pub mark_oldest_ens: i64,
    pub lflags: u8,
}

impl MarginState {
    pub(crate) const fn new() -> MarginState {
        MarginState {
            has_report: false,
            rep_margin_balance: Notional(0),
            rep_maint: Notional(0),
            rep_avail: Notional(0),
            rep_ratio: Rate(0),
            rep_ens: 0,
            rep_perp_notional: Notional(0),
            items: FixedVec::new(),
            wallet: Notional(0),
            upnl: Notional(0),
            est_margin_balance: Notional(0),
            est_ratio: Rate(0),
            rep_liq_dist: Rate::MAX,
            est_liq_dist: Rate::MAX,
            mark_oldest_ens: i64::MAX,
            lflags: 0,
        }
    }
}

/// `VenueBook.vflags` bits.
pub mod vflags {
    pub const INIT: u8 = 1;
    pub const STALE_RATE: u8 = 2;
    pub const DRIFT: u8 = 4;
}

/// Book of one (account, venue).
#[derive(Clone, Debug)]
pub struct VenueBook {
    pub cash: FixedVec<CashEntry, MAX_ASSETS_PER_VENUE>,
    pub inv: FixedVec<InvEntry, MAX_INV_PER_VENUE>,
    pub margin: MarginState,
    pub summary: crate::views::VenueEquityView,
    pub vflags: u8,
}

/// Account equity (USDT).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Equity {
    pub total: Notional,
    pub by_venue: FixedVec<(VenueId, Notional), 16>,
    pub valid: bool,
}

/// Conversion rate of an asset and its age.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RateView {
    pub bid: Px,
    pub ask: Px,
    pub ens: i64,
    pub stale: bool,
}

/// Net-quantity correction handed to OMS `net_adjust` and hft-exec `on_leg_adjust`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct NetAdjust {
    pub account: AccountId,
    pub venue: VenueId,
    pub group: GroupId,
    pub inst: InstId,
    pub delta: Qty,
}

/// PnL booked by one call (`GROUP_NONE` = account level); the sequencer feeds it to risk.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PnlDelta {
    pub account: AccountId,
    pub group: GroupId,
    pub d: PnlBreakdown,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FillApplied {
    pub pnl: PnlDelta,
    pub net_adjust: Option<NetAdjust>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FeeApplied {
    pub pnl: PnlDelta,
    pub net_adjust: Option<NetAdjust>,
}

/// Result of an attributed external fill.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ExternalApplied {
    pub group: GroupId,
    pub leg_ix: u8,
    /// Signed part attributed to `group`.
    pub net_qty: Qty,
    /// Signed part booked at `GROUP_NONE`.
    pub none_qty: Qty,
    /// `|net_qty + none_qty| × px` in USDT.
    pub notional: Notional,
    pub cause: ExternalCause,
}

/// Pairing check of a `BalanceOnlyPush` push.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FundingCheck {
    Ok,
    Mismatch { residual: Notional },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FundingPush {
    Booked { check: FundingCheck },
    Duplicate,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BalanceVerdict {
    /// First report of this asset.
    Initialized,
    /// `quiet = false`: only the `rep_*` fields were updated.
    Reported,
    Ok,
    /// `diff` = report − total.
    Drift {
        asset: AssetId,
        diff: Qty,
    },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BaselineResult {
    Set {
        n_assets: u16,
        notional: Notional,
    },
    /// The (account, venue) has no `BalanceReport` yet.
    NoBalanceView,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LedgerConfigError {
    /// A venue has more assets than `MAX_ASSETS_PER_VENUE`.
    TooManyAssets { venue: VenueId, n: u16 },
}

/// One `Funding` item of a reconciliation report.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FundingReconItem {
    pub account: AccountId,
    pub venue: VenueId,
    /// `INST_NONE`: wallet item of a `BalanceOnlyPush` venue.
    pub inst: InstId,
    pub settle_ts_ns: i64,
    pub exch_amount: Notional,
    pub engine_amount: Notional,
    pub report_id: u64,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum FundingRecon {
    Matched,
    TooRecent,
    OutsideWindow,
    NoHistoryHeld,
    NotApplicable,
    Corrected { adj: AdjustDelta, pnl: PnlDelta },
}

/// `FundingCorrected` note flags.
pub mod fc_flags {
    pub const NO_HISTORY: u64 = 1;
    pub const NEW_RECORD: u64 = 2;
    pub const WINDOW_END: u64 = 4;
}

/// `ExternalFill` note `aflags`.
pub mod aflags {
    pub const AMBIGUOUS: u64 = 1;
    pub const AUTO_RECON: u64 = 2;
}

/// Initial balances of a `SimReset`.
pub type SimInit = [hft_refdata::SimBalance];
