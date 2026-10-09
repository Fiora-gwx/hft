//! The read-only views (README "只读视图"): `SeqViews` (SDK `Views`/`ModelViews`), the `RiskView`
//! adapter of `risk.evaluate`, the `ModeView` adapter of `risk.apply`, the `LegSummary`
//! projection and the group-position cache.

use hft_book::{BookSet, BookState, L2Book};
use hft_event::Level;
use hft_exec::Exec;
use hft_ledger::Ledger;
use hft_oms::Oms;
use hft_refdata::{InstHot, InstKind, RefData, VenueCaps};
use hft_risk::{EquityView, LegSummary, ModeView, Risk, RiskView, Scope};
use hft_risk::mode::CellKey;
use hft_strategy_sdk::{
    BboView, BookRead, BookStatus, FeeView, FundingView, GroupPosView, InstRiskView, LegView, ModelViews, Now,
    OrderRef, OrderStateView, OrderView, RiskStateView, TargetView, Views,
};
use hft_types::{AccountId, AssetId, GroupId, InstId, Notional, Px, Qty, Rate, RunMode, Side, Tsc, VenueId};

use crate::market::Market;

/// `SeqViews` without the executor built from the engine's fields (usable while `exec` is
/// mutably borrowed).
#[macro_export]
#[doc(hidden)]
macro_rules! noexec_views {
    ($e:expr) => {
        $crate::views::SeqViews {
            rd: &$e.rd,
            books: &$e.books,
            oms: &$e.oms,
            risk: &$e.risk,
            ledger: &$e.ledger,
            exec: None,
            market: &$e.market,
            gpos: &$e.gpos,
            now: $e.now,
            model: false,
        }
    };
}

/// Cached `GroupPosView` per (account, group) (`Views::group_pos` returns references).
#[derive(Clone, Debug)]
pub struct GposCache {
    pub keys: Vec<(AccountId, GroupId)>,
    pub vals: Vec<GroupPosView>,
}

static EMPTY_GPOS: GroupPosView = GroupPosView::EMPTY;
static EMPTY_LEGS: LegView = LegView::EMPTY;

impl GposCache {
    pub fn new(cap: usize) -> GposCache {
        GposCache {
            keys: Vec::with_capacity(cap + 16),
            vals: Vec::with_capacity(cap + 16),
        }
    }
    /// Sets the cell list (sorted, command path).
    pub fn set_cells(&mut self, cells: impl Iterator<Item = (AccountId, GroupId)>) {
        self.keys.clear();
        self.keys.extend(cells);
        self.keys.sort_unstable();
        self.keys.dedup();
        self.vals.clear();
        self.vals.resize(self.keys.len(), GroupPosView::EMPTY);
    }
    /// Recomputes every value from the ledger.
    pub fn refresh(&mut self, l: &Ledger) {
        for (k, v) in self.keys.iter().zip(self.vals.iter_mut()) {
            *v = l.group_pos(k.0, k.1);
        }
    }
    pub fn get(&self, a: AccountId, g: GroupId) -> &GroupPosView {
        match self.keys.binary_search(&(a, g)) {
            Ok(i) => self.vals.get(i).unwrap_or(&EMPTY_GPOS),
            Err(_) => &EMPTY_GPOS,
        }
    }
}

/// `BookRead` over an hft-book L2 book (`None` = no L2 subscription → `Uninit`).
#[derive(Copy, Clone, Debug)]
pub struct SeqBook<'a> {
    b: Option<&'a L2Book>,
}

impl BookRead for SeqBook<'_> {
    fn status(&self) -> BookStatus {
        match self.b.map(|b| b.state()) {
            Some(BookState::Live) => BookStatus::Live,
            Some(BookState::Stale) => BookStatus::Stale,
            Some(BookState::Resyncing) => BookStatus::Resyncing,
            _ => BookStatus::Uninit,
        }
    }
    fn upd_tsc(&self) -> Tsc {
        Tsc(0)
    }
    fn best(&self, s: Side) -> Option<Level> {
        let b = self.b?;
        match s {
            Side::Buy => b.best_bid(),
            Side::Sell => b.best_ask(),
        }
    }
    fn levels(&self, s: Side, out: &mut [Level]) -> usize {
        let Some(b) = self.b else { return 0 };
        let mut n = 0;
        for (d, l) in out.iter_mut().zip(b.iter(s)) {
            *d = l;
            n += 1;
        }
        n
    }
}

/// The engine views of one Input (`exec = None` for the adapter handed to `exec.advance`).
#[derive(Copy, Clone)]
pub struct SeqViews<'a> {
    pub rd: &'a RefData,
    pub books: &'a BookSet,
    pub oms: &'a Oms,
    pub risk: &'a Risk,
    pub ledger: &'a Ledger,
    pub exec: Option<&'a Exec>,
    pub market: &'a Market,
    pub gpos: &'a GposCache,
    pub now: Now,
    /// Model side (`target()`/`order()` return `None`).
    pub model: bool,
}

