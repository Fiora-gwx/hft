//! Broken disposal (README "Broken 处置"): entering Broken, the auto bounded close-out (timing,
//! wait conditions, executable time, slices, end table).

use hft_event::NoteLevel;
use hft_strategy_sdk::{
    BrokenReason, EngineOrigin, ExecUpdateKind, Now, PortfolioRead, TargetOrigin, TargetState, Views,
};
use hft_types::{Qty, RejectBits, Side, Tsc};

use crate::calc;
use crate::decision::{acct_group, action, autofix, note, pack, table, wait};
use crate::types::*;
use crate::{Exec, ExecOut};

impl Exec {
    /// Enters Broken (idempotent): record, notification, optional `BrokenReq`, P0 note, auto-fix
    /// timing.
    pub(crate) fn enter_broken(
        &mut self,
        cix: usize,
        reason: BrokenReason,
        arg: u16,
        req: bool,
        now: Now,
        out: &mut ExecOut,
    ) {
        let p = self.params(self.acct_of_cix(cix));
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if matches!(c.view.state, TargetState::Broken(_)) {
            return;
        }
        let mut d = self.dec(cix, action::STATE, now.seq);
        d.state_from = c.view.state.code();
        d.state_to = 4;
        d.detail = pack(table::BROKEN, reason.code(), arg);
        out.decisions.push(d);
        let Some(cm) = self.cells.get_mut(cix) else { return };
        cm.view.state = TargetState::Broken(reason);
        cm.fix = FixState::ZERO;
        cm.aux16 &= !crate::external::AUX_EXT_DEFAULT;
        Self::arm_broken(cm, &p, now);
        let due = cm.auto_fix.due_tsc;
        let ag = acct_group(c.account.0, c.group.0);
        self.notify(cix, ExecUpdateKind::Broken(reason), None, out);
        if req {
            out.broken.push(BrokenReq {
                account: c.account,
                group: c.group,
            });
        }
        let n = self.note(
            cix,
            note::EXEC_BROKEN,
            NoteLevel::P0,
            [ag, u64::from(reason.code()), u64::from(arg), 0, 0, 0, 0, 0],
        );
        out.notes.push(n);
        let saved = c.auto_fix.due_tsc;
        if self
            .cells
            .get(cix)
            .is_some_and(|c| c.auto_fix.phase() == AutoFixPhase::Primed)
        {
            self.reg_timer(cix, ExecTimerKind::AutoFix, due, saved, out);
        }
    }

