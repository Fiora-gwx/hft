//! `advance` (README "推进"): the per-cell step, target pursuit (TakerSlice, PairOnFill,
//! Simultaneous, serial legs), gating, emission of orders and timer maintenance.

use hft_strategy_sdk::{
    BboView, ExecSpec, ExecUpdateKind, FirstLeg, HedgeKind, HedgePlan, IntentClass, LegLink, LegLinkOpt,
    LinkRole, Now, OnFail, OrderIntent, PxMode, StuckReason, TakerSliceParams, TargetOrigin, TargetReason,
    TargetState, Urgency, Views,
};
use hft_types::{GroupId, InstId, Notional, Px, Qty, Rate, RejectBits, Side, Tif, Tsc};

use crate::calc;
use crate::decision::{action, algo, pack, table};
use crate::targets::ratio;
use crate::types::*;
use crate::{Exec, ExecOut, PendOrder};

/// Position and immediate in-flight of one leg.
#[derive(Copy, Clone, Debug)]
pub(crate) struct LegNow {
    pub pos: Qty,
    pub infl: Qty,
}

/// One order to emit.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Emit {
    pub leg: usize,
    pub side: Side,
    pub qty: Qty,
    pub px: Px,
    pub mid: Px,
    pub cap_n: Notional,
    pub kind: ExecOrderKind,
    pub engine: bool,
    pub class: IntentClass,
    pub reduce_only: bool,
    pub link: LegLinkOpt,
    pub hedge_cap: Qty,
    pub act: u8,
    pub dec_action: u8,
    pub dec_detail: u32,
    pub depth: Qty,
    pub first_leg_ix: u8,
    pub algo: u8,
    pub origin: Option<TargetOrigin>,
    pub target_qty: Qty,
    pub cooldown_ms: u32,
}

pub(crate) const RO_BITS: u32 = RejectBits::PX_BAND.0
    | RejectBits::NOTIONAL_ORDER.0
    | RejectBits::NOTIONAL_GROUP.0
    | RejectBits::NOTIONAL_ACCOUNT.0
    | RejectBits::NOTIONAL_VENUE.0;

impl Exec {
    /// Advances every pending cell once, Live accounts before Sim accounts, each in cix order.
    pub fn advance<V: Views>(&mut self, v: &V, now: Now, out: &mut ExecOut) {
        for k in 0..self.accts.len() {
            if self.acct.get(k).is_some_and(|s| s.force_kind != FORCE_NONE) {
                self.apply_force_req(k, v, now);
            }
        }
        let n = self.cell_count();
        for sim in [false, true] {
            for cix in 0..n {
                if !self.is_pending(cix) {
                    continue;
                }
                let k = self.acct_of_cix(cix);
                if self.accts.get(k).is_some_and(|a| a.sim) != sim {
                    continue;
                }
                if !out.room_for_cell() {
                    self.stop_for_capacity(out);
                    return;
                }
                let forced = self.take_pending(cix);
                self.step_cell(cix, forced, v, now, out);
            }
        }
    }

    fn stop_for_capacity(&mut self, out: &mut ExecOut) {
        // Remaining pending cells are processed in full when continued (uniform with restore).
        for (p, f) in self.pending.iter().zip(self.forced.iter_mut()) {
            *f |= *p;
        }
        if !self.continue_set {
            out.timers.push(ExecTimerReq {
                cancel: false,
                timer_id: ExecTimerKind::Continue as u32,
                group: GroupId(0),
                at: Tsc(0),
            });
            self.continue_set = true;
        }
    }