impl core::fmt::Debug for SeqViews<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeqViews").field("now", &self.now).finish_non_exhaustive()
    }
}

/// Quote-asset → USDT rate of an inst (check #6 notional; par assets = 1).
pub fn quote_rate(rd: &RefData, l: &Ledger, i: InstId) -> Rate {
    let q = rd.inst(i).quote;
    rate_of(rd, l, q)
}

/// USDT rate of an asset as `Rate` (1e-9): par or unknown-without-quote → 1.
pub fn rate_of(rd: &RefData, l: &Ledger, asset: AssetId) -> Rate {
    let r = l.rate(asset);
    if r.bid.0 <= 0 || r.ask.0 <= 0 {
        return Rate(1_000_000_000);
    }
    let mid = Px::mid_floor(r.bid, r.ask).0 as i128;
    let e = i32::from(rd.asset(asset).exp.px_exp) + 9;
    let v = if e >= 0 {
        mid.saturating_mul(10i128.saturating_pow(e as u32))
    } else {
        mid / 10i128.saturating_pow((-e) as u32)
    };
    Rate(i64::try_from(v).unwrap_or(i64::MAX))
}

/// Leverage (`Rate` multiple) → hundredths, floored.
pub fn lev_x100(r: Rate) -> u32 {
    let v = (r.0 as i128) / 10_000_000;
    u32::try_from(v.max(0)).unwrap_or(u32::MAX)
}

fn order_state(s: hft_oms::OrderState) -> OrderStateView {
    use hft_oms::OrderState as O;
    match s {
        O::PendingNew => OrderStateView::PendingNew,
        O::Live => OrderStateView::Live,
        O::PendingCancel => OrderStateView::PendingCancel,
        O::PendingAmend => OrderStateView::PendingAmend,
        O::Filled => OrderStateView::Filled,
        O::Cancelled => OrderStateView::Cancelled,
        O::Rejected => OrderStateView::Rejected,
        _ => OrderStateView::Unknown,
    }
}

impl SeqViews<'_> {
    /// The BBO projection shared by every reader (strategies, risk, OMS hedge pricing, exec).
    #[inline]
    pub fn bbo_of(&self, i: InstId) -> BboView {
        self.market.bbo_view(self.books, i)
    }
}

impl Views for SeqViews<'_> {
    type Book<'a>
        = SeqBook<'a>
    where
        Self: 'a;
    type Portfolio<'a>
        = hft_ledger::Portfolio<'a>
    where
        Self: 'a;
    fn now(&self) -> Now {
        self.now
    }
    fn bbo(&self, i: InstId) -> BboView {
        self.bbo_of(i)
    }
    fn book(&self, i: InstId) -> SeqBook<'_> {
        SeqBook { b: self.books.l2(i) }
    }
    fn funding(&self, i: InstId) -> &FundingView {
        static EMPTY: FundingView = FundingView::EMPTY;
        self.market.funding.get(i.0 as usize).unwrap_or(&EMPTY)
    }
    fn mark(&self, i: InstId) -> Px {
        self.market.row(i).map_or(Px(0), |r| r.mark_px)
    }
    fn portfolio(&self, a: AccountId) -> hft_ledger::Portfolio<'_> {
        self.ledger.portfolio(a)
    }
    fn group_pos(&self, a: AccountId, g: GroupId) -> &GroupPosView {
        self.gpos.get(a, g)
    }
    fn legs(&self, a: AccountId, g: GroupId) -> &LegView {
        let l = &self.oms.legs(a, g).view;
        if core::ptr::eq(l, &EMPTY_LEGS) { &EMPTY_LEGS } else { l }
    }
    fn target(&self, a: AccountId, g: GroupId) -> Option<&TargetView> {
        if self.model {
            return None;
        }
        self.exec?.view(a, g)
    }
    fn order(&self, h: OrderRef) -> Option<OrderView> {
        if self.model {
            return None;
        }
        let s = self.oms.slot(h)?;
        if s.r#gen != h.r#gen {
            return None;
        }
        let x = self.oms.slot_ext(h);
        let class = match x.map_or(1, |x| x.class) {
            2 => hft_strategy_sdk::IntentClass::Reduce,
            3 => hft_strategy_sdk::IntentClass::Hedge,
            4 => hft_strategy_sdk::IntentClass::Flatten,
            _ => hft_strategy_sdk::IntentClass::Open,
        };
        Some(OrderView {
            r: h,
            group: s.group,
            inst: s.inst,
            side: s.side,
            tif: s.tif,
            class,
            px: s.px,
            qty: s.qty,
            filled: s.filled,
            state: order_state(s.state),
            tag: x.map_or(0, |x| x.tag),
        })
    }
    fn inst(&self, i: InstId) -> &InstHot {
        self.rd.inst(i)
    }
    fn caps(&self, v: VenueId) -> &VenueCaps {
        self.rd.caps(v)
    }
    fn fee(&self, a: AccountId, i: InstId) -> FeeView {
        self.rd.fees(a, i)
    }
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode {
        let s = self.rd.account(a).bound_instance;
        match self.risk.mode.effective(CellKey {
            strategy: s,
            account: a,
            group: g,
        }) {
            hft_risk::mode::Effective::Shadow => RunMode::Shadow,
            hft_risk::mode::Effective::CloseOnly { .. } => RunMode::CloseOnly,
            hft_risk::mode::Effective::Live => RunMode::Live,
        }
    }
    fn risk(&self, a: AccountId) -> RiskStateView {
        self.risk.view(a, &self.now)
    }
    fn risk_for(&self, a: AccountId, i: InstId) -> InstRiskView {
        let mut v = self.risk.view_for(a, i, &self.now);
        v.quote_rate = quote_rate(self.rd, self.ledger, i);
        let until = self.risk.backoff_until(self.rd.inst(i).venue, a);
        v.rate_backoff = until != 0 && until > self.now.engine_ns;
        v
    }
    fn venue_ready(&self, a: AccountId, v: VenueId) -> bool {
        self.oms.venue_ready(v, a)
    }
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32> {
        let h = self.rd.inst(i);
        if h.kind != InstKind::Perp {
            return None;
        }
        self.risk.leverage(a, h.venue, i).map(lev_x100)
    }
}

