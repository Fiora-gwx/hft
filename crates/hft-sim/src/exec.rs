//! Command execution at `arrive_tsc`: reject rules, taker matching with the consumed-liquidity
//! memory, order table, cancel / amend / query (README "拒单规则", "3. 吃单撮合", "产生的事件").

use hft_event::{GwCmd, GwOp, OrderEvent, OrderEventKind, OrderFlags, RepMsg, SendReport, SendStatus};
use hft_types::{AssetId, ConnId, InstId, Px, Qty, RoundMode, Side, SlotId, Tif, Tsc, VenueId};

use crate::SimExchange;
use crate::fx::{NOTIONAL_EXP, div_round, floor_to, rescale, sat64};
use crate::state::{Bulk, MemEntry, Order, PendCmd};
use crate::types::{MEMORY_RING, RejectReason};

/// `codes::ORDER_STATUS`.
pub(crate) mod st {
    pub const NEW: u8 = 1;
    pub const PARTIAL: u8 = 2;
    pub const FILLED: u8 = 3;
    pub const CANCELLED: u8 = 4;
    pub const REJECTED: u8 = 5;
    pub const EXPIRED: u8 = 6;
    pub const NOT_FOUND: u8 = 7;
}

/// `codes::REJECT_CLASS` GONE.
const GONE: u8 = 1;

fn side_ix(s: Side) -> usize {
    match s {
        Side::Buy => 0,
        Side::Sell => 1,
    }
}

impl SimExchange {
    pub(crate) fn exec_cmd(&mut self, pc: &PendCmd) {
        let ai = usize::from(pc.acct);
        match pc.cmd.op {
            GwOp::Place => self.exec_place(ai, pc),
            GwOp::Cancel => self.exec_cancel(ai, pc),
            GwOp::Amend => self.exec_amend(ai, pc),
            GwOp::CancelAll => self.exec_cancel_all(ai, pc),
            GwOp::Query => self.exec_query(ai, pc),
            _ => {}
        }
    }

    /// Venue of inst id `i` (refdata row of a tracked inst; 0 if untracked).
    fn venue_of(&self, i: InstId) -> VenueId {
        self.inst_index(i)
            .and_then(|ii| self.insts.get(ii))
            .map_or(VenueId(0), |r| r.venue)
    }

    /// Order connection of `(ai, venue)` is Down.
    fn conn_down(&self, ai: usize, venue: VenueId) -> bool {
        self.accts
            .get(ai)
            .and_then(|a| a.conn_up.get(usize::from(venue.0)))
            .is_some_and(|up| !up)
    }

    fn push_no_conn(&mut self, ai: usize, pc: &PendCmd, venue: VenueId, target_gen: u32) {
        let Some(id) = self.accts.get(ai).map(|a| a.id) else {
            return;
        };
        let c = &pc.cmd;
        let sr = SendReport {
            decision_seq: c.decision_seq,
            sent_tsc: Tsc(pc.arrive),
            slot: if c.op == GwOp::Place {
                c.slot
            } else {
                c.target_slot
            },
            r#gen: if c.op == GwOp::Place { c.r#gen } else { target_gen },
            target_slot: c.target_slot,
            target_gen,
            venue,
            account: id,
            conn: ConnId(0),
            status: SendStatus::NoConn,
            op: c.op,
            _pad: [0; 3],
            os_errno: 0,
            _pad2: 0,
        };
        self.push_reply(id, pc.arrive, RepMsg::SendReport(sr), Bulk::None);
    }

