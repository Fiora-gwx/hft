//! `MockViews`: implements `Views` and `ModelViews` over state set explicitly by the test.

use hft_event::Level;
use hft_refdata::{InstHot, RefData, VenueCaps};
use hft_types::{AccountId, AssetId, GroupId, InstId, Notional, Px, Qty, RunMode, Side, Tsc, VenueId};

use crate::intent::OrderRef;
use crate::view_types::{
    BboView, CashView, FeeView, FundingView, GroupPosView, InstRiskView, InventoryView, LegView, MarginView,
    MdStampView, OrderView, PerpPosView, PortfolioRead, RiskStateView, RxTsSrc, TargetView, VenueEquityView,
    VenueSet,
};
use crate::views::{
    BookRead, BookStatus, EngineClockAnchor, MD_STAMPS_KEPT, ModelViews, Now, RefView, Views,
};

/// Minimal reference data of a test (insts, assets, `VenueCaps`, fees, accounts) and the account
/// the instance is bound to.
#[derive(Clone, Debug)]
pub struct RefFixture {
    pub rd: RefData,
    pub account: AccountId,
}

impl RefFixture {
    pub fn new(rd: RefData, account: AccountId) -> RefFixture {
        RefFixture { rd, account }
    }
    /// The instance's `RefView`.
    pub fn view(&self) -> RefView<'_> {
        RefView::new(&self.rd, self.account)
    }
}

/// Book levels of one inst.
#[derive(Clone, Debug)]
pub struct BookFixture {
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
    pub upd_tsc: Tsc,
}

/// `Views::Book` of `MockViews`: `Uninit` when the test set no book.
#[derive(Clone, Copy, Debug)]
pub struct MockBook<'a> {
    b: Option<&'a BookFixture>,
}

impl BookRead for MockBook<'_> {
    fn status(&self) -> BookStatus {
        if self.b.is_some() {
            BookStatus::Live
        } else {
            BookStatus::Uninit
        }
    }
    fn upd_tsc(&self) -> Tsc {
        self.b.map_or(Tsc(0), |b| b.upd_tsc)
    }
    fn best(&self, s: Side) -> Option<Level> {
        let b = self.b?;
        match s {
            Side::Buy => b.bids.first().copied(),
            Side::Sell => b.asks.first().copied(),
        }
    }
    fn levels(&self, s: Side, out: &mut [Level]) -> usize {
        let Some(b) = self.b else { return 0 };
        let src = match s {
            Side::Buy => &b.bids,
            Side::Sell => &b.asks,
        };
        let n = src.len().min(out.len());
        for (d, l) in out.iter_mut().zip(src.iter()) {
            *d = *l;
        }
        n
    }
}

/// Equity data of one account (set through `MockViews::portfolio`).
#[derive(Clone, Debug)]
pub struct PortfolioFixture {
    account: AccountId,
    venues: VenueSet,
    equity: Vec<VenueEquityView>,
    cash: Vec<(VenueId, AssetId, CashView)>,
    inventory: Vec<(VenueId, AssetId, InventoryView)>,
    inventory_of: Vec<(VenueId, GroupId, Qty)>,
    perp: Vec<(VenueId, InstId, PerpPosView)>,
    margin: Vec<(VenueId, MarginView)>,
}

fn upsert<K: PartialEq, V>(v: &mut Vec<(K, V)>, k: K, x: V) {
    match v.iter_mut().find(|(a, _)| *a == k) {
        Some(e) => e.1 = x,
        None => v.push((k, x)),
    }
}

