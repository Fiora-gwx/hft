//! Recovery: `recovery_end` (Unknown, TSC shift, `RESUME_HOLD`), `on_hedge_gate_open`,
//! `mirror_restore`.

use hft_types::{AccountId, Tsc, VenueId};

use crate::Oms;
use crate::types::{
    FixedVec, MirrorRestore, OmsOut, OmsTimerKind, OmsTimerReq, OrderState, SlotFlags, TransitionCause,
    link_flags as lf,
};

/// Shift of a registered TSC field (README "恢复" 2; formula owned by hft-exec).
pub fn shift_tsc(t: Tsc, now: Tsc, stop: Tsc, hz_prev: u64, hz_cur: u64) -> Tsc {
    if t.0 == 0 {
        return Tsc(0);
    }
    let conv = |d: u64| -> u64 {
        let v = i128::from(d) * i128::from(hz_cur) / i128::from(hz_prev.max(1));
        v.clamp(0, i128::from(u64::MAX)) as u64
    };
    if t.0 <= stop.0 {
        Tsc(now.0.saturating_sub(conv(stop.0 - t.0)).max(1))
    } else {
        Tsc(now.0.saturating_add(conv(t.0 - stop.0)))
    }
}

impl Oms {
    /// End of cold-start recovery (README "恢复" 2).
    pub fn recovery_end(
        &mut self,
        now: Tsc,
        stop_tsc: Tsc,
        hz_prev: u64,
        hz_cur: u64,
        cause_seq: u64,
        out: &mut OmsOut,
    ) {
        let dl = u64::from(self.p.hedge_deadline_ms);
        // Held engine run time of each Open/Hedging link (before the shift).
        for e in self.links.iter_mut() {
            if !e.active() || e.flags & lf::RESUME_HOLD != 0 {
                continue;
            }
            let held = if e.first_fill_tsc.0 == 0 {
                0
            } else {
                let d = stop_tsc.0.saturating_sub(e.first_fill_tsc.0);
                let ms = i128::from(d) * 1_000_000 / i128::from(hz_prev.max(1));
                (ms.clamp(0, i128::from(dl)) as u64).min(dl)
            };
            e.held_ms = held as u32;
            e.flags |= lf::RESUME_HOLD;
        }
        let sh = |t: Tsc| shift_tsc(t, now, stop_tsc, hz_prev, hz_cur);
        for e in self.links.iter_mut() {
            if e.active() || e.state == crate::types::LinkState::Broken {
                e.first_fill_tsc = sh(e.first_fill_tsc);
            }
        }
        for g in self.groups.iter_mut() {
            g.st.last_fill_tsc = sh(g.st.last_fill_tsc);
        }
        for t in self.tables.iter_mut().flatten() {
            t.conn_up = false;
            t.hedge_gate = false;
            t.decode_fails = 0;
            for sl in t.slots.iter_mut().skip(1) {
                if sl.state != OrderState::Free {
                    sl.t_state = sh(sl.t_state);
                }
            }
            for tb in t.tombs.iter_mut() {
                if tb.r#gen != 0 {
                    tb.freed_tsc = sh(tb.freed_tsc);
                }
            }
        }
        for ti in 0..self.tables.len() {
            let Some(t) = self.t(ti) else { continue };
            let (n, smoke, tix) = (t.n, t.smoke_slot, t.tix);
            for s in 1..=n {
                let st = self.t(ti).and_then(|t| t.slot(s)).map(|x| x.state);
                let Some(st) = st else { continue };
                if !st.open() {
                    continue;
                }
                if s == smoke
                    && let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize))
                {
                    x.sflags.0 |= SlotFlags::SMOKE_DUE;
                }
                if st == OrderState::Unknown {
                    let id = OmsTimerReq::unknown_query_id(tix, s);
                    let d = Tsc(now.0.saturating_add(self.p.query_interval));
                    self.push_timer(OmsTimerKind::UnknownQuery, id, d, false, out);
                } else {
                    self.transition(
                        ti,
                        s,
                        OrderState::Unknown,
                        TransitionCause::RecoveryEnd,
                        now,
                        cause_seq,
                        out,
                    );
                }
            }
        }
        self.resume_hold_until = Tsc(now.0.saturating_add(self.p.resume_hold_max));
        self.flush(out);
    }

    /// hft-risk reports the hedge-gate subset of (a, v) satisfied: resume the links waiting on it.
    pub fn on_hedge_gate_open(
        &mut self,
        a: AccountId,
        v: VenueId,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) {
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            t.hedge_gate = true;
        }
        for i in 0..self.links.len() {
            let Some(e) = self.links.get(i).copied() else {
                continue;
            };
            if !e.active() || !e.has(lf::RESUME_HOLD) || self.resume_venue(i) != (v, a) {
                continue;
            }
            if self.link_aux(e.link_id).unknown > 0 {
                continue;
            }
            self.resume_link(i, now, cause_seq, true, out);
        }
        self.flush(out);
    }

    /// One gw mirror entry per non-terminal slot of (v, a), from `cursor`, in batches.
    pub fn mirror_restore(
        &self,
        v: VenueId,
        a: AccountId,
        out: &mut FixedVec<MirrorRestore, 256>,
        cursor: &mut u32,
    ) -> bool {
        let Some(t) = self.table(v, a) else { return false };
        let mut s = (*cursor).max(1);
        while s <= t.n {
            if let Some(sl) = t.slot(s)
                && sl.state.open()
            {
                let m = MirrorRestore {
                    slot: s,
                    r#gen: sl.r#gen,
                    epoch: sl.epoch,
                    inst: sl.inst,
                    exch_order_id: sl.exch_order_id,
                };
                if out.push(m).is_err() {
                    *cursor = s;
                    return true;
                }
            }
            s += 1;
        }
        *cursor = s;
        false
    }
}
