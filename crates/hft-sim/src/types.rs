//! Public types of the simulated exchange (README "纯核心 API", "配置", "重启恢复"); no runtime code
//! beyond constructors and accessors.

use hft_event::{Bbo, Funding, IndexPrice, L2BatchView, L2SnapshotPart, MarkPrice, RepMsg, Trade};
use hft_refdata::SimAccountCfg;
use hft_types::{AccountId, AssetId, InstId, Notional, Px, Qty, Rate, Side, SlotId, Tif, Tsc, VenueId};

/// `OrderEvent.reject_code` of a simulated reject = `SIM_CODE_BASE + n` (README "拒单规则").
pub const SIM_CODE_BASE: u32 = 0x1000_0000;
/// Upper bound of `SimSetup.accounts` (Shadow variants × accounts of a prod instance, README "配置").
pub const MAX_SIM_ACCOUNTS: usize = 64;
/// Upper bound of `SimSetup.latency_models`.
pub const MAX_LATENCY_MODELS: usize = 16;
/// Upper bound of `SimSetup.margin_models`.
pub const MAX_MARGIN_MODELS: usize = 16;
/// sim-gw: commands taken from `out-sim` per `poll_once` (M4 task).
pub const CMDS_PER_POLL: usize = 256;
/// sim-gw: free pending-reply slots below which `poll_once` only tails and delivers (M4 task).
pub const REPLY_RESERVE: usize = 4_096;
/// Order table entries per account (resting orders and recently finished records).
pub const ORDER_TABLE_CAP: usize = 1_024;
/// Consumed-liquidity memory entries per (account, inst, side).
pub const MEMORY_RING: usize = 64;
/// Pending-reply heap capacity.
pub const PENDING_REPLY_CAP: usize = 16_384;
/// Pending-command heap capacity.
pub const PENDING_CMD_CAP: usize = 16_384;
/// Venues per Sim account.
pub const MAX_VENUES_PER_ACCOUNT: usize = 8;
/// `MarginReport` items per (account, venue).
pub const MAX_MARGIN_ITEMS: usize = 64;

/// Resting-order fill model (README "4. 挂单").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum MakerFill {
    /// Fills only on trades or book levels strictly through the resting price.
    TradeThrough,
    /// `TradeThrough`, plus FIFO queue position at the resting price: trades at that price
    /// consume the displayed quantity that was ahead of the order first.
    Queue,
    /// The queue rule validated against live Polymarket orders: the quantity ahead is capped by
    /// every later displayed size of the level (`ahead = min(ahead, level)`); a trade at the price
    /// consumes `ahead` first and fills the rest; a trade through the price, or the opposite side
    /// crossing the price, fills the whole remainder.
    QueueCap,
}

/// `[sim]` of the hft-core configuration (README "配置"). `seed` has no default: the only
/// constructor takes it, there is no `Default` impl.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SimTuning {
    pub seed: u64,
    pub consume_decay_ms: u32,
    pub take_avail_bp: u16,
    pub quiet_ms: u32,
    pub report_interval_s: u16,
    pub transfer_delay_ms: u32,
    pub rate_limit: bool,
    pub late_fee: bool,
    pub maker_fill: MakerFill,
}

impl SimTuning {
    /// `seed` with every other key at its default.
    pub const fn with_seed(seed: u64) -> SimTuning {
        SimTuning {
            seed,
            consume_decay_ms: 1_000,
            take_avail_bp: 10_000,
            quiet_ms: 50,
            report_interval_s: 60,
            transfer_delay_ms: 300_000,
            rate_limit: true,
            late_fee: false,
            maker_fill: MakerFill::TradeThrough,
        }
    }
}

/// Quantile table: `n ≤ 64` points, `q_bp` strictly increasing from 0 to 10 000, `us` non-decreasing;
/// sampled with linear interpolation.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Dist {
    pub n: u8,
    pub q_bp: [u16; 64],
    pub us: [u32; 64],
}

impl Dist {
    /// `points` = `(q_bp, us)`; `n = points.len()` (saturating at 255), only the first 64 are stored.
    pub fn from_points(points: &[(u16, u32)]) -> Dist {
        let mut d = Dist {
            n: u8::try_from(points.len()).unwrap_or(u8::MAX),
            q_bp: [0; 64],
            us: [0; 64],
        };
        for (i, (q, u)) in points.iter().take(64).enumerate() {
            if let (Some(dq), Some(du)) = (d.q_bp.get_mut(i), d.us.get_mut(i)) {
                *dq = *q;
                *du = *u;
            }
        }
        d
    }
}

/// Latency condition of the next commands (README "条件延迟池"), set by the caller from its
/// own observations: venue load bucket, reference-move bucket, window phase (each `< 4`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct Cond {
    pub load: u8,
    pub mv: u8,
    pub phase: u8,
}

/// Which latency a pool sample measures (decision → report, µs).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum PoolLeg {
    /// Decision → Ack of a new order.
    Place,
    /// Decision → CancelAck.
    Cancel,
    /// Arrival → match of a marketable (Ioc/Fok) order.
    Taker,
}