impl ModelViews for SeqViews<'_> {}

/// `RiskView` adapter of `risk.evaluate` (README "只读视图").
#[derive(Copy, Clone)]
pub struct RiskAdapter<'a> {
    pub v: SeqViews<'a>,
}

impl core::fmt::Debug for RiskAdapter<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("RiskAdapter")
    }
}

impl RiskView for RiskAdapter<'_> {
    fn pos(&self, a: AccountId, i: InstId) -> Qty {
        self.v.ledger.pos(a, i).qty
    }
    fn held_notional(&self, a: AccountId, s: Scope) -> Notional {
        match s {
            Scope::Group(g) => self.v.ledger.group(a, g).held_notional,
            Scope::Account => self.v.ledger.account(a).held_notional,
            Scope::Venue(v) => {
                let mut n = 0i64;
                for i in self.v.rd.insts_of_venue(v) {
                    let p = self.v.ledger.pos(a, *i);
                    if p.qty.0 != 0 {
                        let px = self.v.market.row(*i).map_or(Px(0), |r| r.mark_px);
                        let px = if px.0 == 0 { mid_px(&self.v, *i) } else { px };
                        let e = self.v.rd.asset(self.v.rd.inst(*i).asset).exp;
                        n = n.saturating_add(Risk::notional(p.qty, px, e, quote_rate(self.v.rd, self.v.ledger, *i)).0);
                    }
                }
                Notional(n)
            }
        }
    }
    fn open_qty(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        let p = self.v.oms.inflight().pos(a, i);
        match side {
            Side::Buy => p.open_buy,
            Side::Sell => p.open_sell,
        }
    }
    fn open_qty_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        let p = self.v.oms.inflight().pos(a, i);
        match side {
            Side::Buy => p.imm_buy,
            Side::Sell => p.imm_sell,
        }
    }
    fn open_notional(&self, a: AccountId, s: Scope) -> Notional {
        let f = self.v.oms.inflight();
        match s {
            Scope::Group(g) => f.open_notional_group(a, g),
            Scope::Venue(v) => f.open_notional_venue(a, v),
            Scope::Account => {
                let mut n = 0i64;
                for v in 0..64u8 {
                    if self.v.rd.account(a).venues & (1u64 << v) != 0 {
                        n = n.saturating_add(f.open_notional_venue(a, VenueId(v)).0);
                    }
                }
                Notional(n)
            }
        }
    }
    fn pending_reduce(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        self.open_qty(a, i, side)
    }
    fn pending_reduce_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        self.open_qty_immediate(a, i, side)
    }
    fn venue_ready(&self, v: VenueId, a: AccountId) -> bool {
        self.v.oms.venue_ready(v, a)
    }
    fn group_has_unknown(&self, a: AccountId, g: GroupId) -> bool {
        self.v.oms.legs(a, g).view.has_unknown
    }
    fn equity(&self, a: AccountId) -> EquityView {
        let e = self.v.ledger.equity(a);
        EquityView {
            total: e.total,
            valid: e.valid,
        }
    }
    fn bbo(&self, i: InstId) -> BboView {
        self.v.bbo_of(i)
    }
    fn avail_quote(&self, a: AccountId, v: VenueId, quote: AssetId) -> Option<Notional> {
        let c = self.v.ledger.cash_view(a, v, quote);
        let free = c.free.0;
        let buy = self.v.oms.inflight().buy_notional(a, v).0;
        let e = self.v.rd.asset(quote).exp;
        // free (qty units of the quote asset) → Notional (1e-6) at par.
        let free_n = rescale(free, i32::from(e.qty_exp), -6);
        Some(Notional(free_n.saturating_sub(buy)))
    }
    fn avail_margin(&self, a: AccountId, v: VenueId) -> Option<Notional> {
        let m = self.v.ledger.margin(a, v);
        if !m.has_report {
            return None;
        }
        Some(Notional(
            m.rep_avail.0.saturating_sub(self.v.oms.inflight().open_margin(a, v).0),
        ))
    }
}