impl PortfolioFixture {
    fn new(account: AccountId, venues: VenueSet) -> PortfolioFixture {
        PortfolioFixture {
            account,
            venues,
            equity: Vec::new(),
            cash: Vec::new(),
            inventory: Vec::new(),
            inventory_of: Vec::new(),
            perp: Vec::new(),
            margin: Vec::new(),
        }
    }
    /// Venues of the account (default: `AccountMeta.venues`).
    pub fn venues(&mut self, s: VenueSet) -> &mut Self {
        self.venues = s;
        self
    }
    /// Per-venue summary row.
    pub fn venue(&mut self, e: VenueEquityView) -> &mut Self {
        match self.equity.iter_mut().find(|x| x.venue == e.venue) {
            Some(x) => *x = e,
            None => self.equity.push(e),
        }
        self
    }
    /// Cash of `asset` on `v` (raw `Qty` values).
    pub fn cash(&mut self, v: VenueId, asset: AssetId, free: i64, locked: i64) -> &mut Self {
        let c = CashView {
            free: Qty(free),
            locked: Qty(locked),
        };
        match self.cash.iter_mut().find(|x| x.0 == v && x.1 == asset) {
            Some(x) => x.2 = c,
            None => self.cash.push((v, asset, c)),
        }
        self
    }
    pub fn inventory(&mut self, v: VenueId, asset: AssetId, i: InventoryView) -> &mut Self {
        match self.inventory.iter_mut().find(|x| x.0 == v && x.1 == asset) {
            Some(x) => x.2 = i,
            None => self.inventory.push((v, asset, i)),
        }
        self
    }
    pub fn inventory_of(&mut self, v: VenueId, g: GroupId, q: Qty) -> &mut Self {
        match self.inventory_of.iter_mut().find(|x| x.0 == v && x.1 == g) {
            Some(x) => x.2 = q,
            None => self.inventory_of.push((v, g, q)),
        }
        self
    }
    pub fn perp(&mut self, v: VenueId, i: InstId, p: PerpPosView) -> &mut Self {
        match self.perp.iter_mut().find(|x| x.0 == v && x.1 == i) {
            Some(x) => x.2 = p,
            None => self.perp.push((v, i, p)),
        }
        self
    }
    pub fn margin(&mut self, v: VenueId, m: MarginView) -> &mut Self {
        upsert(&mut self.margin, v, m);
        self
    }
}

static ZERO_EQUITY: VenueEquityView = VenueEquityView::empty(VenueId(0));
static EMPTY_FUNDING: FundingView = FundingView::EMPTY;
static EMPTY_POS: GroupPosView = GroupPosView::EMPTY;
static EMPTY_LEGS: LegView = LegView::EMPTY;

const ZERO_MARGIN: MarginView = MarginView {
    margin_balance: Notional(0),
    maint_margin: Notional(0),
    avail_margin: Notional(0),
    margin_ratio: hft_types::Rate(0),
    src_ens: 0,
    stale: true,
};

/// `Views::Portfolio` of `MockViews`.
#[derive(Clone, Copy, Debug)]
pub struct MockPortfolio<'a> {
    account: AccountId,
    f: Option<&'a PortfolioFixture>,
}

impl PortfolioRead for MockPortfolio<'_> {
    fn account(&self) -> AccountId {
        self.account
    }
    fn venues(&self) -> VenueSet {
        self.f.map_or(VenueSet::EMPTY, |f| f.venues)
    }
    fn venue(&self, v: VenueId) -> &VenueEquityView {
        self.f
            .and_then(|f| f.equity.iter().find(|e| e.venue == v))
            .unwrap_or(&ZERO_EQUITY)
    }
    fn cash(&self, v: VenueId, asset: AssetId) -> CashView {
        self.f
            .and_then(|f| f.cash.iter().find(|x| x.0 == v && x.1 == asset))
            .map_or(
                CashView {
                    free: Qty(0),
                    locked: Qty(0),
                },
                |x| x.2,
            )
    }
    fn inventory(&self, v: VenueId, asset: AssetId) -> InventoryView {
        self.f
            .and_then(|f| f.inventory.iter().find(|x| x.0 == v && x.1 == asset))
            .map_or(
                InventoryView {
                    total: Qty(0),
                    assigned: Qty(0),
                    unassigned: Qty(0),
                    baseline: Qty(0),
                },
                |x| x.2,
            )
    }
    fn inventory_of(&self, v: VenueId, g: GroupId) -> Qty {
        self.f
            .and_then(|f| f.inventory_of.iter().find(|x| x.0 == v && x.1 == g))
            .map_or(Qty(0), |x| x.2)
    }
    fn perp(&self, v: VenueId, i: InstId) -> PerpPosView {
        self.f
            .and_then(|f| f.perp.iter().find(|x| x.0 == v && x.1 == i))
            .map_or(
                PerpPosView {
                    qty: Qty(0),
                    entry_px: Px(0),
                    upnl: Notional(0),
                    liq_px: Px(0),
                    adl_quantile: 0xFF,
                },
                |x| x.2,
            )
    }
    fn margin(&self, v: VenueId) -> MarginView {
        self.f
            .and_then(|f| f.margin.iter().find(|x| x.0 == v))
            .map_or(ZERO_MARGIN, |x| x.1)
    }
    fn equity_total(&self) -> Notional {
        let Some(f) = self.f else { return Notional(0) };
        Notional(
            f.equity
                .iter()
                .filter(|e| f.venues.contains(e.venue))
                .fold(0i64, |a, e| a.saturating_add(e.equity.0)),
        )
    }
}