/// One pool: the samples of `leg` under a condition; `None` fields are wildcards (the fallbacks).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Pool {
    pub leg: PoolLeg,
    pub load: Option<u8>,
    pub mv: Option<u8>,
    pub phase: Option<u8>,
    pub us: Vec<u32>,
}

/// Conditional latency pools (README "条件延迟池"): a draw takes a uniform sample of the pool of
/// `(leg, load, move, phase)` when it has at least `min_n` samples, else of `(leg, load)`, else of
/// `leg`; with none the account's `LatencyModel` applies. A Place/Cancel draw `T` arrives at
/// `T − split_us` (at least 1 ms) after the decision.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct LatPools {
    pub split_us: u32,
    pub min_n: u16,
    pub pools: Vec<Pool>,
}

/// Matching delay of marketable orders (README "吃单延迟"): `base_us` after arrival, or `slow_us`
/// with probability `slow_ppm` / 10⁶ (a `PoolLeg::Taker` pool replaces `base_us`); the result is
/// reported `resp_us` after the match.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TakerDelay {
    pub base_us: u32,
    pub slow_us: u32,
    pub slow_ppm: u32,
    pub resp_us: u32,
}

/// Trading fee model (README "5. 手续费与扣费资产").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FeeModel {
    /// The refdata fee rows (`FeePair` rates of notional).
    Refdata,
    /// Binary outcome tokens: taker fee = `k_e9` / 10⁹ × p × (1 − p) × qty with p the price as a
    /// fraction of one; makers pay nothing.
    BinaryTaker { k_e9: i64 },
}

/// Venue-behaviour options on top of `SimTuning` (README "场所行为选项"); `Default` = none of them,
/// the behaviour and `state_hash` of a plain `SimExchange`.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueModel {
    /// `Some((entry_us, resp_us))`: cancels use this latency instead of the account model's.
    pub cancel_us: Option<(u32, u32)>,
    /// A cancel that reaches the venue before its order leaves a tombstone: the order is refused
    /// on arrival with `CancelAck` (the Polymarket "Already canceled" answer) and never rests.
    pub tombstone: bool,
    pub taker: Option<TakerDelay>,
    pub fee: FeeModel,
    /// Good-till-date lifetime of resting orders from their arrival (ms; 0 = none): `Expired`.
    pub gtd_ms: u32,
    /// `Some(us)`: resting-order fills are reported this long after the match (else the account
    /// model's response leg).
    pub fill_resp_us: Option<u32>,
    pub pools: Option<LatPools>,
}

impl Default for VenueModel {
    fn default() -> Self {
        VenueModel {
            cancel_us: None,
            tombstone: false,
            taker: None,
            fee: FeeModel::Refdata,
            gtd_ms: 0,
            fill_resp_us: None,
            pools: None,
        }
    }
}

/// `entry` = decision → matching at the exchange, `resp` = matching → report reaches us.
#[allow(clippy::large_enum_variant)] // configuration value, copied once per account at `new`
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum LatencyModel {
    Fixed { entry_us: u32, resp_us: u32 },
    Empirical { entry: Dist, resp: Dist },
}

/// Maintenance margin rate source.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum MmrSource {
    RefdataTiers,
    Flat(Rate),
}

/// Cross-margin model of a Sim account (README "配置").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarginModel {
    pub mmr: MmrSource,
    /// Initial margin = notional / leverage.
    pub leverage: u8,
    pub liq_fee: Rate,
    pub liq_slippage_bp: u16,
}

/// Construction input of `SimExchange::new` (cold; lengths are checked against the `MAX_*` bounds).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct SimSetup {
    pub tuning: SimTuning,
    /// TSC frequency × 1000 (mHz): converts µs/ms parameters to event time.
    pub tsc_hz_milli: u64,
    pub latency_models: Vec<LatencyModel>,
    pub margin_models: Vec<MarginModel>,
    pub accounts: Vec<(AccountId, SimAccountCfg)>,
}

/// Configuration errors (README "纯核心 API").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SimCfgErr {
    UnknownVenue,
    UnknownAsset,
    FeeTier,
    LatencyModelIndex,
    MarginModelIndex,
    BadDist,
    MarginMode,
    SameExchangeMarkets,
    TooManyAccounts,
    MmrTiersMissing,
    /// A recovered order's cash coverage is negative, exceeds filled quantity, or is not perpetual.
    SettlementFacts,
}

/// Reject reasons `n` of `reject_code = SIM_CODE_BASE + n` (README "拒单规则").
#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum RejectReason {
    NotTrading = 1,
    Lot = 2,
    Tick = 3,
    MinNotional = 4,
    PriceLimit = 5,
    Unsupported = 6,
    ReduceOnly = 7,
    PostOnly = 8,
    InsufficientBalance = 9,
    InsufficientMargin = 10,
    RateLimited = 11,
    Injected = 12,
}

