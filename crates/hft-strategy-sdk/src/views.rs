//! Read-only views (README "Views 与 ModelViews", "时间轴"): `Views`, `ModelViews`, `BookRead`,
//! `Now` with its time conversions, `time::ms` and `RefView`.

use hft_event::Level;
use hft_refdata::{AccountMeta, AssetMeta, InstHot, InstStatus, RefData, VenueCaps};
use hft_types::{AccountId, AssetId, GroupId, InstId, Px, RunMode, Side, Tsc, VenueId};

use crate::intent::OrderRef;
use crate::view_types::{
    BboView, FeeView, FundingView, GroupPosView, InstRiskView, LegView, OrderView, PortfolioRead,
    RiskStateView, TargetView,
};

/// State of an inst's L2 book view.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum BookStatus {
    /// No L2 subscription or no snapshot yet.
    Uninit,
    Live,
    Stale,
    Resyncing,
}

/// Read access to one inst's book (`Views::Book<'_>`; implemented over hft-book by the sequencer
/// adapter, and by `testing::MockViews`).
pub trait BookRead {
    fn status(&self) -> BookStatus;
    fn upd_tsc(&self) -> Tsc;
    fn best(&self, s: Side) -> Option<Level>;
    /// Copies at most `out.len()` levels from the touch outwards; returns the count. O(k).
    fn levels(&self, s: Side, out: &mut [Level]) -> usize;
}

/// This run's `EngineClock` anchor and the current segment's UTC calibration pair; the base of
/// [`Now::ens_of`], [`Now::tsc_of`] and [`Now::utc_of`]. Filled by the sequencer (and advisor)
/// from the journal; never from a system clock.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EngineClockAnchor {
    /// `EngineClock.base_engine_ns`.
    pub base_engine_ns: i64,
    /// Record-header tsc of this run's `EngineClock` record.
    pub base_tsc: Tsc,
    /// `EngineClock.tsc_hz_milli` (TSC frequency in mHz); 0 = unknown (conversions degrade to the
    /// anchor values).
    pub tsc_hz_milli: u64,
    /// UTC anchor: the current segment's calibration pair, or after a `UtcAnchor` re-anchoring
    /// the current record's TSC and its engine UTC (hft-sequencer "引擎 UTC").
    pub utc_tsc: Tsc,
    pub utc_ns: i64,
}

impl EngineClockAnchor {
    /// Engine time of a TSC of this run: `base_engine_ns + ⌊(t − base_tsc) × 10^12 / tsc_hz_milli⌋`
    /// (event-model time-stamp semantics; u128/i128 intermediate, saturating).
    #[inline]
    pub const fn ens_of(&self, t: Tsc) -> i64 {
        add_scaled(self.base_engine_ns, t.0, self.base_tsc.0, self.tsc_hz_milli)
    }
    /// UTC ns of a TSC of this run (same calibration as `Now.utc_ns`).
    #[inline]
    pub const fn utc_of(&self, t: Tsc) -> i64 {
        add_scaled(self.utc_ns, t.0, self.utc_tsc.0, self.tsc_hz_milli)
    }
    /// Smallest TSC of this run whose engine time is ≥ `e`; earlier than this run's base → 0.
    #[inline]
    pub const fn tsc_of(&self, e: i64) -> Tsc {
        let d = (e as i128) - (self.base_engine_ns as i128);
        if d < 0 || self.tsc_hz_milli == 0 {
            return if d < 0 { Tsc(0) } else { self.base_tsc };
        }
        // ceil(d × hz_milli / 10^12)
        let num = d * (self.tsc_hz_milli as i128);
        let q = num / 1_000_000_000_000;
        let q = if num % 1_000_000_000_000 != 0 { q + 1 } else { q };
        let t = (self.base_tsc.0 as i128) + q;
        Tsc(if t > u64::MAX as i128 { u64::MAX } else { t as u64 })
    }
    /// TSC ticks per millisecond (`tsc_hz_milli / 10^6`).
    #[inline]
    pub const fn tsc_per_ms(&self) -> u64 {
        self.tsc_hz_milli / 1_000_000
    }
}