    /// Starts the Broken timing of a cell (`broken_since`, auto-fix Primed or WaitHold).
    pub(crate) fn arm_broken(c: &mut TargetCell, p: &ExecParams, now: Now) {
        c.broken_since = now.tsc;
        c.auto_fix = <AutoFixState as zerocopy::FromZeros>::new_zeroed();
        c.auto_fix.phase = if c.hold & MANUAL_HOLD != 0 {
            AutoFixPhase::WaitHold as u8
        } else {
            AutoFixPhase::Primed as u8
        };
        c.auto_fix.due_tsc = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.broken_auto_fix_ms)));
    }

    /// First wait condition holding for the auto-fix (codes `wait::*`; 0 = none). Engine targets are
    /// not a code (checked separately).
    pub(crate) fn wait_reason<V: Views>(&self, cix: usize, v: &V, _now: Now) -> u64 {
        let Some(c) = self.cells.get(cix) else { return 0 };
        let (sum, e) = self.naked(cix, v);
        let l = c.legs[e];
        if !self.venue_open(cix, l.venue) {
            return wait::VENUE;
        }
        let lv = v.legs(c.account, c.group);
        if lv.has_pending
            || lv.has_unknown
            || lv.open_links != 0
            || lv
                .leg
                .iter()
                .take(usize::from(c.n_legs))
                .any(|x| x.inflight_immediate.0 != 0)
        {
            return wait::NOT_QUIESCENT;
        }
        if c.cap_block & CAP_REORG != 0 {
            return wait::REORG;
        }
        if !self.quote_ok(v, c.account, l.inst) {
            return wait::QUOTE;
        }
        let rf = v.risk_for(c.account, l.inst);
        if rf.rate_backoff {
            return wait::RATE_BACKOFF;
        }
        let side = if sum > 0 { Side::Sell } else { Side::Buy };
        if calc::beyond_cross(side, &v.bbo(l.inst), rf.rr_max_cross) {
            return wait::BEYOND_CROSS;
        }
        if l.kind == LEG_SPOT && side == Side::Sell {
            let pf = v.portfolio(c.account);
            let h = self.inst(l.inst);
            let free = pf.cash(l.venue, h.asset).free;
            let inv = pf.inventory_of(l.venue, c.group);
            let want = calc::floor_lot(Qty(sum.abs().min(inv.0)), l.lot.0.max(1));
            let want = if c.auto_fix.phase() == AutoFixPhase::Running {
                Qty(want
                    .0
                    .min(c.auto_fix.budget.0.saturating_sub(c.auto_fix.done_qty.0)))
            } else {
                want
            };
            if free.0 < l.min_qty.0 && want.0 >= l.min_qty.0 {
                return wait::BALANCE;
            }
        }
        0
    }

    /// Accumulates executable time up to `now` and pauses (`run_mark = 0`).
    fn pause(c: &mut TargetCell, now: &Now) {
        let a = &mut c.auto_fix;
        if a.run_mark_tsc.0 != 0 {
            let d = now.tsc.0.saturating_sub(a.run_mark_tsc.0) / now.tsc_per_ms.max(1);
            a.exec_ms = a.exec_ms.saturating_add(d as u32);
            a.run_mark_tsc = Tsc(0);
        }
    }

    pub(crate) fn broken_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let k = self.acct_of_cix(cix);
        let p = self.params(k);
        let engine_target = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
        if engine_target {
            if c.auto_fix.phase() == AutoFixPhase::Running
                && c.active.kind != ACT_AUTOFIX
                && let Some(cm) = self.cells.get_mut(cix)
            {
                cm.auto_fix.phase = AutoFixPhase::Primed as u8;
                cm.auto_fix.run_mark_tsc = Tsc(0);
            }
            self.pursue_engine_broken(cix, v, now, out);
            return;
        }
        match c.auto_fix.phase() {
            AutoFixPhase::Primed => {
                if now.tsc.0 < c.auto_fix.due_tsc.0 {
                    return;
                }
                if self.wait_reason(cix, v, now) != 0 {
                    return;
                }
                self.autofix_start(cix, v, now, out);
                if self
                    .cells
                    .get(cix)
                    .is_some_and(|c| c.auto_fix.phase() == AutoFixPhase::Running)
                {
                    self.autofix_slice(cix, v, now, out);
                }
            }
            AutoFixPhase::Running => {
                let _ = p;
                self.autofix_slice(cix, v, now, out);
            }
            _ => {}
        }
    }

    fn pursue_engine_broken<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        // Engine targets on Broken cells use the serial legs (perp reduce-only first, then spot).
        let ln = self.legs_now(cix, v);
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let mut reached = true;
        for l in c.view.intent.legs() {
            let i = usize::from(l.leg_ix);
            if !self.in_band(cix, i, l.qty, ln[i].pos, v) {
                reached = false;
            }
        }
        if reached && c.active.ext[1] == 0 {
            if c.view.intent.reason == hft_strategy_sdk::TargetReason::Hard
                && matches!(
                    c.view.origin,
                    TargetOrigin::Engine(EngineOrigin::KillFlatten | EngineOrigin::MarginStress)
                )
                && c.view
                    .intent
                    .legs()
                    .iter()
                    .all(|l| ln[usize::from(l.leg_ix)].infl.0 == 0)
            {
                // Flatten/align targets stay until reached; on a Broken cell they are kept until
                // CapCleared{Broken} (state stays Broken).
            }
            return;
        }
        self.serial_step(cix, v, now, out);
    }

    fn autofix_start<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let (sum, e) = self.naked(cix, v);
        let l = c.legs[e];
        if !self.naked_above_dust(cix, v) {
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.auto_fix.leg_ix = e as u8;
            }
            self.autofix_done(cix, sum.unsigned_abs(), now, out);
            return;
        }
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.auto_fix.leg_ix = e as u8;
            cm.auto_fix.start_sign = if sum > 0 { 1 } else { -1 };
        }
        if l.kind == LEG_CHAIN {
            self.autofix_fail(
                cix,
                AutoFixFail::ChainLeg,
                RejectBits(0),
                sum.unsigned_abs(),
                now,
                out,
            );
            return;
        }
        let lv = v.legs(c.account, c.group);
        let mut budget = calc::floor_lot(Qty(sum.abs()), l.lot.0.max(1))
            .0
            .min(lv.leg[e].net.0.abs());
        if l.kind == LEG_SPOT {
            budget = budget.min(v.portfolio(c.account).inventory_of(l.venue, c.group).0);
        }
        let budget = calc::floor_lot(Qty(budget.max(0)), l.lot.0.max(1));
        if budget.0 < l.min_qty.0.max(1) {
            self.autofix_fail(
                cix,
                AutoFixFail::MinOrder,
                RejectBits(0),
                sum.unsigned_abs(),
                now,
                out,
            );
            return;
        }
        if let Some(cm) = self.cells.get_mut(cix) {
            let a = &mut cm.auto_fix;
            a.phase = AutoFixPhase::Running as u8;
            a.started_tsc = now.tsc;
            a.run_mark_tsc = now.tsc;
            a.exec_ms = 0;
            a.budget = budget;
            a.done_qty = Qty(0);
            a.n_slices = 0;
            a.fail = 0;
        }
        let mut d = self.dec(cix, action::AUTO_FIX, now.seq);
        d.origin = hft_event::TargetOriginWire::EngineAutoFix as u8;
        d.algo = crate::decision::algo::ENGINE;
        d.detail = pack(table::AUTO_FIX, autofix::START, 0);
        d.leg_ix = e as u8;
        d.target_qty = budget;
        out.decisions.push(d);
    }

    fn autofix_slice<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let k = self.acct_of_cix(cix);
        let p = self.params(k);
        if c.active.ext[1] != 0 {
            return;
        }
        let w = self.wait_reason(cix, v, now);
        if w != 0 {
            if let Some(cm) = self.cells.get_mut(cix) {
                Self::pause(cm, &now);
            }
            return;
        }
        if let Some(cm) = self.cells.get_mut(cix)
            && cm.auto_fix.run_mark_tsc.0 == 0
        {
            cm.auto_fix.run_mark_tsc = now.tsc;
        }
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let a = c.auto_fix;
        let run = now.tsc.0.saturating_sub(a.run_mark_tsc.0) / now.tsc_per_ms.max(1);
        let (sum, e) = self.naked(cix, v);
        if u64::from(a.exec_ms).saturating_add(run) > u64::from(p.fix_deadline_ms) {
            self.autofix_fail(
                cix,
                AutoFixFail::Deadline,
                RejectBits(0),
                sum.unsigned_abs(),
                now,
                out,
            );
            return;
        }
        if !self.naked_above_dust(cix, v) {
            self.autofix_done(cix, sum.unsigned_abs(), now, out);
            return;
        }
        if sum.signum() != i64::from(a.start_sign) {
            self.autofix_fail(
                cix,
                AutoFixFail::SignFlip,
                RejectBits(0),
                sum.unsigned_abs(),
                now,
                out,
            );
            return;
        }
        if a.done_qty.0 >= a.budget.0 {
            self.autofix_fail(
                cix,
                AutoFixFail::Residual,
                RejectBits(0),
                sum.unsigned_abs(),
                now,
                out,
            );
            return;
        }
        // Spacing: cooldown after the last slice, backoff after a gate, stuck_after after max_nofill.
        if now.tsc.0 < c.wake_at.0 {
            return;
        }
        if c.view.last_slice_tsc.0 != 0
            && a.n_slices > 0
            && now.tsc.0
                < c.view
                    .last_slice_tsc
                    .0
                    .saturating_add(Self::ms(&now, p.force_slice.cooldown_ms))
        {
            let t = Tsc(c
                .view
                .last_slice_tsc
                .0
                .saturating_add(Self::ms(&now, p.force_slice.cooldown_ms)));
            self.reg_timer(cix, ExecTimerKind::Wake, t, c.wake_at, out);
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.wake_at = t;
            }
            return;
        }
        let mut e_ord = match self.fix_order(cix, ExecOrderKind::AutoFix, v) {
            Ok(x) => x,
            Err(crate::fix::FixBuild::Quote) => return,
            Err(_) => {
                self.autofix_fail(
                    cix,
                    AutoFixFail::MinOrder,
                    RejectBits(0),
                    sum.unsigned_abs(),
                    now,
                    out,
                );
                return;
            }
        };
        let l = c.legs[e];
        let rem = a.budget.0.saturating_sub(a.done_qty.0);
        e_ord.qty = calc::floor_lot(Qty(e_ord.qty.0.min(rem)), l.lot.0.max(1));
        if e_ord.qty.0 < l.min_qty.0.max(1) {
            self.autofix_fail(
                cix,
                AutoFixFail::MinOrder,
                RejectBits(0),
                sum.unsigned_abs(),
                now,
                out,
            );
            return;
        }
        e_ord.act = ACT_AUTOFIX;
        e_ord.dec_detail = pack(table::AUTO_FIX, autofix::SLICE, 0);
        e_ord.target_qty = a.budget;
        self.emit(cix, e_ord, v, now, out);
    }

    /// Ends the attempt successfully.
    pub(crate) fn autofix_done(&mut self, cix: usize, naked_after: u64, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.auto_fix.phase = AutoFixPhase::Done as u8;
            cm.auto_fix.run_mark_tsc = Tsc(0);
        }
        let a = c.auto_fix;
        let mut d = self.dec(cix, action::AUTO_FIX, now.seq);
        d.origin = hft_event::TargetOriginWire::EngineAutoFix as u8;
        d.algo = crate::decision::algo::ENGINE;
        d.detail = pack(table::AUTO_FIX, autofix::DONE, 0);
        d.leg_ix = a.leg_ix;
        d.target_qty = a.budget;
        out.decisions.push(d);
        let done = a.done_qty.0.unsigned_abs();
        let n = self.note(
            cix,
            note::EXEC_AUTO_FIX_DONE,
            NoteLevel::P1,
            [
                acct_group(c.account.0, c.group.0),
                u64::from(a.leg_ix),
                done,
                naked_after.saturating_add(done),
                naked_after,
                u64::from(a.n_slices),
                0,
                0,
            ],
        );
        out.notes.push(n);
    }

    /// Ends the attempt with a failure (P0); the cell stays Broken.
    pub(crate) fn autofix_fail(
        &mut self,
        cix: usize,
        fail: AutoFixFail,
        bits: RejectBits,
        naked_after: u64,
        now: Now,
        out: &mut ExecOut,
    ) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.auto_fix.phase = AutoFixPhase::Failed as u8;
            cm.auto_fix.fail = fail as u8;
            cm.auto_fix.run_mark_tsc = Tsc(0);
        }
        let a = c.auto_fix;
        let mut d = self.dec(cix, action::AUTO_FIX, now.seq);
        d.origin = hft_event::TargetOriginWire::EngineAutoFix as u8;
        d.algo = crate::decision::algo::ENGINE;
        d.detail = pack(table::AUTO_FIX, autofix::FAILED, fail as u16);
        d.gate = bits.0;
        d.leg_ix = a.leg_ix;
        d.target_qty = a.budget;
        out.decisions.push(d);
        let done = a.done_qty.0.unsigned_abs();
        let n = self.note(
            cix,
            note::EXEC_AUTO_FIX_FAILED,
            NoteLevel::P0,
            [
                acct_group(c.account.0, c.group.0),
                u64::from(a.leg_ix),
                fail as u64,
                u64::from(bits.0),
                done,
                naked_after.saturating_add(done),
                naked_after,
                0,
            ],
        );
        out.notes.push(n);
    }
}