impl RejectReason {
    /// `SIM_CODE_BASE + n`.
    pub const fn code(self) -> u32 {
        SIM_CODE_BASE + self as u32
    }
}

/// One market record as seen by the simulated exchange (by value; the bodies are borrowed).
#[derive(Copy, Clone, Debug)]
pub enum MarketEventRef<'a> {
    Bbo(&'a Bbo),
    /// `L2Snapshot` of any origin (TopN replaces the book; Exchange is the incremental-depth
    /// snapshot); multi-part snapshots arrive part by part.
    L2Snapshot(L2SnapshotPart<'a>),
    L2Batch(L2BatchView<'a>),
    Trade(&'a Trade),
    Funding(&'a Funding),
    Mark(&'a MarkPrice),
    Index(&'a IndexPrice),
}

/// Input of `SimExchange::on_record`.
#[derive(Copy, Clone, Debug)]
pub enum SimInput<'a> {
    Market(MarketEventRef<'a>),
    /// `ControlCmd::VenueNotice` (journal Input); `kind = 1` (Delisting) makes `inst` (0 = every
    /// inst of the venue) reduce-only, the other kinds are ignored.
    VenueNotice {
        venue: VenueId,
        kind: u8,
        inst: InstId,
    },
    /// Any other record: only its header `tsc` advances time.
    Clock,
}

/// One delivered reply: the ring element plus the bulk items of `MarginReport` (`[MarginItem; n]`)
/// and `AccountConfigReport` (`[LeverageItem; n_lev]`) as bytes, the same split as `rep-sim` /
/// `rep-sim-bulk`.
#[derive(Copy, Clone, Debug)]
pub struct SimReply<'a> {
    pub account: AccountId,
    pub due_tsc: Tsc,
    pub gen_seq: u64,
    pub msg: RepMsg,
    pub bulk: &'a [u8],
}

/// One non-terminal OMS slot of the recovered engine state (README "重启恢复").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SimSeedOrder {
    pub venue: VenueId,
    pub inst: InstId,
    pub slot: SlotId,
    pub r#gen: u32,
    pub side: Side,
    pub tif: Tif,
    /// hft-oms `OrderState` code (`PendingNew = 1`, `Live = 2`, `PendingCancel = 3`, `PendingAmend = 4`).
    pub prev_state: u8,
    pub qty: Qty,
    pub filled: Qty,
    pub px: Px,
    pub avg_px: Px,
    /// Known cumulative perpetual cash from the recovered order, not from the wallet.
    /// `cash_qty = 0` preserves missing old facts; a known zero uses `cash_qty = filled`.
    pub cash_qty: Qty,
    pub cash_sum: Qty,
    pub exch_order_id: u64,
}

/// Recovered engine state of one Sim account (filled by the assembler; README "重启恢复").
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct SimSeedState {
    /// Section 22 `Cash.total` per (venue, asset).
    pub balances: Vec<(VenueId, AssetId, Qty)>,
    /// Section 1: `(inst, qty, entry_px)`.
    pub positions: Vec<(InstId, Qty, Px)>,
    /// Section 2 non-terminal slots.
    pub orders: Vec<SimSeedOrder>,
    /// Section 10: `(inst, next_funding_ns, funding_rate, mark_px)`.
    pub funding: Vec<(InstId, i64, Rate, Px)>,
}

/// Sim-only counters (README "报告字段" Sim 专有 and the diagnostic counters).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct SimCounters {
    pub top_only_fills: u64,
    pub liquidity_memory_hits: u64,
    pub liquidations: u64,
    pub quiet_advance: u64,
    pub book_not_live_rejects: u64,
    pub order_table_full: u64,
    pub reply_overflow: u64,
    /// `VenueModel::tombstone`: orders refused because their cancel arrived first.
    pub tombstone_refusals: u64,
    /// `VenueModel::gtd_ms`: resting orders expired.
    pub gtd_expired: u64,
}

/// One balance of the simulated truth.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SimBalance {
    pub total: Qty,
    pub locked: Qty,
    pub free: Qty,
}

/// One perpetual position of the simulated truth (`entry_px` = cost / |qty|, half-even).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SimPosition {
    pub qty: Qty,
    pub entry_px: Px,
}

/// Cross-margin figures of one (account, perpetual venue) (README "7.").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SimMargin {
    pub wallet: Notional,
    pub unrealized: Notional,
    pub margin_balance: Notional,
    pub maint: Notional,
    pub initial: Notional,
    pub avail: Notional,
    pub margin_ratio: Rate,
}

/// Order-table record (`status` = hft-event `codes::ORDER_STATUS`: New 1, PartiallyFilled 2, Filled 3,
/// Cancelled 4, Rejected 5, Expired 6).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SimOrderView {
    pub slot: SlotId,
    pub r#gen: u32,
    pub inst: InstId,
    pub side: Side,
    pub tif: Tif,
    pub status: u8,
    pub resting: bool,
    pub px: Px,
    pub qty: Qty,
    pub filled: Qty,
    pub avg_px: Px,
    pub exch_order_id: u64,
}
