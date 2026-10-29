//! Internal state rows of `SimExchange` (dense, preallocated by `SimExchange::new`).

use core::cmp::Ordering;

use hft_book::L2Book;
use hft_event::{GwCmd, Level, RepMsg};
use hft_refdata::{
    FeeAssetRule, FeePair, FundingSettleKind, MarginData, MarginMode, MmrTier, PositionMode, PriceBand,
    RateLimit,
};
use hft_types::{AccountId, AssetId, InstId, Side, Tif, VenueId};
use rand_chacha::ChaCha8Rng;

use hft_refdata::SimAccountCfg;

use crate::types::{LatencyModel, MarginModel, SimCounters};

/// Venue slots (venue bitmaps are 64 bits).
pub(crate) const VENUE_SLOTS: usize = 64;
/// No index.
pub(crate) const NONE16: u16 = u16::MAX;

/// Book-replica best level of the `Bbo` stream.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct BboRow {
    pub bid_px: i64,
    pub bid_qty: i64,
    pub ask_px: i64,
    pub ask_qty: i64,
    pub ex_ts: i64,
    pub set: bool,
}

/// One tracked inst: static refdata facts, the book replica and the funding/mark state.
#[derive(Debug)]
pub(crate) struct InstRow {
    pub id: InstId,
    pub venue: VenueId,
    pub perp: bool,
    pub base: AssetId,
    pub quote: AssetId,
    pub px_exp: i8,
    pub qty_exp: i8,
    pub tick: i64,
    pub lot: i64,
    pub min_qty: i64,
    pub min_notional: i64,
    /// Refdata status `Trading` or `ReduceOnly`.
    pub tradable: bool,
    /// Refdata status `ReduceOnly` or a `VenueNotice{Delisting}`.
    pub reduce_only: bool,
    pub tiers: Vec<MmrTier>,
    pub band: PriceBand,
    pub bbo: BboRow,
    pub book: L2Book,
    /// Exchange time of the last applied L2 record; `book_set` once any applied.
    pub book_ex_ts: i64,
    pub book_set: bool,
    pub mark: i64,
    pub mark_set: bool,
    pub rate: i64,
    pub next_funding_ns: i64,
    /// `next_funding_ns` value already settled (no second settlement).
    pub settled_ns: i64,
    /// Binary outcome venue: tracked index of the complementary token (`<a>-up-<k>` ↔
    /// `<a>-dn-<k>`), `NONE16` otherwise. Its trades are mirrored at `one − px` (README §4).
    pub complement: u16,
}

impl InstRow {
    /// `qty_exp + px_exp`: exponent of `qty_raw × px_raw` in quote units.
    pub fn ve(&self) -> i32 {
        i32::from(self.qty_exp) + i32::from(self.px_exp)
    }
}

/// Static facts of one venue used by the simulation.
#[derive(Clone, Debug)]
pub(crate) struct VenueRow {
    pub perp: bool,
    pub reduce_only: bool,
    pub position_mode: PositionMode,
    pub margin_mode: MarginMode,
    pub multi_asset: Option<bool>,
    pub fee_rule: FeeAssetRule,
    pub funding_settle: FundingSettleKind,
    pub margin_data: MarginData,
    /// `Orders`-bucket limits of scope Ip/Uid/Account (at most 4).
    pub limits: Vec<RateLimit>,
    /// Tracked inst indices on this venue (ascending `InstId`).
    pub insts: Vec<u16>,
    /// Margin asset (quote of the venue's perpetual insts).
    pub margin_asset: AssetId,
}

/// Balance of one (account, venue, asset).
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct Bal {
    pub total: i64,
    pub locked: i64,
    /// The last truth report of this entry carried a non-zero value.
    pub reported: bool,
}

/// Perpetual position of one (account, inst); `cost` = Σ |dq| × px (raw product, exponent `ve`).
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct PosRow {
    pub qty: i64,
    pub cost: i128,
    pub reported: bool,
}

/// Consumed-liquidity memory entry.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct MemEntry {
    pub px: i64,
    pub qty: i64,
    pub t0: u64,
    pub used: bool,
}

/// Order-table record.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Order {
    pub used: bool,
    pub resting: bool,
    pub status: u8,
    pub slot: u32,
    pub r#gen: u32,
    pub inst: u16,
    pub side: Side,
    pub tif: Tif,
    pub px: i64,
    pub qty: i64,
    pub filled: i64,
    /// Σ fill qty × px (raw product).
    pub fill_val: i128,
    /// Perpetual order cash facts, including a known zero; 0 quantity = unavailable.
    pub cash_qty: i64,
    pub cash_sum: i64,
    pub exch_id: u64,
    pub ins_seq: u64,
    pub term_seq: u64,
    pub last_ex_ts: i64,
    /// `due_tsc` of the last report of this order.
    pub last_due: u64,
    /// Spot lock held by this resting order (in `lock_asset` raw units).
    pub lock: i64,
    pub lock_asset: AssetId,
    /// `MakerFill::Queue`: displayed quantity ahead of the order at its price.
    pub ahead: i64,
    /// `MakerFill::Queue`: last displayed quantity of the order's price level.
    pub lvl: i64,
    /// `MakerFill::Queue`: decrease of the level not yet matched by a trade at the price, and the
    /// exchange time it was first seen.
    pub unexp: i64,
    pub unexp_ts: i64,
    /// `VenueModel::gtd_ms`: event time at which the resting order expires (0 = never).
    pub exp_at: u64,
}

