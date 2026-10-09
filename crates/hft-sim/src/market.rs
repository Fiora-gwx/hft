//! Time progression and market records: book replicas, the matching view, trade-through fills of
//! resting orders, funding settlement triggers and mark-driven margin checks (README "1. 时间",
//! "2. 簿副本", "4. 挂单", "6. 资金费", "7.").

use hft_book::BookState;
use hft_event::{Level, MarketHdr};
use hft_types::{InstId, Side, VenueId};

use crate::SimExchange;
use crate::types::MarketEventRef;

/// Matching-view source flags.
pub(crate) struct ViewInfo {
    /// Only the `Bbo` stream exists for the inst (one level to take).
    pub top_only: bool,
}

impl SimExchange {
    /// Everything due strictly before `t` (commands) or at/before `t` (transfers, late fees,
    /// periodic reports), in time order.
    pub(crate) fn step_to(&mut self, t: u64) {
        loop {
            let cmd_at = self.cmds.peek().map(|r| r.0.arrive).filter(|a| *a < t);
            let tr_at = self.transfers.iter().map(|x| x.at).filter(|a| *a <= t).min();
            let lf_at = self.late_fees.iter().map(|x| x.at).filter(|a| *a <= t).min();
            let next = [cmd_at, tr_at, lf_at].into_iter().flatten().min();
            let Some(at) = next else {
                break;
            };
            if cmd_at == Some(at) {
                if let Some(core::cmp::Reverse(pc)) = self.cmds.pop() {
                    self.exec_cmd(&pc);
                }
            } else if tr_at == Some(at) {
                self.land_transfers(at);
            } else {
                self.charge_late_fees(at);
            }
        }
        self.periodic(t);
    }

    /// Truth reports every `report_interval_s` and the time-based margin reports.
    fn periodic(&mut self, t: u64) {
        if t >= self.next_truth {
            for ai in 0..self.accts.len() {
                self.truth_reports(ai, t);
            }
            let iv = self.ms_to_tsc(u64::from(self.tune.report_interval_s) * 1_000);
            self.next_truth = t.saturating_add(iv.max(1));
        }
        for ai in 0..self.accts.len() {
            let nv = self.accts.get(ai).map_or(0, |a| a.venues.len());
            for k in 0..nv {
                self.margin_check(ai, k, t, false);
            }
        }
    }

    pub(crate) fn venue_notice(&mut self, venue: VenueId, kind: u8, inst: InstId) {
        if kind != 1 {
            return;
        }
        for r in self.insts.iter_mut() {
            if r.venue == venue && (inst.0 == 0 || r.id == inst) {
                r.reduce_only = true;
            }
        }
    }

    pub(crate) fn apply_market(&mut self, t: u64, m: MarketEventRef<'_>) {
        let h: MarketHdr = match m {
            MarketEventRef::Bbo(b) => b.h,
            MarketEventRef::L2Snapshot(p) => p.hdr.h,
            MarketEventRef::L2Batch(b) => b.hdr.h,
            MarketEventRef::Trade(x) => x.h,
            MarketEventRef::Funding(x) => x.h,
            MarketEventRef::Mark(x) => x.h,
            MarketEventRef::Index(x) => x.h,
        };
        // Funding settles before the record that reaches the settlement time is applied.
        self.settle_due(h.venue, h.ex_ts_ns, t);
        let Some(ii) = self.inst_index(h.inst) else {
            return;
        };
        match m {
            MarketEventRef::Bbo(b) => {
                if let Some(r) = self.insts.get_mut(ii) {
                    r.bbo.bid_px = b.bid_px.0;
                    r.bbo.bid_qty = b.bid_qty.0;
                    r.bbo.ask_px = b.ask_px.0;
                    r.bbo.ask_qty = b.ask_qty.0;
                    r.bbo.ex_ts = h.ex_ts_ns;
                    r.bbo.set = true;
                }
                self.cross_resting(ii, t, h.ex_ts_ns);
            }
            MarketEventRef::L2Snapshot(p) => {
                if self.snapshot_part(ii, &p) {
                    self.cross_resting(ii, t, h.ex_ts_ns);
                }
            }
            MarketEventRef::L2Batch(b) => {
                if let Some(r) = self.insts.get_mut(ii) {
                    r.book.apply_batch(&b.hdr, b.levels);
                    r.book_ex_ts = h.ex_ts_ns;
                    r.book_set = true;
                }
                self.cross_resting(ii, t, h.ex_ts_ns);
            }
            MarketEventRef::Trade(x) => self.trade_through(ii, x.px.0, x.qty.0, t, h.ex_ts_ns),
            MarketEventRef::Funding(x) => {
                if let Some(r) = self.insts.get_mut(ii) {
                    r.rate = x.rate.0;
                    if x.next_funding_ns != r.next_funding_ns {
                        r.next_funding_ns = x.next_funding_ns;
                    }
                }
            }
            MarketEventRef::Mark(x) => {
                let venue = h.venue;
                if let Some(r) = self.insts.get_mut(ii) {
                    r.mark = x.px.0;
                    r.mark_set = true;
                }
                self.on_mark(venue, t, h.ex_ts_ns);
            }
            MarketEventRef::Index(_) => {}
        }
    }