/// `Views` and `ModelViews` over explicitly set state.
///
/// The setters share their names with the `Views` getters. Call setters on a `&mut MockViews`
/// receiver (`let m = &mut v; m.bbo(i, bid, ask)`, or inside a chain started by `at`) and getters
/// on `&MockViews` or as `Views::bbo(&v, i)`: on a plain `MockViews` receiver the `&self` getter
/// is found first.
/// Defaults: no quotes, no books, mode `Live`,
/// `RiskStateView::EMPTY`, `venue_ready = true`, no leverage, `order()` always `None`.
#[derive(Clone, Debug)]
pub struct MockViews {
    rd: RefFixture,
    clk: EngineClockAnchor,
    tsc: Tsc,
    seq: u64,
    boot_seq: u64,
    clock_ok: bool,
    bbos: Vec<(InstId, BboView)>,
    books: Vec<(InstId, BookFixture)>,
    funding: Vec<(InstId, FundingView)>,
    marks: Vec<(InstId, Px)>,
    portfolios: Vec<PortfolioFixture>,
    group_pos: Vec<((AccountId, GroupId), GroupPosView)>,
    legs: Vec<((AccountId, GroupId), LegView)>,
    targets: Vec<((AccountId, GroupId), TargetView)>,
    modes: Vec<((AccountId, GroupId), RunMode)>,
    risks: Vec<(AccountId, RiskStateView)>,
    risk_for: Vec<((AccountId, InstId), InstRiskView)>,
    ready: Vec<((AccountId, VenueId), bool)>,
    leverage: Vec<((AccountId, InstId), Option<u32>)>,
    orders: Vec<OrderView>,
    md: Vec<(InstId, Vec<MdStampView>)>,
}

