//! Repair of the excess leg (README "补腿"), residuals (README "残余与尘埃"), the external-fill
//! default order, `hedge_fix_cmd` and `resolve_broken`.

use hft_event::NoteLevel;
use hft_strategy_sdk::{
    BrokenReason, EngineOrigin, ExecSpec, IntentClass, LegLinkOpt, Now, ResidualPolicy, TargetOrigin,
    TargetState, Views,
};
use hft_types::{AccountId, GroupId, Notional, Qty, Rate, Side, Tsc};

use crate::advance::Emit;
use crate::calc;
use crate::decision::{acct_group, action, algo, hold_release, note};
use crate::external::AUX_EXT_DEFAULT;
use crate::types::*;
use crate::{Exec, ExecOut};

/// Why a repair order cannot be built.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum FixBuild {
    Quote,
    MinOrder,
    Chain,
}

impl Exec {
    /// Repair threshold of the cell.
    pub(crate) fn tol(&self, cix: usize) -> Rate {
        let p = self.params(self.acct_of_cix(cix));
        match self.cells.get(cix) {
            Some(c) if c.tolerance.0 != 0 => c.tolerance,
            _ => p.fix_tolerance_default,
        }
    }

    /// Already-filled naked notional of the excess leg is above the dust line.
    pub(crate) fn naked_above_dust<V: Views>(&self, cix: usize, v: &V) -> bool {
        let Some(c) = self.cells.get(cix) else {
            return false;
        };
        let (sum, e) = self.naked(cix, v);
        if sum == 0 {
            return false;
        }
        let l = c.legs[e];
        let p = self.params(self.acct_of_cix(cix));
        !calc::notional_le(Qty(sum), self.mark(v, l.inst), self.inst(l.inst), p.dust_notional)
    }

    /// Repair trigger (README "补腿" 触发 1): above dust and naked ratio above the tolerance.
    pub(crate) fn fix_trigger<V: Views>(&self, cix: usize, v: &V) -> bool {
        let Some(c) = self.cells.get(cix) else {
            return false;
        };
        if c.is_ungrouped() || !self.naked_above_dust(cix, v) {
            return false;
        }
        let lv = v.legs(c.account, c.group);
        let (sum, _) = self.naked(cix, v);
        let maxabs = lv
            .leg
            .iter()
            .take(usize::from(c.n_legs))
            .map(|l| l.net.0.unsigned_abs())
            .max()
            .unwrap_or(0);
        let tol = self.tol(cix);
        i128::from(sum.unsigned_abs()) * 1_000_000_000 > i128::from(tol.0) * i128::from(maxabs)
    }

    /// The group is quiescent for repair (no link, Unknown, immediate in-flight, active slice).
    fn fix_quiet<V: Views>(&self, cix: usize, v: &V) -> bool {
        let Some(c) = self.cells.get(cix) else {
            return false;
        };
        let lv = v.legs(c.account, c.group);
        lv.open_links == 0
            && !lv.has_unknown
            && lv
                .leg
                .iter()
                .take(usize::from(c.n_legs))
                .all(|l| l.inflight_immediate.0 == 0)
            && c.active.kind == ACT_NONE
            && c.active.ext[1] == 0
    }

