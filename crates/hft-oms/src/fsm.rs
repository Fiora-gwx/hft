//! `place`/`cancel`/`amend`, the report table (one arm per cell) and fill increments.

use hft_event::{OrderAction, OrderEvent, OrderEventKind, OrderFlags, RecordHeader};
use hft_strategy_sdk::{HedgeKind, LinkRole, OrderRef, OrderUpdate, OrderUpdateKind};
use hft_types::{AssetId, GROUP_NONE, GroupId, InstId, Px, Qty, Side, StrategyId, Tif, Tsc};

/// `OrderEvent.detail` of Cancel/AmendReject: the order is already filled or gone (hft-event `RejectClass`).
pub(crate) const REJECT_GONE: u8 = 1;
use crate::Oms;
use crate::notes;
use crate::types::{
    ApprovedIntent, FillOut, OmsOut, OmsTimerKind, OmsTimerReq, OpError, OrderSlot, OrderState, Origin,
    PlaceError, SlotExt, SlotFlags, SlotStamps, TransitionCause, TrigStamps, anomaly, fill_flags,
};

/// One fill increment to apply.
#[derive(Copy, Clone, Debug)]
pub(crate) struct FillIn {
    pub(crate) cum: Qty,
    pub(crate) avg_px: Px,
    /// Price of this fill (OrderEvent fills without `avg_px`).
    pub(crate) px: Px,
    pub(crate) fee: Qty,
    pub(crate) fee_asset: AssetId,
    pub(crate) cash_qty: Qty,
    pub(crate) cash_sum: Qty,
    /// QueryResult / reconciliation: without `avg_px` the order price is used (`PX_EST`).
    pub(crate) query: bool,
    pub(crate) seq: u64,
}

/// What an applied increment did.
#[derive(Copy, Clone, Debug)]
pub(crate) struct FillDone {
    /// Gross increment.
    pub(crate) d: Qty,
    /// Net increment (magnitude).
    pub(crate) d_net: Qty,
    /// Net increment that counts for hedging (excludes the part above `qty`).
    pub(crate) hedge_net: Qty,
    pub(crate) px: Px,
    pub(crate) overfill: bool,
}

/// Net quantity of a fill (README "成交净量"; rule owned by hft-ledger).
#[inline]
pub(crate) fn net_of(d: Qty, fee: Qty, fee_asset: AssetId, base: AssetId, spot: bool, side: Side) -> Qty {
    if spot && fee_asset == base && fee_asset != hft_types::ASSET_NONE {
        match side {
            Side::Buy => Qty(d.0.saturating_sub(fee.0)),
            Side::Sell => Qty(d.0.saturating_add(fee.0)),
        }
    } else {
        d
    }
}

impl Oms {
    // ------------------------------------------------------------------ owner entries

    /// Allocates a slot for an approved intent and emits its `OrderDecision{New}`.
    pub fn place(
        &mut self,
        it: &ApprovedIntent,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) -> Result<OrderRef, PlaceError> {
        let r = self.place_inner(it, now, cause_seq, out);
        if r.is_err() && it.origin == Origin::Oms && it.link_id != 0 {
            self.hedge_place_failed(it.link_id, now, cause_seq, out);
        }
        self.flush(out);
        r
    }

