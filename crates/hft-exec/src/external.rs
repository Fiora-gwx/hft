//! External fills (README "外来成交的默认处理"), `manual_hold`, the per-venue recovery gate and the
//! gate-held-naked alarm (README "人工保持与恢复期抑制").

use hft_event::NoteLevel;
use hft_strategy_sdk::{ExternalCause, ExternalFillView, Now, StuckReason, TargetOrigin, TargetState, Views};
use hft_types::{AccountId, Qty, Tsc, VenueId};

use crate::calc;
use crate::decision::{acct_group, action, external, hold_release, note, pack, table};
use crate::types::*;
use crate::{Exec, ExecOut};

impl Exec {
    /// Releases `manual_hold` (reason: `hold_release::*`); writes the Info note, re-arms the
    /// auto-fix window. `MANUAL_HOLD_DEFERRED` is cleared too unless the reason is an engine target.
    pub(crate) fn release_hold(&mut self, cix: usize, reason: u64, now: Now, out: &mut ExecOut) {
        let p = self.params(self.acct_of_cix(cix));
        let Some(c) = self.cells.get_mut(cix) else { return };
        if reason != hold_release::ENGINE_TARGET {
            c.hold &= !MANUAL_HOLD_DEFERRED;
        }
        if c.hold & MANUAL_HOLD == 0 {
            return;
        }
        c.hold &= !MANUAL_HOLD;
        c.fix.first_tsc = Tsc(0);
        if c.auto_fix.phase() == AutoFixPhase::WaitHold {
            c.auto_fix.phase = AutoFixPhase::Primed as u8;
            c.auto_fix.due_tsc = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.broken_auto_fix_ms)));
            #[cfg(feature = "testkit")]
            self.audit.arm(c, now, p.broken_auto_fix_ms);
            c.deferred |= D_TIMERS;
        }
        let ag = acct_group(c.account.0, c.group.0);
        let n = self.note(
            cix,
            note::EXEC_MANUAL_HOLD,
            NoteLevel::Info,
            [ag, 0, reason, 0, 0, 0, 0, 0],
        );
        out.notes.push(n);
        self.mark_pending(cix, true);
    }

    /// Sets `manual_hold` for a Manual external fill; `true` when newly set (the P1 note written).
    fn set_hold<V: Views>(
        &mut self,
        cix: usize,
        f: &ExternalFillView,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) -> bool {
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        let Some(c) = self.cells.get(cix).copied() else {
            return false;
        };
        let engine_target = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
        if engine_target || st.force_active || st.flatten_lock {
            if let Some(c) = self.cells.get_mut(cix) {
                c.hold |= MANUAL_HOLD_DEFERRED;
            }
            return false;
        }
        if c.hold & MANUAL_HOLD != 0 {
            return false;
        }
        if c.fix.first_tsc.0 != 0 || c.fix.count != 0 || c.fix.next_tsc.0 != 0 {
            if let Some(c) = self.cells.get_mut(cix) {
                c.fix = FixState::ZERO;
            }
            self.cancel_timer(cix, ExecTimerKind::Fix, out);
        }
        self.clear_ext(cix, now, out);
        let Some(c) = self.cells.get_mut(cix) else {
            return false;
        };
        c.hold |= MANUAL_HOLD;
        c.aux16 &= !AUX_EXT_DEFAULT;
        hold_pauses_autofix(c, now);
        if c.has_target
            && c.view.origin == TargetOrigin::Strategy
            && !matches!(c.view.state, TargetState::Broken(_))
        {
            let s = TargetState::Stuck(StuckReason::External);
            if c.view.state != s {
                if c.deferred & D_STATE == 0 {
                    c.def_from = c.view.state.code();
                }
                c.deferred |= D_STATE;
                c.view.state = s;
            }
        }
        let ag = acct_group(c.account.0, c.group.0);
        let naked = self.naked(cix, v).0;
        let n = self.note(
            cix,
            note::EXEC_MANUAL_HOLD,
            NoteLevel::P1,
            [
                ag,
                1,
                u64::from(f.leg_ix),
                u64::from(f.inst.0),
                f.side as u64,
                f.qty.0 as u64,
                naked.unsigned_abs(),
                0,
            ],
        );
        out.notes.push(n);
        true
    }

    /// `(Σ net, excess leg)` of a grouped cell.
    pub(crate) fn naked<V: Views>(&self, cix: usize, v: &V) -> (i64, usize) {
        let Some(c) = self.cells.get(cix) else {
            return (0, 0);
        };
        if c.is_ungrouped() {
            return (0, 0);
        }
        if c.n_legs < 2 {
            return (0, 0);
        }
        let lv = v.legs(c.account, c.group);
        let n = usize::from(c.n_legs);
        let sum: i64 = lv.leg.iter().take(n).map(|l| l.net.0).sum();
        let mut best: Option<usize> = None;
        for i in 0..n {
            let x = lv.leg[i].net.0;
            if x == 0 || x.signum() != sum.signum() {
                continue;
            }
            best = match best {
                None => Some(i),
                Some(b) => {
                    let (xb, xi) = (lv.leg[b].net.0.unsigned_abs(), x.unsigned_abs());
                    if xi > xb || (xi == xb && c.legs[i].reduce_only_cap > c.legs[b].reduce_only_cap) {
                        Some(i)
                    } else {
                        Some(b)
                    }
                }
            };
        }
        (sum, best.unwrap_or(0))
    }

    /// One external fill booked by hft-ledger (README "外来成交的默认处理"); returns the default
    /// handling deadline (0 = no engine default).
    pub fn on_external<V: Views>(
        &mut self,
        a: AccountId,
        f: &ExternalFillView,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) -> Tsc {
        if f.group.0 == 0 {
            if let Some(k) = self.acct_ix(a)
                && self.acct.get(k).is_some_and(|s| s.flatten_lock)
            {
                if let Some(s) = self.acct.get_mut(k)
                    && s.force_kind == FORCE_NONE
                {
                    s.force_kind = FORCE_FLATTEN;
                    s.force_flags &= !FORCE_FULL;
                }
                let u = self.n_grouped + k;
                self.mark_pending(u, true);
            }
            return Tsc(0);
        }
        let Some(cix) = self.cix_of(a, f.group) else {
            return Tsc(0);
        };
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        let manual = f.cause == ExternalCause::Manual;
        let newly = if manual {
            self.set_hold(cix, f, v, now, out)
        } else {
            false
        };
        let Some(c) = self.cells.get(cix).copied() else {
            return Tsc(0);
        };
        let p = self.params(k);
        let engine = (c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_)))
            || st.force_active
            || st.flatten_lock;
        let (arg, ret) = if matches!(c.view.state, TargetState::Broken(_)) {
            (external::BROKEN_KEPT, Tsc(0))
        } else if engine {
            (external::ENGINE_OWNED, Tsc(0))
        } else if manual {
            (external::MANUAL_HOLD, Tsc(0))
        } else {
            let dl = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.external_deadline_ms)));
            (external::TIMED, dl)
        };
        let mut d = self.dec(cix, action::EXTERNAL_SEEN, now.seq);
        d.detail = pack(table::EXTERNAL, f.cause as u8, arg);
        d.leg_ix = f.leg_ix;
        d.qty = f.qty;
        d.px = f.px;
        out.decisions.push(d);
        if arg == external::TIMED {
            let saved = c.ext_deadline;
            self.reg_timer(cix, ExecTimerKind::External, ret, saved, out);
            if let Some(c) = self.cells.get_mut(cix) {
                c.ext_deadline = ret;
                // A new timing supersedes a default still pending from the previous deadline
                // (`ExternalDefault` ends a timing for the cold reader).
                c.aux16 &= !AUX_EXT_DEFAULT;
            }
            if c.has_target && c.view.state != TargetState::Stuck(StuckReason::External) {
                self.set_state(cix, TargetState::Stuck(StuckReason::External), now.seq, out);
            }
        }
        if manual {
            let level = if newly { NoteLevel::Info } else { NoteLevel::P1 };
            let n = self.note(
                cix,
                note::EXEC_MANUAL_EXTERNAL,
                level,
                [
                    u64::from(a.0),
                    u64::from(f.leg_ix),
                    u64::from(f.inst.0),
                    f.side as u64,
                    f.qty.0 as u64,
                    f.px.0 as u64,
                    0,
                    0,
                ],
            );
            out.notes.push(n);
        }
        self.mark_pending(cix, true);
        ret
    }

    /// Deadline of an unhandled external fill reached (README "外来成交的默认处理" 3).
    pub(crate) fn ext_deadline_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if c.ext_deadline.0 == 0 || now.tsc.0 < c.ext_deadline.0 {
            return;
        }
        let k = self.acct_of_cix(cix);
        let (sum, e) = self.naked(cix, v);
        let excess_venue = c.legs[e].venue;
        let open = self
            .acct
            .get(k)
            .is_some_and(|s| s.venue_open & (1u64 << (excess_venue.0 & 63)) != 0);
        if sum != 0 && !open {
            return;
        }
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.ext_deadline = Tsc(0);
            cm.aux16 &= !(1 << ExecTimerKind::External as u16);
        }
        if c.has_target && c.view.origin == TargetOrigin::Strategy {
            let mut d = self.dec(cix, action::TARGET_CLEARED, now.seq);
            d.eflags |= crate::decision::eflags::CLEARED_BY_ENGINE;
            d.state_to = TargetState::Idle.code();
            out.decisions.push(d);
            self.notify(
                cix,
                hft_strategy_sdk::ExecUpdateKind::Done {
                    cleared_by_engine: true,
                },
                None,
                out,
            );
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.has_target = false;
                cm.view.state = TargetState::Idle;
                cm.pair_lot = Qty(0);
            }
        }
        if self.fix_trigger(cix, v) {
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.aux16 |= AUX_EXT_DEFAULT;
                if cm.fix.first_tsc.0 == 0 {
                    cm.fix.first_tsc = now.tsc;
                }
            }
        } else {
            let mut d = self.dec(cix, action::EXTERNAL_DEFAULT, now.seq);
            d.leg_ix = e as u8;
            out.decisions.push(d);
            let ag = acct_group(c.account.0, c.group.0);
            let n = self.note(
                cix,
                note::EXEC_EXTERNAL_DEFAULT,
                NoteLevel::P1,
                [ag, e as u64, 0, sum.unsigned_abs(), 0, 0, 0, 0],
            );
            out.notes.push(n);
        }
    }

    /// `MANUAL_HOLD_DEFERRED` becomes `MANUAL_HOLD` when the engine target is cleared.
    pub(crate) fn convert_deferred_hold<V: Views>(
        &mut self,
        cix: usize,
        v: &V,
        _now: Now,
        out: &mut ExecOut,
    ) {
        let Some(c) = self.cells.get_mut(cix) else { return };
        if c.hold & MANUAL_HOLD_DEFERRED == 0 {
            return;
        }
        c.hold &= !MANUAL_HOLD_DEFERRED;
        c.hold |= MANUAL_HOLD;
        c.fix = FixState::ZERO;
        hold_pauses_autofix(c, _now);
        let ag = acct_group(c.account.0, c.group.0);
        let naked = self.naked(cix, v).0;
        let n = self.note(
            cix,
            note::EXEC_MANUAL_HOLD,
            NoteLevel::P1,
            [ag, 1, 0, 0, 0, 0, naked.unsigned_abs(), 0],
        );
        out.notes.push(n);
    }

    // ------------------------------------------------------------------ recovery gate

    /// The recovery release condition of (a, v) holds for the first time this run.
    pub fn on_venue_gate_open(&mut self, a: AccountId, venue: VenueId, now: Now) {
        let Some(k) = self.acct_ix(a) else { return };
        let bit = 1u64 << (venue.0 & 63);
        let p = self.params(k);
        let Some(st) = self.acct.get_mut(k) else { return };
        if st.venue_open & bit != 0 {
            return;
        }
        st.venue_open |= bit;
        for cix in 0..self.cell_count() {
            let Some(c) = self.cells.get_mut(cix) else {
                continue;
            };
            if c.account != a
                || !c
                    .legs
                    .iter()
                    .take(usize::from(c.n_legs))
                    .any(|l| l.venue == venue)
            {
                continue;
            }
            c.fix.first_tsc = Tsc(0);
            c.fix.count = 0;
            if c.ext_deadline.0 != 0 {
                c.ext_deadline = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.external_deadline_ms)));
            }
            if c.auto_fix.phase() == AutoFixPhase::Primed {
                c.auto_fix.due_tsc = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.broken_auto_fix_ms)));
                #[cfg(feature = "testkit")]
                self.audit.arm(c, now, p.broken_auto_fix_ms);
            }
            c.deferred |= D_TIMERS;
            self.mark_pending(cix, true);
        }
    }

    /// The account's start gate released (cold start and recovery).
    pub fn on_gate_open(&mut self, a: AccountId, now: Now) {
        let Some(k) = self.acct_ix(a) else { return };
        let mut venues = self.accts.get(k).map_or(0, |x| x.venues);
        for c in self
            .cells
            .iter()
            .take(self.cell_count())
            .filter(|c| c.account == a)
        {
            for l in c.legs.iter().take(usize::from(c.n_legs)) {
                venues |= 1u64 << (l.venue.0 & 63);
            }
        }
        if let Some(st) = self.acct.get_mut(k) {
            st.gate_open = true;
        }
        for vb in 0..64u8 {
            if venues & (1u64 << vb) != 0 {
                self.on_venue_gate_open(a, VenueId(vb), now);
            }
        }
        self.mark_account(k, true);
    }

    pub(crate) fn venue_open(&self, cix: usize, venue: VenueId) -> bool {
        let k = self.acct_of_cix(cix);
        self.acct
            .get(k)
            .is_some_and(|s| s.venue_open & (1u64 << (venue.0 & 63)) != 0)
    }

    // ------------------------------------------------------------------ gate-held-naked alarm

    /// `(case, alarm time, start)` the alarm should be registered for; case 1 suppresses case 2.
    pub(crate) fn alarm_plan(&self, cix: usize) -> Option<(u8, Tsc, Tsc)> {
        let c = self.cells.get(cix)?;
        if c.is_ungrouped() || c.hold & MANUAL_HOLD != 0 {
            return None;
        }
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k)?;
        let p = self.params(k);
        let broken = matches!(c.view.state, TargetState::Broken(_));
        let alarm = u64::from(p.gate_naked_alarm_ms);
        let per_ms = self.tsc_per_ms.max(1);
        if !st.gate_open && (broken || c.fix.first_tsc.0 != 0) {
            let s0 = if broken { c.broken_since } else { c.fix.first_tsc };
            let start = s0.max(st.gate_closed_tsc);
            return Some((
                1,
                Tsc(start.0.saturating_add(alarm.saturating_mul(per_ms))),
                start,
            ));
        }
        let engine_target = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
        if broken && c.auto_fix.phase() == AutoFixPhase::Primed && !engine_target {
            let due = c.auto_fix.due_tsc;
            return Some((2, Tsc(due.0.saturating_add(alarm.saturating_mul(per_ms))), due));
        }
        None
    }

    pub(crate) fn alarm_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        self.tsc_per_ms = now.tsc_per_ms;
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let plan = self.alarm_plan(cix);
        if c.deferred & D_ALARM != 0 {
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.deferred &= !D_ALARM;
            }
            if let Some((case, at, start)) = plan
                && now.tsc.0 >= at.0
            {
                let reason = if case == 2 || matches!(c.view.state, TargetState::Broken(_)) {
                    self.wait_reason(cix, v, now)
                } else {
                    0
                };
                if case == 1 || reason != 0 {
                    let (sum, e) = self.naked(cix, v);
                    let l = c.legs[e];
                    let mark = self.mark(v, l.inst);
                    let nn = calc::notional_ceil(Qty(sum), mark, self.inst(l.inst));
                    let dur = now.tsc.0.saturating_sub(start.0) / now.tsc_per_ms.max(1);
                    let n = self.note(
                        cix,
                        note::EXEC_GATE_HELD_NAKED,
                        NoteLevel::P0,
                        [
                            acct_group(c.account.0, c.group.0),
                            u64::from(case),
                            reason,
                            dur,
                            e as u64,
                            sum.unsigned_abs(),
                            u64::try_from(nn.max(0)).unwrap_or(u64::MAX),
                            0,
                        ],
                    );
                    out.notes.push(n);
                }
            }
        }
        let mask = self.cells.get(cix).map_or(0, |c| c.timer_mask());
        let has = mask & (1 << (ExecTimerKind::Alarm as u8)) != 0;
        if let Some((_, at, _)) = plan
            && now.tsc.0 < at.0
        {
            let dirty = c.deferred & D_TIMERS != 0;
            if !has {
                self.reg_timer(cix, ExecTimerKind::Alarm, at, Tsc(0), out);
            } else if dirty {
                self.force_timer(cix, ExecTimerKind::Alarm, at, out);
            }
        }
    }
}

/// `aux16` bit: an external-fill default is due (sent as `ExternalDefault` by the repair path).
pub(crate) const AUX_EXT_DEFAULT: u16 = 1 << 14;

/// `manual_hold` set: a Primed/Running auto-fix waits (`WaitHold`; executable time kept).
fn hold_pauses_autofix(c: &mut TargetCell, now: Now) {
    match c.auto_fix.phase() {
        AutoFixPhase::Primed => c.auto_fix.phase = AutoFixPhase::WaitHold as u8,
        AutoFixPhase::Running => {
            let a = &mut c.auto_fix;
            if a.run_mark_tsc.0 != 0 {
                let d = now.tsc.0.saturating_sub(a.run_mark_tsc.0) / now.tsc_per_ms.max(1);
                a.exec_ms = a.exec_ms.saturating_add(u32::try_from(d).unwrap_or(u32::MAX));
                a.run_mark_tsc = Tsc(0);
            }
            a.phase = AutoFixPhase::WaitHold as u8;
        }
        _ => {}
    }
}
