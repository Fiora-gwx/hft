//! Registration entries: order updates, place results, link notices, timers, risk events, mode
//! changes, market touches and leg adjustments.

use hft_event::NoteLevel;
use hft_strategy_sdk::{
    BrokenReason, CapScopeView, EngineOrigin, ExecUpdateKind, MarginLevel, Now, OrderUpdate, OrderUpdateKind,
    RiskEvent, StuckReason, TargetOrigin, TargetState, Views,
};
use hft_types::{AccountId, CapSource, InstId, Qty, RejectBits, RunMode, Tsc};

use crate::advance::RO_BITS;
use crate::decision::{acct_group, action, engine, note, pack, table};
use crate::types::*;
use crate::{Exec, ExecOut, PendOrder};

/// What ended an execution-layer slice.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum End {
    /// Terminal (with or without fills).
    Done,
    /// Exchange reject or counted local send failure.
    Fail,
    /// Uncounted local send failure (gate).
    Gate,
}

fn is_target_kind(k: ExecOrderKind) -> bool {
    matches!(
        k,
        ExecOrderKind::Slice
            | ExecOrderKind::PairFirst
            | ExecOrderKind::SimulFirst
            | ExecOrderKind::ForceStep
    )
}

impl Exec {
    fn leg_of_venue(&self, cix: usize, venue: hft_types::VenueId) -> Option<usize> {
        let c = self.cells.get(cix)?;
        (0..usize::from(c.n_legs)).find(|&i| c.legs[i].venue == venue)
    }

    fn pend_of(&self, cix: usize, n: u16) -> Option<PendOrder> {
        self.pend.get(cix)?.iter().find(|p| p.valid && p.n == n).copied()
    }

    /// Clears the slice tracking of leg `i`; returns the slice's signed fill.
    fn untrack(&mut self, cix: usize, i: usize) -> Qty {
        let Some(c) = self.cells.get_mut(cix) else {
            return Qty(0);
        };
        let bit = 1u8 << i;
        c.active.ext[1] &= !bit;
        c.active.ext[3] &= !bit;
        c.aux16 &= !(1 << (AUX_UNKNOWN_SHIFT as usize + i));
        if let Some(sl) = c.slot.get_mut(i) {
            *sl = 0;
        }
        let cum = if c.is_ungrouped() {
            // own_infl keeps only the released Unknown remainder.
            c.aux[i] = c.view.filled[i];
            Qty(0)
        } else {
            let x = c.aux[i];
            c.aux[i] = Qty(0);
            x
        };
        if c.active.ext[1] == 0 && !matches!(c.active.kind, ACT_PAIR | ACT_SIMUL) {
            c.active.kind = ACT_NONE;
            c.active.ext[0] = 0;
        }
        cum
    }