    fn place_inner(
        &mut self,
        it: &ApprovedIntent,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) -> Result<OrderRef, PlaceError> {
        let oi = &it.intent;
        let ti = self.tix(oi.venue, oi.account).ok_or(PlaceError::NoTable)?;
        let link = oi.link.get().copied();
        let first = link.is_some_and(|l| l.role == LinkRole::First) && it.origin != Origin::Oms;
        if first && self.free_link_ix(oi.account).is_none() {
            return Err(PlaceError::LinkTableFull);
        }
        let t = self.tm(ti).ok_or(PlaceError::NoTable)?;
        let s = t.free.pop().ok_or(PlaceError::NoFreeSlot)?;
        let epoch = (self.engine_epoch & 0x7FFF) as u16;
        let role = link.map_or(0, |l| l.role as u8);
        let mut oflags: u16 = 0;
        if it.reduce_only {
            oflags |= OrderFlags::REDUCE_ONLY.0;
        }
        if oi.tif == Tif::PostOnly {
            oflags |= OrderFlags::POST_ONLY.0;
        }
        if first
            || (it.origin == Origin::Oms
                && matches!(
                    link.map(|l| l.role),
                    Some(LinkRole::Second | LinkRole::SecondRetry | LinkRole::Reverse)
                ))
        {
            oflags |= OrderFlags::LINKED.0;
        }
        if link.is_some_and(|l| l.role == LinkRole::HedgeFix) {
            oflags |= OrderFlags::HEDGE_FIX.0;
        }
        if it.risk_reducing {
            oflags |= OrderFlags::RISK_REDUCING.0;
        }
        let smoke = it.origin == Origin::Command && it.smoke_ttl_ms > 0;
        if smoke {
            oflags |= OrderFlags::SMOKE.0;
        }
        let group = if smoke { GROUP_NONE } else { oi.group };
        let leg_ix = self.leg_ix_of(oi.account, group, oi.inst);
        let t = self.tm(ti).ok_or(PlaceError::NoTable)?;
        let (venue, account) = (t.venue, t.account);
        let Some(sl) = t.slots.get_mut(s as usize) else {
            return Err(PlaceError::NoFreeSlot);
        };
        let r#gen = sl.r#gen.wrapping_add(1).max(1);
        *sl = OrderSlot {
            state: OrderState::PendingNew,
            side: oi.side,
            tif: oi.tif,
            prev_state: OrderState::Free,
            oflags,
            venue,
            account,
            r#gen,
            slot_id: s,
            inst: oi.inst,
            strategy: it.strategy,
            query_count: 0,
            epoch,
            group,
            link_id: 0,
            qty: oi.qty,
            filled: Qty(0),
            px: oi.px,
            exch_order_id: 0,
            filled_notional: 0,
            pending_px: Px(0),
            pending_qty: Qty(0),
            fee_accum: 0,
            fee_asset: AssetId(0),
            sflags: SlotFlags(if it.origin == Origin::Exec {
                SlotFlags::EXEC
            } else {
                0
            }),
            reject_code: 0,
            t_state: now,
            seq_last: seq,
        };
        if let Some(ex) = t.ext.get_mut(s as usize) {
            *ex = SlotExt {
                net_filled: Qty(0),
                tag: oi.tag,
                leg_ix,
                role,
                origin: it.origin as u8,
                class: it.class as u8,
                lev_x100: it.lev_x100,
                _pad1: 0,
                max_rest_ms: oi.max_rest_ms,
                reason: it.reason,
                intent_no: it.intent_no,
                fee_cum: Qty(0),
                cash_qty: Qty(0),
                cash_sum: Qty(0),
                _rsv: [0; 1],
            };
        }
        if let Some(st) = t.stamps.get_mut(s as usize) {
            *st = stamps_of(&it.trig, SlotStamps::OP_NEW);
        }
        if smoke {
            t.smoke_slot = s;
        }
        let h = OrderRef::new(venue, account, s, r#gen);
        // Link bookkeeping before the in-flight contribution (the contribution carries link_id).
        let mut link_id = 0;
        if first && let Some(l) = link {
            link_id = self.create_link(ti, s, &l, it, now, seq, out);
        } else if it.origin == Origin::Oms && it.link_id != 0 {
            link_id = it.link_id;
        }
        if let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            sl.link_id = link_id;
        }
        let c = self.contrib(ti, s);
        self.apply(ti, &c, 1);
        if it.origin == Origin::Oms && link_id != 0 {
            self.hedge_placed(link_id, ti, s, it, now, seq, out);
        }
        if smoke {
            let deadline = Tsc(now.0.saturating_add(self.ms(it.smoke_ttl_ms)));
            self.push_timer(
                OmsTimerKind::SmokeTtl,
                OmsTimerReq::smoke_id(account, venue),
                deadline,
                false,
                out,
            );
        }
        self.push_notify(
            h,
            OrderState::Free,
            OrderState::PendingNew,
            TransitionCause::Place,
            seq,
            out,
        );
        self.push_decision(ti, OrderAction::New, s, seq, out);
        if first
            && let Some(l) = link
            && l.hedge.kind == HedgeKind::Simultaneous
        {
            self.simultaneous_second(link_id, now, seq, out);
        }
        Ok(h)
    }

    /// Owner cancel (`Live`, `PendingNew`, `PendingAmend` without `CANCEL_SENT`).
    pub fn cancel(
        &mut self,
        s: OrderRef,
        trig: &TrigStamps,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) -> Result<(), OpError> {
        let (ti, i) = self.locate(s).ok_or(OpError::StaleRef)?;
        let sl = self.t(ti).and_then(|t| t.slot(i)).ok_or(OpError::StaleRef)?;
        if !matches!(
            sl.state,
            OrderState::Live | OrderState::PendingNew | OrderState::PendingAmend
        ) || sl.sflags.has(SlotFlags::CANCEL_SENT)
        {
            return Err(OpError::NotAllowed);
        }
        self.cancel_slot(ti, i, TransitionCause::CancelReq, Some(trig), now, cause_seq, out);
        self.flush(out);
        Ok(())
    }

    /// Owner amend (`Live` on a venue with `AMEND`).
    #[allow(clippy::too_many_arguments)]
    pub fn amend(
        &mut self,
        s: OrderRef,
        px: Px,
        qty: Qty,
        trig: &TrigStamps,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) -> Result<(), OpError> {
        let (ti, i) = self.locate(s).ok_or(OpError::StaleRef)?;
        let t = self.t(ti).ok_or(OpError::StaleRef)?;
        let sl = t.slot(i).ok_or(OpError::StaleRef)?;
        if sl.state != OrderState::Live || !t.amend || self.inst(sl.inst).no_amend {
            return Err(OpError::NotAllowed);
        }
        if t.amend_down_only && (px != sl.px || qty.0 > sl.qty.0) {
            return Err(OpError::AmendUnsupported);
        }
        let t = self.tm(ti).ok_or(OpError::StaleRef)?;
        if let Some(sl) = t.slots.get_mut(i as usize) {
            sl.pending_px = px;
            sl.pending_qty = qty;
            sl.sflags.0 |= SlotFlags::AMEND_SENT;
        }
        if let Some(st) = t.stamps.get_mut(i as usize)
            && st.op == SlotStamps::OP_NONE
        {
            *st = stamps_of(trig, SlotStamps::OP_AMEND);
        }
        self.transition(
            ti,
            i,
            OrderState::PendingAmend,
            TransitionCause::AmendReq,
            now,
            cause_seq,
            out,
        );
        self.push_decision(ti, OrderAction::Amend, i, cause_seq, out);
        self.flush(out);
        Ok(())
    }