    /// Reassembles a snapshot part; true when a whole snapshot was applied.
    fn snapshot_part(&mut self, ii: usize, p: &hft_event::L2SnapshotPart<'_>) -> bool {
        let ii16 = ii as u16;
        if p.part == 0 {
            self.snap.active = true;
            self.snap.inst = ii16;
            self.snap.next_part = 0;
            self.snap.bids.clear();
            self.snap.asks.clear();
        }
        if !self.snap.active || self.snap.inst != ii16 || self.snap.next_part != p.part {
            self.snap.active = false;
            return false;
        }
        let room = self.snap.bids.capacity().saturating_sub(self.snap.bids.len());
        self.snap
            .bids
            .extend_from_slice(p.bids.get(..p.bids.len().min(room)).unwrap_or(&[]));
        let room = self.snap.asks.capacity().saturating_sub(self.snap.asks.len());
        self.snap
            .asks
            .extend_from_slice(p.asks.get(..p.asks.len().min(room)).unwrap_or(&[]));
        self.snap.next_part = p.part.wrapping_add(1);
        if self.snap.next_part < p.parts.max(1) {
            return false;
        }
        self.snap.active = false;
        let mut hdr = p.hdr;
        hdr.n_bids = self.snap.bids.len() as u16;
        hdr.n_asks = self.snap.asks.len() as u16;
        // Bids then asks, contiguous (reuse the asks buffer's tail is not possible: copy into bids).
        let nb = self.snap.bids.len();
        let room = self.snap.bids.capacity().saturating_sub(nb);
        let take = self.snap.asks.len().min(room);
        let asks: &[Level] = self.snap.asks.get(..take).unwrap_or(&[]);
        self.snap.bids.extend_from_slice(asks);
        hdr.n_asks = take as u16;
        let ex = p.hdr.h.ex_ts_ns;
        if let Some(r) = self.insts.get_mut(ii) {
            r.book.apply_snapshot(&hdr, &self.snap.bids);
            r.book_ex_ts = ex;
            r.book_set = true;
        }
        true
    }

    /// Book replica of inst `ii` is not usable (`Stale`/`Resyncing`).
    pub(crate) fn book_not_live(&self, ii: usize) -> bool {
        self.insts
            .get(ii)
            .is_some_and(|r| r.book_set && matches!(r.book.state(), BookState::Stale | BookState::Resyncing))
    }

    /// Fills `self.levels` with the opposite side of a `side` taker, best first, prices not worse
    /// than `limit` (README "2. 簿副本": top = the newer of Bbo and book top; deeper levels from the
    /// book, worse than that top).
    pub(crate) fn view_levels(&mut self, ii: usize, side: Side, limit: i64) -> ViewInfo {
        self.levels.clear();
        let Some(r) = self.insts.get(ii) else {
            return ViewInfo { top_only: false };
        };
        let buy = side == Side::Buy;
        let ok = |px: i64| if buy { px <= limit } else { px >= limit };
        let bbo_lvl = if r.bbo.set {
            if buy && r.bbo.ask_px > 0 && r.bbo.ask_qty > 0 {
                Some((r.bbo.ask_px, r.bbo.ask_qty))
            } else if !buy && r.bbo.bid_px > 0 && r.bbo.bid_qty > 0 {
                Some((r.bbo.bid_px, r.bbo.bid_qty))
            } else {
                None
            }
        } else {
            None
        };
        let book_live = r.book_set && r.book.state() == BookState::Live;
        let book_side = if buy { Side::Sell } else { Side::Buy };
        let cap = self.levels.capacity();
        if book_live {
            let bbo_newer = r.bbo.set && r.bbo.ex_ts > r.book_ex_ts;
            let mut top: Option<i64> = None;
            if bbo_newer && let Some((p, q)) = bbo_lvl {
                top = Some(p);
                if ok(p) {
                    self.levels.push((p, q));
                }
            }
            for l in r.book.iter(book_side) {
                if self.levels.len() >= cap {
                    break;
                }
                let p = l.px.0;
                if let Some(tp) = top {
                    let worse = if buy { p > tp } else { p < tp };
                    if !worse {
                        continue;
                    }
                }
                if !ok(p) {
                    break;
                }
                if l.qty.0 > 0 {
                    self.levels.push((p, l.qty.0));
                }
            }
            ViewInfo { top_only: false }
        } else {
            if let Some((p, q)) = bbo_lvl
                && ok(p)
            {
                self.levels.push((p, q));
            }
            ViewInfo {
                top_only: r.bbo.set && !r.book_set,
            }
        }
    }