impl Order {
    pub const EMPTY: Order = Order {
        used: false,
        resting: false,
        status: 0,
        slot: 0,
        r#gen: 0,
        inst: 0,
        side: Side::Buy,
        tif: Tif::Gtc,
        px: 0,
        qty: 0,
        filled: 0,
        fill_val: 0,
        cash_qty: 0,
        cash_sum: 0,
        exch_id: 0,
        ins_seq: 0,
        term_seq: 0,
        last_ex_ts: 0,
        last_due: 0,
        lock: 0,
        lock_asset: AssetId(0),
        ahead: 0,
        lvl: 0,
        unexp: 0,
        unexp_ts: 0,
        exp_at: 0,
    };

    pub fn remaining(&self) -> i64 {
        self.qty.saturating_sub(self.filled)
    }
}

/// Last `MarginReport` of one (account, perpetual venue).
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct MarginRep {
    pub has: bool,
    pub last_tsc: u64,
    pub last_ratio: i64,
}

/// Fixed-window order counter of one rate limit.
#[derive(Copy, Clone, Debug, Default)]
pub(crate) struct RateWin {
    pub win: u64,
    pub cnt: u32,
}

/// One Sim account.
#[derive(Debug)]
pub(crate) struct Acct {
    pub id: AccountId,
    pub cfg: SimAccountCfg,
    pub lat: LatencyModel,
    pub mm: MarginModel,
    /// Venues of the account, ascending.
    pub venues: Vec<VenueId>,
    /// `VenueId` → index into `venues` (`u8::MAX` = not in the account).
    pub vslot: [u8; VENUE_SLOTS],
    /// `venues.len() × n_assets`.
    pub bal: Vec<Bal>,
    pub pos: Vec<PosRow>,
    pub fees: Vec<FeePair>,
    pub orders: Vec<Order>,
    pub n_resting: u32,
    /// `n_insts × 2 × MEMORY_RING` (side 0 = consumed asks by buys, 1 = consumed bids by sells).
    pub mem: Vec<MemEntry>,
    pub rng: ChaCha8Rng,
    pub next_exch_id: u64,
    pub next_trade_id: u64,
    pub ins_seq: u64,
    pub term_seq: u64,
    pub conn_up: [bool; VENUE_SLOTS],
    pub rate: Vec<[RateWin; 4]>,
    pub mrep: Vec<MarginRep>,
    pub counters: SimCounters,
}

/// Pending order-class command, executed at `arrive`.
#[derive(Copy, Clone, Debug)]
pub(crate) struct PendCmd {
    pub arrive: u64,
    pub seq: u64,
    pub acct: u8,
    /// 0 = arrival; 1 = the delayed match of a marketable order (`VenueModel::taker`).
    pub phase: u8,
    pub cmd: GwCmd,
}

impl PartialEq for PendCmd {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for PendCmd {}
impl PartialOrd for PendCmd {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for PendCmd {
    fn cmp(&self, o: &Self) -> Ordering {
        (self.arrive, self.seq).cmp(&(o.arrive, o.seq))
    }
}

/// Bulk payload of a pending reply.
#[derive(Copy, Clone, Debug)]
pub(crate) enum Bulk {
    None,
    /// `[LeverageItem]` of the venue's tradable perpetual insts, built at delivery.
    Leverage {
        venue: VenueId,
        lev: u8,
    },
    /// Index into the margin-item slab.
    Margin(u16),
}

/// Pending reply, delivered at `due`.
#[derive(Copy, Clone, Debug)]
pub(crate) struct PendReply {
    pub due: u64,
    pub seq: u64,
    pub account: AccountId,
    pub msg: RepMsg,
    pub bulk: Bulk,
    /// Start-up reply (kept by `seed_account_from`).
    pub keep: bool,
}

impl PartialEq for PendReply {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for PendReply {}
impl PartialOrd for PendReply {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for PendReply {
    fn cmp(&self, o: &Self) -> Ordering {
        (self.due, self.seq).cmp(&(o.due, o.seq))
    }
}

/// `SimTransfer` in flight to its target venue.
#[derive(Copy, Clone, Debug)]
pub(crate) struct PendTransfer {
    pub at: u64,
    pub acct: u8,
    pub venue: VenueId,
    pub asset: AssetId,
    pub amount: i64,
}

/// Fee charged later (`late_fee`).
#[derive(Copy, Clone, Debug)]
pub(crate) struct LateFee {
    pub at: u64,
    pub acct: u8,
    pub venue: VenueId,
    pub asset: AssetId,
    pub fee: i64,
}

/// Multi-part snapshot reassembly of one inst.
#[derive(Debug, Default)]
pub(crate) struct SnapAsm {
    pub inst: u16,
    pub active: bool,
    pub next_part: u16,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