    pub(crate) fn fix_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if c.is_ungrouped() || matches!(c.view.state, TargetState::Broken(_)) {
            return;
        }
        let k = self.acct_of_cix(cix);
        let p = self.params(k);
        if !self.fix_trigger(cix, v) {
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.aux16 &= !AUX_EXT_DEFAULT;
                if cm.fix != FixState::ZERO {
                    cm.fix = FixState::ZERO;
                }
            }
            self.residual_step(cix, v, now, out);
            return;
        }
        if v.risk(c.account).hard {
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.fix = FixState::ZERO;
            }
            return;
        }
        let ext_default = c.aux16 & AUX_EXT_DEFAULT != 0;
        if !self.fix_quiet(cix, v)
            || c.cap_block != 0
            || c.serial != 0
            || (c.ext_deadline.0 != 0 && !ext_default)
            || c.hold & MANUAL_HOLD != 0
        {
            return;
        }
        // A capacity continuation (or restore) can revisit a rejected target before its wake.
        // Do not start a new repair window during that backoff; an existing window still expires.
        // A target gated by the exposure gate (#9) is the exception: the gate stays closed while
        // the group is naked, so its backoff would hold the repair off for good (the repair is
        // risk-reducing and passes #9).
        let exposure_gated = c.gated_bits.0 & hft_types::RejectBits::EXPOSURE_GATE.0 != 0;
        if c.fix.first_tsc.0 == 0 && now.tsc.0 < c.wake_at.0 && !exposure_gated {
            return;
        }
        let (_, e) = self.naked(cix, v);
        if !self.venue_open(cix, c.legs[e].venue) {
            if let Some(cm) = self.cells.get_mut(cix)
                && cm.fix.first_tsc.0 == 0
            {
                cm.fix.first_tsc = now.tsc;
            }
            return;
        }
        if let Some(cm) = self.cells.get_mut(cix)
            && cm.fix.first_tsc.0 == 0
        {
            cm.fix.first_tsc = now.tsc;
        }
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if c.fix.count >= p.fix_max
            || now.tsc.0.saturating_sub(c.fix.first_tsc.0) > Self::ms(&now, p.fix_deadline_ms)
        {
            self.enter_broken(cix, BrokenReason::FixExhausted, 0, true, now, out);
            return;
        }
        if c.fix.next_tsc.0 != 0 && now.tsc.0 < c.fix.next_tsc.0 {
            let t = c.fix.next_tsc;
            self.reg_timer(cix, ExecTimerKind::Fix, t, t, out);
            return;
        }
        if now.tsc.0 < c.wake_at.0 && !exposure_gated {
            return;
        }
        let kind = if ext_default {
            ExecOrderKind::ExternalDefault
        } else {
            ExecOrderKind::HedgeFix
        };
        let _ = self.fix_send(cix, kind, v, now, out);
    }

    /// Builds the repair order of the excess leg.
    pub(crate) fn fix_order<V: Views>(
        &self,
        cix: usize,
        kind: ExecOrderKind,
        v: &V,
    ) -> Result<Emit, FixBuild> {
        let Some(c) = self.cells.get(cix) else {
            return Err(FixBuild::MinOrder);
        };
        let (sum, e) = self.naked(cix, v);
        let l = c.legs[e];
        if l.kind == LEG_CHAIN {
            return Err(FixBuild::Chain);
        }
        let h = *self.inst(l.inst);
        let lv = v.legs(c.account, c.group);
        let net = lv.leg[e].net;
        let mut q = calc::floor_lot(Qty(sum.abs()), l.lot.0.max(1)).0.min(net.0.abs());
        let side = if net.0 > 0 { Side::Sell } else { Side::Buy };
        if l.kind == LEG_SPOT && side == Side::Sell {
            use hft_strategy_sdk::PortfolioRead;
            let pf = v.portfolio(c.account);
            q = q
                .min(pf.inventory_of(l.venue, c.group).0)
                .min(pf.cash(l.venue, h.asset).free.0);
        }
        if q < l.min_qty.0.max(1) {
            return Err(FixBuild::MinOrder);
        }
        if !self.quote_ok(v, c.account, l.inst) {
            return Err(FixBuild::Quote);
        }
        let p = self.params(self.acct_of_cix(cix));
        let bbo = v.bbo(l.inst);
        let rf = v.risk_for(c.account, l.inst);
        let r = v.risk(c.account);
        let px = calc::limit_px(
            side,
            &bbo,
            p.force_slice.max_cross,
            rf.rr_max_cross,
            r.conn_liveness_ms,
            &h,
        );
        if px.0 <= 0 {
            return Err(FixBuild::Quote);
        }
        let cap_n = self.cap_n(cix, IntentClass::Hedge, kind, true, &p.force_slice, &r);
        let qcap = calc::qty_under_cap(cap_n, px, rf.quote_rate, &h);
        let q = calc::floor_lot(Qty(q.min(qcap.0)), l.lot.0.max(1));
        if q.0 < l.min_qty.0.max(1) || calc::notional_le(q, px, &h, Notional(h.min_notional.0 - 1)) {
            return Err(FixBuild::MinOrder);
        }
        let dec_action = match kind {
            ExecOrderKind::ResidualTrim => action::RESIDUAL_TRIM,
            ExecOrderKind::ExternalDefault => action::EXTERNAL_DEFAULT,
            ExecOrderKind::CmdHedgeFix => action::CMD_HEDGE_FIX,
            ExecOrderKind::AutoFix => action::AUTO_FIX,
            _ => action::HEDGE_FIX,
        };
        Ok(Emit {
            leg: e,
            side,
            qty: q,
            px,
            mid: calc::mid(&bbo),
            cap_n,
            kind,
            engine: true,
            class: IntentClass::Hedge,
            reduce_only: l.reduce_only_cap != 0,
            link: LegLinkOpt::NONE,
            hedge_cap: Qty(sum.abs().saturating_add(l.lot.0)),
            act: ACT_NONE,
            dec_action,
            dec_detail: 0,
            depth: Qty(0),
            first_leg_ix: 0xFF,
            algo: algo::ENGINE,
            origin: match kind {
                ExecOrderKind::ExternalDefault => Some(TargetOrigin::Engine(EngineOrigin::External)),
                ExecOrderKind::CmdHedgeFix => Some(TargetOrigin::Engine(EngineOrigin::Command)),
                ExecOrderKind::AutoFix => Some(TargetOrigin::Engine(EngineOrigin::AutoFix)),
                _ => None,
            },
            target_qty: Qty(0),
            cooldown_ms: 0,
        })
    }

    /// Sends a repair-family order (cooldown and Fix timer); `false` when nothing was sent.
    fn fix_send<V: Views>(
        &mut self,
        cix: usize,
        kind: ExecOrderKind,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) -> bool {
        let p = self.params(self.acct_of_cix(cix));
        let e = match self.fix_order(cix, kind, v) {
            Ok(e) => e,
            Err(_) => return false,
        };
        self.emit(cix, e, v, now, out);
        let next = Tsc(now.tsc.0.saturating_add(Self::ms(&now, p.fix_cooldown_ms)));
        let saved = self.cells.get(cix).map_or(Tsc(0), |c| c.fix.next_tsc);
        self.reg_timer(cix, ExecTimerKind::Fix, next, saved, out);
        if let Some(c) = self.cells.get_mut(cix) {
            c.fix.next_tsc = next;
        }
        if kind == ExecOrderKind::ExternalDefault {
            let Some(c) = self.cells.get(cix).copied() else {
                return true;
            };
            let (sum, _) = self.naked(cix, v);
            let n = self.note(
                cix,
                note::EXEC_EXTERNAL_DEFAULT,
                NoteLevel::P0,
                [
                    acct_group(c.account.0, c.group.0),
                    u64::from(e.leg as u8),
                    e.qty.0 as u64,
                    sum.unsigned_abs(),
                    0,
                    0,
                    0,
                    0,
                ],
            );
            out.notes.push(n);
            if let Some(c) = self.cells.get_mut(cix) {
                c.aux16 &= !AUX_EXT_DEFAULT;
            }
        }
        true
    }

    /// Residual rules (README "残余与尘埃"): above dust, within tolerance, below the hedge legs'
    /// minimum order.
    fn residual_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if !self.naked_above_dust(cix, v)
            || !self.fix_quiet(cix, v)
            || c.cap_block != 0
            || c.serial != 0
            || c.ext_deadline.0 != 0
            || c.hold & MANUAL_HOLD != 0
            || v.risk(c.account).hard
        {
            return;
        }
        let (sum, e) = self.naked(cix, v);
        let residual = (0..usize::from(c.n_legs))
            .filter(|&i| i != e)
            .all(|i| sum.abs() < c.legs[i].min_qty.0);
        if !residual {
            return;
        }
        let target_policy = match c.view.intent.exec {
            ExecSpec::PairOnFill(pp) if c.has_target => Some(pp.residual),
            ExecSpec::Simultaneous(sp) if c.has_target => Some(sp.residual),
            _ => None,
        };
        let policy = c
            .link_residual()
            .or(target_policy)
            .unwrap_or(ResidualPolicy::Hold);
        if policy != ResidualPolicy::TrimExcess {
            return;
        }
        if !self.venue_open(cix, c.legs[e].venue) {
            return;
        }
        if c.fix.next_tsc.0 != 0 && now.tsc.0 < c.fix.next_tsc.0 || now.tsc.0 < c.wake_at.0 {
            return;
        }
        let _ = self.fix_send(cix, ExecOrderKind::ResidualTrim, v, now, out);
    }

    /// `HedgeFix` command (control plane): flattens the excess leg now.
    pub fn hedge_fix_cmd<V: Views>(
        &mut self,
        a: AccountId,
        g: GroupId,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) -> Result<(), ExecCmdErr> {
        let Some(cix) = self.cix_of(a, g).filter(|_| g.0 != 0) else {
            return Err(ExecCmdErr::UnknownGroup);
        };
        if v.risk(a).hard {
            return Err(ExecCmdErr::Hard);
        }
        let Some(c) = self.cells.get(cix).copied() else {
            return Err(ExecCmdErr::UnknownGroup);
        };
        let lv = v.legs(a, g);
        if lv.open_links != 0
            || lv.has_unknown
            || lv.has_pending
            || lv
                .leg
                .iter()
                .take(usize::from(c.n_legs))
                .any(|l| l.inflight_immediate.0 != 0)
        {
            return Err(ExecCmdErr::NotQuiescent);
        }
        if !self.naked_above_dust(cix, v) {
            return Err(ExecCmdErr::NothingToFix);
        }
        let e = match self.fix_order(cix, ExecOrderKind::CmdHedgeFix, v) {
            Ok(e) => e,
            Err(FixBuild::Quote) => return Err(ExecCmdErr::QuoteUnavailable),
            Err(FixBuild::Chain) => return Err(ExecCmdErr::ChainLeg),
            Err(FixBuild::MinOrder) => return Err(ExecCmdErr::NothingToFix),
        };
        self.emit(cix, e, v, now, out);
        self.release_hold(cix, hold_release::HEDGE_FIX_CMD, now, out);
        if let Some(c) = self.cells.get_mut(cix)
            && matches!(c.view.state, TargetState::Broken(_))
            && matches!(
                c.auto_fix.phase(),
                AutoFixPhase::Primed
                    | AutoFixPhase::Running
                    | AutoFixPhase::WaitHard
                    | AutoFixPhase::WaitHold
            )
        {
            c.auto_fix.phase = AutoFixPhase::Cancelled as u8;
            c.auto_fix.run_mark_tsc = Tsc(0);
        }
        Ok(())
    }

    /// HARD release: does (a, g) become Broken (README "HARD 解除时的 Broken 判定")? Read-only.
    pub fn resolve_broken<V: Views>(&self, a: AccountId, g: GroupId, v: &V) -> bool {
        let Some(cix) = self.cix_of(a, g).filter(|_| g.0 != 0) else {
            return false;
        };
        let Some(c) = self.cells.get(cix) else {
            return false;
        };
        v.legs(a, g).open_links == 0 && c.hold & MANUAL_HOLD == 0 && self.fix_trigger(cix, v)
    }
}
