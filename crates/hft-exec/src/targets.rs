//! Target acceptance and replacement (README "目标的接受与替换", "两腿 lot 对齐"), `clear_target`,
//! `note_strategy_action` and `clear_account`.

use hft_strategy_sdk::{
    ExecSpec, LegTarget, Now, TakerSliceParams, TargetIntent, TargetOrigin, TargetReason, TargetReject,
    TargetState, Urgency, Views,
};
use hft_types::{AccountId, GroupId, Notional, Qty, Rate, StrategyInstanceId, Tsc};

use crate::calc;
use crate::decision::{action, eflags, external, hold_release, pack, table};
use crate::types::*;
use crate::{Exec, ExecOut};

/// An all-zero target of (a, g) (placeholder of cells without a target).
pub(crate) fn zero_intent(account: AccountId, group: GroupId) -> TargetIntent {
    TargetIntent {
        account,
        _pad0: [0; 3],
        group,
        legs: [LegTarget::ZERO; 4],
        n_legs: 0,
        reason: TargetReason::Rebalance,
        urgency: Urgency::Normal,
        _pad1: [0; 5],
        band: Rate(0),
        exec: ExecSpec::TakerSlice(ZERO_SLICE),
        tag: 0,
        _pad2: 0,
    }
}

pub(crate) const ZERO_SLICE: TakerSliceParams = TakerSliceParams {
    depth_frac: Rate(0),
    depth_levels: 0,
    _pad0: [0; 7],
    max_cross: Rate(0),
    max_chunk_notional: Notional(0),
    min_chunk_notional: Notional(0),
    cooldown_ms: 0,
    _pad1: 0,
};

fn slice_ok(p: &TakerSliceParams) -> bool {
    (1..=10).contains(&p.depth_levels) && p.depth_frac.0 > 0 && p.depth_frac.0 <= 1_000_000_000
}

/// `t` is 0, or has the sign of `r` and `|t| ≤ |r|`.
pub(crate) fn closer_or_equal(t: Qty, r: Qty) -> bool {
    t.0 == 0 || (calc::sgn(t) == calc::sgn(r) && t.0.unsigned_abs() <= r.0.unsigned_abs())
}

/// Pair ratio `(num, den)` of a spec (1/1 for non-pair algorithms).
pub(crate) fn ratio(e: &ExecSpec) -> (u32, u32) {
    match e {
        ExecSpec::PairOnFill(p) => (p.ratio_num, p.ratio_den),
        ExecSpec::MakerFirst(m) => (m.hedge.ratio_num, m.hedge.ratio_den),
        _ => (1, 1),
    }
}

pub(crate) fn is_pair(e: &ExecSpec) -> bool {
    matches!(
        e,
        ExecSpec::PairOnFill(_) | ExecSpec::Simultaneous(_) | ExecSpec::MakerFirst(_)
    )
}

impl Exec {
    /// Accepts, replaces or rejects a strategy target (README "目标的接受与替换").
    pub fn set_target<V: Views>(
        &mut self,
        owner: StrategyInstanceId,
        t: &TargetIntent,
        ro: bool,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) -> Result<(), TargetReject> {
        let cix = match self.cix_of(t.account, t.group) {
            Some(c) if t.group.0 != 0 && self.cells.get(c).is_some_and(|x| x.owner == owner) => c,
            _ => {
                self.reject_record(None, t, TargetReject::NotOwner, now, out);
                return Err(TargetReject::NotOwner);
            }
        };
        if let Err(e) = self.validate(cix, t, ro, v) {
            self.reject_record(Some(cix), t, e, now, out);
            return Err(e);
        }
        let Some(c) = self.cells.get(cix) else {
            return Err(TargetReject::NotOwner);
        };
        if c.has_target && c.view.intent == *t {
            return Ok(());
        }
        self.accept(cix, t, TargetOrigin::Strategy, v, now, out);
        Ok(())
    }

