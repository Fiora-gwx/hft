//! Unknown convergence: entering, querying, QueryResult/ReconcileReport convergence, NotFound
//! (trade-history confirmation or `ExpireAt` expiry), escalation and `force_unknown`.

use hft_event::{OrderAction, OrderEvent, ReconcileReport, RecordHeader};
use hft_refdata::StaleGuardKind;
use hft_types::{AccountId, Px, Qty, Tsc, VenueId};

use crate::Oms;
use crate::fsm::FillIn;
use crate::notes;
use crate::types::{OmsOut, OmsTimerKind, OmsTimerReq, OrderState, SlotFlags, TransitionCause, anomaly};

/// `ReconcileReport.item` codes.
const ITEM_OPEN_ORDER: u8 = 3;
const ITEM_UNKNOWN_RESOLVED: u8 = 4;
/// Order status codes (`OrderEvent.detail` of QueryResult, `ReconcileReport.order_status`).
pub(crate) const ST_NEW: u8 = 1;
pub(crate) const ST_PARTIAL: u8 = 2;
pub(crate) const ST_FILLED: u8 = 3;
pub(crate) const ST_CANCELLED: u8 = 4;
pub(crate) const ST_REJECTED: u8 = 5;
pub(crate) const ST_EXPIRED: u8 = 6;
pub(crate) const ST_NOT_FOUND: u8 = 7;

fn status_target(code: u8) -> Option<OrderState> {
    Some(match code {
        ST_NEW | ST_PARTIAL => OrderState::Live,
        ST_FILLED => OrderState::Filled,
        ST_CANCELLED | ST_EXPIRED => OrderState::Cancelled,
        ST_REJECTED => OrderState::Rejected,
        _ => return None,
    })
}

impl Oms {
    /// The slot just entered Unknown: register its query timer, query now if the connection is up.
    pub(crate) fn unknown_entered(&mut self, ti: usize, s: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        self.schedule_query(ti, s, now, seq, out);
    }

    /// Timer + immediate `Query` when the order connection is up.
    pub(crate) fn schedule_query(&mut self, ti: usize, s: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let up = t.conn_up;
        let id = OmsTimerReq::unknown_query_id(t.tix, s);
        let deadline = Tsc(now.0.saturating_add(self.p.query_interval));
        self.push_timer(OmsTimerKind::UnknownQuery, id, deadline, false, out);
        self.query_or_reconcile(ti, s, up, now, seq, out);
    }

    /// A non-linked slot of an `ExpireAt` venue: a NotFound after `not_found_grace` finalises it
    /// (README "Unknown 收敛流程" 4).
    pub(crate) fn expiry_bound(&self, ti: usize, s: u32) -> bool {
        self.t(ti).is_some_and(|t| {
            t.stale_guard == StaleGuardKind::ExpireAt && t.ext.get(s as usize).is_some_and(|x| x.role == 0)
        })
    }

    /// `now − sent_tsc > not_found_grace` (`t_state` when `sent_tsc` is 0).
    fn past_grace(&self, ti: usize, s: u32, now: Tsc) -> bool {
        let Some(t) = self.t(ti) else { return false };
        let (Some(sl), Some(st)) = (t.slot(s), t.stamps.get(s as usize)) else {
            return false;
        };
        let start = if st.sent_tsc != 0 {
            st.sent_tsc
        } else {
            sl.t_state.0
        };
        now.0.saturating_sub(start) > t.grace
    }

    pub(crate) fn query_or_reconcile(
        &mut self,
        ti: usize,
        s: u32,
        up: bool,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        if sl.sflags.has(SlotFlags::HIST_WAIT) {
            self.needs_rest(&sl, true, out);
        } else if sl.state == OrderState::Unknown && self.expiry_bound(ti, s) && self.past_grace(ti, s, now) {
            // Past the grace only a NotFound is missing to finalise it: query whatever the
            // query budget says (README "Unknown 收敛流程" 4).
            if up {
                self.push_decision(ti, OrderAction::Query, s, seq, out);
            }
        } else if sl.sflags.has(SlotFlags::REST_QUERY) || sl.query_count > self.p.unknown_max_queries {
            self.needs_rest(&sl, false, out);
        } else if up {
            self.push_decision(ti, OrderAction::Query, s, seq, out);
        }
    }