/// Rescales `raw × 10^from` to exponent `to` (floor).
pub fn rescale(raw: i64, from: i32, to: i32) -> i64 {
    let d = from - to;
    let r = raw as i128;
    let v = if d >= 0 {
        r.saturating_mul(10i128.saturating_pow(d as u32))
    } else {
        r.div_euclid(10i128.saturating_pow((-d) as u32))
    };
    i64::try_from(v).unwrap_or(if v > 0 { i64::MAX } else { i64::MIN })
}

fn mid_px(v: &SeqViews<'_>, i: InstId) -> Px {
    let b = v.bbo_of(i);
    if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
        Px::mid_floor(b.bid_px, b.ask_px)
    } else {
        Px(0)
    }
}

/// `ModeView` adapter of `risk.apply`.
#[derive(Copy, Clone)]
pub struct ModeAdapter<'a> {
    pub oms: &'a Oms,
    pub ledger: &'a Ledger,
    pub exec: &'a Exec,
}

impl core::fmt::Debug for ModeAdapter<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ModeAdapter")
    }
}

impl ModeView for ModeAdapter<'_> {
    fn cell_flat(&self, c: CellKey) -> bool {
        let gp = self.ledger.group_pos(c.account, c.group);
        let legs = &self.oms.legs(c.account, c.group).view;
        let pos_flat = gp.legs.iter().all(|l| l.qty.0 == 0);
        let inflight = legs
            .leg
            .iter()
            .all(|l| l.inflight_immediate.0 == 0 && l.inflight_resting.0 == 0 && l.residual.0 == 0 && l.net.0 == 0);
        pos_flat && inflight && !legs.has_pending
    }
    fn naked_cleared(&self, _a: AccountId, _g: GroupId) -> bool {
        true
    }
    fn recon_resumable(&self, _a: AccountId, _v: VenueId) -> bool {
        true
    }
    fn daily_loss_resumable(&self, _a: AccountId) -> bool {
        true
    }
    fn unknown_count(&self, a: AccountId, v: VenueId) -> u32 {
        self.oms.table_view(v, a).map_or(0, |t| t.unknown_count)
    }
    fn lev_bad(&self, _a: AccountId, _v: VenueId) -> bool {
        false
    }
    fn engine_reduce_pending(&self) -> bool {
        self.exec.engine_reduce_pending()
    }
}

/// `LegSummary` of a group from the OMS `LegView` (README "process" 7): `naked_filled_notional =
/// |Σ leg.net| × mark` (mark of the group's first perp leg, else the last fill price).
pub fn leg_summary(oms: &Oms, market: &Market, rd: &RefData, a: AccountId, g: GroupId, spec: Option<&hft_event::GroupSpec>) -> LegSummary {
    let st = oms.legs(a, g);
    let v = &st.view;
    let mut net_sum: i128 = 0;
    let mut quiescent = !v.has_pending && !v.has_unknown;
    for (k, l) in v.leg.iter().enumerate() {
        if k >= usize::from(v.n_legs) {
            break;
        }
        net_sum += i128::from(l.net.0);
        if l.inflight_immediate.0 != 0 {
            quiescent = false;
        }
    }
    let mut px = Px(0);
    let mut asset = AssetId(0);
    if let Some(sp) = spec {
        asset = sp.asset;
        for l in sp.legs.iter().take(usize::from(sp.n_legs)) {
            if rd.inst(l.inst).kind == InstKind::Perp {
                px = market.row(l.inst).map_or(Px(0), |r| r.mark_px);
                break;
            }
        }
    }
    if px.0 == 0 {
        px = st.last_px();
    }
    let qty = Qty(i64::try_from(net_sum.unsigned_abs()).unwrap_or(i64::MAX));
    let naked = Risk::notional(qty, px, rd.asset(asset).exp, Rate(1_000_000_000));
    LegSummary {
        naked_ratio: v.naked_ratio,
        naked_notional: v.naked_notional,
        naked_filled_notional: naked,
        quiescent,
    }
}