    fn validate<V: Views>(&self, cix: usize, t: &TargetIntent, ro: bool, v: &V) -> Result<(), TargetReject> {
        let c = self.cells.get(cix).ok_or(TargetReject::NotOwner)?;
        let n = usize::from(t.n_legs);
        if n == 0 || n > 4 {
            return Err(TargetReject::BadLegs);
        }
        let mut seen = 0u8;
        for l in t.legs() {
            if l.leg_ix >= c.n_legs || seen & (1 << l.leg_ix) != 0 {
                return Err(TargetReject::BadLegs);
            }
            seen |= 1 << l.leg_ix;
        }
        for l in t.legs() {
            let leg = c.legs.get(usize::from(l.leg_ix)).copied().unwrap_or(c.legs[0]);
            if leg.kind == LEG_SPOT && l.qty.0 < 0 {
                return Err(TargetReject::SpotShort);
            }
        }
        if t.legs().iter().any(|l| {
            c.legs
                .get(usize::from(l.leg_ix))
                .is_some_and(|x| x.kind == LEG_CHAIN)
        }) {
            return Err(TargetReject::SpecInvalid);
        }
        if matches!(t.exec, ExecSpec::MakerFirst(_)) {
            return Err(TargetReject::AlgoUnsupported);
        }
        if !slice_ok(t.exec.slice()) {
            return Err(TargetReject::SpecInvalid);
        }
        if let ExecSpec::PairOnFill(p) = &t.exec {
            if p.ratio_num == 0 || p.ratio_den == 0 {
                return Err(TargetReject::SpecInvalid);
            }
            if p.first_leg == hft_strategy_sdk::FirstLeg::Fixed && p.first_leg_ix >= 2 {
                return Err(TargetReject::SpecInvalid);
            }
        }
        if is_pair(&t.exec) {
            if c.n_legs > 2 || n > 2 {
                return Err(TargetReject::TooManyLegs);
            }
            if c.n_legs < 2 || n < 2 {
                return Err(TargetReject::SpecInvalid);
            }
        }
        if matches!(c.view.state, TargetState::Broken(_)) {
            return Err(TargetReject::CellBroken);
        }
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        let lv = v.legs(c.account, c.group);
        let engine = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
        let reference = |i: usize| -> Qty {
            if engine && let Some(q) = c.target_of(i) {
                return q;
            }
            lv.leg.get(i).map_or(Qty(0), |x| x.net)
        };
        if st.force_active
            && !st.flatten_lock
            && !t
                .legs()
                .iter()
                .all(|l| closer_or_equal(l.qty, reference(usize::from(l.leg_ix))))
        {
            return Err(TargetReject::ForcedReduceActive);
        }
        if st.flatten_lock
            && !t
                .legs()
                .iter()
                .all(|l| closer_or_equal(l.qty, reference(usize::from(l.leg_ix))))
        {
            return Err(TargetReject::FlattenLocked);
        }
        if ro
            && !t
                .legs()
                .iter()
                .all(|l| closer_or_equal(l.qty, lv.leg.get(usize::from(l.leg_ix)).map_or(Qty(0), |x| x.net)))
        {
            return Err(TargetReject::ReduceOnlyCmd);
        }
        if is_pair(&t.exec) && matches!(t.reason, TargetReason::Rebalance | TargetReason::Manual) {
            let increasing = t.legs().iter().any(|l| {
                let pos = lv.leg.get(usize::from(l.leg_ix)).map_or(Qty(0), |x| x.net);
                !closer_or_equal(l.qty, pos)
            });
            if increasing {
                let (num, den) = ratio(&t.exec);
                let (l0, l1) = (c.legs[0], c.legs[1]);
                let pl = hft_types::pair_lot(l0.lot.0, l1.lot.0, num, den);
                let Some(pl) = pl else {
                    return Err(TargetReject::LotMismatch);
                };
                if pl.0 > l0.lot.0.max(l1.lot.0) {
                    let mut px = v.mark(l0.inst);
                    if px.0 <= 0 {
                        let b = v.bbo(l0.inst);
                        if b.bid_px.0 > 0 && b.ask_px.0 > 0 {
                            px = calc::mid(&b);
                        }
                    }
                    if px.0 > 0 {
                        let r = v.risk(c.account);
                        let thr = t.exec.slice().max_chunk_notional.0.min(r.max_order_notional.0);
                        if !calc::notional_le(pl, px, self.inst(l0.inst), Notional(thr)) {
                            return Err(TargetReject::LotMismatch);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn reject_record(
        &self,
        cix: Option<usize>,
        t: &TargetIntent,
        e: TargetReject,
        now: Now,
        out: &mut ExecOut,
    ) {
        let mut d = match cix {
            Some(c) => self.dec(c, action::TARGET_REJECTED, now.seq),
            None => {
                let mut d = <hft_event::ExecDecision as zerocopy::FromZeros>::new_zeroed();
                d.trigger_seq = now.seq;
                d.group = t.group;
                d.account = t.account;
                d.owner = t.group.strategy();
                d.action = action::TARGET_REJECTED;
                d.origin = hft_event::TargetOriginWire::Strategy as u8;
                d.leg_ix = 0xFF;
                d.first_leg_ix = 0xFF;
                d
            }
        };
        d.detail = pack(table::TARGET_REJECT, e.code(), 0);
        d.tag = t.tag;
        d.reason = t.reason as u8;
        d.urgency = t.urgency as u8;
        out.decisions.push(d);
    }

    /// Installs `t` as the cell's target (strategy or engine path of `set_target`).
    pub(crate) fn accept<V: Views>(
        &mut self,
        cix: usize,
        t: &TargetIntent,
        origin: TargetOrigin,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) {
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        let Some(c) = self.cells.get_mut(cix) else { return };
        let replaced = c.has_target;
        let old_state = c.view.state;
        c.view.intent = *t;
        c.view.origin = origin;
        c.view.set_seq = now.seq;
        c.view.set_tsc = now.tsc;
        c.view.filled = [Qty(0); 4];
        c.view.state = TargetState::Working;
        c.has_target = true;
        c.fail_streak = 0;
        c.nofill_streak = 0;
        c.fund_shrink = 0;
        c.gated_since = Tsc(0);
        c.gated_bits = hft_types::RejectBits(0);
        c.deferred &= !D_ENGINE_CLEARED;
        if !(st.force_active || st.flatten_lock) {
            c.serial = 0;
        }
        match &t.exec {
            ExecSpec::PairOnFill(p) => c.tolerance = p.fix_tolerance,
            ExecSpec::Simultaneous(p) => c.tolerance = p.fix_tolerance,
            _ => {}
        }
        let mut d = self.dec(
            cix,
            if replaced {
                action::TARGET_REPLACED
            } else {
                action::TARGET_SET
            },
            now.seq,
        );
        d.state_from = old_state.code();
        d.state_to = TargetState::Working.code();
        if v.mode(t.account, t.group) == hft_types::RunMode::Shadow {
            d.eflags |= eflags::SHADOW;
        }
        out.decisions.push(d);
        self.clear_ext(cix, now, out);
        self.release_hold(cix, hold_release::STRATEGY, now, out);
        self.recompute_pair_lot(cix);
        // Immediately due: never later than a pending registration (set only, no cancel).
        self.reg_timer(cix, ExecTimerKind::Wake, now.tsc, Tsc(u64::MAX), out);
        if let Some(c) = self.cells.get_mut(cix) {
            c.wake_at = now.tsc;
        }
        self.mark_pending(cix, true);
    }

    /// Clears a pending external-fill timer of the cell (record `External/0/Cleared`).
    pub(crate) fn clear_ext(&mut self, cix: usize, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get_mut(cix) else { return };
        if c.ext_deadline.0 == 0 {
            return;
        }
        c.ext_deadline = Tsc(0);
        let mut d = self.dec(cix, action::EXTERNAL_SEEN, now.seq);
        d.detail = pack(table::EXTERNAL, 0, external::CLEARED);
        out.decisions.push(d);
        self.cancel_timer(cix, ExecTimerKind::External, out);
    }

    /// Clears a strategy target.
    pub fn clear_target(
        &mut self,
        owner: StrategyInstanceId,
        a: AccountId,
        g: GroupId,
        now: Now,
        out: &mut ExecOut,
    ) {
        let Some(cix) = self.cix_of(a, g).filter(|_| g.0 != 0) else {
            return;
        };
        if self.cells.get(cix).is_none_or(|c| c.owner != owner) {
            return;
        }
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        if st.flatten_lock || st.force_active {
            let e = if st.flatten_lock {
                TargetReject::FlattenLocked
            } else {
                TargetReject::ForcedReduceActive
            };
            let mut d = self.dec(cix, action::TARGET_REJECTED, now.seq);
            d.detail = pack(table::TARGET_REJECT, e.code(), 0);
            out.decisions.push(d);
            return;
        }
        let Some(c) = self.cells.get_mut(cix) else { return };
        if !c.has_target {
            return;
        }
        let from = c.view.state;
        let to = if matches!(from, TargetState::Broken(_)) {
            from
        } else {
            TargetState::Idle
        };
        let mut d = self.dec(cix, action::TARGET_CLEARED, now.seq);
        d.state_from = from.code();
        d.state_to = to.code();
        let Some(c) = self.cells.get_mut(cix) else { return };
        c.has_target = false;
        c.view.state = to;
        c.serial = 0;
        c.pair_lot = Qty(0);
        c.fail_streak = 0;
        c.nofill_streak = 0;
        c.gated_since = Tsc(0);
        c.gated_bits = hft_types::RejectBits(0);
        out.decisions.push(d);
        self.clear_ext(cix, now, out);
    }

    /// The strategy acted on (a, g) itself (`Ctx::place` or a chain intent): the external fill is
    /// handled and `manual_hold` released.
    pub fn note_strategy_action(&mut self, a: AccountId, g: GroupId, now: Now, out: &mut ExecOut) {
        let Some(cix) = self.cix_of(a, g).filter(|_| g.0 != 0) else {
            return;
        };
        self.clear_ext(cix, now, out);
        if self
            .cells
            .get(cix)
            .is_some_and(|c| c.view.state == TargetState::Stuck(hft_strategy_sdk::StuckReason::External))
        {
            self.set_state(cix, TargetState::Working, now.seq, out);
        }
        self.release_hold(cix, hold_release::STRATEGY, now, out);
        self.mark_pending(cix, true);
    }

    /// Clears every target and execution-layer timer of an account (`SimReset`, account removal);
    /// records are written by the next `advance`. `flatten_lock` is kept.
    pub fn clear_account(&mut self, a: AccountId, _now: Now, _out: &mut ExecOut) {
        let Some(k) = self.acct_ix(a) else { return };
        for cix in 0..self.cell_count() {
            let Some(c) = self.cells.get_mut(cix) else {
                continue;
            };
            if c.account != a {
                continue;
            }
            if c.has_target {
                c.has_target = false;
                c.deferred |= D_CLEARED_BY_CMD;
                if !matches!(c.view.state, TargetState::Broken(_)) && c.view.state != TargetState::Idle {
                    if c.deferred & D_STATE == 0 {
                        c.def_from = c.view.state.code();
                    }
                    c.deferred |= D_STATE;
                    c.view.state = TargetState::Idle;
                }
            }
            c.fix = FixState::ZERO;
            if c.ext_deadline.0 != 0 {
                c.ext_deadline = Tsc(0);
                c.deferred |= D_EXT_CLR;
            }
            c.auto_fix = <AutoFixState as zerocopy::FromZeros>::new_zeroed();
            if c.hold & MANUAL_HOLD != 0 {
                c.deferred |= D_HOLD_REL;
            }
            c.hold &= LINK_HOLD;
            c.serial = 0;
            c.pair_lot = Qty(0);
            c.fail_streak = 0;
            c.nofill_streak = 0;
            self.mark_pending(cix, true);
        }
        let _ = k;
    }

    /// Recomputes `pair_lot` (leg 0 as first leg) of a pair target.
    pub(crate) fn recompute_pair_lot(&mut self, cix: usize) {
        let Some(c) = self.cells.get_mut(cix) else { return };
        c.pair_lot = Qty(0);
        if !c.has_target || !is_pair(&c.view.intent.exec) || c.n_legs != 2 {
            return;
        }
        let (num, den) = ratio(&c.view.intent.exec);
        if let Some(p) = hft_types::pair_lot(c.legs[0].lot.0, c.legs[1].lot.0, num, den) {
            c.pair_lot = p;
        }
    }
}