    /// Cancels one cancellable slot: `→ PendingCancel`, `CANCEL_SENT`, `OrderDecision{Cancel}`.
    /// `trig = Some` stamps the latency sample (owner cancels only).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cancel_slot(
        &mut self,
        ti: usize,
        s: u32,
        cause: TransitionCause,
        trig: Option<&TrigStamps>,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        self.mark_cancel(ti, s, cause, trig, now, seq, out);
        self.push_decision(ti, OrderAction::Cancel, s, seq, out);
    }

    /// State part of a cancel without its decision (batch `CancelAll` uses one decision).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mark_cancel(
        &mut self,
        ti: usize,
        s: u32,
        cause: TransitionCause,
        trig: Option<&TrigStamps>,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        if let Some(t) = self.tm(ti) {
            if let Some(tr) = trig
                && let Some(st) = t.stamps.get_mut(s as usize)
                && st.op == SlotStamps::OP_NONE
            {
                *st = stamps_of(tr, SlotStamps::OP_CANCEL);
            }
            if let Some(sl) = t.slots.get_mut(s as usize) {
                sl.sflags.0 |= SlotFlags::CANCEL_SENT;
            }
        }
        self.transition(ti, s, OrderState::PendingCancel, cause, now, seq, out);
    }

    // ------------------------------------------------------------------ reports

    /// One `OrderEvent` (README "回报处理表", "回报匹配与成交增量").
    pub fn on_order_event(&mut self, ev: &OrderEvent, hdr: &RecordHeader, out: &mut OmsOut) {
        self.fee_ctx = None;
        let now = Tsc(hdr.tsc);
        let seq = hdr.seq;
        if ev.slot.0 != 0
            && self.has_foreign_cash(ev.venue, ev.account, ev.inst, ev.exch_order_id)
            && self
                .tix(ev.venue, ev.account)
                .and_then(|ti| self.t(ti))
                .is_some_and(|t| {
                    t.slot(ev.slot.0).is_some_and(|s| {
                        s.state != OrderState::Free && s.r#gen == ev.r#gen && s.epoch == ev.epoch
                    }) || t.tombs.iter().any(|b| {
                        b.r#gen == ev.r#gen && b.epoch == ev.epoch && b.exch_order_id == ev.exch_order_id
                    })
                })
        {
            out.cash_error = Some(crate::CashError::ExternalOverlap);
            return;
        }
        match self.tix(ev.venue, ev.account) {
            None => self.stats.unmatched_table += 1,
            Some(ti) => {
                if ev.slot.0 == 0 {
                    self.foreign_report(ti, ev, now, seq, out);
                } else {
                    let s = ev.slot.0;
                    let matched = self.t(ti).and_then(|t| t.slot(s)).is_some_and(|sl| {
                        sl.state != OrderState::Free && sl.r#gen == ev.r#gen && sl.epoch == ev.epoch
                    });
                    if matched {
                        self.slot_report(ti, s, ev, hdr, out);
                    } else if !self.tomb_report(ti, ev, now, seq, out) {
                        self.stale_gen_report(ti, ev, now, seq, out);
                    }
                }
            }
        }
        self.flush(out);
    }

    fn slot_report(&mut self, ti: usize, s: u32, ev: &OrderEvent, hdr: &RecordHeader, out: &mut OmsOut) {
        let now = Tsc(hdr.tsc);
        let seq = hdr.seq;
        let Some((sl, ex)) = self
            .t(ti)
            .and_then(|t| Some((*t.slot(s)?, *t.ext.get(s as usize)?)))
        else {
            return;
        };
        if self.inst_mismatch((sl.inst, sl.strategy, sl.group), ev, out) {
            return;
        }
        if let Err(e) = crate::cash::delta(
            self.inst(sl.inst).perp_cash,
            (sl.filled, ex.cash_qty, ex.cash_sum),
            (ev.cum_filled, ev.cash_qty, ev.cash_sum),
        ) {
            out.cash_error = Some(e);
            return;
        }
        if self.inst(sl.inst).perp_cash
            && ev.cum_filled.0 > sl.filled.0
            && self
                .t(ti)
                .and_then(|t| t.ext_held.get(sl.exch_order_id))
                .is_some_and(|x| x.0 > 0)
        {
            out.cash_error = Some(crate::CashError::ExternalOverlap);
            return;
        }
        if ev.what == OrderEventKind::FeeOnly {
            self.fee_only_slot(ti, s, ev, out);
            return;
        }
        self.maybe_sample(ti, s, ev, hdr, out);
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        let st = sl.state;
        use OrderState as S;
        match ev.what {
            OrderEventKind::Ack => {
                let had_eid = sl.exch_order_id != 0;
                self.set_eid_acked(ti, s, ev.exch_order_id);
                match st {
                    S::PendingNew | S::Unknown => {
                        self.transition(ti, s, S::Live, TransitionCause::Ack, now, seq, out);
                    }
                    S::PendingCancel | S::PendingAmend => {}
                    _ => {
                        let _ = had_eid;
                        self.stats.dup += 1;
                    }
                }
            }
            OrderEventKind::Reject => match st {
                S::PendingNew | S::Unknown => {
                    if let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                        sl.reject_code = ev.reject_code;
                    }
                    self.transition(ti, s, S::Rejected, TransitionCause::Reject, now, seq, out);
                }
                S::Live | S::PendingCancel | S::PendingAmend => self.table_anomaly(ti, s, ev, now, seq, out),
                _ => self.stats.dup += 1,
            },
            OrderEventKind::PartialFill | OrderEventKind::Fill => {
                self.fee_of_applied_fill(ti, s, ev, out);
                self.fill_report(ti, s, ev, now, seq, out);
                if let Some(x) = self.tm(ti).and_then(|t| t.ext.get_mut(s as usize))
                    && ev.cum_filled.0 > x.fee_cum.0
                {
                    x.fee_cum = ev.cum_filled;
                }
            }
            OrderEventKind::CancelAck | OrderEventKind::Expired => {
                if st.terminal() {
                    if ev.cum_filled.0 > sl.filled.0 {
                        self.table_anomaly(ti, s, ev, now, seq, out);
                    } else {
                        self.stats.dup += 1;
                    }
                    return;
                }
                self.pre_fill(ti, s, ev, now, seq, out);
                let cause = if ev.what == OrderEventKind::Expired {
                    TransitionCause::Expired
                } else {
                    TransitionCause::CancelAck
                };
                let filled_all = self
                    .t(ti)
                    .and_then(|t| t.slot(s))
                    .is_some_and(|x| x.filled.0 >= x.qty.0);
                let to = if filled_all { S::Filled } else { S::Cancelled };
                self.transition(ti, s, to, cause, now, seq, out);
            }
            OrderEventKind::CancelReject => {
                let gone = ev.detail == REJECT_GONE;
                match st {
                    S::PendingCancel | S::PendingAmend if gone => {
                        self.pre_fill(ti, s, ev, now, seq, out);
                        self.gone(ti, s, TransitionCause::CancelReject, now, seq, out);
                    }
                    S::PendingCancel if !gone => {
                        self.pre_fill(ti, s, ev, now, seq, out);
                        self.transition(ti, s, S::Live, TransitionCause::CancelReject, now, seq, out);
                    }
                    S::Unknown => {}
                    S::PendingNew | S::Live | S::PendingAmend => self.table_anomaly(ti, s, ev, now, seq, out),
                    _ => self.stats.dup += 1,
                }
            }
            OrderEventKind::AmendAck => match st {
                S::PendingAmend => {
                    self.pre_fill(ti, s, ev, now, seq, out);
                    self.amend_applied(ti, s, sl.pending_px, sl.pending_qty);
                    let full = self
                        .t(ti)
                        .and_then(|t| t.slot(s))
                        .is_some_and(|x| x.filled.0 >= x.qty.0);
                    let to = if full { S::Filled } else { S::Live };
                    self.transition(ti, s, to, TransitionCause::AmendAck, now, seq, out);
                }
                S::PendingCancel if sl.sflags.has(SlotFlags::AMEND_SENT) => {
                    self.pre_fill(ti, s, ev, now, seq, out);
                    self.amend_applied(ti, s, sl.pending_px, sl.pending_qty);
                    self.clear_flag(ti, s, SlotFlags::AMEND_SENT);
                }
                S::Unknown => {
                    self.pre_fill(ti, s, ev, now, seq, out);
                    self.amend_applied(ti, s, ev.px, ev.qty);
                    self.transition(ti, s, S::Live, TransitionCause::AmendAck, now, seq, out);
                }
                S::PendingNew | S::Live | S::PendingCancel => self.table_anomaly(ti, s, ev, now, seq, out),
                _ => self.stats.dup += 1,
            },
            OrderEventKind::AmendReject => {
                let gone = ev.detail == REJECT_GONE;
                match st {
                    S::PendingAmend if gone => {
                        self.pre_fill(ti, s, ev, now, seq, out);
                        self.gone(ti, s, TransitionCause::AmendReject, now, seq, out);
                    }
                    S::PendingAmend => {
                        self.pre_fill(ti, s, ev, now, seq, out);
                        self.transition(ti, s, S::Live, TransitionCause::AmendReject, now, seq, out);
                    }
                    S::PendingCancel if sl.sflags.has(SlotFlags::AMEND_SENT) => {
                        self.pre_fill(ti, s, ev, now, seq, out);
                        self.clear_flag(ti, s, SlotFlags::AMEND_SENT);
                    }
                    S::Unknown => {}
                    S::PendingNew | S::Live | S::PendingCancel => {
                        self.table_anomaly(ti, s, ev, now, seq, out)
                    }
                    _ => self.stats.dup += 1,
                }
            }
            OrderEventKind::QueryResult => self.query_result(ti, s, ev, now, seq, out),
            OrderEventKind::OutcomeUnknown => match st {
                S::PendingNew | S::PendingCancel | S::PendingAmend => {
                    self.transition(ti, s, S::Unknown, TransitionCause::OutcomeUnknown, now, seq, out);
                }
                S::Live => self.set_verify(ti, s, now, seq, out),
                S::Unknown => {}
                _ => self.stats.dup += 1,
            },
            OrderEventKind::FeeOnly => {}
        }
    }

    /// A `PartialFill`/`Fill` whose quantity an earlier non-fill report (`CancelReject`,
    /// `CancelAck`, `QueryResult`, …) already applied without a fee: its fee is booked like a
    /// `FeeOnly` (README "回报匹配与成交增量" 9). A repeated fill report (`cum_filled ≤ fee_cum`)
    /// books nothing; `fee_cum` also covers an increment whose non-fill report carried a fee.
    fn fee_of_applied_fill(&mut self, ti: usize, s: u32, ev: &OrderEvent, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let (Some(sl), Some(ex)) = (t.slot(s), t.ext.get(s as usize)) else {
            return;
        };
        if ev.fee.0 != 0 && ev.cum_filled.0 > ex.fee_cum.0 && ev.cum_filled.0 <= sl.filled.0 {
            self.fee_only_slot(ti, s, ev, out);
        }
    }

    /// `CancelReject`/`AmendReject` with `GONE`: Filled when fully filled, else Unknown + query.
    fn gone(&mut self, ti: usize, s: u32, cause: TransitionCause, now: Tsc, seq: u64, out: &mut OmsOut) {
        let full = self
            .t(ti)
            .and_then(|t| t.slot(s))
            .is_some_and(|x| x.filled.0 >= x.qty.0);
        let to = if full {
            OrderState::Filled
        } else {
            OrderState::Unknown
        };
        self.transition(ti, s, to, cause, now, seq, out);
        // The rejected cancel/amend was answered: it is no longer in flight (scenarios FP-4).
        // `Unknown` otherwise keeps both bits for a request the connection loss left unanswered.
        if to == OrderState::Unknown {
            let f = if cause == TransitionCause::CancelReject {
                SlotFlags::CANCEL_SENT
            } else {
                SlotFlags::AMEND_SENT
            };
            self.clear_flag(ti, s, f);
        }
    }

    fn set_eid_acked(&mut self, ti: usize, s: u32, eid: u64) {
        let before = self.contrib(ti, s);
        if let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            if sl.exch_order_id == 0 {
                sl.exch_order_id = eid;
            }
            if sl.state.open() {
                sl.sflags.0 |= SlotFlags::ACKED;
            }
        }
        let after = self.contrib(ti, s);
        self.apply(ti, &before, -1);
        self.apply(ti, &after, 1);
    }

    pub(crate) fn clear_flag(&mut self, ti: usize, s: u32, f: u16) {
        if let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            sl.sflags.0 &= !f;
        }
    }

    /// Applies an amend's price/quantity (`qty < filled` → `qty := filled`).
    fn amend_applied(&mut self, ti: usize, s: u32, px: Px, qty: Qty) {
        let before = self.contrib(ti, s);
        if let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            if px.0 != 0 {
                sl.px = px;
            }
            if qty.0 > 0 {
                sl.qty = Qty(qty.0.max(sl.filled.0));
            }
        }
        let after = self.contrib(ti, s);
        self.apply(ti, &before, -1);
        self.apply(ti, &after, 1);
    }

    /// "异常 Note" cell: apply a fill increment if any, keep the state, `OmsAnomaly{Table}`.
    fn table_anomaly(&mut self, ti: usize, s: u32, ev: &OrderEvent, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        let terminal = sl.state.terminal();
        let had_fill = ev.cum_filled.0 > sl.filled.0;
        if had_fill {
            self.fill_report(ti, s, ev, now, seq, out);
            if terminal {
                return; // fill_report already wrote the anomaly for a terminal slot
            }
        }
        self.anomaly_note(&sl, anomaly::TABLE, ev.what as u64, out);
    }

    /// Report identity check after the `(slot, gen, epoch)` match (README "回报匹配与成交增量").
    /// `inst = INST_NONE` means the venue did not supply it (e.g. WS API error replies) and is
    /// accepted. A different nonzero inst is a strict-cash `CashError::Identity` when the matched
    /// inst is `perp_cash`; elsewhere the report is ignored with a P0 `OmsAnomaly{InstMismatch}`.
    /// `true` = the caller stops processing the report.
    pub(crate) fn inst_mismatch(
        &mut self,
        own: (InstId, StrategyId, GroupId),
        ev: &OrderEvent,
        out: &mut OmsOut,
    ) -> bool {
        let (own, strategy, group) = own;
        if ev.inst == hft_types::INST_NONE || ev.inst == own {
            return false;
        }
        if self.inst(own).perp_cash {
            out.cash_error = Some(crate::CashError::Identity);
            return true;
        }
        let n = notes::note_at(
            notes::ANOMALY,
            hft_event::NoteLevel::P0,
            strategy,
            group,
            own,
            [
                anomaly::INST_MISMATCH,
                u64::from(ev.inst.0),
                ev.what as u64,
                u64::from(ev.slot.0),
                u64::from(ev.r#gen),
                ev.exch_order_id,
                0,
                0,
            ],
        );
        self.note(n, out);
        true
    }

    pub(crate) fn anomaly_note(&mut self, sl: &OrderSlot, kind: u64, arg: u64, out: &mut OmsOut) {
        let n = notes::note(
            notes::ANOMALY,
            sl.strategy,
            sl.group,
            sl.inst,
            [
                kind,
                arg,
                sl.state as u64,
                u64::from(sl.slot_id),
                u64::from(sl.r#gen),
                0,
                0,
                0,
            ],
        );
        self.note(n, out);
    }

    /// Fill increment carried by a non-fill report: applied with a same-state transition note.
    fn pre_fill(&mut self, ti: usize, s: u32, ev: &OrderEvent, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        if ev.cum_filled.0 <= sl.filled.0 {
            return;
        }
        let f = FillIn {
            cum: ev.cum_filled,
            avg_px: ev.avg_px,
            px: ev.px,
            fee: ev.fee,
            fee_asset: ev.fee_asset,
            cash_qty: ev.cash_qty,
            cash_sum: ev.cash_sum,
            query: false,
            seq,
        };
        if let Some(fd) = self.apply_fill(ti, s, &f, now, out) {
            self.link_fill(ti, s, &fd, now, seq, out);
            self.push_notify(sl.handle(), sl.state, sl.state, TransitionCause::Fill, seq, out);
            self.fill_update(ti, s, &fd, out);
        }
    }

    /// `PartialFill`/`Fill` in any matched state.
    pub(crate) fn fill_report(
        &mut self,
        ti: usize,
        s: u32,
        ev: &OrderEvent,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let f = FillIn {
            cum: ev.cum_filled,
            avg_px: ev.avg_px,
            px: ev.px,
            fee: ev.fee,
            fee_asset: ev.fee_asset,
            cash_qty: ev.cash_qty,
            cash_sum: ev.cash_sum,
            query: false,
            seq,
        };
        self.fill_to_state(ti, s, &f, None, now, seq, out);
    }

    /// Applies a fill and moves the slot per the fill rows of the table. `to` forces the final
    /// state (QueryResult/reconciliation), else the PartialFill/Fill rows decide.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fill_to_state(
        &mut self,
        ti: usize,
        s: u32,
        f: &FillIn,
        forced: Option<(OrderState, TransitionCause)>,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        let st = sl.state;
        let Some(fd) = self.apply_fill(ti, s, f, now, out) else {
            if out.cash_error.is_some() {
                return;
            }
            if forced.is_none() {
                self.stats.dup += 1;
            }
            if let Some((to, cause)) = forced
                && to != st
            {
                self.transition(ti, s, to, cause, now, seq, out);
            }
            return;
        };
        if st == OrderState::PendingNew {
            self.or_flag(ti, s, SlotFlags::FILL_BEFORE_ACK);
        }
        let full = f.cum.0 >= sl.qty.0;
        use OrderState as S;
        let (to, cause) = match forced {
            Some(x) => x,
            None => {
                let to = match st {
                    _ if full => S::Filled,
                    S::PendingNew | S::Live => S::Live,
                    S::PendingCancel | S::PendingAmend => st,
                    S::Unknown => match sl.prev_state {
                        S::PendingCancel | S::PendingAmend => sl.prev_state,
                        _ => S::Live,
                    },
                    _ => st,
                };
                (to, TransitionCause::Fill)
            }
        };
        if st.terminal() && forced.is_none() {
            // Fill after a terminal state: booked, anomaly, ask for reconciliation.
            self.anomaly_note(&sl, crate::types::anomaly::TABLE, 3, out);
            self.needs_rest(&sl, true, out);
        }
        // The slot is "acked" once anything filled.
        if st.open() && to.open() && to != S::Live {
            self.or_flag(ti, s, SlotFlags::ACKED);
        }
        self.link_fill(ti, s, &fd, now, seq, out);
        if to != st {
            self.transition(ti, s, to, cause, now, seq, out);
        } else {
            self.push_notify(sl.handle(), st, st, cause, seq, out);
        }
        self.fill_update(ti, s, &fd, out);
    }

    pub(crate) fn or_flag(&mut self, ti: usize, s: u32, f: u16) {
        let before = self.contrib(ti, s);
        if let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            sl.sflags.0 |= f;
        }
        let after = self.contrib(ti, s);
        self.apply(ti, &before, -1);
        self.apply(ti, &after, 1);
    }

    /// Owner `OrderUpdate::Filled` of an applied increment (smoke orders have no owner).
    pub(crate) fn fill_update(&mut self, ti: usize, s: u32, fd: &FillDone, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let (Some(sl), Some(ex)) = (t.slot(s), t.ext.get(s as usize)) else {
            return;
        };
        if ex.origin == Origin::Command as u8 {
            return;
        }
        let u = OrderUpdate {
            r: sl.handle(),
            group: sl.group,
            tag: ex.tag,
            kind: OrderUpdateKind::Filled {
                d: fd.d_net,
                px: fd.px,
                fee: sl.fee_accum,
                fee_asset: sl.fee_asset,
                cum: sl.filled,
                done: sl.state == OrderState::Filled,
            },
        };
        let _ = out.updates.push(u);
    }

    /// Applies a fill increment to the slot, the in-flight book, the LegTracker and `FillOut`.
    /// `None` when the cumulative quantity does not increase.
    pub(crate) fn apply_fill(
        &mut self,
        ti: usize,
        s: u32,
        f: &FillIn,
        now: Tsc,
        out: &mut OmsOut,
    ) -> Option<FillDone> {
        let t = self.t(ti)?;
        let sl = *t.slot(s)?;
        let ex = *t.ext.get(s as usize)?;
        let cash = match crate::cash::delta(
            self.inst(sl.inst).perp_cash,
            (sl.filled, ex.cash_qty, ex.cash_sum),
            (f.cum, f.cash_qty, f.cash_sum),
        ) {
            Ok(x) => x,
            Err(e) => {
                out.cash_error = Some(e);
                return None;
            }
        };
        let d = f.cum.0.saturating_sub(sl.filled.0);
        if d <= 0 {
            return None;
        }
        let info = *self.inst(sl.inst);
        let mut flags = 0u8;
        let overfill = f.cum.0 > sl.qty.0;
        if overfill {
            flags |= fill_flags::OVERFILL;
        }
        let hedge_gross = f
            .cum
            .0
            .min(sl.qty.0)
            .saturating_sub(sl.filled.0.min(sl.qty.0))
            .max(0);
        let (incr, new_notional) = if f.avg_px.0 != 0 {
            let n = i128::from(f.avg_px.0) * i128::from(f.cum.0);
            (n - sl.filled_notional, n)
        } else {
            let px = if f.query { sl.px } else { f.px };
            if f.query {
                flags |= fill_flags::PX_EST;
            }
            let i = i128::from(px.0) * i128::from(d);
            (i, sl.filled_notional + i)
        };
        let px = Px(div_half_even(incr, i128::from(d)));
        let fee_asset = if f.fee_asset.0 != 0 {
            f.fee_asset
        } else {
            sl.fee_asset
        };
        let d_net = net_of(Qty(d), f.fee, f.fee_asset, info.asset, info.spot, sl.side);
        // External-fill de-duplication (README "回报匹配与成交增量" 4).
        let mut booked = d;
        let mut booked_net = d_net.0;
        if sl.exch_order_id != 0
            && let Some(b) = self.t(ti).and_then(|t| t.ext_held.get(sl.exch_order_id))
            && b.0 > 0
        {
            let m = d.min(b.0);
            booked = d - m;
            booked_net = d_net.0.saturating_sub(m).max(0);
            flags |= fill_flags::EXT_DEDUP;
            if let Some(t) = self.tm(ti) {
                t.ext_held.set(sl.exch_order_id, Qty(b.0 - m));
            }
            let n = notes::note(
                notes::EXT_DEDUP,
                sl.strategy,
                sl.group,
                sl.inst,
                [
                    sl.exch_order_id,
                    d as u64,
                    m as u64,
                    u64::from(sl.slot_id),
                    u64::from(sl.r#gen),
                    0,
                    0,
                    0,
                ],
            );
            self.note(n, out);
        }
        let before = self.contrib(ti, s);
        if let Some(t) = self.tm(ti) {
            if let Some(x) = t.slots.get_mut(s as usize) {
                x.filled = f.cum;
                x.filled_notional = new_notional;
                x.fee_accum = x.fee_accum.saturating_add(f.fee.0);
                x.fee_asset = fee_asset;
                if x.state.open() {
                    x.sflags.0 |= SlotFlags::ACKED;
                }
            }
            if let Some(e) = t.ext.get_mut(s as usize) {
                e.net_filled = Qty(e.net_filled.0.saturating_add(d_net.0));
                // A fee booked with this increment (whatever report kind carried it) covers the
                // executions up to `cum`: a later fill report repeating them books no fee again.
                if f.fee.0 != 0 && f.cum.0 > e.fee_cum.0 {
                    e.fee_cum = f.cum;
                }
                if cash.is_some() {
                    e.cash_qty = f.cash_qty;
                    e.cash_sum = f.cash_sum;
                }
            }
        }
        let after = self.contrib(ti, s);
        self.apply(ti, &before, -1);
        self.apply(ti, &after, 1);
        let signed = booked_net.saturating_mul(sl.side.sign());
        self.leg_fill(sl.account, sl.group, ex.leg_ix, Qty(signed), px, now);
        if flags & fill_flags::PX_EST != 0 {
            let n = notes::note(
                notes::FILL_PX_ESTIMATED,
                sl.strategy,
                sl.group,
                sl.inst,
                [
                    u64::from(sl.slot_id),
                    u64::from(sl.r#gen),
                    d as u64,
                    px.0 as u64,
                    0,
                    0,
                    0,
                    0,
                ],
            );
            self.note(n, out);
            self.needs_rest(&sl, true, out);
        }
        if booked > 0 {
            let fo = FillOut {
                slot: sl.handle(),
                account: sl.account,
                venue: sl.venue,
                inst: sl.inst,
                group: sl.group,
                leg_ix: ex.leg_ix,
                side: sl.side,
                class: ex.class,
                flags,
                qty: Qty(booked),
                net_qty: Qty(signed),
                px,
                fee: f.fee.0,
                fee_asset: f.fee_asset,
                seq: f.seq,
                cash,
                notional: (booked == d).then_some(incr),
            };
            let r = out.fills.push(fo);
            debug_assert!(r.is_ok(), "OmsOut.fills overflow");
        }
        if ex.origin == Origin::Command as u8 {
            let n = notes::note_at(
                notes::ANOMALY,
                hft_event::NoteLevel::P0,
                sl.strategy,
                sl.group,
                sl.inst,
                [
                    anomaly::SMOKE_FILLED,
                    d as u64,
                    u64::from(sl.slot_id),
                    u64::from(sl.r#gen),
                    0,
                    0,
                    0,
                    0,
                ],
            );
            self.note(n, out);
        }
        let hedge_net = d_net.0.saturating_sub(d - hedge_gross).max(0);
        if overfill {
            let n = notes::note(
                notes::OVERFILL,
                sl.strategy,
                sl.group,
                sl.inst,
                [
                    u64::from(sl.slot_id),
                    u64::from(sl.r#gen),
                    f.cum.0 as u64,
                    sl.qty.0 as u64,
                    0,
                    0,
                    0,
                    0,
                ],
            );
            self.note(n, out);
        }
        Some(FillDone {
            d: Qty(d),
            d_net,
            hedge_net: Qty(hedge_net),
            px,
            overfill,
        })
    }

    /// `Note{OmsNeedsRestQuery}` for a slot.
    pub(crate) fn needs_rest(&mut self, sl: &OrderSlot, trades: bool, out: &mut OmsOut) {
        let v0 = u64::from(sl.venue.0) | u64::from(sl.account.0) << 8 | u64::from(trades) << 16;
        let n = notes::note(
            notes::NEEDS_REST_QUERY,
            sl.strategy,
            sl.group,
            sl.inst,
            [
                v0,
                u64::from(sl.slot_id),
                u64::from(sl.r#gen),
                u64::from(sl.epoch),
                sl.exch_order_id,
                0,
                0,
                0,
            ],
        );
        self.note(n, out);
    }

    /// Sequencer stamp of `decided_tsc` for the slot's pending sample.
    pub fn stamp_decided(&mut self, s: OrderRef, decided_tsc: Tsc) {
        if let Some((ti, i)) = self.locate(s)
            && let Some(st) = self.tm(ti).and_then(|t| t.stamps.get_mut(i as usize))
            && st.op != SlotStamps::OP_NONE
        {
            st.decided_tsc = decided_tsc.0;
        }
    }

    /// Default strategy for unknown owners.
    #[allow(dead_code)]
    pub(crate) fn owner_or_bound(&self, sl: &OrderSlot) -> hft_types::StrategyId {
        if sl.strategy.0 != 0 {
            sl.strategy
        } else {
            self.strategy_of(sl.account)
        }
    }
}

/// `SlotStamps` from a trigger and an operation.
pub(crate) fn stamps_of(tr: &TrigStamps, op: u8) -> SlotStamps {
    SlotStamps {
        trig_ex_ts_ns: tr.ex_ts_ns,
        trig_ex_send_ns: tr.ex_send_ns,
        trig_hw_rx_ns: tr.hw_rx_ns,
        trig_sw_rx_ns: tr.sw_rx_ns,
        trig_rx_tsc: tr.rx_tsc.0,
        trig_proc_tsc: tr.proc_tsc.0,
        decided_tsc: 0,
        sent_tsc: 0,
        trigger_seq: tr.trigger_seq,
        trig_kind: tr.kind,
        op,
        _pad: [0; 5],
    }
}

/// `n / d` rounded half-to-even (`d > 0`); saturates to i64.
pub(crate) fn div_half_even(n: i128, d: i128) -> i64 {
    if d == 0 {
        return 0;
    }
    let q = n.div_euclid(d);
    let r = n.rem_euclid(d);
    let up = match (2 * r).cmp(&d) {
        core::cmp::Ordering::Less => false,
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => q & 1 != 0,
    };
    let v = if up { q + 1 } else { q };
    v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}