    /// Builds an `OrderEvent` of order `o` of account `ai`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn order_event(
        &self,
        ai: usize,
        o: &Order,
        what: OrderEventKind,
        px: i64,
        qty: i64,
        fee: i64,
        fee_asset: AssetId,
        detail: u8,
        reject_code: u32,
        ex_ts: i64,
        due: u64,
    ) -> OrderEvent {
        let (inst, venue) = self
            .insts
            .get(usize::from(o.inst))
            .map_or((InstId(0), VenueId(0)), |r| (r.id, r.venue));
        let avg = if o.filled > 0 {
            sat64(div_round(o.fill_val, i128::from(o.filled), RoundMode::HalfEven))
        } else {
            0
        };
        OrderEvent {
            venue,
            account: self.accts.get(ai).map_or(hft_types::AccountId(0), |a| a.id),
            what,
            detail,
            slot: SlotId(o.slot),
            r#gen: o.r#gen,
            inst,
            px: Px(px),
            qty: Qty(qty),
            cum_filled: Qty(o.filled),
            fee: Qty(fee),
            fee_asset,
            epoch: 0,
            reject_code,
            exch_order_id: o.exch_id,
            ex_ts_ns: ex_ts.max(o.last_ex_ts),
            hw_rx_ns: 0,
            rx_tsc: due,
            avg_px: Px(avg),
            cash_qty: Qty(o.cash_qty),
            cash_sum: Qty(o.cash_sum),
            trade_id: 0,
        }
    }

    pub(crate) fn emit(&mut self, ai: usize, ev: OrderEvent, due: u64) {
        let Some(id) = self.accts.get(ai).map(|a| a.id) else {
            return;
        };
        let mut ev = ev;
        let mut due = due;
        let fee_only = ev.what == OrderEventKind::FeeOnly;
        if let Some(o) = self.order_by_slot_mut(ai, ev.slot.0) {
            o.last_ex_ts = o.last_ex_ts.max(ev.ex_ts_ns);
            // Reports of one order are delivered in the order they were produced.
            if !fee_only {
                due = due.max(o.last_due);
                o.last_due = due;
            }
        }
        ev.rx_tsc = due;
        self.push_reply(
            id,
            due,
            RepMsg::OrderEvent(ev, hft_event::RecordFlags(0)),
            Bulk::None,
        );
    }

    /// Latest record of `slot` (highest insertion sequence).
    pub(crate) fn find_slot(&self, ai: usize, slot: u32) -> Option<usize> {
        let a = self.accts.get(ai)?;
        let mut best: Option<(u64, usize)> = None;
        for (k, o) in a.orders.iter().enumerate() {
            if o.used && o.slot == slot && best.is_none_or(|(s, _)| o.ins_seq > s) {
                best = Some((o.ins_seq, k));
            }
        }
        best.map(|(_, k)| k)
    }

    fn order_by_slot_mut(&mut self, ai: usize, slot: u32) -> Option<&mut Order> {
        let k = self.find_slot(ai, slot)?;
        self.accts.get_mut(ai)?.orders.get_mut(k)
    }

    /// A free order-table entry (unused, else the earliest finished record).
    pub(crate) fn alloc_order(&mut self, ai: usize) -> Option<usize> {
        let a = self.accts.get(ai)?;
        if let Some(k) = a.orders.iter().position(|o| !o.used) {
            return Some(k);
        }
        a.orders
            .iter()
            .enumerate()
            .filter(|(_, o)| !o.resting)
            .min_by_key(|(_, o)| o.term_seq)
            .map(|(k, _)| k)
    }

    /// Marks order `k` finished with `status`.
    pub(crate) fn finish(&mut self, ai: usize, k: usize, status: u8) {
        let Some(a) = self.accts.get_mut(ai) else {
            return;
        };
        a.term_seq += 1;
        let ts = a.term_seq;
        let mut was_resting = false;
        if let Some(o) = a.orders.get_mut(k) {
            was_resting = o.resting;
            o.resting = false;
            o.status = status;
            o.term_seq = ts;
        }
        if was_resting {
            a.n_resting = a.n_resting.saturating_sub(1);
        }
        self.set_lock(ai, k, 0);
    }

    fn exec_place(&mut self, ai: usize, pc: &PendCmd) {
        let c = pc.cmd;
        let t = pc.arrive;
        let ii = self.inst_index(c.inst);
        let venue = self.venue_of(c.inst);
        let in_acct = self
            .accts
            .get(ai)
            .and_then(|a| a.vslot.get(usize::from(venue.0)))
            .is_some_and(|s| *s != u8::MAX);
        if in_acct && self.conn_down(ai, venue) {
            self.push_no_conn(ai, pc, venue, 0);
            return;
        }
        let due = self.resp_due(ai, t);
        let ex_ts = ii.map_or(0, |ii| self.inst_ex_ts(ii));
        let post_only = c.tif == Tif::PostOnly || OrderFlags(c.oflags).contains(OrderFlags::POST_ONLY);
        let Some(a) = self.accts.get_mut(ai) else {
            return;
        };
        a.ins_seq += 1;
        let exch_id = a.next_exch_id;
        a.next_exch_id += 1;
        let mut o = Order {
            used: true,
            resting: false,
            status: 0,
            slot: c.slot.0,
            r#gen: c.r#gen,
            inst: ii.map_or(u16::MAX, |x| x as u16),
            side: c.side,
            tif: if post_only { Tif::PostOnly } else { c.tif },
            px: c.px.0,
            qty: c.qty.0,
            filled: 0,
            fill_val: 0,
            cash_qty: 0,
            cash_sum: 0,
            exch_id,
            ins_seq: a.ins_seq,
            term_seq: 0,
            last_ex_ts: 0,
            last_due: 0,
            lock: 0,
            lock_asset: AssetId(0),
        };
        let verdict = match ii {
            Some(ii) if in_acct => self.check_place(ai, ii, &c, t),
            _ => Err(RejectReason::NotTrading),
        };
        let Some(k) = self.alloc_order(ai) else {
            if let Some(a) = self.accts.get_mut(ai) {
                a.counters.order_table_full += 1;
            }
            let ev = self.order_event(
                ai,
                &o,
                OrderEventKind::Reject,
                o.px,
                o.qty,
                0,
                AssetId(0),
                0,
                RejectReason::NotTrading.code(),
                ex_ts,
                due,
            );
            self.emit(ai, ev, due);
            return;
        };
        if let Err(r) = verdict {
            o.status = st::REJECTED;
            if let Some(slot) = self.accts.get_mut(ai).and_then(|a| a.orders.get_mut(k)) {
                *slot = o;
            }
            self.finish(ai, k, st::REJECTED);
            let o = self.order_at(ai, k);
            let ev = self.order_event(
                ai,
                &o,
                OrderEventKind::Reject,
                o.px,
                o.qty,
                0,
                AssetId(0),
                0,
                r.code(),
                ex_ts,
                due,
            );
            self.emit(ai, ev, due);
            return;
        }
        let Some(ii) = ii else {
            return;
        };
        o.status = st::NEW;
        if let Some(slot) = self.accts.get_mut(ai).and_then(|a| a.orders.get_mut(k)) {
            *slot = o;
        }
        let ack = self.order_event(
            ai,
            &o,
            OrderEventKind::Ack,
            o.px,
            o.qty,
            0,
            AssetId(0),
            0,
            0,
            ex_ts,
            due,
        );
        self.emit(ai, ack, due);
        if o.tif == Tif::PostOnly {
            self.rest(ai, k);
            return;
        }
        let fok = o.tif == Tif::Fok;
        let top_only = self.take(ai, ii, o.side, o.px, o.qty, t, fok);
        let fills = core::mem::take(&mut self.fills);
        for &(px, q) in &fills {
            if top_only && let Some(a) = self.accts.get_mut(ai) {
                a.counters.top_only_fills += 1;
            }
            self.apply_fill(ai, k, px, q, false, due, ex_ts);
        }
        self.fills = fills;
        let o = self.order_at(ai, k);
        if o.remaining() <= 0 {
            self.finish(ai, k, st::FILLED);
            return;
        }
        match o.tif {
            Tif::Gtc => self.rest(ai, k),
            _ => {
                self.finish(ai, k, st::EXPIRED);
                let o = self.order_at(ai, k);
                let ev = self.order_event(
                    ai,
                    &o,
                    OrderEventKind::Expired,
                    o.px,
                    o.remaining(),
                    0,
                    AssetId(0),
                    0,
                    0,
                    ex_ts,
                    due,
                );
                self.emit(ai, ev, due);
            }
        }
    }

    pub(crate) fn order_at(&self, ai: usize, k: usize) -> Order {
        self.accts
            .get(ai)
            .and_then(|a| a.orders.get(k))
            .copied()
            .unwrap_or(Order::EMPTY)
    }

    /// Order `k` becomes resting with its remaining quantity locked.
    fn rest(&mut self, ai: usize, k: usize) {
        let o = self.order_at(ai, k);
        if let Some(a) = self.accts.get_mut(ai) {
            if let Some(x) = a.orders.get_mut(k) {
                x.resting = true;
                x.status = if o.filled > 0 { st::PARTIAL } else { st::NEW };
            }
            a.n_resting += 1;
        }
        let (_, amt) = self.lock_for(ai, usize::from(o.inst), o.side, o.px, o.remaining());
        self.set_lock(ai, k, amt);
    }

    /// One fill of order `k`: settlement, order bookkeeping and the `PartialFill`/`Fill` event.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_fill(
        &mut self,
        ai: usize,
        k: usize,
        px: i64,
        q: i64,
        maker: bool,
        due: u64,
        ex_ts: i64,
    ) {
        let o = self.order_at(ai, k);
        let ii = usize::from(o.inst);
        let rate = self
            .accts
            .get(ai)
            .and_then(|a| a.fees.get(ii))
            .map_or(0, |f| if maker { f.maker.0 } else { f.taker.0 });
        let (fee, fee_asset, cash) = self.settle_fill(ai, ii, o.side, px, q, rate, due);
        let perp = self.insts.get(ii).is_some_and(|r| r.perp);
        if let Some(x) = self.accts.get_mut(ai).and_then(|a| a.orders.get_mut(k)) {
            x.filled = x.filled.saturating_add(q);
            x.fill_val = x.fill_val.saturating_add(i128::from(q) * i128::from(px));
            if perp {
                x.cash_qty = x.cash_qty.saturating_add(q);
                x.cash_sum = x.cash_sum.saturating_add(cash);
            }
        }
        let o = self.order_at(ai, k);
        if o.resting {
            let (_, amt) = self.lock_for(ai, ii, o.side, o.px, o.remaining());
            self.set_lock(ai, k, amt);
            if let Some(x) = self.accts.get_mut(ai).and_then(|a| a.orders.get_mut(k)) {
                x.status = st::PARTIAL;
            }
        }
        let what = if o.remaining() <= 0 {
            OrderEventKind::Fill
        } else {
            OrderEventKind::PartialFill
        };
        let shown_fee = if self.tune.late_fee { 0 } else { fee };
        let ev = self.order_event(ai, &o, what, px, q, shown_fee, fee_asset, 0, 0, ex_ts, due);
        self.emit(ai, ev, due);
        if self.tune.late_fee && fee != 0 {
            let at = due.saturating_add(self.ms_to_tsc(50));
            let fo = self.order_event(
                ai,
                &o,
                OrderEventKind::FeeOnly,
                px,
                0,
                fee,
                fee_asset,
                0,
                0,
                ex_ts,
                at,
            );
            self.emit(ai, fo, at);
        }
    }

    /// Trade-through fill of resting order `k`.
    pub(crate) fn maker_fill(&mut self, ai: usize, k: usize, q: i64, _t: u64, due: u64, ex_ts: i64) {
        let o = self.order_at(ai, k);
        self.apply_fill(ai, k, o.px, q, true, due, ex_ts);
        if self.order_at(ai, k).remaining() <= 0 {
            self.finish(ai, k, st::FILLED);
        }
    }

    /// Walks the matching view up to `limit` (README "3. 吃单撮合"); fills land in `self.fills`.
    /// Returns whether the inst has only the `Bbo` stream.
    #[allow(clippy::too_many_arguments)]
    fn take(&mut self, ai: usize, ii: usize, side: Side, limit: i64, qty: i64, t: u64, fok: bool) -> bool {
        let info = self.view_levels(ii, side, limit);
        self.fills.clear();
        let lot = self.insts.get(ii).map_or(1, |r| r.lot);
        let bp = i128::from(self.tune.take_avail_bp);
        let decay = i128::from(self.ms_to_tsc(u64::from(self.tune.consume_decay_ms)));
        let si = side_ix(side);
        let levels = core::mem::take(&mut self.levels);
        for &(px, vis) in &levels {
            let (mem, hit) = self.mem_scaled(ai, ii, si, px, t, decay);
            if hit && let Some(a) = self.accts.get_mut(ai) {
                a.counters.liquidity_memory_hits += 1;
            }
            let avail = if decay > 0 {
                let num = i128::from(vis) * bp * decay - mem * 10_000;
                div_round(num.max(0), 10_000 * decay, RoundMode::Floor)
            } else {
                div_round(i128::from(vis) * bp, 10_000, RoundMode::Floor)
            };
            let avail = sat64(floor_to(avail, lot));
            self.fills.push((px, avail.max(0)));
        }
        self.levels = levels;
        if fok {
            let total = self.fills.iter().fold(0i64, |s, (_, a)| s.saturating_add(*a));
            if total < qty {
                self.fills.clear();
                return info.top_only;
            }
        }
        let mut rem = qty;
        for f in self.fills.iter_mut() {
            let q = rem.min(f.1).max(0);
            f.1 = q;
            rem -= q;
        }
        self.fills.retain(|f| f.1 > 0);
        let fills = core::mem::take(&mut self.fills);
        for &(px, q) in &fills {
            self.remember(ai, ii, si, px, q, t, decay);
        }
        self.fills = fills;
        info.top_only
    }

    /// Σ remaining × decay of the live memory entries at `px` (and whether any is live).
    fn mem_scaled(&self, ai: usize, ii: usize, si: usize, px: i64, t: u64, decay: i128) -> (i128, bool) {
        let Some(a) = self.accts.get(ai) else {
            return (0, false);
        };
        let base = (ii * 2 + si) * MEMORY_RING;
        let Some(ring) = a.mem.get(base..base + MEMORY_RING) else {
            return (0, false);
        };
        let mut s = 0i128;
        let mut hit = false;
        for e in ring.iter().filter(|e| e.used && e.px == px) {
            let dt = i128::from(t.saturating_sub(e.t0));
            if dt < decay {
                s += i128::from(e.qty) * (decay - dt);
                hit = true;
            }
        }
        (s, hit)
    }

    #[allow(clippy::too_many_arguments)]
    fn remember(&mut self, ai: usize, ii: usize, si: usize, px: i64, q: i64, t: u64, decay: i128) {
        let Some(a) = self.accts.get_mut(ai) else {
            return;
        };
        let base = (ii * 2 + si) * MEMORY_RING;
        let Some(ring) = a.mem.get_mut(base..base + MEMORY_RING) else {
            return;
        };
        let free = ring
            .iter()
            .position(|e| !e.used || i128::from(t.saturating_sub(e.t0)) >= decay);
        let k = free.or_else(|| ring.iter().enumerate().min_by_key(|(_, e)| e.t0).map(|(k, _)| k));
        if let Some(e) = k.and_then(|k| ring.get_mut(k)) {
            *e = MemEntry {
                px,
                qty: q,
                t0: t,
                used: true,
            };
        }
    }

    // ---- reject rules ----

    /// Rules 1–10 of README "拒单规则" for a new order.
    fn check_place(&mut self, ai: usize, ii: usize, c: &GwCmd, t: u64) -> Result<(), RejectReason> {
        let post_only = c.tif == Tif::PostOnly || OrderFlags(c.oflags).contains(OrderFlags::POST_ONLY);
        let reduce_only = OrderFlags(c.oflags).contains(OrderFlags::REDUCE_ONLY);
        // 1. trading status; book replica usable; reduce-only inst only reduces.
        let (tradable, ro, perp, venue) = self.insts.get(ii).map_or((false, false, false, VenueId(0)), |r| {
            (r.tradable, r.reduce_only, r.perp, r.venue)
        });
        if !tradable {
            return Err(RejectReason::NotTrading);
        }
        if self.book_not_live(ii) {
            if let Some(a) = self.accts.get_mut(ai) {
                a.counters.book_not_live_rejects += 1;
            }
            return Err(RejectReason::NotTrading);
        }
        if ro && self.increases(ai, ii, c.side, c.qty.0) {
            return Err(RejectReason::NotTrading);
        }
        self.check_shape(ii, c.px.0, c.qty.0)?;
        // 5. price band.
        self.check_band(ii, c.px.0)?;
        // 6. reduce-only flag.
        if reduce_only {
            let supported = self
                .venues
                .get(usize::from(venue.0))
                .is_some_and(|v| v.reduce_only);
            if !supported {
                return Err(RejectReason::Unsupported);
            }
            if !perp || self.increases(ai, ii, c.side, c.qty.0) {
                return Err(RejectReason::ReduceOnly);
            }
        }
        // 7. post-only must not cross.
        if post_only && self.would_cross(ii, c.side, c.px.0) {
            return Err(RejectReason::PostOnly);
        }
        // 8./9. balance or margin.
        self.check_funds(ai, ii, c.side, c.px.0, c.qty.0, 0)?;
        // 10. rate limit.
        if self.tune.rate_limit && !self.rate_ok(ai, venue, t) {
            return Err(RejectReason::RateLimited);
        }
        Ok(())
    }

    /// Rules 2–4.
    fn check_shape(&self, ii: usize, px: i64, qty: i64) -> Result<(), RejectReason> {
        let Some(r) = self.insts.get(ii) else {
            return Err(RejectReason::NotTrading);
        };
        if qty <= 0 || qty % r.lot != 0 || qty < r.min_qty {
            return Err(RejectReason::Lot);
        }
        if px <= 0 || px % r.tick != 0 {
            return Err(RejectReason::Tick);
        }
        let notional = rescale(
            i128::from(qty) * i128::from(px),
            r.ve(),
            NOTIONAL_EXP,
            RoundMode::Floor,
        );
        if notional < i128::from(r.min_notional) {
            return Err(RejectReason::MinNotional);
        }
        Ok(())
    }

    /// Rule 5: `ref × (1 − down) ≤ px ≤ ref × (1 + up)`; ref = mark (perpetual) or mid; skipped when
    /// the band is not registered or no reference exists.
    fn check_band(&mut self, ii: usize, px: i64) -> Result<(), RejectReason> {
        let Some(r) = self.insts.get(ii) else {
            return Ok(());
        };
        let (up, down) = (r.band.up.0, r.band.down.0);
        if up == 0 && down == 0 {
            return Ok(());
        }
        let refx2 = if r.perp && r.mark_set {
            Some(2 * i128::from(r.mark))
        } else {
            self.mid_x2(ii)
        };
        let Some(refx2) = refx2 else {
            return Ok(());
        };
        let p2 = 2 * i128::from(px) * 1_000_000_000;
        let hi = refx2 * (1_000_000_000 + i128::from(up));
        let lo = refx2 * (1_000_000_000 - i128::from(down));
        if p2 > hi || p2 < lo {
            return Err(RejectReason::PriceLimit);
        }
        Ok(())
    }

    /// An order of `side`/`qty` would increase the absolute position (spot: any buy).
    fn increases(&self, ai: usize, ii: usize, side: Side, qty: i64) -> bool {
        let perp = self.insts.get(ii).is_some_and(|r| r.perp);
        if !perp {
            return side == Side::Buy;
        }
        let pos = self
            .accts
            .get(ai)
            .and_then(|a| a.pos.get(ii))
            .map_or(0, |p| p.qty);
        let new = i128::from(pos) + i128::from(qty) * i128::from(side.sign());
        new.abs() > i128::from(pos).abs() || (pos != 0 && new.signum() == -i128::from(pos.signum()))
    }

    /// A post-only order at `px` would trade immediately.
    fn would_cross(&mut self, ii: usize, side: Side, px: i64) -> bool {
        match self.best_opposite(ii, side) {
            Some(p) => {
                if side == Side::Buy {
                    px >= p
                } else {
                    px <= p
                }
            }
            None => false,
        }
    }

    /// Rules 8–9; `held` = what the order being amended already holds (spot lock or margin).
    fn check_funds(
        &self,
        ai: usize,
        ii: usize,
        side: Side,
        px: i64,
        qty: i64,
        held: i128,
    ) -> Result<(), RejectReason> {
        let Some(r) = self.insts.get(ii) else {
            return Err(RejectReason::NotTrading);
        };
        if r.perp {
            let pos = self
                .accts
                .get(ai)
                .and_then(|a| a.pos.get(ii))
                .map_or(0, |p| p.qty);
            let new = i128::from(pos) + i128::from(qty) * i128::from(side.sign());
            let inc = (new.abs() - i128::from(pos).abs()).max(0);
            if inc == 0 {
                return Ok(());
            }
            let need = self.initial_e(ai, ii, inc, px);
            let avail = self.margin_calc(ai, r.venue).avail_e + held;
            if avail < need {
                return Err(RejectReason::InsufficientMargin);
            }
            Ok(())
        } else {
            let (asset, need) = self.lock_for(ai, ii, side, px, qty);
            let free = self.bal(ai, r.venue, asset);
            let free = i128::from(free.total) - i128::from(free.locked) + held;
            if free < i128::from(need) {
                return Err(RejectReason::InsufficientBalance);
            }
            Ok(())
        }
    }

    /// Rule 10: fixed-window `Orders` counters of the venue's limits.
    fn rate_ok(&mut self, ai: usize, venue: VenueId, t: u64) -> bool {
        let ms = self.tsc_to_ms(t);
        let Some(vr) = self.venues.get(usize::from(venue.0)) else {
            return true;
        };
        let Some(a) = self.accts.get_mut(ai) else {
            return true;
        };
        let Some(&vs) = a.vslot.get(usize::from(venue.0)) else {
            return true;
        };
        let Some(wins) = a.rate.get_mut(usize::from(vs)) else {
            return true;
        };
        let mut ok = true;
        for (l, w) in vr.limits.iter().zip(wins.iter_mut()) {
            let win = ms / u64::from(l.window_ms);
            if w.win != win {
                w.win = win;
                w.cnt = 0;
            }
            w.cnt = w.cnt.saturating_add(u32::from(l.default_cost.max(1)));
            if w.cnt > l.limit {
                ok = false;
            }
        }
        ok
    }

    // ---- cancel / amend / cancel-all / query ----

    fn target(&self, ai: usize, c: &GwCmd) -> Option<usize> {
        self.find_slot(ai, c.target_slot.0)
    }

    fn exec_cancel(&mut self, ai: usize, pc: &PendCmd) {
        let c = pc.cmd;
        let k = self.target(ai, &c);
        let o = k.map(|k| self.order_at(ai, k));
        let venue = o
            .and_then(|o| self.insts.get(usize::from(o.inst)).map(|r| r.venue))
            .unwrap_or_else(|| self.venue_of(c.inst));
        if self.conn_down(ai, venue) {
            self.push_no_conn(ai, pc, venue, o.map_or(0, |o| o.r#gen));
            return;
        }
        let t = pc.arrive;
        let due = self.resp_due(ai, t);
        match (k, o) {
            (Some(k), Some(o)) if o.resting => {
                let ex = self.inst_ex_ts(usize::from(o.inst));
                self.finish(ai, k, st::CANCELLED);
                let o = self.order_at(ai, k);
                let ev = self.order_event(
                    ai,
                    &o,
                    OrderEventKind::CancelAck,
                    o.px,
                    o.remaining(),
                    0,
                    AssetId(0),
                    0,
                    0,
                    ex,
                    due,
                );
                self.emit(ai, ev, due);
            }
            _ => {
                let mut ghost = o.unwrap_or(Order {
                    used: true,
                    slot: c.target_slot.0,
                    inst: self.inst_index(c.inst).map_or(u16::MAX, |x| x as u16),
                    ..Order::EMPTY
                });
                ghost.slot = c.target_slot.0;
                let ev = self.order_event(
                    ai,
                    &ghost,
                    OrderEventKind::CancelReject,
                    ghost.px,
                    0,
                    0,
                    AssetId(0),
                    GONE,
                    0,
                    0,
                    due,
                );
                self.push_event_raw(ai, ev, due);
            }
        }
    }

    /// Pushes an event without touching the order table.
    fn push_event_raw(&mut self, ai: usize, ev: OrderEvent, due: u64) {
        if let Some(id) = self.accts.get(ai).map(|a| a.id) {
            self.push_reply(
                id,
                due,
                RepMsg::OrderEvent(ev, hft_event::RecordFlags(0)),
                Bulk::None,
            );
        }
    }

    fn exec_amend(&mut self, ai: usize, pc: &PendCmd) {
        let c = pc.cmd;
        let k = self.target(ai, &c);
        let o = k.map(|k| self.order_at(ai, k));
        let venue = o
            .and_then(|o| self.insts.get(usize::from(o.inst)).map(|r| r.venue))
            .unwrap_or_else(|| self.venue_of(c.inst));
        if self.conn_down(ai, venue) {
            self.push_no_conn(ai, pc, venue, o.map_or(0, |o| o.r#gen));
            return;
        }
        let t = pc.arrive;
        let due = self.resp_due(ai, t);
        let ok = match (k, o) {
            (Some(k), Some(o)) if o.resting => {
                let ii = usize::from(o.inst);
                let new_rem = c.qty.0.saturating_sub(o.filled);
                let shape = self.check_shape(ii, c.px.0, c.qty.0).is_ok() && new_rem > 0;
                let cross = o.tif == Tif::PostOnly && self.would_cross(ii, o.side, c.px.0);
                let held = if self.insts.get(ii).is_some_and(|r| r.perp) {
                    self.initial_e(ai, ii, i128::from(o.remaining()), o.px)
                } else {
                    i128::from(o.lock)
                };
                let funds =
                    shape && !cross && self.check_funds(ai, ii, o.side, c.px.0, new_rem, held).is_ok();
                if funds {
                    if let Some(x) = self.accts.get_mut(ai).and_then(|a| a.orders.get_mut(k)) {
                        x.px = c.px.0;
                        x.qty = c.qty.0;
                    }
                    let (_, amt) = self.lock_for(ai, ii, o.side, c.px.0, new_rem);
                    self.set_lock(ai, k, amt);
                    Some(k)
                } else {
                    None
                }
            }
            _ => None,
        };
        let ex = o.map_or(0, |o| self.inst_ex_ts(usize::from(o.inst)));
        match ok {
            Some(k) => {
                let o = self.order_at(ai, k);
                let ev = self.order_event(
                    ai,
                    &o,
                    OrderEventKind::AmendAck,
                    o.px,
                    o.qty,
                    0,
                    AssetId(0),
                    0,
                    0,
                    ex,
                    due,
                );
                self.emit(ai, ev, due);
            }
            None => {
                let mut ghost = o.unwrap_or(Order {
                    used: true,
                    inst: self.inst_index(c.inst).map_or(u16::MAX, |x| x as u16),
                    ..Order::EMPTY
                });
                ghost.slot = c.target_slot.0;
                let gone = o.is_none_or(|o| !o.resting);
                let ev = self.order_event(
                    ai,
                    &ghost,
                    OrderEventKind::AmendReject,
                    c.px.0,
                    c.qty.0,
                    0,
                    AssetId(0),
                    if gone { GONE } else { 0 },
                    0,
                    ex,
                    due,
                );
                self.push_event_raw(ai, ev, due);
            }
        }
    }

    fn exec_cancel_all(&mut self, ai: usize, pc: &PendCmd) {
        let c = pc.cmd;
        let only = if c.inst.0 == 0 {
            None
        } else {
            self.inst_index(c.inst)
        };
        if c.inst.0 != 0 && only.is_none() {
            return;
        }
        let venue = only.and_then(|ii| self.insts.get(ii)).map(|r| r.venue);
        if let Some(v) = venue
            && self.conn_down(ai, v)
        {
            self.push_no_conn(ai, pc, v, 0);
            return;
        }
        let t = pc.arrive;
        let mut due = None;
        let n = self.accts.get(ai).map_or(0, |a| a.orders.len());
        let mut down_seen: Option<VenueId> = None;
        for k in 0..n {
            let o = self.order_at(ai, k);
            if !o.used || !o.resting || only.is_some_and(|ii| usize::from(o.inst) != ii) {
                continue;
            }
            let ov = self
                .insts
                .get(usize::from(o.inst))
                .map_or(VenueId(0), |r| r.venue);
            if self.conn_down(ai, ov) {
                down_seen = Some(ov);
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
            let ex = self.inst_ex_ts(usize::from(o.inst));
            self.finish(ai, k, st::CANCELLED);
            let o = self.order_at(ai, k);
            let ev = self.order_event(
                ai,
                &o,
                OrderEventKind::CancelAck,
                o.px,
                o.remaining(),
                0,
                AssetId(0),
                0,
                0,
                ex,
                d,
            );
            self.emit(ai, ev, d);
        }
        if let Some(v) = down_seen {
            self.push_no_conn(ai, pc, v, 0);
        }
    }

    fn exec_query(&mut self, ai: usize, pc: &PendCmd) {
        let c = pc.cmd;
        let k = self.target(ai, &c);
        let o = k.map(|k| self.order_at(ai, k));
        let venue = o
            .and_then(|o| self.insts.get(usize::from(o.inst)).map(|r| r.venue))
            .unwrap_or_else(|| self.venue_of(c.inst));
        if self.conn_down(ai, venue) {
            self.push_no_conn(ai, pc, venue, o.map_or(0, |o| o.r#gen));
            return;
        }
        let due = self.resp_due(ai, pc.arrive);
        let (o, status) = match o {
            Some(o) => {
                let s = if o.resting {
                    if o.filled > 0 { st::PARTIAL } else { st::NEW }
                } else {
                    o.status
                };
                (o, s)
            }
            None => (
                Order {
                    used: true,
                    slot: c.target_slot.0,
                    inst: self.inst_index(c.inst).map_or(u16::MAX, |x| x as u16),
                    ..Order::EMPTY
                },
                st::NOT_FOUND,
            ),
        };
        let ex = if o.inst == u16::MAX {
            0
        } else {
            self.inst_ex_ts(usize::from(o.inst))
        };
        let ev = self.order_event(
            ai,
            &o,
            OrderEventKind::QueryResult,
            o.px,
            o.qty,
            0,
            AssetId(0),
            status,
            0,
            ex,
            due,
        );
        self.push_event_raw(ai, ev, due);
    }
}