    pub(crate) fn step_cell<V: Views>(
        &mut self,
        cix: usize,
        forced: bool,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let k = self.acct_of_cix(cix);
        let p = self.params(k);
        // HARD (lazy): clears FixState, pauses the auto-fix; release re-arms its window.
        let hard = v.risk(c.account).hard;
        if hard {
            self.apply_hard(cix, now);
        } else if c.auto_fix.phase() == AutoFixPhase::WaitHard
            && let Some(c) = self.cells.get_mut(cix)
        {
            c.auto_fix.phase = AutoFixPhase::Primed as u8;
            c.auto_fix.due_tsc = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.broken_auto_fix_ms)));
            c.deferred |= D_TIMERS;
        }
        self.flush_deferred(cix, v, now, out);
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if c.deferred & D_ENGINE_CLEARED != 0 && c.active.kind == ACT_NONE {
            self.engine_clear(cix, v, now, out);
        }
        if self.is_broken(cix) {
            self.broken_step(cix, v, now, out);
        } else {
            let c = self.cells.get(cix).copied();
            let skip = c.is_some_and(|c| !forced && now.tsc.0 < c.wake_at.0);
            if !skip {
                if c.is_some_and(|c| c.has_target) {
                    self.pursue(cix, v, now, out);
                }
                if !c.is_some_and(|c| c.is_ungrouped()) {
                    self.ext_deadline_step(cix, v, now, out);
                    self.fix_step(cix, v, now, out);
                }
            }
        }
        self.alarm_step(cix, v, now, out);
        self.timer_step(cix, now, out);
    }

    fn apply_hard(&mut self, cix: usize, now: Now) {
        let Some(c) = self.cells.get_mut(cix) else { return };
        c.fix = FixState::ZERO;
        match c.auto_fix.phase() {
            AutoFixPhase::Primed => c.auto_fix.phase = AutoFixPhase::WaitHard as u8,
            AutoFixPhase::Running => {
                let a = &mut c.auto_fix;
                if a.run_mark_tsc.0 != 0 {
                    let d = now.tsc.0.saturating_sub(a.run_mark_tsc.0) / now.tsc_per_ms.max(1);
                    a.exec_ms = a.exec_ms.saturating_add(d as u32);
                    a.run_mark_tsc = Tsc(0);
                }
                a.phase = AutoFixPhase::WaitHard as u8;
            }
            _ => {}
        }
    }

    /// Writes the records a multi-cell entry left in `deferred`.
    fn flush_deferred<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let d = c.deferred;
        if d == 0 {
            return;
        }
        if d & D_ENGINE_SET != 0 {
            self.engine_set_records(cix, v, now, out);
        }
        if d & D_STATE != 0 {
            let mut r = self.dec(cix, action::STATE, now.seq);
            r.state_from = c.def_from;
            let (det, gate) = crate::state_detail(c.view.state);
            r.detail = det;
            r.gate = gate;
            out.decisions.push(r);
            match c.view.state {
                TargetState::Stuck(s) => self.notify(cix, ExecUpdateKind::Stuck(s), None, out),
                TargetState::Broken(b) => self.notify(cix, ExecUpdateKind::Broken(b), None, out),
                TargetState::Idle if d & D_CLEARED_BY_CMD == 0 && c.def_from == 4 => self.notify(
                    cix,
                    ExecUpdateKind::Done {
                        cleared_by_engine: true,
                    },
                    None,
                    out,
                ),
                _ => {}
            }
        }
        if d & D_CLEARED_BY_CMD != 0 {
            let mut r = self.dec(cix, action::TARGET_CLEARED, now.seq);
            r.eflags |= crate::decision::eflags::CLEARED_BY_ENGINE;
            if d & D_STATE != 0 {
                r.state_from = c.view.state.code();
            }
            out.decisions.push(r);
            self.notify(
                cix,
                ExecUpdateKind::Done {
                    cleared_by_engine: true,
                },
                None,
                out,
            );
        }
        if d & D_HOLD_REL != 0 && d & D_ENGINE_SET == 0 {
            let n = self.note(
                cix,
                crate::decision::note::EXEC_MANUAL_HOLD,
                hft_event::NoteLevel::Info,
                [
                    crate::decision::acct_group(c.account.0, c.group.0),
                    0,
                    crate::decision::hold_release::ACCOUNT_CLEARED,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
            );
            out.notes.push(n);
        }
        if d & D_EXT_CLR != 0 {
            let mut r = self.dec(cix, action::EXTERNAL_SEEN, now.seq);
            r.detail = pack(table::EXTERNAL, 0, crate::decision::external::CLEARED);
            out.decisions.push(r);
        }
        if let Some(c) = self.cells.get_mut(cix) {
            c.deferred &= D_ENGINE_CLEARED | D_TIMERS | D_ALARM;
            c.def_from = 0;
        }
    }

    // ------------------------------------------------------------------ leg state

    /// Position and immediate in-flight per leg (grouped: LegTracker; ungrouped: ledger + own_infl).
    pub(crate) fn legs_now<V: Views>(&self, cix: usize, v: &V) -> [LegNow; 4] {
        let mut r = [LegNow {
            pos: Qty(0),
            infl: Qty(0),
        }; 4];
        let Some(c) = self.cells.get(cix) else { return r };
        if c.is_ungrouped() {
            for (i, x) in r.iter_mut().enumerate().take(usize::from(c.n_legs)) {
                let l = c.legs[i];
                x.pos = self.ungrouped_pos(c.account, l.venue, l.inst, v);
                x.infl = c.aux[i];
            }
        } else {
            let lv = v.legs(c.account, c.group);
            for (i, x) in r.iter_mut().enumerate() {
                x.pos = lv.leg[i].net;
                x.infl = lv.leg[i].inflight_immediate;
            }
        }
        r
    }

    /// Account perp position on `venue` minus the grouped cells' positions of `inst`.
    pub(crate) fn ungrouped_pos<V: Views>(
        &self,
        a: hft_types::AccountId,
        venue: hft_types::VenueId,
        inst: InstId,
        v: &V,
    ) -> Qty {
        use hft_strategy_sdk::PortfolioRead;
        let acct = v.portfolio(a).perp(venue, inst).qty;
        let mut grouped = 0i64;
        let lo = self.inst_ix.partition_point(|x| x.0 < inst);
        for (i, cix) in self.inst_ix.iter().skip(lo) {
            if *i != inst {
                break;
            }
            let Some(c) = self.cells.get(usize::from(*cix)) else {
                continue;
            };
            if c.account != a {
                continue;
            }
            let gp = v.group_pos(a, c.group);
            for (ix, l) in c.legs.iter().enumerate().take(usize::from(c.n_legs)) {
                if l.inst == inst {
                    grouped = grouped.saturating_add(gp.legs[ix].qty.0);
                }
            }
        }
        Qty(acct.0.saturating_sub(grouped))
    }

    /// Mark (or mid when no mark) of an inst.
    pub(crate) fn mark<V: Views>(&self, v: &V, i: InstId) -> Px {
        let m = v.mark(i);
        if m.0 > 0 {
            return m;
        }
        let b = v.bbo(i);
        if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
            calc::mid(&b)
        } else {
            Px(0)
        }
    }

    /// Quote of leg inst usable (both sides and connection health).
    pub(crate) fn quote_ok<V: Views>(&self, v: &V, a: hft_types::AccountId, i: InstId) -> bool {
        let b = v.bbo(i);
        b.bid_px.0 > 0 && b.ask_px.0 > 0 && v.quote_ok(a, i)
    }

    /// Whether leg `i` with target `t` and quantity `x` is within the no-trade band (target 0:
    /// dust line).
    pub(crate) fn in_band<V: Views>(&self, cix: usize, i: usize, t: Qty, x: Qty, v: &V) -> bool {
        let Some(c) = self.cells.get(cix) else { return true };
        let p = self.params(self.acct_of_cix(cix));
        let l = c.legs[i];
        let h = self.inst(l.inst);
        let mark = self.mark(v, l.inst);
        if t.0 == 0 {
            return calc::notional_le(x, mark, h, p.dust_notional);
        }
        let floor = calc::floor_lot(calc::qty_of_notional(p.band_floor_notional, mark, h), l.lot.0);
        let base = Qty(t.0.unsigned_abs().max(floor.0.unsigned_abs()) as i64);
        let band = if c.has_target { c.view.intent.band } else { Rate(0) };
        let width = calc::mul_rate(base, band);
        (t.0 - x.0).unsigned_abs() <= width.0.unsigned_abs()
    }

    pub(crate) fn cooldown_eff(&self, cix: usize, cd: u32) -> u32 {
        let p = self.params(self.acct_of_cix(cix));
        match self.urgency_of(cix) {
            Urgency::Passive => cd.saturating_mul(u32::from(p.passive_cooldown_mult)),
            Urgency::Normal => cd,
            Urgency::Urgent => cd.min(p.urgent_cooldown_ms),
        }
    }

    pub(crate) fn stuck_after_eff(&self, cix: usize) -> u32 {
        let p = self.params(self.acct_of_cix(cix));
        match self.urgency_of(cix) {
            Urgency::Passive => p
                .stuck_after_ms
                .saturating_mul(u32::from(p.passive_cooldown_mult)),
            Urgency::Normal => p.stuck_after_ms,
            Urgency::Urgent => p.stuck_after_ms / 2,
        }
    }

    /// Whether the cell's current target is an engine `MarginStress`/`Hard` target.
    pub(crate) fn engine_risk_target(&self, cix: usize) -> bool {
        self.cells.get(cix).is_some_and(|c| {
            c.has_target
                && matches!(c.view.origin, TargetOrigin::Engine(_))
                && matches!(
                    c.view.intent.reason,
                    TargetReason::MarginStress | TargetReason::Hard
                )
        })
    }

    /// Orders of this cell go out as engine orders (engine target, or account in force/flatten).
    pub(crate) fn engine_orders(&self, cix: usize) -> bool {
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        st.force_active
            || st.flatten_lock
            || self
                .cells
                .get(cix)
                .is_some_and(|c| matches!(c.view.origin, TargetOrigin::Engine(_)))
    }

    /// Effective target of leg `i` against quantity `x`: engine orders never increase `|pos|`, so
    /// for a cell whose orders go out as engine orders the target is clamped into `[0, x]` (a
    /// position already at or beyond the target toward zero counts as reached).
    pub(crate) fn leg_target(&self, cix: usize, t: Qty, x: Qty) -> Qty {
        if !self.engine_orders(cix) {
            return t;
        }
        if x.0 >= 0 {
            Qty(t.0.clamp(0, x.0))
        } else {
            Qty(t.0.clamp(x.0, 0))
        }
    }

    // ------------------------------------------------------------------ gating

    /// Records a gate (precheck failure or gated rejection): `gated_since`, bits, Stuck after the
    /// urgency-scaled `stuck_after_ms`; `backoff` re-arms the cell `gated_backoff_ms` later.
    pub(crate) fn gate(
        &mut self,
        cix: usize,
        bits: RejectBits,
        backoff: bool,
        target: bool,
        now: Now,
        out: &mut ExecOut,
    ) {
        let p = self.params(self.acct_of_cix(cix));
        let after = self.stuck_after_eff(cix);
        let Some(c) = self.cells.get_mut(cix) else { return };
        if c.gated_since.0 == 0 {
            c.gated_since = now.tsc;
        }
        c.gated_bits = bits;
        let since = c.gated_since;
        let state = c.view.state;
        if backoff {
            let at = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.gated_backoff_ms)));
            let saved = c.wake_at;
            self.reg_timer(cix, ExecTimerKind::Wake, at, saved, out);
            if let Some(c) = self.cells.get_mut(cix) {
                c.wake_at = at;
            }
        }
        if !target || matches!(state, TargetState::Broken(_)) {
            return;
        }
        if now.tsc.0.saturating_sub(since.0) >= Self::ms(&now, after) {
            let r = crate::gated_reason(bits);
            if !matches!(
                state,
                TargetState::Stuck(StuckReason::SliceFails) | TargetState::Stuck(StuckReason::MinOrder)
            ) && state != TargetState::Stuck(r)
            {
                self.set_state(cix, TargetState::Stuck(r), now.seq, out);
                if let Some(c) = self.cells.get_mut(cix) {
                    c.stuck_since = now.tsc;
                }
                if self.engine_orders(cix) && matches!(r, StuckReason::Gated(_)) {
                    self.stuck_note(cix, now, out);
                }
            }
        }
    }

    pub(crate) fn stuck_note(&self, cix: usize, _now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix) else { return };
        let (code, gate) = match c.view.state {
            TargetState::Stuck(r) => (
                u64::from(r.code()),
                if let StuckReason::Gated(b) = r {
                    u64::from(b.0)
                } else {
                    0
                },
            ),
            _ => (0, 0),
        };
        let n = self.note(
            cix,
            crate::decision::note::EXEC_STUCK,
            hft_event::NoteLevel::P1,
            [
                crate::decision::acct_group(c.account.0, c.group.0),
                code,
                gate,
                0,
                0,
                0,
                0,
                0,
            ],
        );
        out.notes.push(n);
    }

    // ------------------------------------------------------------------ pursuit

    fn pursue<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        // Other external events can change the Stuck reason while a manual hold remains.
        // The durable hold bit, rather than the presentation state alone, owns this gate.
        if c.hold & MANUAL_HOLD != 0 && c.view.origin == TargetOrigin::Strategy {
            return;
        }
        let ln = self.legs_now(cix, v);
        let engine_risk = self.engine_risk_target(cix);
        // Unknown slices of engine reduce-only perp legs are released (README "推进" 4).
        if engine_risk
            && (c.serial != 0 || c.is_ungrouped() || matches!(c.view.intent.exec, ExecSpec::TakerSlice(_)))
        {
            self.release_unknown(cix);
        }
        let c = self.cells.get(cix).copied().unwrap_or(c);
        // Completion.
        let mut reached = true;
        for l in c.view.intent.legs() {
            let i = usize::from(l.leg_ix);
            let t = self.leg_target(cix, l.qty, ln[i].pos);
            if !self.in_band(cix, i, t, ln[i].pos, v) {
                reached = false;
            }
        }
        let infl_zero = ln.iter().take(usize::from(c.n_legs)).all(|x| x.infl.0 == 0);
        if reached && infl_zero && c.active.kind == ACT_NONE {
            if c.view.state != TargetState::Idle {
                self.on_reached(cix, v, now, out);
            }
            return;
        }
        if c.view.state == TargetState::Idle {
            self.set_state(cix, TargetState::Working, now.seq, out);
        }
        let c = self.cells.get(cix).copied().unwrap_or(c);
        // Stuck states that stop target slices.
        if matches!(
            c.view.state,
            TargetState::Stuck(StuckReason::SliceFails) | TargetState::Stuck(StuckReason::External)
        ) {
            return;
        }
        if c.serial != 0 || (engine_risk && c.is_ungrouped()) {
            self.serial_step(cix, v, now, out);
            return;
        }
        match c.view.intent.exec {
            ExecSpec::TakerSlice(_) => self.taker_step(cix, v, now, out),
            ExecSpec::PairOnFill(_) | ExecSpec::Simultaneous(_) => self.pair_step(cix, v, now, out),
            ExecSpec::MakerFirst(_) => {}
        }
    }

    /// Releases `Unknown` reduce-only perp slices of an engine target (`own_infl`/LegTracker keep
    /// counting their remainder).
    fn release_unknown(&mut self, cix: usize) {
        let Some(c) = self.cells.get_mut(cix) else { return };
        for i in 0..usize::from(c.n_legs) {
            let bit = 1u8 << i;
            let l = c.legs[i];
            if c.active.ext[1] & bit != 0
                && c.leg_unknown(i)
                && l.kind == LEG_PERP
                && l.reduce_only_cap != 0
                && c.active.kind != ACT_PAIR
                // Ungrouped cell: at most one released remainder per leg (its terminal update
                // settles exactly that remainder).
                && (!c.is_ungrouped() || c.view.filled[i].0 == 0)
            {
                c.active.ext[1] &= !bit;
                c.aux16 &= !(1 << (AUX_UNKNOWN_SHIFT as usize + i));
                if c.is_ungrouped() {
                    c.view.filled[i] = c.aux[i];
                }
                // The leg's next slice records its own slot.
                if let Some(sl) = c.slot.get_mut(i) {
                    *sl = u32::MAX;
                }
                if c.active.ext[1] == 0 {
                    c.active.kind = ACT_NONE;
                    c.active.ext[0] = 0;
                }
            }
        }
    }

    fn on_reached<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let engine = matches!(c.view.origin, TargetOrigin::Engine(_));
        let clear = engine && c.view.intent.reason == TargetReason::Hard;
        self.set_state(cix, TargetState::Idle, now.seq, out);
        self.notify(
            cix,
            ExecUpdateKind::Done {
                cleared_by_engine: clear,
            },
            None,
            out,
        );
        if let Some(c) = self.cells.get_mut(cix) {
            c.serial = 0;
            c.fail_streak = 0;
            c.nofill_streak = 0;
            c.gated_since = Tsc(0);
            c.gated_bits = RejectBits(0);
            if clear {
                c.has_target = false;
                c.pair_lot = Qty(0);
            }
        }
        if engine {
            self.convert_deferred_hold(cix, v, now, out);
        }
    }

    // ------------------------------------------------------------------ prechecks

    /// Gate bits of an increasing leg (mode and risk); 0 = open allowed.
    fn open_gate<V: Views>(&self, cix: usize, v: &V) -> (RejectBits, bool) {
        let Some(c) = self.cells.get(cix) else {
            return (RejectBits(0), false);
        };
        let r = v.risk(c.account);
        let mut b = 0u32;
        let engine = self.engine_orders(cix);
        let mode = v.mode(c.account, c.group);
        if !engine && mode == hft_types::RunMode::CloseOnly {
            b |= RejectBits::CLOSE_ONLY.0;
        }
        if r.hard {
            b |= RejectBits::HARD.0;
        }
        if r.open_blocked {
            b |= RejectBits::KILLED.0;
        }
        if r.margin_level >= hft_strategy_sdk::MarginLevel::L1 {
            b |= RejectBits::EXPOSURE_GATE.0;
        }
        // Shadow (or Live with open_allowed = false) after its one slice.
        let shadow_stuck = c.view.state == TargetState::Stuck(StuckReason::Shadow);
        (RejectBits(b), shadow_stuck)
    }

    fn time_ok(&self, cix: usize, cd_ms: u32, now: &Now) -> bool {
        let Some(c) = self.cells.get(cix) else {
            return false;
        };
        if now.tsc.0 < c.wake_at.0 {
            return false;
        }
        if c.view.last_slice_tsc.0 != 0 {
            let cd = self.cooldown_eff(cix, cd_ms);
            if now.tsc.0 < c.view.last_slice_tsc.0.saturating_add(Self::ms(now, cd)) {
                return false;
            }
        }
        true
    }

    /// Registers the Wake timer for the end of the cooldown when slicing waits on time.
    fn wake_for_cooldown(&mut self, cix: usize, cd_ms: u32, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let cd = self.cooldown_eff(cix, cd_ms);
        let t = c
            .view
            .last_slice_tsc
            .0
            .saturating_add(Self::ms(&now, cd))
            .max(c.wake_at.0);
        if t > now.tsc.0 {
            self.reg_timer(cix, ExecTimerKind::Wake, Tsc(t), c.wake_at, out);
            if let Some(c) = self.cells.get_mut(cix) {
                c.wake_at = Tsc(t);
            }
        }
    }

    // ------------------------------------------------------------------ TakerSlice

    fn taker_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let k = self.acct_of_cix(cix);
        let p = self.params(k);
        let engine = self.engine_orders(cix);
        let slice = if engine && matches!(c.view.origin, TargetOrigin::Engine(_)) {
            p.force_slice
        } else {
            *c.view.intent.exec.slice()
        };
        if c.active.kind != ACT_NONE
            && c.active
                .order_kind()
                .is_some_and(|k| k != ExecOrderKind::Slice && k != ExecOrderKind::ForceStep)
        {
            return;
        }
        if !self.time_ok(cix, slice.cooldown_ms, &now) {
            self.wake_for_cooldown(cix, slice.cooldown_ms, now, out);
            return;
        }
        let ln = self.legs_now(cix, v);
        let mut gated = 0u32;
        let mut emitted = false;
        let mut any_wait = false;
        let mut noliq = false;
        for lt in c.view.intent.legs() {
            let i = usize::from(lt.leg_ix);
            let cur = self.cells.get(cix).copied().unwrap_or(c);
            if cur.active.ext[1] & (1 << i) != 0 {
                continue;
            }
            let l = c.legs[i];
            let x = ln[i];
            let engine_risk = self.engine_risk_target(cix);
            if x.infl.0 != 0 && !(engine_risk && l.kind == LEG_PERP && l.reduce_only_cap != 0) {
                any_wait = true;
                continue;
            }
            let eff = Qty(x.pos.0.saturating_add(x.infl.0));
            let tq = self.leg_target(cix, lt.qty, eff);
            if self.in_band(cix, i, tq, eff, v) {
                continue;
            }
            match self.leg_slice(cix, i, tq, x, &slice, ExecOrderKind::Slice, v, now) {
                LegOut::Emit(e) => {
                    self.emit(cix, e, v, now, out);
                    emitted = true;
                }
                LegOut::Gate(b) => gated |= b.0,
                LegOut::Wait => any_wait = true,
                LegOut::MinOrder => self.min_order(cix, i, tq, x.pos, v, now, out),
                LegOut::NoLiq => noliq = true,
                LegOut::Nothing => {}
            }
        }
        if !emitted && gated != 0 {
            self.gate(cix, RejectBits(gated), true, true, now, out);
        } else if !emitted && noliq && !any_wait {
            self.noliq(cix, now, out);
        }
    }

    /// Zero-target leg below the exchange minimum: above dust → Stuck{MinOrder}.
    #[allow(clippy::too_many_arguments)]
    fn min_order<V: Views>(
        &mut self,
        cix: usize,
        i: usize,
        t: Qty,
        pos: Qty,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) {
        if t.0 != 0 || self.in_band(cix, i, Qty(0), pos, v) {
            return;
        }
        if self
            .cells
            .get(cix)
            .is_some_and(|c| c.view.state != TargetState::Stuck(StuckReason::MinOrder))
        {
            self.set_state(cix, TargetState::Stuck(StuckReason::MinOrder), now.seq, out);
            self.stuck_note(cix, now, out);
        }
    }

    /// Computes one TakerSlice-style slice of leg `i` toward `t`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn leg_slice<V: Views>(
        &self,
        cix: usize,
        i: usize,
        t: Qty,
        x: LegNow,
        slice: &TakerSliceParams,
        kind: ExecOrderKind,
        v: &V,
        _now: Now,
    ) -> LegOut {
        let Some(c) = self.cells.get(cix) else {
            return LegOut::Nothing;
        };
        let l = c.legs[i];
        let h = *self.inst(l.inst);
        let eff = Qty(x.pos.0.saturating_add(x.infl.0));
        let diff = t.0.saturating_sub(eff.0);
        if diff == 0 {
            return LegOut::Nothing;
        }
        let side = if diff > 0 { Side::Buy } else { Side::Sell };
        let reducing = eff.0 != 0 && eff.0.signum() != diff.signum();
        let engine = self.engine_orders(cix);
        let engine_target = matches!(c.view.origin, TargetOrigin::Engine(_));
        let risk_reason = matches!(
            c.view.intent.reason,
            TargetReason::MarginStress | TargetReason::Hard
        );
        let a = c.account;
        // Precheck.
        if reducing {
            if v.risk(a).hard {
                return LegOut::Gate(RejectBits::HARD);
            }
        } else {
            let (b, shadow_stuck) = self.open_gate(cix, v);
            if shadow_stuck {
                return LegOut::Wait;
            }
            if b.0 != 0 {
                return LegOut::Gate(b);
            }
        }
        if c.view.state == TargetState::Stuck(StuckReason::Shadow)
            && v.mode(a, c.group) == hft_types::RunMode::Shadow
        {
            return LegOut::Wait;
        }
        if !self.quote_ok(v, a, l.inst) {
            return LegOut::Gate(RejectBits::STALE_MID);
        }
        let bbo = v.bbo(l.inst);
        let rf = v.risk_for(a, l.inst);
        let r = v.risk(a);
        let p = self.params(self.acct_of_cix(cix));
        let (mc, rr) = self.cross_params(engine_target, reducing, risk_reason, slice, &p, &rf);
        let px = calc::limit_px(side, &bbo, mc, rr, r.conn_liveness_ms, &h);
        if px.0 <= 0 {
            return LegOut::Gate(RejectBits::STALE_MID);
        }
        let urgency = self.urgency_of(cix);
        let levels = if urgency == Urgency::Passive {
            1
        } else {
            slice.depth_levels
        };
        let (dq, _) = calc::depth(&v.book(l.inst), &bbo, side, levels, mc, &h);
        let class = if !reducing {
            IntentClass::Open
        } else if engine || risk_reason {
            IntentClass::Flatten
        } else {
            IntentClass::Reduce
        };
        let cap_n = self.cap_n(cix, class, kind, engine_target, slice, &r);
        let qcap = calc::qty_under_cap(cap_n, px, rf.quote_rate, &h);
        let mut chunk = (diff.unsigned_abs() as i64)
            .min(calc::mul_rate(dq, slice.depth_frac).0)
            .min(qcap.0);
        if reducing {
            chunk = chunk.min(eff.0.unsigned_abs() as i64);
            if engine_target && risk_reason {
                // Before resting orders are cancelled: ≤ |pos| − |same-direction immediate in-flight|.
                let same = if x.infl.0.signum() == diff.signum() {
                    x.infl.0.unsigned_abs()
                } else {
                    0
                };
                let room = x.pos.0.unsigned_abs().saturating_sub(same) as i64;
                chunk = chunk.min(room);
            }
        }
        if l.kind == LEG_SPOT && side == Side::Sell {
            use hft_strategy_sdk::PortfolioRead;
            let pf = v.portfolio(a);
            let inv = pf.inventory_of(l.venue, c.group);
            let free = pf.cash(l.venue, h.asset).free;
            let lim = if c.is_ungrouped() {
                free.0
            } else {
                inv.0.min(free.0)
            };
            chunk = chunk.min(lim.max(0));
        }
        let chunk = calc::floor_lot(Qty(chunk.max(0)), l.lot.0.max(1));
        let absdiff = Qty(diff.unsigned_abs() as i64);
        let min_ok = |q: Qty| {
            q.0 >= l.min_qty.0.max(1) && !calc::notional_le(q, px, &h, Notional(h.min_notional.0 - 1))
        };
        let below_chunk = calc::notional_le(chunk, px, &h, Notional(slice.min_chunk_notional.0 - 1));
        let qty = if chunk.0 > 0 && !below_chunk && min_ok(chunk) {
            chunk
        } else {
            let last = calc::floor_lot(absdiff, l.lot.0.max(1));
            if !min_ok(last) {
                return LegOut::MinOrder;
            }
            let diff_below_chunk =
                calc::notional_le(absdiff, px, &h, Notional(slice.min_chunk_notional.0 - 1));
            if diff_below_chunk && last.0 <= qcap.0 {
                last
            } else {
                return LegOut::NoLiq;
            }
        };
        let reduce_only = reducing && l.reduce_only_cap != 0;
        let kind = if engine_target && kind == ExecOrderKind::Slice {
            ExecOrderKind::ForceStep
        } else {
            kind
        };
        LegOut::Emit(Emit {
            leg: i,
            side,
            qty,
            px,
            mid: calc::mid(&bbo),
            cap_n,
            kind,
            engine,
            class,
            reduce_only,
            link: LegLinkOpt::NONE,
            hedge_cap: Qty(0),
            act: if engine_target {
                if c.serial != 0 || c.is_ungrouped() || self.is_broken(cix) {
                    ACT_SERIAL
                } else {
                    ACT_FORCE
                }
            } else {
                ACT_SLICE
            },
            dec_action: action::SLICE,
            dec_detail: 0,
            depth: dq,
            first_leg_ix: 0xFF,
            algo: if engine_target && (c.serial != 0 || c.is_ungrouped() || self.is_broken(cix)) {
                algo::ENGINE
            } else {
                c.view.intent.exec.algo()
            },
            origin: None,
            target_qty: t,
            cooldown_ms: slice.cooldown_ms,
        })
    }

    /// `(max_cross_eff, rr_eff)` (README "限价与单笔名义" 1).
    pub(crate) fn cross_params(
        &self,
        engine_target: bool,
        reducing: bool,
        risk_reason: bool,
        slice: &TakerSliceParams,
        p: &ExecParams,
        rf: &hft_strategy_sdk::InstRiskView,
    ) -> (Rate, Rate) {
        let mc = if engine_target {
            p.force_slice.max_cross
        } else if reducing {
            Rate(slice.max_cross.0.min(rf.rr_max_cross.0))
        } else {
            Rate(slice.max_cross.0.min(rf.ioc_max_cross.0))
        };
        let rr = if engine_target && risk_reason && rf.l3_max_cross.0 > 0 {
            Rate(rf.rr_max_cross.0.max(rf.l3_max_cross.0))
        } else {
            rf.rr_max_cross
        };
        (mc, rr)
    }

    /// Per-order notional cap `cap_n` (README "限价与单笔名义" 3).
    pub(crate) fn cap_n(
        &self,
        cix: usize,
        class: IntentClass,
        kind: ExecOrderKind,
        engine_target: bool,
        slice: &TakerSliceParams,
        r: &hft_strategy_sdk::RiskStateView,
    ) -> Notional {
        let p = self.params(self.acct_of_cix(cix));
        let chunk = if engine_target || kind.is_hedge() || kind == ExecOrderKind::ForceStep {
            p.force_slice.max_chunk_notional.0
        } else {
            slice.max_chunk_notional.0.min(r.max_order_notional.0)
        };
        let risk = match (kind, class) {
            (ExecOrderKind::AutoFix, _) => r.max_reduce_notional.0.min(r.max_hedge_notional.0),
            (k, _) if k.is_hedge() => r.max_hedge_notional.0,
            (_, IntentClass::Open) => r.max_order_notional.0,
            _ => r.max_reduce_notional.0,
        };
        Notional(chunk.min(risk))
    }

    // ------------------------------------------------------------------ PairOnFill / Simultaneous

    fn pair_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let a = c.account;
        let engine_risk = self.engine_risk_target(cix);
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        let forced_order = engine_risk
            || st.force_active
            || st.flatten_lock
            || matches!(
                c.view.intent.reason,
                TargetReason::MarginStress | TargetReason::Hard
            );
        // SerialFallback (engine targets): second leg unusable, link in RESUME_HOLD, first slot Unknown.
        if engine_risk && c.n_legs == 2 {
            let perp = self.perp_first(cix, v);
            let h = 1 - perp;
            let hl = c.legs[h];
            let mut reason = 0u16;
            if !self.quote_ok(v, a, hl.inst) {
                reason = crate::decision::serial::STALE_QUOTE;
            } else if !v.venue_ready(a, hl.venue) {
                reason = crate::decision::serial::NOT_READY;
            } else if c.active.kind == ACT_PAIR && c.hold & LINK_HOLD != 0 {
                reason = crate::decision::serial::RESUME_HOLD;
            } else if c.active.kind == ACT_PAIR && c.leg_unknown(usize::from(c.active.leg_ix)) {
                reason = crate::decision::serial::FIRST_UNKNOWN;
            }
            let perp_ok = self.quote_ok(v, a, c.legs[perp].inst);
            if reason != 0 && (perp_ok || c.active.kind == ACT_PAIR) {
                self.serial_fallback(cix, perp, reason, v, now, out);
                self.serial_step(cix, v, now, out);
                return;
            }
        }
        if c.active.kind != ACT_NONE {
            return;
        }
        let pslice = *c.view.intent.exec.slice();
        let p = self.params(k);
        let slice = if matches!(c.view.origin, TargetOrigin::Engine(_)) {
            p.force_slice
        } else {
            pslice
        };
        if !self.time_ok(cix, slice.cooldown_ms, &now) {
            self.wake_for_cooldown(cix, slice.cooldown_ms, now, out);
            return;
        }
        let ln = self.legs_now(cix, v);
        let eff = [
            Qty(ln[0].pos.0.saturating_add(ln[0].infl.0)),
            Qty(ln[1].pos.0.saturating_add(ln[1].infl.0)),
        ];
        let t0 = self.leg_target(cix, c.target_of(0).unwrap_or(ln[0].pos), eff[0]);
        let t1 = self.leg_target(cix, c.target_of(1).unwrap_or(ln[1].pos), eff[1]);
        let tg = [t0, t1];
        let out_of_band = [
            !self.in_band(cix, 0, tg[0], eff[0], v),
            !self.in_band(cix, 1, tg[1], eff[1], v),
        ];
        if !out_of_band[0] && !out_of_band[1] {
            return;
        }
        if out_of_band[0] != out_of_band[1] {
            // One leg left: single-leg tail (README PairOnFill 6).
            let i = if out_of_band[0] { 0 } else { 1 };
            if ln[i].infl.0 != 0 {
                return;
            }
            self.tail_step(cix, i, tg[i], ln[i], &slice, v, now, out);
            return;
        }
        // First leg.
        let f = if forced_order {
            self.perp_first(cix, v)
        } else {
            self.first_by_param(cix, v, &tg, &eff)
        };
        let h = 1 - f;
        let df = tg[f].0 - eff[f].0;
        let side_f = if df > 0 { Side::Buy } else { Side::Sell };
        let dh = tg[h].0 - eff[h].0;
        let side_h = if dh > 0 { Side::Buy } else { Side::Sell };
        let (lf, lh) = (c.legs[f], c.legs[h]);
        let (hf, hh) = (*self.inst(lf.inst), *self.inst(lh.inst));
        let red_f = eff[f].0 != 0 && eff[f].0.signum() != df.signum();
        let red_h = eff[h].0 != 0 && eff[h].0.signum() != dh.signum();
        // Prechecks.
        let mut bits = 0u32;
        let r = v.risk(a);
        for red in [red_f, red_h] {
            if red {
                if r.hard {
                    bits |= RejectBits::HARD.0;
                }
            } else {
                let (b, ss) = self.open_gate(cix, v);
                if ss {
                    return;
                }
                bits |= b.0;
            }
        }
        if bits != 0 {
            self.gate(cix, RejectBits(bits), true, true, now, out);
            return;
        }
        if !self.quote_ok(v, a, lf.inst) || (!engine_risk && !self.quote_ok(v, a, lh.inst)) {
            self.gate(cix, RejectBits::STALE_MID, true, true, now, out);
            return;
        }
        let bf = v.bbo(lf.inst);
        let bh = v.bbo(lh.inst);
        let rff = v.risk_for(a, lf.inst);
        let rfh = v.risk_for(a, lh.inst);
        let engine_target = matches!(c.view.origin, TargetOrigin::Engine(_));
        let risk_reason = matches!(
            c.view.intent.reason,
            TargetReason::MarginStress | TargetReason::Hard
        );
        let (mc, rr) = self.cross_params(engine_target, red_f, risk_reason, &slice, &p, &rff);
        let px = calc::limit_px(side_f, &bf, mc, rr, r.conn_liveness_ms, &hf);
        if px.0 <= 0 {
            self.gate(cix, RejectBits::STALE_MID, true, true, now, out);
            return;
        }
        let urgency = self.urgency_of(cix);
        let levels = if urgency == Urgency::Passive {
            1
        } else {
            slice.depth_levels
        };
        let (dq_f, _) = calc::depth(&v.book(lf.inst), &bf, side_f, levels, mc, &hf);
        let (dq_h, _) = calc::depth(&v.book(lh.inst), &bh, side_h, levels, mc, &hh);
        let (num, den) = if f == 0 {
            ratio(&c.view.intent.exec)
        } else {
            let (n, d) = ratio(&c.view.intent.exec);
            (d, n)
        };
        let dq_h_f = Qty((i128::from(dq_h.0) * i128::from(den) / i128::from(num.max(1))) as i64);
        let weak = Qty(dq_f.0.min(dq_h_f.0));
        let engine = self.engine_orders(cix);
        let class = if !red_f {
            IntentClass::Open
        } else if engine || risk_reason {
            IntentClass::Flatten
        } else {
            IntentClass::Reduce
        };
        let cap_n = self.cap_n(cix, class, ExecOrderKind::PairFirst, engine_target, &slice, &r);
        let qcap = calc::qty_under_cap(cap_n, px, rff.quote_rate, &hf);
        let mut c_q = df
            .unsigned_abs()
            .min(calc::mul_rate(weak, slice.depth_frac).0.max(0) as u64)
            .min(qcap.0.max(0) as u64) as i64;
        if red_f {
            c_q = c_q.min(eff[f].0.unsigned_abs() as i64);
        }
        // Second-leg notional upper bound ≤ max_hedge_notional.
        let mid_h = calc::mid(&bh);
        let px_h = mid_h.mul_rate(rfh.rr_max_cross, hft_types::RoundMode::Ceil);
        let qh_cap = calc::qty_for_cap(r.max_hedge_notional, px_h, rfh.quote_rate, &hh);
        let qf_cap = (i128::from(qh_cap) * i128::from(den) / i128::from(num.max(1))) as i64;
        c_q = c_q.min(qf_cap);
        let shrink = c.fund_shrink.min(4);
        let unshrunk = c_q;
        c_q >>= shrink;
        // Rounding unit.
        let pl_exact = hft_types::pair_lot(lf.lot.0, lh.lot.0, num, den);
        let coarse_f = {
            // coarser lot in first-leg units
            let h_in_f = (i128::from(lh.lot.0) * i128::from(den) / i128::from(num.max(1))) as i64;
            lf.lot.0.max(h_in_f).max(1)
        };
        let reducing_target = (red_f && red_h)
            || matches!(
                c.view.intent.reason,
                TargetReason::Exit | TargetReason::MarginStress | TargetReason::Hard
            );
        let unit = match pl_exact {
            Some(pl) if pl.0 > lf.lot.0.max(lh.lot.0) && reducing_target => {
                let thr = slice.max_chunk_notional.0.min(r.max_order_notional.0);
                let m = self.mark(v, lf.inst);
                if calc::notional_le(pl, m, &hf, Notional(thr)) {
                    pl.0
                } else {
                    coarse_f
                }
            }
            Some(pl) => pl.0,
            None => coarse_f,
        };
        let pair_min = self.pair_min(unit, &lf, &lh, num, den, &bf, &bh, side_f, side_h, &hf, &hh);
        let mut q = calc::floor_lot(Qty(c_q), unit);
        if shrink > 0 && q.0 < pair_min && unshrunk >= pair_min {
            q = Qty(pair_min);
        }
        // One pair_lot when a lot's notional exceeds the slice cap (still under the risk cap).
        if q.0 == 0 && unit > 0 && df.unsigned_abs() >= unit as u64 && unshrunk < unit {
            let risk_cap = match class {
                IntentClass::Open => r.max_order_notional,
                _ => r.max_reduce_notional,
            };
            if calc::usdt_ceil(Qty(unit), px, rff.quote_rate, &hf) <= i128::from(risk_cap.0)
                && calc::mul_rate(weak, slice.depth_frac).0 >= unit
            {
                q = Qty(unit);
            }
        }
        if q.0 < pair_min.max(1) {
            // Remainder below one pair: leave to the residual rules; otherwise wait for liquidity.
            if df.unsigned_abs() < pair_min as u64 {
                return;
            }
            self.noliq(cix, now, out);
            return;
        }
        let second_in_band = self.in_band(cix, h, tg[h], eff[h], v);
        let is_simul = matches!(c.view.intent.exec, ExecSpec::Simultaneous(_));
        let reduce_only_h = red_h && lh.reduce_only_cap != 0;
        let (kind, act, link) = if second_in_band {
            (ExecOrderKind::Slice, ACT_SLICE, LegLinkOpt::NONE)
        } else {
            let plan = match c.view.intent.exec {
                ExecSpec::PairOnFill(pp) => HedgePlan {
                    kind: HedgeKind::OnFill,
                    px_mode: pp.hedge_px_mode,
                    on_fail: OnFail::Retry,
                    tif: Tif::Ioc,
                    venue: lh.venue,
                    account: a,
                    side: side_h,
                    reduce_only: u8::from(reduce_only_h),
                    inst: lh.inst,
                    ratio_num: num,
                    ratio_den: den,
                    residual: pp.residual,
                    _pad: [0; 3],
                    px_param: pp.hedge_cross,
                    max_slip: pp.hedge_max_slip,
                    limit_px: Px(0),
                },
                ExecSpec::Simultaneous(sp) => HedgePlan {
                    kind: HedgeKind::Simultaneous,
                    px_mode: PxMode::Cross,
                    on_fail: match sp.on_fail {
                        hft_strategy_sdk::SimulOnFail::Reverse => OnFail::Reverse,
                        hft_strategy_sdk::SimulOnFail::Hold => OnFail::Hold,
                    },
                    tif: Tif::Ioc,
                    venue: lh.venue,
                    account: a,
                    side: side_h,
                    reduce_only: u8::from(reduce_only_h),
                    inst: lh.inst,
                    ratio_num: num,
                    ratio_den: den,
                    residual: sp.residual,
                    _pad: [0; 3],
                    px_param: slice.max_cross,
                    max_slip: slice.max_cross,
                    limit_px: Px(0),
                },
                _ => HedgePlan::NONE,
            };
            let tol = match c.view.intent.exec {
                ExecSpec::PairOnFill(pp) => pp.fix_tolerance,
                ExecSpec::Simultaneous(sp) => sp.fix_tolerance,
                _ => Rate(0),
            };
            (
                if is_simul {
                    ExecOrderKind::SimulFirst
                } else {
                    ExecOrderKind::PairFirst
                },
                if is_simul { ACT_SIMUL } else { ACT_PAIR },
                LegLinkOpt::some(LegLink {
                    group: c.group,
                    role: LinkRole::First,
                    _pad: [0; 3],
                    tolerance: tol,
                    hedge: plan,
                }),
            )
        };
        let kind = if engine_target {
            ExecOrderKind::ForceStep
        } else {
            kind
        };
        let e = Emit {
            leg: f,
            side: side_f,
            qty: q,
            px,
            mid: calc::mid(&bf),
            cap_n,
            kind,
            engine,
            class,
            reduce_only: red_f && lf.reduce_only_cap != 0,
            link,
            hedge_cap: Qty(0),
            act,
            dec_action: action::SLICE,
            dec_detail: 0,
            depth: weak,
            first_leg_ix: f as u8,
            algo: c.view.intent.exec.algo(),
            origin: None,
            target_qty: tg[f],
            cooldown_ms: slice.cooldown_ms,
        };
        self.emit(cix, e, v, now, out);
    }

    /// No slice possible for lack of depth: `Stuck{NoLiquidity}` after `stuck_after_ms`.
    pub(crate) fn noliq(&mut self, cix: usize, now: Now, out: &mut ExecOut) {
        let p = self.params(self.acct_of_cix(cix));
        let after = self.stuck_after_eff(cix);
        if let Some(c) = self.cells.get_mut(cix)
            && c.gated_since.0 == 0
        {
            c.gated_since = now.tsc;
        }
        let at = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.gated_backoff_ms)));
        let saved = self.cells.get(cix).map_or(Tsc(0), |c| c.wake_at);
        self.reg_timer(cix, ExecTimerKind::Wake, at, saved, out);
        if let Some(c) = self.cells.get_mut(cix) {
            c.wake_at = at;
        }
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if c.gated_since.0 != 0
            && now.tsc.0.saturating_sub(c.gated_since.0) >= Self::ms(&now, after)
            && matches!(
                c.view.state,
                TargetState::Working | TargetState::Stuck(StuckReason::Gated(_))
            )
        {
            self.set_state(cix, TargetState::Stuck(StuckReason::NoLiquidity), now.seq, out);
        }
    }

    /// Smallest multiple of `unit` meeting both legs' `min_qty` and `min_notional` at the touch.
    #[allow(clippy::too_many_arguments)]
    fn pair_min(
        &self,
        unit: i64,
        lf: &CellLeg,
        lh: &CellLeg,
        num: u32,
        den: u32,
        bf: &BboView,
        bh: &BboView,
        sf: Side,
        sh: Side,
        hf: &hft_refdata::InstHot,
        hh: &hft_refdata::InstHot,
    ) -> i64 {
        if unit <= 0 {
            return 0;
        }
        let pf = calc::touch(bf, sf);
        let ph = calc::touch(bh, sh);
        let mut k = 1i64;
        for _ in 0..64 {
            let q = Qty(unit.saturating_mul(k));
            let qh = Qty((i128::from(q.0) * i128::from(num) / i128::from(den.max(1))) as i64);
            let ok = q.0 >= lf.min_qty.0
                && qh.0 >= lh.min_qty.0
                && !calc::notional_le(q, pf, hf, Notional(hf.min_notional.0 - 1))
                && !calc::notional_le(qh, ph, hh, Notional(hh.min_notional.0 - 1));
            if ok {
                return q.0;
            }
            // Jump close to the bound to keep the loop short.
            k = k.saturating_add(1);
        }
        unit.saturating_mul(k)
    }

    /// Single-leg tail of a pair target: reducing the excess → `ResidualTrim`, increasing → slice.
    #[allow(clippy::too_many_arguments)]
    fn tail_step<V: Views>(
        &mut self,
        cix: usize,
        i: usize,
        t: Qty,
        x: LegNow,
        slice: &TakerSliceParams,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let eff = Qty(x.pos.0 + x.infl.0);
        let diff = t.0 - eff.0;
        let reducing = eff.0 != 0 && eff.0.signum() != diff.signum();
        let excess = reducing
            && eff.0.unsigned_abs() > t.0.unsigned_abs()
            && (t.0 == 0 || t.0.signum() == eff.0.signum());
        if excess && !self.engine_risk_target(cix) {
            let lv = v.legs(c.account, c.group);
            let naked = Qty(lv
                .leg
                .iter()
                .take(usize::from(c.n_legs))
                .map(|l| l.net.0)
                .sum::<i64>()
                .abs());
            let mut sl = *slice;
            sl.depth_frac = Rate(1_000_000_000);
            if let LegOut::Emit(mut e) =
                self.leg_slice(cix, i, t, x, &sl, ExecOrderKind::ResidualTrim, v, now)
            {
                let l = c.legs[i];
                // A Hedge-class order never exceeds the filled naked quantity (hedge_cap_qty).
                let q = calc::floor_lot(Qty(e.qty.0.min(naked.0)), l.lot.0.max(1));
                if q.0 < l.min_qty.0.max(1) {
                    return;
                }
                e.qty = q;
                e.kind = ExecOrderKind::ResidualTrim;
                e.class = IntentClass::Hedge;
                e.engine = true;
                e.hedge_cap = Qty(naked.0.saturating_add(l.lot.0));
                e.dec_action = action::RESIDUAL_TRIM;
                e.act = ACT_SLICE;
                self.emit(cix, e, v, now, out);
            }
            return;
        }
        match self.leg_slice(cix, i, t, x, slice, ExecOrderKind::Slice, v, now) {
            LegOut::Emit(e) => self.emit(cix, e, v, now, out),
            LegOut::Gate(b) => self.gate(cix, b, true, true, now, out),
            LegOut::MinOrder => self.min_order(cix, i, t, x.pos, v, now, out),
            _ => {}
        }
    }

    /// Perp-first leg index for risk targets (README "腿顺序").
    pub(crate) fn perp_first<V: Views>(&self, cix: usize, v: &V) -> usize {
        use hft_strategy_sdk::PortfolioRead;
        let Some(c) = self.cells.get(cix) else { return 0 };
        let (l0, l1) = (c.legs[0], c.legs[1]);
        match (l0.kind == LEG_PERP, l1.kind == LEG_PERP) {
            (true, false) => 0,
            (false, true) => 1,
            (true, true) => {
                let k = self.acct_of_cix(cix);
                let fv = self.acct.get(k).map_or(0, |s| s.force_venues);
                let in0 = fv & (1u64 << (l0.venue.0 & 63)) != 0;
                let in1 = fv & (1u64 << (l1.venue.0 & 63)) != 0;
                if in0 != in1 {
                    return if in0 { 0 } else { 1 };
                }
                let pf = v.portfolio(c.account);
                let (m0, m1) = (pf.margin(l0.venue).margin_ratio, pf.margin(l1.venue).margin_ratio);
                if m0 != m1 {
                    return if m0 > m1 { 0 } else { 1 };
                }
                let lv = v.legs(c.account, c.group);
                let n0 = calc::notional_ceil(lv.leg[0].net, self.mark(v, l0.inst), self.inst(l0.inst));
                let n1 = calc::notional_ceil(lv.leg[1].net, self.mark(v, l1.inst), self.inst(l1.inst));
                if n1 > n0 { 1 } else { 0 }
            }
            (false, false) => self.by_liquidity(cix, v, None),
        }
    }

    fn first_by_param<V: Views>(&self, cix: usize, v: &V, tg: &[Qty; 2], eff: &[Qty; 2]) -> usize {
        let Some(c) = self.cells.get(cix) else { return 0 };
        let pp = match c.view.intent.exec {
            ExecSpec::PairOnFill(p) => Some((p.first_leg, p.first_leg_ix)),
            _ => None,
        };
        let sides = Some([
            if tg[0].0 > eff[0].0 { Side::Buy } else { Side::Sell },
            if tg[1].0 > eff[1].0 { Side::Buy } else { Side::Sell },
        ]);
        let find = |kind: u8| (0..2).find(|&i| c.legs[i].kind == kind);
        match pp {
            Some((FirstLeg::PerpFirst, _)) => {
                find(LEG_PERP).unwrap_or_else(|| self.by_liquidity(cix, v, sides))
            }
            Some((FirstLeg::SpotFirst, _)) => {
                find(LEG_SPOT).unwrap_or_else(|| self.by_liquidity(cix, v, sides))
            }
            Some((FirstLeg::Fixed, ix)) => usize::from(ix.min(1)),
            _ => self.by_liquidity(cix, v, sides),
        }
    }

    /// Leg with the smaller visible depth notional on the side it would trade (ties: leg 0).
    fn by_liquidity<V: Views>(&self, cix: usize, v: &V, sides: Option<[Side; 2]>) -> usize {
        let Some(c) = self.cells.get(cix) else { return 0 };
        let slice = *c.view.intent.exec.slice();
        let lv = v.legs(c.account, c.group);
        let mut val = [0i128; 2];
        for (i, x) in val.iter_mut().enumerate() {
            let l = c.legs[i];
            let side = match sides {
                Some(s) => s[i],
                None => {
                    if lv.leg[i].net.0 > 0 {
                        Side::Sell
                    } else {
                        Side::Buy
                    }
                }
            };
            let levels = if self.urgency_of(cix) == Urgency::Passive {
                1
            } else {
                slice.depth_levels.max(1)
            };
            let mc = if slice.max_cross.0 > 0 {
                slice.max_cross
            } else {
                self.params(self.acct_of_cix(cix)).force_slice.max_cross
            };
            *x = calc::depth(
                &v.book(l.inst),
                &v.bbo(l.inst),
                side,
                levels,
                mc,
                self.inst(l.inst),
            )
            .1;
        }
        if val[1] < val[0] { 1 } else { 0 }
    }

    // ------------------------------------------------------------------ emission

    /// Writes one order, its decision and the cell's slice tracking.
    pub(crate) fn emit<V: Views>(&mut self, cix: usize, e: Emit, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let n = c.active.n.wrapping_add(1).max(1);
        let id = ExecOrderId { cix: cix as u16, n };
        let l = c.legs[e.leg];
        let tag = if c.has_target { c.view.intent.tag } else { 0 };
        let intent = OrderIntent {
            venue: l.venue,
            account: c.account,
            side: e.side,
            tif: Tif::Ioc,
            class: e.class,
            flags: if e.reduce_only {
                OrderIntent::REDUCE_ONLY
            } else {
                0
            },
            _pad: [0; 2],
            inst: l.inst,
            group: c.group,
            px: e.px,
            qty: e.qty,
            link: e.link,
            tag,
            max_rest_ms: 0,
        };
        let reason =
            if e.kind == ExecOrderKind::CmdHedgeFix || e.kind == ExecOrderKind::AutoFix || !c.has_target {
                TargetReason::Manual
            } else {
                c.view.intent.reason
            };
        let intent = if e.kind.is_hedge() {
            OrderIntent {
                link: LegLinkOpt::some(LegLink {
                    group: c.group,
                    role: LinkRole::HedgeFix,
                    _pad: [0; 3],
                    tolerance: Rate(0),
                    hedge: HedgePlan::NONE,
                }),
                ..intent
            }
        } else {
            intent
        };
        out.orders.push(ExecOrder {
            id,
            intent,
            origin: if e.engine {
                ExecOrigin::Engine
            } else {
                ExecOrigin::Target
            },
            kind: e.kind,
            reason,
            hedge_cap_qty: e.hedge_cap,
        });
        let signed = if e.side == Side::Sell { -e.qty.0 } else { e.qty.0 };
        if let Some(pend) = self.pend.get_mut(cix) {
            let slot = pend
                .iter()
                .position(|p| !p.valid || p.n == n.wrapping_sub(4))
                .unwrap_or(usize::from(n % 4));
            if let Some(s) = pend.get_mut(slot) {
                *s = PendOrder {
                    n,
                    leg: e.leg as u8,
                    kind: e.kind,
                    sell: e.side == Side::Sell,
                    qty: e.qty,
                    px: e.px,
                    mid: e.mid,
                    cap_n: e.cap_n,
                    valid: true,
                };
            }
        }
        let ln = self.legs_now(cix, v);
        let Some(cm) = self.cells.get_mut(cix) else { return };
        let bit = 1u8 << e.leg;
        cm.active.kind = e.act;
        cm.active.leg_ix = e.leg as u8;
        cm.active.n = n;
        cm.active.link_id = 0;
        cm.active.first = OrderRefRow::of(hft_strategy_sdk::OrderRef::new(l.venue, c.account, 0, 0));
        cm.active.planned = Qty(signed);
        cm.active.ext[0] = e.kind.code();
        cm.active.ext[1] |= bit;
        if e.side == Side::Sell {
            cm.active.ext[2] |= bit;
        } else {
            cm.active.ext[2] &= !bit;
        }
        cm.active.ext[3] &= !bit;
        cm.aux16 &= !(1 << (AUX_UNKNOWN_SHIFT as usize + e.leg));
        if cm.is_ungrouped() {
            cm.aux[e.leg] = Qty(cm.aux[e.leg].0.saturating_add(signed));
        } else {
            cm.aux[e.leg] = Qty(0);
        }
        cm.view.last_slice_tsc = now.tsc;
        cm.view.n_slices = cm.view.n_slices.saturating_add(1);
        let state = cm.view.state;
        let mut d = self.dec(cix, e.dec_action, now.seq);
        d.detail = e.dec_detail;
        d.algo = e.algo;
        if let Some(o) = e.origin {
            d.origin = hft_event::TargetOriginWire::from(o) as u8;
        }
        d.leg_ix = e.leg as u8;
        d.first_leg_ix = e.first_leg_ix;
        d.side = e.side as u8;
        d.qty = e.qty;
        d.px = e.px;
        d.target_qty = e.target_qty;
        d.pos_qty = ln[e.leg].pos;
        d.inflight_qty = ln[e.leg].infl;
        d.depth_qty = e.depth;
        d.exec_order_id = id.packed();
        out.decisions.push(d);
        let target_kind = matches!(
            e.kind,
            ExecOrderKind::Slice
                | ExecOrderKind::PairFirst
                | ExecOrderKind::SimulFirst
                | ExecOrderKind::ForceStep
        );
        if target_kind
            && matches!(
                state,
                TargetState::Stuck(StuckReason::Gated(_))
                    | TargetState::Stuck(StuckReason::StaleQuote)
                    | TargetState::Stuck(StuckReason::NoLiquidity)
            )
        {
            self.set_state(cix, TargetState::Working, now.seq, out);
        }
        if target_kind {
            let cd = self.cooldown_eff(cix, e.cooldown_ms);
            let at = Tsc(now.tsc.0.saturating_add(Self::ms(&now, cd)));
            let saved = self.cells.get(cix).map_or(Tsc(0), |c| c.wake_at);
            self.reg_timer(cix, ExecTimerKind::Wake, at, saved, out);
            if let Some(c) = self.cells.get_mut(cix) {
                c.wake_at = at;
            }
        }
    }

    // ------------------------------------------------------------------ timers

    /// Re-registers timers after state changes (`D_TIMERS`) and cancels the ones no longer needed.
    fn timer_step(&mut self, cix: usize, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if c.deferred & D_TIMERS == 0 {
            return;
        }
        if let Some(c) = self.cells.get_mut(cix) {
            c.deferred &= !D_TIMERS;
        }
        let mask = c.timer_mask();
        let has = |k: ExecTimerKind| mask & (1 << (k as u8)) != 0;
        // External
        if c.ext_deadline.0 != 0 {
            if has(ExecTimerKind::External) {
                self.force_timer(cix, ExecTimerKind::External, c.ext_deadline, out);
            } else {
                self.reg_timer(cix, ExecTimerKind::External, c.ext_deadline, Tsc(0), out);
            }
        } else if has(ExecTimerKind::External) {
            self.cancel_timer(cix, ExecTimerKind::External, out);
        }
        // AutoFix
        let ph = c.auto_fix.phase();
        if ph == AutoFixPhase::Primed && c.auto_fix.due_tsc.0 > now.tsc.0 {
            if has(ExecTimerKind::AutoFix) {
                self.force_timer(cix, ExecTimerKind::AutoFix, c.auto_fix.due_tsc, out);
            } else {
                self.reg_timer(cix, ExecTimerKind::AutoFix, c.auto_fix.due_tsc, Tsc(0), out);
            }
        } else if ph == AutoFixPhase::None && has(ExecTimerKind::AutoFix) {
            self.cancel_timer(cix, ExecTimerKind::AutoFix, out);
        }
    }

    /// Cancel + set of an already registered timer.
    pub(crate) fn force_timer(&mut self, cix: usize, kind: ExecTimerKind, at: Tsc, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix) else { return };
        let id = ExecTimerReq::id(kind, c.account);
        let group = c.group;
        out.timers.push(ExecTimerReq {
            cancel: true,
            timer_id: id,
            group,
            at: Tsc(0),
        });
        out.timers.push(ExecTimerReq {
            cancel: false,
            timer_id: id,
            group,
            at,
        });
    }
}

/// Outcome of a leg-slice computation.
#[derive(Copy, Clone, Debug)]
pub(crate) enum LegOut {
    Emit(Emit),
    Gate(RejectBits),
    Wait,
    MinOrder,
    NoLiq,
    Nothing,
}
