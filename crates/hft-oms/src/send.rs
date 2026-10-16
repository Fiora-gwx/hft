//! `SendReport` handling, local send-failure classes, connection state and `REPORT_DECODE`.

use hft_event::{ConnRole, ConnState, ConnStateKind, GwOp, RecordHeader, SendReport, SendStatus};
use hft_types::{GROUP_NONE, Tsc};

use crate::Oms;
use crate::notes;
use crate::types::{OmsOut, OrderState, SlotFlags, SlotStamps, TransitionCause};

/// `ConnState.reason` "report frame decode failed" (code table owned by hft-venue).
pub const REASON_REPORT_DECODE: u32 = 12;

impl Oms {
    /// One `SendReport` (README "SendReport 处理").
    pub fn on_send_report(&mut self, ev: &SendReport, hdr: &RecordHeader, out: &mut OmsOut) {
        let now = Tsc(hdr.tsc);
        let seq = hdr.seq;
        let Some(ti) = self.tix(ev.venue, ev.account) else {
            return;
        };
        let written = ev.status == SendStatus::Written;
        let wr_err = ev.status == SendStatus::WriteError;
        match ev.op {
            GwOp::Place => {
                let s = ev.slot.0;
                let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
                    return;
                };
                if sl.r#gen != ev.r#gen || sl.state == OrderState::Free {
                    return;
                }
                if written {
                    if let Some(st) = self.tm(ti).and_then(|t| t.stamps.get_mut(s as usize))
                        && st.op == SlotStamps::OP_NEW
                    {
                        st.sent_tsc = ev.sent_tsc.0;
                    }
                } else if sl.state == OrderState::PendingNew {
                    if wr_err {
                        self.transition(
                            ti,
                            s,
                            OrderState::Unknown,
                            TransitionCause::WriteError,
                            now,
                            seq,
                            out,
                        );
                    } else {
                        if let Some(t) = self.tm(ti) {
                            if let Some(x) = t.slots.get_mut(s as usize) {
                                x.reject_code = u32::from(ev.status as u8);
                            }
                            if let Some(st) = t.stamps.get_mut(s as usize) {
                                st.op = SlotStamps::OP_NONE;
                            }
                        }
                        self.local_fail = Some(ev.status);
                        self.transition(
                            ti,
                            s,
                            OrderState::Rejected,
                            TransitionCause::LocalSendFail,
                            now,
                            seq,
                            out,
                        );
                        self.local_fail = None;
                    }
                }
            }
            GwOp::Cancel | GwOp::Amend => {
                let s = ev.target_slot.0;
                let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
                    return;
                };
                if sl.r#gen != ev.target_gen || sl.state == OrderState::Free {
                    return;
                }
                let op = if ev.op == GwOp::Cancel {
                    SlotStamps::OP_CANCEL
                } else {
                    SlotStamps::OP_AMEND
                };
                if written {
                    if let Some(st) = self.tm(ti).and_then(|t| t.stamps.get_mut(s as usize))
                        && st.op == op
                    {
                        st.sent_tsc = ev.sent_tsc.0;
                    }
                } else if wr_err {
                    self.set_verify(ti, s, now, seq, out);
                } else {
                    let flag = if ev.op == GwOp::Cancel {
                        SlotFlags::CANCEL_SENT
                    } else {
                        SlotFlags::AMEND_SENT
                    };
                    if let Some(t) = self.tm(ti) {
                        if let Some(x) = t.slots.get_mut(s as usize) {
                            x.sflags.0 &= !flag;
                        }
                        if let Some(st) = t.stamps.get_mut(s as usize)
                            && st.op == op
                        {
                            st.op = SlotStamps::OP_NONE;
                        }
                    }
                    self.cancel_not_sent(ti, s, ev, out);
                }
            }
            GwOp::Query => {
                let s = ev.target_slot.0;
                let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
                    return;
                };
                if sl.r#gen != ev.target_gen || sl.state == OrderState::Free {
                    return;
                }
                if written {
                    self.query_written(ti, s, out);
                } else if ev.status == SendStatus::QueryUnsupported {
                    if let Some(slot) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                        slot.sflags.0 |= SlotFlags::REST_QUERY;
                    }
                    self.needs_rest(&sl, false, out);
                }
            }
            GwOp::CancelAll => {
                if written {
                    return;
                }
                let n = self.t(ti).map_or(0, |t| t.n);
                for s in 1..=n {
                    let hit = self.t(ti).and_then(|t| t.slot(s)).is_some_and(|x| {
                        x.state == OrderState::PendingCancel && x.sflags.has(SlotFlags::CANCEL_SENT)
                    });
                    if !hit {
                        continue;
                    }
                    if wr_err {
                        self.set_verify(ti, s, now, seq, out);
                    } else {
                        self.clear_flag(ti, s, SlotFlags::CANCEL_SENT);
                    }
                }
                if !wr_err {
                    self.cancel_not_sent(ti, 0, ev, out);
                }
            }
            _ => {}
        }
        self.flush(out);
    }

    fn cancel_not_sent(&mut self, ti: usize, s: u32, ev: &SendReport, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let (strategy, group, inst, r#gen) = match t.slot(s) {
            Some(x) if s != 0 => (x.strategy, x.group, x.inst, x.r#gen),
            _ => (self.strategy_of(t.account), GROUP_NONE, hft_types::InstId(0), 0),
        };
        let v0 = u64::from(t.venue.0) | u64::from(t.account.0) << 8;
        let n = notes::note(
            notes::CANCEL_NOT_SENT,
            strategy,
            group,
            inst,
            [
                v0,
                u64::from(s),
                u64::from(r#gen),
                u64::from(ev.op as u8),
                u64::from(ev.status as u8),
                0,
                0,
                0,
            ],
        );
        self.note(n, out);
    }

    /// `ConnState` of an order or user-stream connection (README "Unknown 收敛流程" 1, 3; "REPORT_DECODE").
    pub fn on_conn_state(&mut self, ev: &ConnState, hdr: &RecordHeader, out: &mut OmsOut) {
        let now = Tsc(hdr.tsc);
        let seq = hdr.seq;
        if ev.account.0 == 0 {
            return;
        }
        let Some(ti) = self.tix(ev.venue, ev.account) else {
            return;
        };
        let n = self.t(ti).map_or(0, |t| t.n);
        match (ev.role, ev.state) {
            (ConnRole::Order, ConnStateKind::Down) => {
                if let Some(t) = self.tm(ti) {
                    t.conn_up = false;
                }
                self.order_gap(ti, n, TransitionCause::ConnDown, now, seq, out);
            }
            (ConnRole::Order, ConnStateKind::Up) => {
                let was = self.t(ti).is_some_and(|t| t.ready());
                if let Some(t) = self.tm(ti) {
                    t.conn_up = true;
                }
                for s in 1..=n {
                    let q = self.t(ti).and_then(|t| t.slot(s)).is_some_and(|x| {
                        x.state == OrderState::Unknown || (x.state.open() && x.sflags.has(SlotFlags::VERIFY))
                    });
                    if q {
                        self.query_or_reconcile(ti, s, true, seq, out);
                    }
                }
                self.check_ready(ti, was, out);
            }
            (ConnRole::UserStream, ConnStateKind::Down) => {
                for s in 1..=n {
                    if self
                        .t(ti)
                        .and_then(|t| t.slot(s))
                        .is_some_and(|x| x.state.open() && x.state != OrderState::Unknown)
                    {
                        self.set_verify(ti, s, now, seq, out);
                    }
                }
            }
            (ConnRole::UserStream, ConnStateKind::Up) => {
                if let Some(t) = self.tm(ti) {
                    t.decode_fails = 0;
                }
            }
            (_, ConnStateKind::Degraded) if ev.reason == REASON_REPORT_DECODE => {
                let lim = self.p.decode_fail_down_n;
                let hit = self.tm(ti).is_some_and(|t| {
                    t.decode_fails = t.decode_fails.saturating_add(1);
                    if t.decode_fails >= lim {
                        t.decode_fails = 0;
                        true
                    } else {
                        false
                    }
                });
                if hit {
                    self.order_gap(ti, n, TransitionCause::ReportDecode, now, seq, out);
                } else {
                    for s in 1..=n {
                        if self
                            .t(ti)
                            .and_then(|t| t.slot(s))
                            .is_some_and(|x| x.state.open() && x.state != OrderState::Unknown)
                        {
                            self.set_verify(ti, s, now, seq, out);
                        }
                    }
                }
            }
            _ => {}
        }
        self.flush(out);
    }

    /// Order-connection gap: Pending* → Unknown, Live → VERIFY.
    fn order_gap(&mut self, ti: usize, n: u32, cause: TransitionCause, now: Tsc, seq: u64, out: &mut OmsOut) {
        for s in 1..=n {
            let st = self.t(ti).and_then(|t| t.slot(s)).map(|x| x.state);
            match st {
                Some(OrderState::PendingNew | OrderState::PendingCancel | OrderState::PendingAmend) => {
                    self.transition(ti, s, OrderState::Unknown, cause, now, seq, out);
                }
                Some(OrderState::Live) => self.set_verify(ti, s, now, seq, out),
                _ => {}
            }
        }
    }
}