    /// Marks a non-terminal slot `VERIFY` and queries it.
    pub(crate) fn set_verify(&mut self, ti: usize, s: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        if let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            if !sl.state.open() {
                return;
            }
            sl.sflags.0 |= SlotFlags::VERIFY;
        }
        self.schedule_query(ti, s, now, seq, out);
    }

    /// `Timer{Oms, UnknownQuery}`: query again (or re-request the trade history) and re-register.
    pub fn on_unknown_query(&mut self, id: u32, now: Tsc, cause_seq: u64, out: &mut OmsOut) {
        let (tix, s) = OmsTimerReq::split_unknown_query_id(id);
        let ti = usize::from(tix);
        let Some(t) = self.t(ti) else { return };
        let Some(sl) = t.slot(s).copied() else { return };
        if !(sl.state == OrderState::Unknown || (sl.state.open() && sl.sflags.has(SlotFlags::VERIFY))) {
            return;
        }
        self.query_or_reconcile(ti, s, t.conn_up, now, cause_seq, out);
        let id = OmsTimerReq::unknown_query_id(tix, s);
        let deadline = Tsc(now.0.saturating_add(self.p.query_interval));
        self.push_timer(OmsTimerKind::UnknownQuery, id, deadline, false, out);
        if sl.link_id != 0 {
            self.check_deadline(sl.link_id, now, cause_seq, out);
        }
        self.flush(out);
    }

    /// `QueryResult` (README "Unknown 收敛流程" 4).
    pub(crate) fn query_result(
        &mut self,
        ti: usize,
        s: u32,
        ev: &OrderEvent,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        let f = FillIn {
            cum: ev.cum_filled,
            avg_px: ev.avg_px,
            px: ev.px,
            fee: ev.fee,
            fee_asset: ev.fee_asset,
            cash_qty: ev.cash_qty,
            cash_sum: ev.cash_sum,
            execution_cash: ev.fill_cash_qty.0 > 0,
            query: true,
            done: false,
            seq,
        };
        if sl.state.terminal() {
            self.fill_to_state(
                ti,
                s,
                &f,
                Some((sl.state, TransitionCause::QueryResult)),
                now,
                seq,
                out,
            );
            return;
        }
        if ev.detail == ST_NOT_FOUND {
            if sl.state != OrderState::Unknown {
                self.clear_flag(ti, s, SlotFlags::VERIFY);
                self.transition(
                    ti,
                    s,
                    OrderState::Unknown,
                    TransitionCause::QueryResult,
                    now,
                    seq,
                    out,
                );
            }
            self.not_found(ti, s, now, seq, out);
            return;
        }
        let Some(to) = status_target(ev.detail) else {
            return;
        };
        self.clear_flag(ti, s, SlotFlags::VERIFY);
        if to == OrderState::Rejected
            && let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize))
        {
            x.reject_code = ev.reject_code;
        }
        if to == OrderState::Live && (ev.px.0 != 0 || ev.qty.0 > 0) {
            self.set_px_qty(ti, s, ev.px, ev.qty);
        }
        self.fill_to_state(ti, s, &f, Some((to, TransitionCause::QueryResult)), now, seq, out);
    }

    fn set_px_qty(&mut self, ti: usize, s: u32, px: Px, qty: Qty) {
        let before = self.contrib(ti, s);
        if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            if px.0 != 0 {
                x.px = px;
            }
            if qty.0 > 0 {
                x.qty = Qty(qty.0.max(x.filled.0));
            }
        }
        let after = self.contrib(ti, s);
        self.apply(ti, &before, -1);
        self.apply(ti, &after, 1);
    }

    /// NotFound of an Unknown slot.
    fn not_found(&mut self, ti: usize, s: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        let past = self.past_grace(ti, s, now);
        if !self.expiry_bound(ti, s) {
            if past {
                if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                    x.sflags.0 |= SlotFlags::HIST_WAIT;
                }
                self.needs_rest(&sl, true, out);
            }
        } else if past {
            if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                x.reject_code = 0;
            }
            self.transition(
                ti,
                s,
                OrderState::Rejected,
                TransitionCause::NotFoundExpire,
                now,
                seq,
                out,
            );
        }
    }

    /// `ReconcileReport` (`OpenOrder` / `UnknownResolved` entries).
    pub fn on_reconcile(&mut self, ev: &ReconcileReport, hdr: &RecordHeader, out: &mut OmsOut) {
        if ev.item != ITEM_OPEN_ORDER && ev.item != ITEM_UNKNOWN_RESOLVED {
            return;
        }
        let now = Tsc(hdr.tsc);
        let seq = hdr.seq;
        let Some(ti) = self.tix(ev.venue, ev.account) else {
            return;
        };
        let s = ev.slot.0;
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        let matched = sl.state != OrderState::Free
            && sl.r#gen == ev.r#gen
            && sl.epoch == ev.epoch
            && (sl.state == OrderState::Unknown || (sl.state.open() && sl.sflags.has(SlotFlags::VERIFY)));
        if !matched {
            self.anomaly_note(&sl, anomaly::RECON_IGNORED, u64::from(ev.item), out);
            self.flush(out);
            return;
        }
        if ev.item == ITEM_OPEN_ORDER {
            self.clear_flag(ti, s, SlotFlags::VERIFY);
            if sl.state == OrderState::Unknown {
                self.transition(ti, s, OrderState::Live, TransitionCause::Reconcile, now, seq, out);
            }
            self.flush(out);
            return;
        }
        // This reconciliation format carries no cash facts. Do not advance a strict venue's
        // order from quantity alone; a complete execution/query report must supply the facts.
        if self.inst(sl.inst).perp_cash && ev.exch_qty.0 > sl.filled.0 {
            out.cash_error = Some(crate::CashError::Missing);
            return;
        }
        let ex = self.t(ti).and_then(|t| t.ext.get(s as usize)).copied();
        let f = FillIn {
            cum: ev.exch_qty,
            avg_px: Px(0),
            px: Px(0),
            fee: Qty(0),
            fee_asset: hft_types::AssetId(0),
            cash_qty: ex
                .filter(|_| ev.exch_qty == sl.filled)
                .map_or(Qty(0), |x| x.cash_qty),
            cash_sum: ex
                .filter(|_| ev.exch_qty == sl.filled)
                .map_or(Qty(0), |x| x.cash_sum),
            execution_cash: false,
            query: true,
            done: false,
            seq,
        };
        if ev.order_status == ST_NOT_FOUND {
            if ev.history_checked != 1 {
                self.flush(out);
                return;
            }
            self.clear_flag(ti, s, SlotFlags::VERIFY);
            let to = if ev.exch_qty.0 <= 0 {
                if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                    x.reject_code = 0;
                }
                OrderState::Rejected
            } else if ev.exch_qty.0 >= sl.qty.0 {
                OrderState::Filled
            } else {
                OrderState::Cancelled
            };
            self.fill_to_state(ti, s, &f, Some((to, TransitionCause::Reconcile)), now, seq, out);
        } else if let Some(to) = status_target(ev.order_status) {
            self.clear_flag(ti, s, SlotFlags::VERIFY);
            self.fill_to_state(ti, s, &f, Some((to, TransitionCause::Reconcile)), now, seq, out);
        }
        self.flush(out);
    }

    /// Recovery decision mismatch: every non-terminal slot of (v, a) → Unknown; returns the count.
    pub fn force_unknown(
        &mut self,
        v: VenueId,
        a: AccountId,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) -> u32 {
        let Some(ti) = self.tix(v, a) else { return 0 };
        let n = self.t(ti).map_or(0, |t| t.n);
        let mut k = 0;
        for s in 1..=n {
            let st = self.t(ti).and_then(|t| t.slot(s)).map(|x| x.state);
            if let Some(st) = st
                && st.open()
                && st != OrderState::Unknown
            {
                self.transition(
                    ti,
                    s,
                    OrderState::Unknown,
                    TransitionCause::ForceUnknown,
                    now,
                    cause_seq,
                    out,
                );
                k += 1;
            }
        }
        self.flush(out);
        k
    }

    /// `query_count += 1` on a written query; `OmsUnknownStuck` once per Unknown episode.
    pub(crate) fn query_written(&mut self, ti: usize, s: u32, out: &mut OmsOut) {
        let max = self.p.unknown_max_queries;
        let Some(sl) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) else {
            return;
        };
        if sl.sflags.has(SlotFlags::REST_QUERY) {
            return;
        }
        sl.query_count = sl.query_count.saturating_add(1);
        let stuck = (sl.state == OrderState::Unknown || sl.sflags.has(SlotFlags::VERIFY))
            && (sl.query_count > max || sl.query_count == u8::MAX);
        if stuck {
            sl.sflags.0 |= SlotFlags::REST_QUERY;
        }
        let sl = *sl;
        if stuck {
            let v0 = u64::from(sl.venue.0) | u64::from(sl.account.0) << 8;
            let n = notes::note(
                notes::UNKNOWN_STUCK,
                sl.strategy,
                sl.group,
                sl.inst,
                [
                    v0,
                    u64::from(sl.slot_id),
                    u64::from(sl.r#gen),
                    u64::from(sl.query_count),
                    0,
                    0,
                    0,
                    0,
                ],
            );
            self.note(n, out);
            self.needs_rest(&sl, false, out);
        }
    }
}
