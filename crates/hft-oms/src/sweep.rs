//! StaleOrderSweep (`Timer{Oms, Sweep}`) and the resumption timeout.

use hft_types::{Tif, Tsc};

use crate::Oms;
use crate::types::{
    LinkBroken, LinkState, OmsOut, OrderState, SlotFlags, Tomb, TransitionCause, link_flags as lf, link_note,
    out_kind,
};

impl Oms {
    /// `Timer{Oms, Sweep}`: at most `sweep_batch` non-free slots per table, cursor carried over.
    pub fn on_sweep(&mut self, now: Tsc, out: &mut OmsOut) {
        let seq = 0;
        for ti in 0..self.tables.len() {
            let Some(t) = self.t(ti) else { continue };
            let n = t.n;
            let mut cur = t.sweep_cursor;
            let mut seen = 0u32;
            let batch = u32::from(self.p.sweep_batch);
            let mut steps = 0u32;
            while seen < batch && steps < n {
                cur = if cur >= n { 1 } else { cur + 1 };
                steps += 1;
                self.expire_tombs(ti, cur, now);
                let st = self.t(ti).and_then(|t| t.slot(cur)).map(|x| x.state);
                if st.is_none_or(|s| s == OrderState::Free) {
                    if cur == n {
                        break;
                    }
                    continue;
                }
                seen += 1;
                self.sweep_slot(ti, cur, now, seq, out);
                if cur == n {
                    break;
                }
            }
            if let Some(t) = self.tm(ti) {
                t.sweep_cursor = if cur >= n { 0 } else { cur };
            }
        }
        self.resume_timeouts(now, seq, out);
        self.flush(out);
    }

    fn expire_tombs(&mut self, ti: usize, s: u32, now: Tsc) {
        let tg = usize::from(self.p.tomb_gens);
        let keep = self.p.tomb_keep;
        let Some(t) = self.tm(ti) else { return };
        for tb in t.tombs.iter_mut().skip(s as usize * tg).take(tg) {
            if tb.r#gen != 0 && now.0.saturating_sub(tb.freed_tsc.0) > keep {
                *tb = Tomb::EMPTY;
            }
        }
    }

    fn sweep_slot(&mut self, ti: usize, s: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let (Some(sl), Some(ex)) = (t.slot(s).copied(), t.ext.get(s as usize).copied()) else {
            return;
        };
        let age = now.0.saturating_sub(sl.t_state.0);
        let ioc = matches!(sl.tif, Tif::Ioc | Tif::Fok);
        match sl.state {
            OrderState::PendingNew if age > t.ack_to => {
                self.transition(
                    ti,
                    s,
                    OrderState::Unknown,
                    TransitionCause::Timeout,
                    now,
                    seq,
                    out,
                );
            }
            OrderState::PendingCancel | OrderState::PendingAmend if age > t.cancel_to => {
                self.transition(
                    ti,
                    s,
                    OrderState::Unknown,
                    TransitionCause::Timeout,
                    now,
                    seq,
                    out,
                );
            }
            OrderState::Live if ioc && age > t.ioc_to => {
                self.transition(
                    ti,
                    s,
                    OrderState::Unknown,
                    TransitionCause::Timeout,
                    now,
                    seq,
                    out,
                );
            }
            OrderState::Live => {
                if !ioc
                    && ex.max_rest_ms != 0
                    && age > self.ms(ex.max_rest_ms)
                    && !sl.sflags.has(SlotFlags::AGED)
                {
                    if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                        x.sflags.0 |= SlotFlags::AGED;
                    }
                    self.push_slot_item(out_kind::AGED, sl.handle(), seq, out);
                }
                if sl.sflags.has(SlotFlags::SMOKE_DUE) && !sl.sflags.has(SlotFlags::CANCEL_SENT) {
                    self.cancel_slot(ti, s, TransitionCause::SmokeTtl, None, now, seq, out);
                }
            }
            OrderState::Unknown => {
                if age > self.p.unknown_alarm && !sl.sflags.has(SlotFlags::ALARMED) {
                    if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                        x.sflags.0 |= SlotFlags::ALARMED;
                    }
                    self.push_slot_item(out_kind::ALARM, sl.handle(), seq, out);
                }
                if sl.link_id != 0 {
                    self.check_deadline(sl.link_id, now, seq, out);
                }
            }
            st if st.terminal() && age > self.p.reuse_quarantine && !sl.sflags.has(SlotFlags::QUEUED) => {
                self.recycle(ti, s, now);
            }
            _ => {}
        }
    }

    /// `now ≥ resume_hold_until`: links still in `RESUME_HOLD` time out (README "恢复" 5).
    fn resume_timeouts(&mut self, now: Tsc, seq: u64, out: &mut OmsOut) {
        if self.resume_hold_until.0 == 0 || now.0 < self.resume_hold_until.0 {
            return;
        }
        for i in 0..self.links.len() {
            let Some(e) = self.links.get(i).copied() else {
                continue;
            };
            if !e.active() || !e.has(lf::RESUME_HOLD) {
                continue;
            }
            let (v, a) = self.resume_venue(i);
            let gate = self.table(v, a).is_some_and(|t| t.hedge_gate);
            let reason = if gate && self.link_aux(e.link_id).unknown > 0 {
                2
            } else {
                1
            };
            if let Some(x) = self.links.get_mut(i) {
                x.flags &= !lf::RESUME_HOLD;
            }
            if e.first_net.0 != 0 {
                self.break_with(
                    i,
                    LinkBroken::ResumeTimeout,
                    link_note::RESUME_TIMEOUT_P0,
                    reason,
                    seq,
                    out,
                );
            } else {
                self.push_link(e.link_id, link_note::RESUME_TIMEOUT_P1, reason, seq, out);
            }
        }
        self.resume_hold_until = Tsc(0);
    }

    /// Broken with a specific riding Note on the `Link` item.
    pub(crate) fn break_with(
        &mut self,
        i: usize,
        reason: LinkBroken,
        note: u8,
        from: u8,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if !e.active() {
            return;
        }
        if let Some(x) = self.links.get_mut(i) {
            x.state = LinkState::Broken;
            x.plan = (x.plan & 0x00FF_FFFF) | u32::from(reason as u8) << 24;
        }
        if let Some(gi) = self.group_ix(e.account, e.group)
            && let Some(g) = self.groups.get_mut(gi)
        {
            g.st.view.open_links = g.st.view.open_links.saturating_sub(1);
            g.st.broken_links = g.st.broken_links.saturating_add(1);
        }
        self.push_link(e.link_id, note, from, seq, out);
    }
}