const fn add_scaled(base: i64, t: u64, t0: u64, hz_milli: u64) -> i64 {
    if hz_milli == 0 {
        return base;
    }
    let dt = (t as i128) - (t0 as i128);
    let num = dt * 1_000_000_000_000;
    let q = num.div_euclid(hz_milli as i128);
    let r = (base as i128) + q;
    if r > i64::MAX as i128 {
        i64::MAX
    } else if r < i64::MIN as i128 {
        i64::MIN
    } else {
        r as i64
    }
}

/// Time of the current event (never a system clock). Built by the sequencer from the current
/// journal record; `engine_ns` and `utc_ns` follow from `clk` and `tsc`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Now {
    /// Record-header tsc of the current event; meaningful within this run only.
    pub tsc: Tsc,
    /// Engine time: continuous across restarts, downtime not counted. Persisted times use it.
    pub engine_ns: i64,
    /// Engine UTC: the journal's calibration pair re-anchored by `UtcAnchor` Inputs
    /// (deterministic, non-decreasing); unusable for time decisions when `clock_ok = false`.
    pub utc_ns: i64,
    pub seq: u64,
    /// Seq of this run's `EpochStart`; the only cross-run Advice expiry criterion.
    pub boot_seq: u64,
    /// TSC ticks per ms (from the journal's `tsc_hz`); TSC comparisons within this run only.
    pub tsc_per_ms: u64,
    pub clock_ok: bool,
    clk: EngineClockAnchor,
}

impl Now {
    /// Builds the time of the record at `tsc` (`seq`) of the run anchored by `clk`.
    pub const fn new(tsc: Tsc, seq: u64, boot_seq: u64, clock_ok: bool, clk: EngineClockAnchor) -> Now {
        Now {
            tsc,
            engine_ns: clk.ens_of(tsc),
            utc_ns: clk.utc_of(tsc),
            seq,
            boot_seq,
            tsc_per_ms: clk.tsc_per_ms(),
            clock_ok,
            clk,
        }
    }
    /// The anchor this `Now` was built from.
    pub const fn anchor(&self) -> &EngineClockAnchor {
        &self.clk
    }
    /// This run's TSC → engine_ns (same formula as the records' engine_ns, bit for bit).
    #[inline]
    pub const fn ens_of(&self, t: Tsc) -> i64 {
        self.clk.ens_of(t)
    }
    /// engine_ns → this run's TSC (timer registration, comparing with `*_tsc` view fields);
    /// times before this run saturate to 0 ("already past").
    #[inline]
    pub const fn tsc_of(&self, e: i64) -> Tsc {
        self.clk.tsc_of(e)
    }
    /// This run's TSC → UTC ns (same calibration as `utc_ns`).
    #[inline]
    pub const fn utc_of(&self, t: Tsc) -> i64 {
        self.clk.utc_of(t)
    }
}

/// Time helpers.
pub mod time {
    /// Milliseconds parameter → engine_ns interval (× 1 000 000).
    #[inline]
    pub const fn ms(ms: u32) -> i64 {
        (ms as i64) * 1_000_000
    }
}