    /// Exchange time of the newest data of inst `ii`.
    pub(crate) fn inst_ex_ts(&self, ii: usize) -> i64 {
        self.insts.get(ii).map_or(0, |r| {
            let b = if r.bbo.set { r.bbo.ex_ts } else { 0 };
            let l = if r.book_set { r.book_ex_ts } else { 0 };
            b.max(l)
        })
    }

    /// Best opposite price of a `side` taker (None = empty).
    pub(crate) fn best_opposite(&mut self, ii: usize, side: Side) -> Option<i64> {
        let limit = if side == Side::Buy { i64::MAX } else { i64::MIN };
        self.view_levels(ii, side, limit);
        self.levels.first().map(|l| l.0)
    }

    /// Mid of inst `ii` from the matching view (None when a side is missing).
    pub(crate) fn mid_x2(&mut self, ii: usize) -> Option<i128> {
        let a = self.best_opposite(ii, Side::Buy)?;
        let b = self.best_opposite(ii, Side::Sell)?;
        Some(i128::from(a) + i128::from(b))
    }

    /// Trade-through of the public trade `(px, qty)` on resting orders of inst `ii`.
    fn trade_through(&mut self, ii: usize, px: i64, qty: i64, t: u64, ex_ts: i64) {
        for ai in 0..self.accts.len() {
            if self.accts.get(ai).is_none_or(|a| a.n_resting == 0) {
                continue;
            }
            for side in [Side::Buy, Side::Sell] {
                // Buy resting at P fills when trade px < P; sell when trade px > P.
                self.collect_resting(ai, ii, side, |p| if side == Side::Buy { px < p } else { px > p });
                self.fill_resting(ai, qty, t, ex_ts);
            }
        }
    }

    /// Resting orders crossed by the opposite side of the book (best ask < buy price; best bid >
    /// sell price), filled from the crossing levels.
    pub(crate) fn cross_resting(&mut self, ii: usize, t: u64, ex_ts: i64) {
        if self.accts.iter().all(|a| a.n_resting == 0) {
            return;
        }
        for side in [Side::Buy, Side::Sell] {
            // Highest buy price / lowest sell price among resting orders bounds the crossing levels.
            let mut bound: Option<i64> = None;
            for a in &self.accts {
                if a.n_resting == 0 {
                    continue;
                }
                for o in a
                    .orders
                    .iter()
                    .filter(|o| o.used && o.resting && usize::from(o.inst) == ii)
                {
                    if o.side == side {
                        bound = Some(match bound {
                            None => o.px,
                            Some(b) if side == Side::Buy => b.max(o.px),
                            Some(b) => b.min(o.px),
                        });
                    }
                }
            }
            let Some(bp) = bound else {
                continue;
            };
            // Levels strictly better than the bound price.
            let lim = if side == Side::Buy {
                bp.saturating_sub(1)
            } else {
                bp.saturating_add(1)
            };
            self.view_levels(ii, side, lim);
            if self.levels.is_empty() {
                continue;
            }
            for ai in 0..self.accts.len() {
                if self.accts.get(ai).is_none_or(|a| a.n_resting == 0) {
                    continue;
                }
                self.view_levels(ii, side, lim);
                if self.levels.is_empty() {
                    break;
                }
                self.collect_resting(ai, ii, side, |_| true);
                let cand = core::mem::take(&mut self.cand);
                let mut used = 0i64;
                let mut due = None;
                for &oi in &cand {
                    let Some(o) = self
                        .accts
                        .get(ai)
                        .and_then(|a| a.orders.get(usize::from(oi)))
                        .copied()
                    else {
                        continue;
                    };
                    // Quantity of the levels strictly better than the resting price.
                    let total = self
                        .levels
                        .iter()
                        .filter(|(lp, _)| if side == Side::Buy { *lp < o.px } else { *lp > o.px })
                        .fold(0i64, |s, (_, q)| s.saturating_add(*q));
                    let q = o.remaining().min(total.saturating_sub(used));
                    if q <= 0 {
                        continue;
                    }
                    let d = match due {
                        Some(d) => d,
                        None => {
                            let d = self.resp_due(ai, t);
                            due = Some(d);
                            d
                        }
                    };
                    used = used.saturating_add(q);
                    self.maker_fill(ai, usize::from(oi), q, t, d, ex_ts);
                }
                self.cand = cand;
            }
        }
    }