impl MockViews {
    /// Starts at TSC 0 of a 1 GHz run with `engine_ns = 0`, `utc_ns = 0`, `boot_seq = 0`.
    pub fn new(rd: RefFixture) -> MockViews {
        MockViews {
            rd,
            clk: EngineClockAnchor {
                base_engine_ns: 0,
                base_tsc: Tsc(0),
                tsc_hz_milli: 1_000_000_000_000,
                utc_tsc: Tsc(0),
                utc_ns: 0,
            },
            tsc: Tsc(0),
            seq: 0,
            boot_seq: 0,
            clock_ok: true,
            bbos: Vec::new(),
            books: Vec::new(),
            funding: Vec::new(),
            marks: Vec::new(),
            portfolios: Vec::new(),
            group_pos: Vec::new(),
            legs: Vec::new(),
            targets: Vec::new(),
            modes: Vec::new(),
            risks: Vec::new(),
            risk_for: Vec::new(),
            ready: Vec::new(),
            leverage: Vec::new(),
            orders: Vec::new(),
            md: Vec::new(),
        }
    }
    /// The reference data and bound account.
    pub fn refdata(&self) -> &RefFixture {
        &self.rd
    }
    /// The instance's `RefView`.
    pub fn refview(&self) -> RefView<'_> {
        self.rd.view()
    }
    /// Sets the current time (and its run anchor, `boot_seq`, `seq`, `clock_ok`).
    pub fn at(&mut self, now: Now) -> &mut Self {
        self.clk = *now.anchor();
        self.tsc = now.tsc;
        self.seq = now.seq;
        self.boot_seq = now.boot_seq;
        self.clock_ok = now.clock_ok;
        self
    }
    /// Moves to TSC `t` of the current run.
    pub fn at_tsc(&mut self, t: Tsc) -> &mut Self {
        self.tsc = t;
        self
    }
    /// Moves to the first TSC of the current run whose engine time is ≥ `e`.
    pub fn at_engine_ns(&mut self, e: i64) -> &mut Self {
        self.tsc = self.clk.tsc_of(e);
        self
    }
    /// Sets the seq of the current record.
    pub fn seq(&mut self, seq: u64) -> &mut Self {
        self.seq = seq;
        self
    }
    /// Simulates a restart: TSC restarts at `new_tsc`, engine time continues (downtime not counted),
    /// `boot_seq` advances, UTC unchanged; quotes keep their prices but lose `rx_tsc` and
    /// connection health; funding and book receive times are cleared.
    pub fn restart(&mut self, new_tsc: Tsc, new_boot_seq: u64) -> &mut Self {
        self.restart_after(new_tsc, new_boot_seq, 0)
    }
    /// `restart` after `down_ns` of downtime (UTC advances by it, engine time does not).
    pub fn restart_after(&mut self, new_tsc: Tsc, new_boot_seq: u64, down_ns: i64) -> &mut Self {
        let now = self.now();
        self.clk = EngineClockAnchor {
            base_engine_ns: now.engine_ns,
            base_tsc: new_tsc,
            tsc_hz_milli: self.clk.tsc_hz_milli,
            utc_tsc: new_tsc,
            utc_ns: now.utc_ns.saturating_add(down_ns),
        };
        self.tsc = new_tsc;
        self.boot_seq = new_boot_seq;
        self.seq = self.seq.max(new_boot_seq);
        for (_, b) in &mut self.bbos {
            b.rx_tsc = Tsc(0);
            b.hw_rx_ns = 0;
            b.rx_ts_src = RxTsSrc::None;
            b.conn_live_tsc = Tsc(0);
            b.conn_up = false;
            b.resync = false;
        }
        for (_, f) in &mut self.funding {
            f.upd_tsc = Tsc(0);
        }
        for (_, b) in &mut self.books {
            b.upd_tsc = Tsc(0);
        }
        self
    }
    fn with_bbo(&mut self, i: InstId, f: impl FnOnce(&mut BboView)) {
        if !self.bbos.iter().any(|(x, _)| *x == i) {
            self.bbos.push((i, BboView::EMPTY));
        }
        if let Some(e) = self.bbos.iter_mut().find(|(x, _)| *x == i) {
            f(&mut e.1);
        }
    }
    /// Sets the quote and `rx_tsc = now.tsc`; connection health unchanged.
    pub fn bbo(&mut self, i: InstId, bid: (Px, Qty), ask: (Px, Qty)) -> &mut Self {
        let (tsc, seq) = (self.tsc, self.seq);
        self.with_bbo(i, |b| {
            b.bid_px = bid.0;
            b.bid_qty = bid.1;
            b.ask_px = ask.0;
            b.ask_qty = ask.1;
            b.rx_tsc = tsc;
            b.seq = seq;
            b.update_id = b.update_id.wrapping_add(1);
        });
        self
    }
    /// The current `BboView` of `i` (same as `Views::bbo`; on a `&mut MockViews` receiver the
    /// setter `bbo` wins method lookup).
    pub fn bbo_of(&self, i: InstId) -> BboView {
        Views::bbo(self, i)
    }
    /// Changes only the receive stamps.
    pub fn bbo_rx(&mut self, i: InstId, rx_tsc: Tsc, hw_rx_ns: i64, src: RxTsSrc) -> &mut Self {
        self.with_bbo(i, |b| {
            b.rx_tsc = rx_tsc;
            b.hw_rx_ns = hw_rx_ns;
            b.rx_ts_src = src;
        });
        self
    }
    /// Sets the aggregated connection health; the quote is unchanged.
    pub fn conn(&mut self, i: InstId, live_tsc: Tsc, up: bool, resync: bool) -> &mut Self {
        self.with_bbo(i, |b| {
            b.conn_live_tsc = live_tsc;
            b.conn_up = up;
            b.resync = resync;
        });
        self
    }
    /// Book levels (best first); the book becomes `Live`.
    pub fn book(&mut self, i: InstId, bids: &[Level], asks: &[Level]) -> &mut Self {
        let f = BookFixture {
            bids: bids.to_vec(),
            asks: asks.to_vec(),
            upd_tsc: self.tsc,
        };
        upsert(&mut self.books, i, f);
        self
    }
    pub fn funding(&mut self, i: InstId, f: FundingView) -> &mut Self {
        upsert(&mut self.funding, i, f);
        self
    }
    pub fn mark(&mut self, i: InstId, px: Px) -> &mut Self {
        upsert(&mut self.marks, i, px);
        self
    }
    /// Edits the account's equity data (venues default to `AccountMeta.venues` for the bound
    /// account).
    pub fn portfolio(&mut self, a: AccountId, f: impl FnOnce(&mut PortfolioFixture)) -> &mut Self {
        if !self.portfolios.iter().any(|p| p.account == a) {
            let venues = if a == self.rd.account {
                VenueSet(self.rd.rd.account(a).venues)
            } else {
                VenueSet::EMPTY
            };
            self.portfolios.push(PortfolioFixture::new(a, venues));
        }
        if let Some(p) = self.portfolios.iter_mut().find(|p| p.account == a) {
            f(p);
        }
        self
    }
    pub fn group_pos(&mut self, a: AccountId, g: GroupId, p: GroupPosView) -> &mut Self {
        upsert(&mut self.group_pos, (a, g), p);
        self
    }
    pub fn legs(&mut self, a: AccountId, g: GroupId, l: LegView) -> &mut Self {
        upsert(&mut self.legs, (a, g), l);
        self
    }
    pub fn target(&mut self, a: AccountId, g: GroupId, t: Option<TargetView>) -> &mut Self {
        match t {
            Some(t) => upsert(&mut self.targets, (a, g), t),
            None => self.targets.retain(|(k, _)| *k != (a, g)),
        }
        self
    }
    pub fn mode(&mut self, a: AccountId, g: GroupId, m: RunMode) -> &mut Self {
        upsert(&mut self.modes, (a, g), m);
        self
    }
    pub fn risk(&mut self, a: AccountId, r: RiskStateView) -> &mut Self {
        upsert(&mut self.risks, a, r);
        self
    }
    pub fn risk_for(&mut self, a: AccountId, i: InstId, r: InstRiskView) -> &mut Self {
        upsert(&mut self.risk_for, (a, i), r);
        self
    }
    pub fn venue_ready(&mut self, a: AccountId, v: VenueId, ready: bool) -> &mut Self {
        upsert(&mut self.ready, (a, v), ready);
        self
    }
    pub fn leverage(&mut self, a: AccountId, i: InstId, x100: Option<u32>) -> &mut Self {
        upsert(&mut self.leverage, (a, i), x100);
        self
    }
    /// Sets the `OrderView` returned by `Views::order` for `o.r` (same slot and gen); replaces an
    /// earlier one of that slot.
    pub fn set_order(&mut self, o: OrderView) -> &mut Self {
        self.orders
            .retain(|x| (x.r.venue, x.r.account, x.r.slot) != (o.r.venue, o.r.account, o.r.slot));
        self.orders.push(o);
        self
    }
    /// Removes the order of slot `r.slot` (`Views::order` → `None`).
    pub fn clear_order(&mut self, r: OrderRef) -> &mut Self {
        self.orders
            .retain(|x| (x.r.venue, x.r.account, x.r.slot) != (r.venue, r.account, r.slot));
        self
    }
    /// Appends one market-data stamp of `i` (`Views::md_count` + 1; `md_stamp` keeps the last
    /// `MD_STAMPS_KEPT`).
    pub fn md_push(&mut self, i: InstId, m: MdStampView) -> &mut Self {
        match self.md.iter_mut().find(|(x, _)| *x == i) {
            Some(e) => e.1.push(m),
            None => self.md.push((i, vec![m])),
        }
        self
    }
}