/// Read-only views (static dispatch). Implemented by hft-sequencer's adapter; tests use
/// `testing::MockViews`. Every method is O(1) (book level reads O(k); `bbo` aggregates
/// ≤ `MAX_BBO_CONN_SLOTS` slots) and allocation-free; values reflect the engine components after
/// applying the current input event and before this `on_event`'s intents are processed.
pub trait Views {
    type Book<'a>: BookRead
    where
        Self: 'a;
    type Portfolio<'a>: PortfolioRead
    where
        Self: 'a;
    fn now(&self) -> Now;
    /// Latest quote + aggregated BBO connection health, by value.
    fn bbo(&self, i: InstId) -> BboView;
    /// Insts without an L2 subscription: `status = Uninit`.
    fn book(&self, i: InstId) -> Self::Book<'_>;
    fn funding(&self, i: InstId) -> &FundingView;
    fn mark(&self, i: InstId) -> Px;
    fn portfolio(&self, a: AccountId) -> Self::Portfolio<'_>;
    fn group_pos(&self, a: AccountId, g: GroupId) -> &GroupPosView;
    fn legs(&self, a: AccountId, g: GroupId) -> &LegView;
    fn target(&self, a: AccountId, g: GroupId) -> Option<&TargetView>;
    /// `gen` mismatch → `None`.
    fn order(&self, h: OrderRef) -> Option<OrderView>;
    /// Hot refdata row (market kind, tick, lot, min notional, asset, quote asset).
    fn inst(&self, i: InstId) -> &InstHot;
    /// The only entry for exchange differences.
    fn caps(&self, v: VenueId) -> &VenueCaps;
    fn fee(&self, a: AccountId, i: InstId) -> FeeView;
    /// Effective mode of the cell.
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode;
    fn risk(&self, a: AccountId) -> RiskStateView;
    /// Per-inst risk limits (the inst's price-band class; not the account minimum).
    fn risk_for(&self, a: AccountId, i: InstId) -> InstRiskView;
    /// (venue, account) has no Unknown slot and its order connection is up.
    fn venue_ready(&self, a: AccountId, v: VenueId) -> bool;
    /// Actual perp leverage in hundredths (300 = 3x) from the latest `AccountConfigReport`
    /// (Rate ÷ 10^7, floored); spot, item 0 or no report → `None`.
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32>;
    /// Quote health at the engine threshold:
    /// `bbo(i).quote_live(now.tsc, risk(a).conn_liveness_ms × now.tsc_per_ms)`. Provided; not
    /// overridden by implementors.
    #[inline]
    fn quote_ok(&self, a: AccountId, i: InstId) -> bool {
        let now = self.now();
        let silence = u64::from(self.risk(a).conn_liveness_ms).saturating_mul(now.tsc_per_ms);
        self.bbo(i).quote_live(now.tsc, silence)
    }
}

/// Model-side views: the advisor's journal-rebuilt state as of the record the Model is processing
/// (`now()` = that record). `target()`/`order()` return `None` on the model side.
pub trait ModelViews: Views {}

/// Borrowed reference data restricted to what one instance needs (its bound account).
#[derive(Copy, Clone, Debug)]
pub struct RefView<'a> {
    rd: &'a RefData,
    account: AccountId,
}

impl<'a> RefView<'a> {
    /// The view of instance bound to `account`.
    pub const fn new(rd: &'a RefData, account: AccountId) -> RefView<'a> {
        RefView { rd, account }
    }
    /// The underlying refdata (engine and test use).
    pub const fn refdata(&self) -> &'a RefData {
        self.rd
    }
    /// Hot row (market kind, tick, lot, min notional, asset). Only for insts referenced by the
    /// instance's GroupSpecs or subscriptions (checked at assembly); others → the NONE row.
    pub fn inst(&self, i: InstId) -> &'a InstHot {
        self.rd.inst(i)
    }
    pub fn asset(&self, a: AssetId) -> &'a AssetMeta {
        self.rd.asset(a)
    }
    /// Every inst of the asset on every venue in refdata, ascending, including venues outside
    /// this account.
    pub fn insts_of(&self, a: AssetId) -> &'a [InstId] {
        self.rd.insts_of_asset(a)
    }
    pub fn caps(&self, v: VenueId) -> &'a VenueCaps {
        self.rd.caps(v)
    }
    pub fn fee(&self, a: AccountId, i: InstId) -> FeeView {
        self.rd.fees(a, i)
    }
    /// The bound account only.
    pub fn account(&self) -> &'a AccountMeta {
        self.rd.account(self.account)
    }
    /// Inst not in refdata → `None`.
    pub fn try_inst(&self, i: InstId) -> Option<&'a InstHot> {
        let h = self.rd.inst(i);
        (i.0 != 0 && h.status != InstStatus::Invalid).then_some(h)
    }
    /// Inst not in refdata, `a` not the bound account, or the inst's venue outside the account's
    /// venues → `None`.
    pub fn try_fee(&self, a: AccountId, i: InstId) -> Option<FeeView> {
        let h = self.try_inst(i)?;
        if a != self.account || a.0 == 0 {
            return None;
        }
        let venues = self.account().venues;
        let v = h.venue.0;
        (v < 64 && venues & (1u64 << v) != 0).then(|| self.rd.fees(a, i))
    }
}