    /// `self.cand` = resting orders of account `ai` on inst `ii` with `side` whose price passes
    /// `crossed`, in priority order (better price first, then earlier).
    fn collect_resting(&mut self, ai: usize, ii: usize, side: Side, crossed: impl Fn(i64) -> bool) {
        self.cand.clear();
        let Some(a) = self.accts.get(ai) else {
            return;
        };
        for (k, o) in a.orders.iter().enumerate() {
            if o.used && o.resting && usize::from(o.inst) == ii && o.side == side && crossed(o.px) {
                self.cand.push(k as u16);
            }
        }
        let orders = &a.orders;
        self.cand.sort_unstable_by_key(|&k| {
            let o = orders
                .get(usize::from(k))
                .copied()
                .unwrap_or(crate::state::Order::EMPTY);
            let pk = if side == Side::Buy { -o.px } else { o.px };
            (pk, o.ins_seq)
        });
    }

    /// Fills the collected candidates of account `ai` with up to `qty` in total.
    fn fill_resting(&mut self, ai: usize, qty: i64, t: u64, ex_ts: i64) {
        if self.cand.is_empty() {
            return;
        }
        let cand = core::mem::take(&mut self.cand);
        let mut left = qty;
        let mut due = None;
        for &oi in &cand {
            if left <= 0 {
                break;
            }
            let rem = self
                .accts
                .get(ai)
                .and_then(|a| a.orders.get(usize::from(oi)))
                .map_or(0, |o| o.remaining());
            let q = rem.min(left);
            if q <= 0 {
                continue;
            }
            let d = match due {
                Some(d) => d,
                None => {
                    let d = self.resp_due(ai, t);
                    due = Some(d);
                    d
                }
            };
            left -= q;
            self.maker_fill(ai, usize::from(oi), q, t, d, ex_ts);
        }
        self.cand = cand;
    }

    /// Mark update of `venue`: liquidation check, then margin reports.
    fn on_mark(&mut self, venue: VenueId, t: u64, ex_ts: i64) {
        for ai in 0..self.accts.len() {
            let Some(k) = self
                .accts
                .get(ai)
                .and_then(|a| a.vslot.get(usize::from(venue.0)).copied())
                .filter(|s| *s != u8::MAX)
            else {
                continue;
            };
            let k = usize::from(k);
            if self.venues.get(usize::from(venue.0)).is_none_or(|v| !v.perp) {
                continue;
            }
            if self.liquidation_due(ai, venue) {
                self.liquidate(ai, venue, t, ex_ts);
                self.margin_check(ai, k, t, true);
            } else {
                self.margin_check(ai, k, t, false);
            }
        }
    }

    /// Settles every perpetual inst of `venue` whose settlement time `≤ ex_ts`.
    fn settle_due(&mut self, venue: VenueId, ex_ts: i64, t: u64) {
        let Some(vr) = self.venues.get(usize::from(venue.0)) else {
            return;
        };
        if !vr.perp {
            return;
        }
        self.cand.clear();
        for &ii in &vr.insts {
            if let Some(r) = self.insts.get(usize::from(ii))
                && r.perp
                && r.next_funding_ns > 0
                && r.settled_ns != r.next_funding_ns
                && ex_ts >= r.next_funding_ns
            {
                self.cand.push(ii);
            }
        }
        if self.cand.is_empty() {
            return;
        }
        let list = core::mem::take(&mut self.cand);
        for &ii in &list {
            if let Some(r) = self.insts.get_mut(usize::from(ii)) {
                r.settled_ns = r.next_funding_ns;
            }
        }
        self.settle_insts(venue, &list, t);
        self.cand = list;
    }
}