impl Views for MockViews {
    type Book<'a> = MockBook<'a>;
    type Portfolio<'a> = MockPortfolio<'a>;

    fn now(&self) -> Now {
        Now::new(self.tsc, self.seq, self.boot_seq, self.clock_ok, self.clk)
    }
    fn bbo(&self, i: InstId) -> BboView {
        self.bbos
            .iter()
            .find(|(x, _)| *x == i)
            .map_or(BboView::EMPTY, |e| e.1)
    }
    fn book(&self, i: InstId) -> MockBook<'_> {
        MockBook {
            b: self.books.iter().find(|(x, _)| *x == i).map(|e| &e.1),
        }
    }
    fn funding(&self, i: InstId) -> &FundingView {
        self.funding
            .iter()
            .find(|(x, _)| *x == i)
            .map_or(&EMPTY_FUNDING, |e| &e.1)
    }
    fn mark(&self, i: InstId) -> Px {
        self.marks.iter().find(|(x, _)| *x == i).map_or(Px(0), |e| e.1)
    }
    fn portfolio(&self, a: AccountId) -> MockPortfolio<'_> {
        MockPortfolio {
            account: a,
            f: self.portfolios.iter().find(|p| p.account == a),
        }
    }
    fn group_pos(&self, a: AccountId, g: GroupId) -> &GroupPosView {
        self.group_pos
            .iter()
            .find(|(k, _)| *k == (a, g))
            .map_or(&EMPTY_POS, |e| &e.1)
    }
    fn legs(&self, a: AccountId, g: GroupId) -> &LegView {
        self.legs
            .iter()
            .find(|(k, _)| *k == (a, g))
            .map_or(&EMPTY_LEGS, |e| &e.1)
    }
    fn target(&self, a: AccountId, g: GroupId) -> Option<&TargetView> {
        self.targets.iter().find(|(k, _)| *k == (a, g)).map(|e| &e.1)
    }
    fn order(&self, h: OrderRef) -> Option<OrderView> {
        self.orders.iter().find(|o| o.r == h).copied()
    }
    fn inst(&self, i: InstId) -> &InstHot {
        self.rd.rd.inst(i)
    }
    fn caps(&self, v: VenueId) -> &VenueCaps {
        self.rd.rd.caps(v)
    }
    fn fee(&self, a: AccountId, i: InstId) -> FeeView {
        self.rd.rd.fees(a, i)
    }
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode {
        self.modes
            .iter()
            .find(|(k, _)| *k == (a, g))
            .map_or(RunMode::Live, |e| e.1)
    }
    fn risk(&self, a: AccountId) -> RiskStateView {
        self.risks
            .iter()
            .find(|(k, _)| *k == a)
            .map_or(RiskStateView::EMPTY, |e| e.1)
    }
    fn risk_for(&self, a: AccountId, i: InstId) -> InstRiskView {
        self.risk_for
            .iter()
            .find(|(k, _)| *k == (a, i))
            .map_or(InstRiskView::EMPTY, |e| e.1)
    }
    fn venue_ready(&self, a: AccountId, v: VenueId) -> bool {
        self.ready.iter().find(|(k, _)| *k == (a, v)).is_none_or(|e| e.1)
    }
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32> {
        self.leverage.iter().find(|(k, _)| *k == (a, i)).and_then(|e| e.1)
    }
    fn md_count(&self, i: InstId) -> u64 {
        self.md
            .iter()
            .find(|(x, _)| *x == i)
            .map_or(0, |e| e.1.len() as u64)
    }
    fn md_stamp(&self, i: InstId, n: u64) -> Option<MdStampView> {
        let v = &self.md.iter().find(|(x, _)| *x == i)?.1;
        let count = v.len() as u64;
        if n == 0 || n > count || count - n >= MD_STAMPS_KEPT {
            return None;
        }
        v.get(usize::try_from(n - 1).ok()?).copied()
    }
}

impl ModelViews for MockViews {}