    /// One `OrderUpdate` of an EXEC slot.
    pub fn on_order_update<V: Views>(&mut self, u: &OrderUpdate, v: &V, now: Now, out: &mut ExecOut) {
        let Some(cix) = self.cix_of(u.r.account, u.group) else {
            return;
        };
        let Some(i) = self.leg_of_venue(cix, u.r.venue) else {
            return;
        };
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let bit = 1u8 << i;
        let slot = c.slot.get(i).copied().unwrap_or(0);
        let tracked = c.active.ext[1] & bit != 0 && (!c.is_ungrouped() || slot == 0 || slot == u.r.slot);
        let kind = c.active.order_kind().unwrap_or(ExecOrderKind::Slice);
        let sell = c.active.ext[2] & bit != 0;
        let end = match u.kind {
            OrderUpdateKind::Filled { d, done, .. } => {
                // An untracked fill of the ungrouped cell settles the released remainder (same sign).
                let untracked_neg = !tracked && c.is_ungrouped() && c.aux[i].0 < 0;
                let sd = if sell || untracked_neg {
                    -d.0.abs()
                } else {
                    d.0.abs()
                };
                if let Some(cm) = self.cells.get_mut(cix) {
                    if cm.is_ungrouped() {
                        cm.aux[i] = Qty(cm.aux[i].0.saturating_sub(sd));
                        if !tracked {
                            cm.view.filled[i] = Qty(cm.view.filled[i].0.saturating_sub(sd));
                        }
                    } else {
                        cm.view.filled[i] = Qty(cm.view.filled[i].0.saturating_add(sd));
                        if tracked {
                            cm.aux[i] = Qty(cm.aux[i].0.saturating_add(sd));
                        }
                    }
                    if tracked {
                        cm.active.ext[3] |= bit;
                        cm.nofill_streak = 0;
                        cm.fail_streak = 0;
                        if kind == ExecOrderKind::AutoFix {
                            cm.auto_fix.done_qty = Qty(cm.auto_fix.done_qty.0.saturating_add(d.0.abs()));
                        }
                    }
                }
                if done { Some(End::Done) } else { None }
            }
            OrderUpdateKind::Cancelled => Some(End::Done),
            OrderUpdateKind::Rejected { bits, venue_code } => {
                if bits.0 & RejectBits::LOCAL_SEND_FAIL.0 != 0 {
                    match venue_code {
                        2 | 3 | 5 | 7 => Some(End::Gate),
                        6 => Some(End::Fail),
                        _ => None,
                    }
                } else {
                    Some(End::Fail)
                }
            }
            OrderUpdateKind::Unknown => {
                if tracked && let Some(cm) = self.cells.get_mut(cix) {
                    cm.aux16 |= 1 << (AUX_UNKNOWN_SHIFT as usize + i);
                }
                None
            }
            _ => None,
        };
        let Some(end) = end else {
            self.mark_pending(cix, true);
            return;
        };
        if !tracked {
            if let Some(cm) = self.cells.get_mut(cix)
                && cm.is_ungrouped()
            {
                cm.aux[i] = Qty(cm.aux[i].0.saturating_sub(cm.view.filled[i].0));
                cm.view.filled[i] = Qty(0);
            }
            self.mark_pending(cix, true);
            return;
        }
        let had_fill = self.cells.get(cix).is_some_and(|c| c.active.ext[3] & bit != 0);
        if kind == ExecOrderKind::AutoFix
            && let Some(cm) = self.cells.get_mut(cix)
        {
            cm.auto_fix.n_slices = cm.auto_fix.n_slices.saturating_add(1);
        }
        let cum = self.untrack(cix, i);
        if had_fill && matches!(c.active.kind, ACT_PAIR | ACT_SIMUL) {
            // The link (OMS) now owns the second leg; `LinkNotice{Done}` ends the slice.
        } else if matches!(c.active.kind, ACT_PAIR | ACT_SIMUL)
            && let Some(cm) = self.cells.get_mut(cix)
        {
            cm.active.kind = ACT_NONE;
            cm.active.ext[0] = 0;
        }
        if cum.0 != 0 {
            let origin = if kind == ExecOrderKind::AutoFix {
                Some(TargetOrigin::Engine(EngineOrigin::AutoFix))
            } else {
                None
            };
            self.notify(
                cix,
                ExecUpdateKind::Progress {
                    leg_ix: i as u8,
                    d: cum,
                },
                origin,
                out,
            );
        }
        let p = self.params(self.acct_of_cix(cix));
        match end {
            End::Done if !had_fill => {
                if let Some(cm) = self.cells.get_mut(cix) {
                    cm.nofill_streak = cm.nofill_streak.saturating_add(1);
                    if kind.is_fix() {
                        cm.fix.count = cm.fix.count.saturating_add(1);
                    }
                }
                let nf = self.cells.get(cix).map_or(0, |c| c.nofill_streak);
                if nf >= p.max_nofill {
                    let at = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.stuck_after_ms)));
                    let saved = self.cells.get(cix).map_or(Tsc(0), |c| c.wake_at);
                    self.reg_timer(cix, ExecTimerKind::Wake, at, saved, out);
                    if let Some(cm) = self.cells.get_mut(cix) {
                        cm.wake_at = at;
                    }
                    if is_target_kind(kind) && !self.is_broken(cix) {
                        self.set_state(cix, TargetState::Stuck(StuckReason::NoLiquidity), now.seq, out);
                    }
                }
            }
            End::Done => {}
            End::Fail => {
                if kind == ExecOrderKind::AutoFix {
                    let n = self.naked(cix, v).0.unsigned_abs();
                    self.autofix_fail(cix, AutoFixFail::Rejected, RejectBits(0), n, now, out);
                    self.mark_pending(cix, true);
                    return;
                }
                self.count_fail(cix, kind, now, out);
            }
            End::Gate => {
                let target = is_target_kind(kind);
                self.gate(cix, RejectBits::LOCAL_SEND_FAIL, true, target, now, out);
            }
        }
        if kind == ExecOrderKind::AutoFix
            && end == End::Done
            && self
                .cells
                .get(cix)
                .is_some_and(|c| c.auto_fix.phase() == AutoFixPhase::Running)
        {
            self.autofix_after_slice(cix, v, now, out);
        }
        self.mark_pending(cix, true);
    }

    /// Ends the attempt right after its slice when the group reached dust (or the budget is used).
    fn autofix_after_slice<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let lv = v.legs(c.account, c.group);
        let quiet = !lv.has_pending
            && !lv.has_unknown
            && lv
                .leg
                .iter()
                .take(usize::from(c.n_legs))
                .all(|l| l.inflight_immediate.0 == 0);
        let naked = self.naked(cix, v).0.unsigned_abs();
        if !self.naked_above_dust(cix, v) {
            if quiet {
                self.autofix_done(cix, naked, now, out);
            }
        } else if c.auto_fix.done_qty.0 >= c.auto_fix.budget.0 {
            self.autofix_fail(cix, AutoFixFail::Residual, RejectBits(0), naked, now, out);
        }
    }

    /// Execution failure of a slice: streaks, repair count, `Stuck{SliceFails}`.
    fn count_fail(&mut self, cix: usize, kind: ExecOrderKind, now: Now, out: &mut ExecOut) {
        let p = self.params(self.acct_of_cix(cix));
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.fail_streak = cm.fail_streak.saturating_add(1);
            if kind.is_fix() {
                cm.fix.count = cm.fix.count.saturating_add(1);
            }
        }
        let fs = self.cells.get(cix).map_or(0, |c| c.fail_streak);
        if is_target_kind(kind)
            && fs >= p.max_slice_fails
            && !self.is_broken(cix)
            && self
                .cells
                .get(cix)
                .is_some_and(|c| c.has_target && c.view.state != TargetState::Stuck(StuckReason::SliceFails))
        {
            self.set_state(cix, TargetState::Stuck(StuckReason::SliceFails), now.seq, out);
            self.stuck_note(cix, now, out);
        }
    }

    /// Result of one `ExecOrder` in the intent pipeline (same event).
    pub fn on_place_result(&mut self, id: ExecOrderId, r: PlaceResult, now: Now, out: &mut ExecOut) {
        let cix = usize::from(id.cix);
        let Some(po) = self.pend_of(cix, id.n) else { return };
        if let Some(pend) = self.pend.get_mut(cix)
            && let Some(x) = pend.iter_mut().find(|p| p.valid && p.n == id.n)
        {
            x.valid = false;
        }
        let i = usize::from(po.leg);
        let kind = po.kind;
        match r {
            PlaceResult::Placed(rf) => {
                if let Some(cm) = self.cells.get_mut(cix) {
                    if cm.active.n == id.n {
                        cm.active.first = OrderRefRow::of(rf);
                    }
                    if cm.is_ungrouped()
                        && let Some(sl) = cm.slot.get_mut(i)
                    {
                        *sl = rf.slot;
                    }
                    if matches!(kind, ExecOrderKind::PairFirst | ExecOrderKind::SimulFirst) {
                        cm.fund_shrink = 0;
                    }
                }
                if is_target_kind(kind) || kind == ExecOrderKind::AutoFix {
                    let st = self.cells.get(cix).map(|c| c.view.state);
                    if let Some(cm) = self.cells.get_mut(cix) {
                        cm.gated_since = Tsc(0);
                        cm.gated_bits = RejectBits(0);
                    }
                    if is_target_kind(kind)
                        && matches!(
                            st,
                            Some(TargetState::Stuck(StuckReason::Gated(_)))
                                | Some(TargetState::Stuck(StuckReason::StaleQuote))
                        )
                    {
                        self.set_state(cix, TargetState::Working, now.seq, out);
                    }
                }
            }
            PlaceResult::Shadowed => {
                self.drop_slice(cix, i, po);
                if is_target_kind(kind) && !self.is_broken(cix) {
                    self.set_state(cix, TargetState::Stuck(StuckReason::Shadow), now.seq, out);
                }
            }
            PlaceResult::Rejected(bits) => {
                self.drop_slice(cix, i, po);
                self.rejected(cix, po, bits, now, out);
            }
        }
        self.mark_pending(cix, true);
    }

    /// Removes a slice that never reached the OMS.
    fn drop_slice(&mut self, cix: usize, i: usize, po: PendOrder) {
        if let Some(cm) = self.cells.get_mut(cix) {
            let bit = 1u8 << i;
            if cm.active.n == po.n || cm.active.ext[1] & bit != 0 {
                cm.active.ext[1] &= !bit;
                if cm.is_ungrouped() {
                    let signed = if po.sell { -po.qty.0 } else { po.qty.0 };
                    cm.aux[i] = Qty(cm.aux[i].0.saturating_sub(signed));
                }
                if cm.active.ext[1] == 0 {
                    cm.active.kind = ACT_NONE;
                    cm.active.ext[0] = 0;
                }
            }
        }
    }

    fn rejected(&mut self, cix: usize, po: PendOrder, bits: RejectBits, now: Now, out: &mut ExecOut) {
        let kind = po.kind;
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let engine = !matches!(
            kind,
            ExecOrderKind::Slice | ExecOrderKind::PairFirst | ExecOrderKind::SimulFirst
        ) || self.engine_orders(cix);
        if bits.0 & RejectBits::HEDGE_LIMIT.0 != 0 && kind.is_hedge() {
            if kind == ExecOrderKind::AutoFix {
                let n = c
                    .auto_fix
                    .budget
                    .0
                    .saturating_sub(c.auto_fix.done_qty.0)
                    .unsigned_abs();
                self.autofix_fail(cix, AutoFixFail::RiskReject, bits, n, now, out);
            } else {
                self.enter_broken(cix, BrokenReason::HedgeVeto, 0, true, now, out);
            }
            return;
        }
        if engine && bits.0 & RO_BITS != 0 {
            if kind == ExecOrderKind::AutoFix {
                let n = c
                    .auto_fix
                    .budget
                    .0
                    .saturating_sub(c.auto_fix.done_qty.0)
                    .unsigned_abs();
                self.autofix_fail(cix, AutoFixFail::RiskReject, bits, n, now, out);
                return;
            }
            let act = match kind {
                ExecOrderKind::HedgeFix => action::HEDGE_FIX,
                ExecOrderKind::ResidualTrim => action::RESIDUAL_TRIM,
                ExecOrderKind::ExternalDefault => action::EXTERNAL_DEFAULT,
                ExecOrderKind::CmdHedgeFix => action::CMD_HEDGE_FIX,
                _ => action::FORCE_STEP,
            };
            let mut d = self.dec(cix, act, now.seq);
            d.detail = pack(table::ENGINE, engine::RISK_REJECT, 0);
            d.gate = bits.0;
            d.leg_ix = po.leg;
            d.qty = po.qty;
            d.px = po.px;
            out.decisions.push(d);
            let n = self.note(
                cix,
                note::EXEC_ENGINE_ORDER_REJECTED,
                NoteLevel::P0,
                [
                    acct_group(c.account.0, c.group.0),
                    u64::from(po.leg),
                    u64::from(kind.code()),
                    u64::from(bits.0),
                    po.qty.0 as u64,
                    po.px.0 as u64,
                    po.mid.0 as u64,
                    po.cap_n.0 as u64,
                ],
            );
            out.notes.push(n);
            self.count_fail(cix, kind, now, out);
            return;
        }
        // Gated: no count, backoff.
        if bits.0 & RejectBits::LINK_FUNDS.0 != 0
            && let Some(cm) = self.cells.get_mut(cix)
        {
            cm.fund_shrink = (cm.fund_shrink + 1).min(4);
        }
        if kind == ExecOrderKind::AutoFix {
            let mut d = self.dec(cix, action::AUTO_FIX, now.seq);
            d.origin = hft_event::TargetOriginWire::EngineAutoFix as u8;
            d.detail = pack(table::AUTO_FIX, crate::decision::autofix::SLICE, 0);
            d.gate = bits.0;
            d.leg_ix = po.leg;
            out.decisions.push(d);
        }
        self.gate(cix, bits, true, is_target_kind(kind), now, out);
    }

    /// Link state change from the OMS.
    pub fn on_link(&mut self, l: &LinkNotice, now: Now, out: &mut ExecOut) {
        let Some(cix) = self.cix_of(l.account, l.group) else {
            return;
        };
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let matched = matches!(c.active.kind, ACT_PAIR | ACT_SIMUL)
            && (c.active.link_id == l.link_id || c.active.first.get() == l.first || c.active.link_id == 0);
        let terminal = matches!(l.state, LinkPhase::Done | LinkPhase::Broken);
        if let Some(cm) = self.cells.get_mut(cix) {
            if l.tolerance.0 != 0 {
                cm.tolerance = l.tolerance;
            }
            let code: u16 = match l.residual {
                hft_strategy_sdk::ResidualPolicy::Defer => 1,
                hft_strategy_sdk::ResidualPolicy::TrimExcess => 2,
                hft_strategy_sdk::ResidualPolicy::Hold => 3,
            };
            cm.aux16 = (cm.aux16 & !(3 << AUX_RESID_SHIFT)) | (code << AUX_RESID_SHIFT);
            if terminal || l.flags & LinkNotice::RESUME_HOLD == 0 {
                cm.hold &= !LINK_HOLD;
            }
            if matched {
                cm.active.link_id = l.link_id;
                if terminal {
                    cm.active.kind = ACT_NONE;
                    cm.active.ext[0] = 0;
                    cm.active.ext[1] = 0;
                    cm.active.link_id = 0;
                }
            }
        }
        if l.state == LinkPhase::Broken {
            let (reason, arg) = match l.broken {
                LinkBrokenCause::HedgeLimit => (BrokenReason::HedgeVeto, 0),
                LinkBrokenCause::ReverseFailed => (BrokenReason::ReverseFailed, 0),
                x => (BrokenReason::LinkBroken, x as u16),
            };
            self.enter_broken(cix, reason, arg, true, now, out);
        }
        self.mark_pending(cix, true);
    }

    /// `Timer{owner = Exec}` fired (`token = timer_id | group << 32`).
    pub fn on_timer(&mut self, token: u64, now: Now, out: &mut ExecOut) {
        self.tsc_per_ms = now.tsc_per_ms;
        let kind = ExecTimerKind::from_code((token & 0xFF) as u8);
        let account = AccountId(((token >> 8) & 0xFF) as u8);
        let group = hft_types::GroupId((token >> 32) as u32);
        let Some(kind) = kind else { return };
        if kind == ExecTimerKind::Continue {
            self.continue_set = false;
            return;
        }
        let Some(cix) = self.cix_of(account, group) else {
            return;
        };
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.aux16 &= !(1u16 << (kind as u16));
        }
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let early = |t: Tsc| t.0 != 0 && now.tsc.0 < t.0;
        match kind {
            ExecTimerKind::Wake => {
                if early(c.wake_at) {
                    self.reg_timer(cix, kind, c.wake_at, Tsc(0), out);
                } else {
                    self.mark_pending(cix, false);
                }
            }
            ExecTimerKind::Fix => {
                if early(c.fix.next_tsc) {
                    self.reg_timer(cix, kind, c.fix.next_tsc, Tsc(0), out);
                } else {
                    self.mark_pending(cix, true);
                }
            }
            ExecTimerKind::External => {
                if c.ext_deadline.0 == 0 {
                    return;
                }
                if early(c.ext_deadline) {
                    self.reg_timer(cix, kind, c.ext_deadline, Tsc(0), out);
                } else {
                    self.mark_pending(cix, true);
                }
            }
            ExecTimerKind::AutoFix => {
                if c.auto_fix.phase() == AutoFixPhase::Primed && early(c.auto_fix.due_tsc) {
                    self.reg_timer(cix, kind, c.auto_fix.due_tsc, Tsc(0), out);
                } else {
                    self.mark_pending(cix, true);
                }
            }
            ExecTimerKind::Alarm => match self.alarm_plan(cix) {
                Some((_, at, _)) if now.tsc.0 < at.0 => self.reg_timer(cix, kind, at, Tsc(0), out),
                _ => {
                    if let Some(cm) = self.cells.get_mut(cix) {
                        cm.deferred |= D_ALARM;
                    }
                    self.mark_pending(cix, true);
                }
            },
            ExecTimerKind::Continue => {}
        }
    }

    /// Risk event of account `a` (multi-cell entry: state only, records in `advance`).
    pub fn on_risk(&mut self, a: AccountId, e: &RiskEvent, now: Now) {
        let Some(k) = self.acct_ix(a) else { return };
        let p = self.params(k);
        match *e {
            RiskEvent::MarginStress { level, .. } => {
                if level < MarginLevel::L3 {
                    let was = self.acct.get(k).is_some_and(|s| s.force_active);
                    if let Some(st) = self.acct.get_mut(k) {
                        st.force_active = false;
                        st.force_venues = 0;
                        if st.force_kind == FORCE_VENUES {
                            st.force_kind = FORCE_NONE;
                        }
                    }
                    if was {
                        for cix in 0..self.cell_count() {
                            if let Some(c) = self.cells.get_mut(cix)
                                && c.account == a
                                && c.has_target
                                && c.view.origin == TargetOrigin::Engine(EngineOrigin::MarginStress)
                                && c.view.intent.reason == hft_strategy_sdk::TargetReason::MarginStress
                            {
                                c.deferred |= D_ENGINE_CLEARED;
                            }
                        }
                    }
                }
                self.mark_account(k, true);
            }
            RiskEvent::HedgeBroken { group } => self.cap_broken(a, group, true, &p, now),
            RiskEvent::CapSet { source, scope } => {
                if source == CapSource::BROKEN {
                    self.cap_broken(a, scope.group, true, &p, now);
                } else if source == CapSource::REORG {
                    self.cap_flag(a, scope, CAP_REORG, true);
                } else {
                    self.mark_account(k, true);
                }
            }
            RiskEvent::CapCleared { source, scope } => {
                if source == CapSource::BROKEN {
                    self.cap_broken(a, scope.group, false, &p, now);
                } else if source == CapSource::REORG {
                    self.cap_flag(a, scope, CAP_REORG, false);
                } else if source == CapSource::KILL
                    && let Some(st) = self.acct.get_mut(k)
                {
                    st.flatten_lock = false;
                }
                for cix in 0..self.cell_count() {
                    if let Some(c) = self.cells.get_mut(cix)
                        && c.account == a
                    {
                        c.wake_at = Tsc(0);
                    }
                }
                self.mark_account(k, true);
            }
            _ => self.mark_account(k, true),
        }
    }

    fn cap_flag(&mut self, a: AccountId, scope: CapScopeView, bit: u8, set: bool) {
        if let Some(cix) = self.cix_of(a, scope.group).filter(|_| scope.group.0 != 0) {
            if let Some(c) = self.cells.get_mut(cix) {
                if set {
                    c.cap_block |= bit;
                } else {
                    c.cap_block &= !bit;
                }
            }
            self.mark_pending(cix, true);
        }
    }

    /// `CapSet{Broken}` / `CapCleared{Broken}` of a group.
    fn cap_broken(&mut self, a: AccountId, g: hft_types::GroupId, set: bool, p: &ExecParams, now: Now) {
        let Some(cix) = self.cix_of(a, g).filter(|_| g.0 != 0) else {
            return;
        };
        let Some(c) = self.cells.get_mut(cix) else { return };
        if set {
            c.cap_block |= CAP_BROKEN;
            if !matches!(c.view.state, TargetState::Broken(_)) {
                if c.deferred & D_STATE == 0 {
                    c.def_from = c.view.state.code();
                }
                c.deferred |= D_STATE | D_TIMERS;
                c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
                c.fix = FixState::ZERO;
                Self::arm_broken(c, p, now);
            }
        } else {
            c.cap_block &= !CAP_BROKEN;
            if matches!(c.view.state, TargetState::Broken(_)) {
                if c.deferred & D_STATE == 0 {
                    c.def_from = c.view.state.code();
                }
                c.deferred |= D_STATE | D_TIMERS;
                c.auto_fix = <AutoFixState as zerocopy::FromZeros>::new_zeroed();
                c.broken_since = Tsc(0);
                let engine_target = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
                if engine_target {
                    c.view.state = TargetState::Working;
                } else {
                    if c.has_target {
                        c.has_target = false;
                        c.pair_lot = Qty(0);
                        c.deferred |= D_CLEARED_BY_CMD;
                    }
                    c.view.state = TargetState::Idle;
                }
            }
        }
        self.mark_pending(cix, true);
    }

    /// Effective mode of (a, g) changed.
    pub fn on_mode(
        &mut self,
        a: AccountId,
        g: hft_types::GroupId,
        _old: RunMode,
        new: RunMode,
        now: Now,
        out: &mut ExecOut,
    ) {
        let Some(cix) = self.cix_of(a, g) else { return };
        if let Some(c) = self.cells.get_mut(cix) {
            c.wake_at = Tsc(0);
        }
        if new == RunMode::Live
            && self
                .cells
                .get(cix)
                .is_some_and(|c| c.view.state == TargetState::Stuck(StuckReason::Shadow))
        {
            self.set_state(cix, TargetState::Working, now.seq, out);
        }
        self.mark_pending(cix, true);
    }

    /// Market data of `inst` changed: active cells referencing it become pending.
    pub fn touch_inst(&mut self, inst: InstId) {
        let lo = self.inst_ix.partition_point(|x| x.0 < inst);
        let mut j = lo;
        while let Some(&(i, cix)) = self.inst_ix.get(j) {
            if i != inst {
                break;
            }
            let cix = usize::from(cix);
            if self.cells.get(cix).is_some_and(Self::live) {
                self.mark_pending(cix, false);
            }
            j += 1;
        }
        for k in 0..self.accts.len() {
            let u = self.n_grouped + k;
            if self.cells.get(u).is_some_and(|c| {
                c.has_target && c.legs.iter().take(usize::from(c.n_legs)).any(|l| l.inst == inst)
            }) {
                self.mark_pending(u, false);
            }
        }
    }

    fn live(c: &TargetCell) -> bool {
        c.has_target
            || c.view.state != TargetState::Idle
            || c.ext_deadline.0 != 0
            || c.fix.first_tsc.0 != 0
            || c.auto_fix.phase != 0
            || c.deferred != 0
    }

    /// `FeeOnly` net adjustment, `AdoptExchange` or `WriteOffResidual` changed the legs of (a, g).
    pub fn on_leg_adjust(&mut self, a: AccountId, g: hft_types::GroupId) {
        if let Some(cix) = self.cix_of(a, g) {
            self.mark_pending(cix, true);
        }
    }
}
