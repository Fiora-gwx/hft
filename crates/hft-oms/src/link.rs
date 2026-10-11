//! LinkTable: creation, OnFill second legs, Simultaneous, failure handling (retry / reverse / hold),
//! deadlines, gating and vetoes (README "LegLink 与 HedgePlan", "OnFill", "Simultaneous").

use hft_event::SendStatus;
use hft_strategy_sdk::{HedgeKind, HedgePlan, LegLink, LinkRole, OnFail, OrderRef};
use hft_types::{AccountId, InstId, Px, Qty, RoundMode, Side, Tsc, VenueId};

use crate::Oms;
use crate::fsm::FillDone;
use crate::notes;
use crate::types::{
    ApprovedIntent, HedgeGate, HedgePxErr, HedgeRequest, LinkBroken, LinkEntry, LinkState, OmsOut,
    OmsTimerKind, OrderState, link_flags as lf, link_note,
};

/// Low 24 bits of `LinkEntry.plan` = plan index; bits 24..31 = `LinkBroken` code.
const PLAN_IX_MASK: u32 = 0x00FF_FFFF;

#[inline]
fn floor_lot(q: i64, lot: i64) -> i64 {
    if lot <= 0 {
        return q;
    }
    q - q % lot
}

impl Oms {
    // ------------------------------------------------------------------ lookups

    /// Lowest free link index of an account (Free or Done entries are free).
    pub(crate) fn free_link_ix(&self, a: AccountId) -> Option<usize> {
        let aix = self.aix(a)?;
        let lpa = self.p.lpa as usize;
        let base = aix * lpa;
        (base..base + lpa).find(|&i| {
            self.links
                .get(i)
                .is_some_and(|e| matches!(e.state, LinkState::Free | LinkState::Done))
        })
    }

    pub(crate) fn broken_reason(&self, i: usize) -> LinkBroken {
        let e = self.links.get(i);
        let code = e.map_or(0, |e| (e.plan >> 24) as u8);
        if e.is_some_and(|e| e.state == LinkState::Broken) {
            LinkBroken::try_from(code).unwrap_or(LinkBroken::None)
        } else {
            LinkBroken::None
        }
    }

    /// Inst of the first leg (from its slot or its tombstone).
    pub(crate) fn first_inst(&self, i: usize) -> InstId {
        let Some(e) = self.links.get(i) else {
            return InstId(0);
        };
        let f = e.first;
        if let Some((sl, _, _)) = self.raw_slot(f.venue, f.account, f.slot)
            && sl.r#gen == f.r#gen
            && sl.state != OrderState::Free
        {
            return sl.inst;
        }
        self.tomb(f.venue, f.account, f.slot, f.r#gen)
            .map_or(InstId(0), |t| t.inst)
    }

    /// Side of the first leg (opposite of the hedge side).
    pub(crate) fn first_side(&self, i: usize) -> Side {
        self.plans.get(i).map_or(Side::Buy, |(_, p)| p.side.opposite())
    }

    pub(crate) fn first_terminal(&self, e: &LinkEntry) -> bool {
        let f = e.first;
        match self.raw_slot(f.venue, f.account, f.slot) {
            Some((sl, _, _)) if sl.r#gen == f.r#gen && sl.state != OrderState::Free => sl.state.terminal(),
            _ => true,
        }
    }

    /// (venue, account) whose hedge gate resumes the link.
    pub(crate) fn resume_venue(&self, i: usize) -> (VenueId, AccountId) {
        let (Some(e), Some((_, p))) = (self.links.get(i), self.plans.get(i)) else {
            return (VenueId(0), AccountId(0));
        };
        match p.on_fail {
            OnFail::Reverse => (e.first.venue, e.first.account),
            _ => (p.venue, p.account),
        }
    }

    fn hedge_lot(&self, plan: &HedgePlan) -> i64 {
        self.inst(plan.inst).lot
    }

    /// Unhedged quantity in hedge-leg units (signed: > 0 = first leg ahead).
    fn diff_h(e: &LinkEntry, p: &HedgePlan) -> i128 {
        i128::from(e.first_net.0) * i128::from(p.ratio_num) / i128::from(p.ratio_den.max(1))
            - i128::from(e.hedge_filled.0)
    }

    /// Updates `first_fill_tsc` after first_net / hedge_filled changed.
    fn update_fft(&mut self, i: usize, before: i128, now: Tsc) {
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        let lot = i128::from(self.hedge_lot(&p).max(1));
        let Some(e) = self.links.get_mut(i) else { return };
        let after = Self::diff_h(e, &p).abs();
        if after < lot {
            e.first_fill_tsc = Tsc(0);
        } else if before.abs() < lot {
            e.first_fill_tsc = Tsc(now.0.max(1));
        }
    }

    /// Naked quantity in the hedge direction (README "LegTracker"); `None` = untracked (no cap).
    pub(crate) fn naked_dir(&self, i: usize) -> Option<i64> {
        let e = self.links.get(i)?;
        let (_, p) = self.plans.get(i)?;
        let finst = self.first_inst(i);
        let fx = self.leg_ix_of(e.account, e.group, finst);
        let hx = self.leg_ix_of(e.account, e.group, p.inst);
        let lv = &self.legs(e.account, e.group).view;
        let lf_ = lv.leg.get(usize::from(fx))?;
        let lh = lv.leg.get(usize::from(hx))?;
        let num = i128::from(p.ratio_num);
        let den = i128::from(p.ratio_den.max(1));
        let ex = i128::from(lf_.net.0) * num / den + i128::from(lh.net.0);
        let wf = i128::from(lf_.inflight_immediate.0) * num / den;
        let wh = i128::from(lh.inflight_immediate.0);
        let v = match p.side {
            Side::Sell => (ex + wf.min(0) + wh.min(0)).max(0),
            Side::Buy => (-(ex + wf.max(0) + wh.max(0))).max(0),
        };
        Some(v.min(i128::from(i64::MAX)) as i64)
    }

    // ------------------------------------------------------------------ creation

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_link(
        &mut self,
        ti: usize,
        s: u32,
        l: &LegLink,
        it: &ApprovedIntent,
        _now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) -> u32 {
        let Some(t) = self.t(ti) else { return 0 };
        let account = t.account;
        let Some(sl) = t.slot(s) else { return 0 };
        let first = sl.handle();
        #[cfg(feature = "testkit")]
        let first_epoch = sl.epoch;
        let group = sl.group;
        let Some(ix) = self.free_link_ix(account) else {
            return 0;
        };
        let link_id = ix as u32 + 1;
        #[cfg(feature = "testkit")]
        self.audit_reuse(ix, first_epoch, seq);
        if let Some(e) = self.links.get_mut(ix) {
            *e = LinkEntry {
                link_id,
                group,
                account,
                state: LinkState::Open,
                retries: 0,
                flags: 0,
                plan: ix as u32 & PLAN_IX_MASK,
                first,
                held_ms: 0,
                first_net: Qty(0),
                hedge_req: Qty(0),
                hedge_filled: Qty(0),
                hedge_open: Qty(0),
                first_fill_tsc: Tsc(0),
                ref_px: it.hedge_ref_px,
            };
        }
        if let Some(p) = self.plans.get_mut(ix) {
            *p = (l.tolerance, l.hedge);
        }
        if let Some(a) = self.aux.get_mut(ix) {
            *a = crate::LinkAux::default();
        }
        if let Some(gi) = self.group_ix(account, group)
            && let Some(g) = self.groups.get_mut(gi)
        {
            g.st.view.open_links = g.st.view.open_links.saturating_add(1);
            g.st.tolerance = l.tolerance;
            g.st.last_link = link_id;
        }
        self.push_link(link_id, link_note::NONE, 0, seq, out);
        link_id
    }

    fn set_state(&mut self, i: usize, to: LinkState, reason: LinkBroken, seq: u64, out: &mut OmsOut) {
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if e.state == to {
            return;
        }
        let was_active = e.active();
        if let Some(x) = self.links.get_mut(i) {
            x.state = to;
            if to == LinkState::Broken {
                x.plan = (x.plan & PLAN_IX_MASK) | u32::from(reason as u8) << 24;
            }
        }
        if let Some(gi) = self.group_ix(e.account, e.group)
            && let Some(g) = self.groups.get_mut(gi)
        {
            if was_active && !matches!(to, LinkState::Open | LinkState::Hedging) {
                g.st.view.open_links = g.st.view.open_links.saturating_sub(1);
            }
            if to == LinkState::Broken {
                g.st.broken_links = g.st.broken_links.saturating_add(1);
            }
        }
        let note = if to == LinkState::Broken {
            link_note::HEDGE_BROKEN
        } else {
            link_note::NONE
        };
        self.push_link(e.link_id, note, 0, seq, out);
    }

    pub(crate) fn set_broken(&mut self, i: usize, reason: LinkBroken, seq: u64, out: &mut OmsOut) {
        if self.links.get(i).is_some_and(|e| e.active()) {
            self.set_state(i, LinkState::Broken, reason, seq, out);
        }
    }

    fn set_hedging(&mut self, i: usize, seq: u64, out: &mut OmsOut) {
        if self.links.get(i).is_some_and(|e| e.state == LinkState::Open) {
            self.set_state(i, LinkState::Hedging, LinkBroken::None, seq, out);
        }
    }

    /// Done when the first leg is terminal and nothing is owed, open, pending or reversing.
    pub(crate) fn done_check(&mut self, i: usize, seq: u64, out: &mut OmsOut) {
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if !e.active() {
            return;
        }
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        if !self.first_terminal(&e)
            || e.owed().0 > 0
            || e.hedge_open.0 > 0
            || e.has(lf::RETRY_PENDING | lf::HEDGE_QUEUED)
            || self.link_aux(e.link_id).rev_open > 0
        {
            return;
        }
        if e.has(lf::REVERSING) && self.reverse_qty(i).is_some() {
            return;
        }
        if p.kind == HedgeKind::Simultaneous && !e.has(lf::REVERSING) && self.simul_imbalance(i) {
            return;
        }
        self.set_state(i, LinkState::Done, LinkBroken::None, seq, out);
    }

    /// Simultaneous legs out of balance by at least one lot of the leg to reverse.
    fn simul_imbalance(&self, i: usize) -> bool {
        self.reverse_qty(i).is_some()
    }

    // ------------------------------------------------------------------ requests

    fn push_hedge(&mut self, req: HedgeRequest, out: &mut OmsOut) {
        let r = out.hedges.push(req);
        debug_assert!(r.is_ok(), "OmsOut.hedges overflow on a direct path");
    }

    fn base_req(&self, i: usize, role: LinkRole, seq: u64) -> Option<HedgeRequest> {
        let e = self.links.get(i)?;
        let (_, p) = self.plans.get(i)?;
        Some(HedgeRequest {
            link_id: e.link_id,
            role,
            venue: p.venue,
            account: p.account,
            side: p.side,
            reduce_only: p.reduce_only != 0,
            inst: p.inst,
            group: e.group,
            strategy: self.strategy_of(e.account),
            qty: Qty(0),
            basis: Qty(0),
            hedge_cap_qty: Qty(0),
            ref_px: e.ref_px,
            peer_venue: e.first.venue,
            peer_account: e.first.account,
            trigger_seq: seq,
        })
    }

    /// Quantity, inst side and basis of a reverse order; `None` when balanced within a lot.
    fn reverse_qty(&self, i: usize) -> Option<(bool, Qty, Qty)> {
        let e = self.links.get(i)?;
        let (_, p) = self.plans.get(i)?;
        let d = Self::diff_h(e, p);
        if d > 0 {
            // First leg ahead: reverse on the first leg, in first-leg units.
            let unhedged = i128::from(e.first_net.0)
                - i128::from(e.hedge_filled.0) * i128::from(p.ratio_den.max(1))
                    / i128::from(p.ratio_num.max(1));
            let lot = self.inst(self.first_inst(i)).lot;
            let q = floor_lot(unhedged.min(i128::from(i64::MAX)) as i64, lot);
            (q > 0).then_some((true, Qty(q), Qty(unhedged as i64)))
        } else {
            let lot = self.inst(p.inst).lot;
            let b = (-d).min(i128::from(i64::MAX)) as i64;
            let q = floor_lot(b, lot);
            (q > 0).then_some((false, Qty(q), Qty(b)))
        }
    }

    /// Emits a `Reverse` request for the current imbalance; `false` when there is none.
    fn emit_reverse(&mut self, i: usize, seq: u64, out: &mut OmsOut) -> bool {
        let Some((on_first, q, basis)) = self.reverse_qty(i) else {
            return false;
        };
        let Some(mut r) = self.base_req(i, LinkRole::Reverse, seq) else {
            return false;
        };
        let Some(e) = self.links.get(i).copied() else {
            return false;
        };
        let Some((_, p)) = self.plans.get(i).copied() else {
            return false;
        };
        if on_first {
            let inst = self.first_inst(i);
            r.venue = e.first.venue;
            r.account = e.first.account;
            r.inst = inst;
            r.side = self.first_side(i).opposite();
            r.peer_venue = p.venue;
            r.peer_account = p.account;
        } else {
            r.side = p.side.opposite();
        }
        let lot = self.inst(r.inst).lot;
        r.reduce_only = true;
        r.qty = q;
        r.basis = basis;
        r.hedge_cap_qty = Qty(basis.0.saturating_add(lot));
        r.ref_px = Px(0);
        self.push_hedge(r, out);
        self.set_hedging(i, seq, out);
        true
    }

    /// SecondRetry quantity: `min(floor_to_lot(owed), floor_to_lot(naked_dir))`, writing off the
    /// capped part from `hedge_req`. Returns the quantity (0 = nothing to send).
    fn retry_qty(&mut self, i: usize, seq: u64, out: &mut OmsOut) -> (Qty, Qty) {
        let Some(e) = self.links.get(i).copied() else {
            return (Qty(0), Qty(0));
        };
        let Some((_, p)) = self.plans.get(i).copied() else {
            return (Qty(0), Qty(0));
        };
        let owed = e.owed().0;
        if owed <= 0 {
            return (Qty(0), Qty(owed.max(0)));
        }
        let info = *self.inst(p.inst);
        let owed_l = floor_lot(owed, info.lot);
        let naked = self.naked_dir(i);
        let mut q = match naked {
            Some(n) => owed_l.min(floor_lot(n, info.lot)),
            None => owed_l,
        };
        if q < info.min_qty.0 {
            q = 0;
        }
        if q < owed_l
            && let Some(n) = naked
        {
            #[cfg(feature = "testkit")]
            self.audit_cap(i, i128::from(owed - q));
            let note = notes::note(
                notes::HEDGE_NAKED_CAPPED,
                self.strategy_of(e.account),
                e.group,
                p.inst,
                [u64::from(e.link_id), owed as u64, q as u64, n as u64, 0, 0, 0, 0],
            );
            self.note(note, out);
        }
        if q < owed
            && let Some(x) = self.links.get_mut(i)
        {
            x.hedge_req = Qty(x.hedge_req.0.saturating_sub(owed - q));
        }
        let _ = seq;
        (Qty(q), Qty(owed))
    }

    fn emit_second_retry(&mut self, i: usize, q: Qty, basis: Qty, seq: u64, out: &mut OmsOut) {
        let Some(mut r) = self.base_req(i, LinkRole::SecondRetry, seq) else {
            return;
        };
        let lot = self.plans.get(i).map_or(0, |(_, p)| self.inst(p.inst).lot);
        r.qty = q;
        r.basis = basis;
        r.hedge_cap_qty = Qty(basis.0.saturating_add(lot));
        self.push_hedge(r, out);
        self.set_hedging(i, seq, out);
    }

    // ------------------------------------------------------------------ fills

    /// Link effects of a fill increment of a linked slot.
    pub(crate) fn link_fill(
        &mut self,
        ti: usize,
        s: u32,
        fd: &FillDone,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(t) = self.t(ti) else { return };
        let (Some(sl), Some(ex)) = (t.slot(s).copied(), t.ext.get(s as usize).copied()) else {
            return;
        };
        if sl.link_id == 0 {
            return;
        }
        let Some(i) = self.link_ix(sl.link_id) else { return };
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        let before = Self::diff_h(&e, &p);
        match ex.role {
            r if r == LinkRole::First as u8 => {
                if e.first != sl.handle() {
                    return;
                }
                if let Some(x) = self.links.get_mut(i) {
                    x.first_net = Qty(x.first_net.0.saturating_add(fd.hedge_net.0));
                }
                self.update_fft(i, before, now);
                if e.active() && p.kind == HedgeKind::OnFill {
                    self.on_fill(i, fd.hedge_net, seq, out);
                }
                if fd.overfill {
                    self.set_broken(i, LinkBroken::Overfill, seq, out);
                }
            }
            r if r == LinkRole::Second as u8 || r == LinkRole::SecondRetry as u8 => {
                if let Some(x) = self.links.get_mut(i) {
                    x.hedge_filled = Qty(x.hedge_filled.0.saturating_add(fd.d_net.0));
                    x.hedge_open = Qty(x.hedge_open.0.saturating_sub(fd.d.0).max(0));
                }
                self.update_fft(i, before, now);
            }
            r if r == LinkRole::Reverse as u8 => {
                let on_first = sl.inst == self.first_inst(i) && sl.inst != p.inst;
                if let Some(x) = self.links.get_mut(i) {
                    if on_first {
                        x.first_net = Qty(x.first_net.0.saturating_sub(fd.d_net.0));
                    } else {
                        x.hedge_filled = Qty(x.hedge_filled.0.saturating_sub(fd.d_net.0));
                        x.hedge_req = Qty(x.hedge_req.0.saturating_sub(fd.d_net.0).max(0));
                    }
                }
                self.update_fft(i, before, now);
            }
            _ => {}
        }
    }

    /// OnFill steps 2–5 for a first-leg net increment.
    fn on_fill(&mut self, i: usize, d_net: Qty, seq: u64, out: &mut OmsOut) {
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        let info = *self.inst(p.inst);
        let hx = self.leg_ix_of(e.account, e.group, p.inst);
        let res = self.residual_get(e.account, e.group, hx);
        let num = i128::from(p.ratio_num);
        let den = i128::from(p.ratio_den.max(1));
        // Converted cumulatively (first_net already includes d_net) so truncation never accumulates.
        let new = i128::from(e.first_net.0);
        let old = new - i128::from(d_net.0);
        let raw = (new * num / den - old * num / den) as i64 + res;
        let q_fill = floor_lot(raw, info.lot);
        let below = q_fill < info.min_qty.0.max(1)
            || (e.ref_px.0 > 0
                && e.ref_px.mul_qty(Qty(q_fill), info.exp, RoundMode::HalfEven).0.0 < info.min_notional.0);
        if below {
            self.residual_add(e.account, e.group, hx, raw - res);
            return;
        }
        self.residual_add(e.account, e.group, hx, raw - q_fill - res);
        let naked = self.naked_dir(i);
        let mut q = q_fill;
        if let Some(n) = naked {
            q = q_fill.min(floor_lot(n, info.lot));
            if q < q_fill {
                #[cfg(feature = "testkit")]
                self.audit_cap(i, i128::from(q_fill - q));
                let note = notes::note(
                    notes::HEDGE_NAKED_CAPPED,
                    self.strategy_of(e.account),
                    e.group,
                    p.inst,
                    [
                        u64::from(e.link_id),
                        q_fill as u64,
                        q as u64,
                        n as u64,
                        0,
                        0,
                        0,
                        0,
                    ],
                );
                self.note(note, out);
            }
        }
        if q < info.min_qty.0.max(1) {
            return;
        }
        if let Some(x) = self.links.get_mut(i) {
            x.hedge_req = Qty(x.hedge_req.0.saturating_add(q));
        }
        if let Some(mut r) = self.base_req(i, LinkRole::Second, seq) {
            r.qty = Qty(q);
            r.basis = Qty(raw);
            r.hedge_cap_qty = Qty(raw.saturating_add(info.lot));
            self.push_hedge(r, out);
        }
        self.set_hedging(i, seq, out);
    }

    /// Simultaneous: the second leg in the same event as the first leg's `place`.
    pub(crate) fn simultaneous_second(&mut self, link_id: u32, _now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(i) = self.link_ix(link_id) else { return };
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        let f = e.first;
        let Some(qty) = self.raw_slot(f.venue, f.account, f.slot).map(|(sl, _, _)| sl.qty) else {
            return;
        };
        let info = *self.inst(p.inst);
        let hx = self.leg_ix_of(e.account, e.group, p.inst);
        let res = self.residual_get(e.account, e.group, hx);
        let raw = (i128::from(qty.0) * i128::from(p.ratio_num) / i128::from(p.ratio_den.max(1))) as i64 + res;
        let q = floor_lot(raw, info.lot);
        self.residual_add(e.account, e.group, hx, raw - q - res);
        if q < info.min_qty.0.max(1) {
            return;
        }
        if let Some(x) = self.links.get_mut(i) {
            x.hedge_req = Qty(x.hedge_req.0.saturating_add(q));
        }
        if let Some(mut r) = self.base_req(i, LinkRole::Second, seq) {
            r.qty = Qty(q);
            r.basis = Qty(raw);
            r.hedge_cap_qty = Qty(raw.saturating_add(info.lot));
            self.push_hedge(r, out);
        }
        self.set_hedging(i, seq, out);
    }

    // ------------------------------------------------------------------ placement results

    /// One of this crate's hedge requests got a slot.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hedge_placed(
        &mut self,
        link_id: u32,
        ti: usize,
        s: u32,
        it: &ApprovedIntent,
        _now: Tsc,
        _seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(i) = self.link_ix(link_id) else { return };
        let Some(ex) = self.t(ti).and_then(|t| t.ext.get(s as usize)).copied() else {
            return;
        };
        let qty = it.intent.qty;
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        let mut slip_note = false;
        if let Some(x) = self.links.get_mut(i) {
            if ex.role == LinkRole::Second as u8 || ex.role == LinkRole::SecondRetry as u8 {
                x.hedge_open = Qty(x.hedge_open.0.saturating_add(qty.0));
            }
            x.flags &= !(lf::STALE_NOTED | lf::BEYOND_NOTED);
            if it.hedge_px.slipped {
                if x.flags & lf::SLIP_NOTED == 0 {
                    x.flags |= lf::SLIP_NOTED;
                    slip_note = true;
                }
            } else {
                x.flags &= !lf::SLIP_NOTED;
            }
        }
        if slip_note {
            let inst = it.intent.inst;
            let n = notes::note(
                notes::HEDGE_SLIP_CAPPED,
                self.strategy_of(e.account),
                e.group,
                inst,
                [
                    u64::from(link_id),
                    e.ref_px.0 as u64,
                    it.hedge_px.touch.0 as u64,
                    it.hedge_px.cap.0 as u64,
                    0,
                    0,
                    0,
                    0,
                ],
            );
            self.note(n, out);
        }
    }

    /// `place` of one of this crate's hedge requests failed: an execution failure (counted).
    pub(crate) fn hedge_place_failed(&mut self, link_id: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        if let Some(i) = self.link_ix(link_id) {
            self.failure(i, true, Tsc(0), now, seq, out);
        }
    }

    /// Link effects of a linked slot reaching a terminal state.
    pub(crate) fn link_slot_terminal(&mut self, ti: usize, s: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let (Some(sl), Some(ex)) = (t.slot(s).copied(), t.ext.get(s as usize).copied()) else {
            return;
        };
        if sl.link_id == 0 {
            return;
        }
        let Some(i) = self.link_ix(sl.link_id) else { return };
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        let rem = sl.remaining().0;
        let counted = self.local_fail.is_none_or(|st| st == SendStatus::EncodeError);
        let simul = p.kind == HedgeKind::Simultaneous;
        match ex.role {
            r if r == LinkRole::First as u8 => {
                if self.links.get(i).is_some_and(|e| e.first == sl.handle()) {
                    self.simul_or_done(i, simul, now, seq, out);
                }
            }
            r if r == LinkRole::Second as u8 || r == LinkRole::SecondRetry as u8 => {
                if let Some(x) = self.links.get_mut(i) {
                    x.hedge_open = Qty(x.hedge_open.0.saturating_sub(rem).max(0));
                    x.flags &= !lf::RESUME_RETRY;
                }
                if simul && !self.links.get(i).is_some_and(|e| e.has(lf::REVERSING)) {
                    self.simul_or_done(i, true, now, seq, out);
                } else if rem > 0 {
                    self.failure(i, counted, Tsc(0), now, seq, out);
                } else {
                    self.done_check(i, seq, out);
                }
            }
            r if r == LinkRole::Reverse as u8 => {
                if let Some(x) = self.links.get_mut(i) {
                    x.flags &= !lf::RESUME_RETRY;
                }
                if rem > 0 {
                    self.failure(i, counted, Tsc(0), now, seq, out);
                } else {
                    self.done_check(i, seq, out);
                }
            }
            _ => {}
        }
    }

    /// Simultaneous: once both legs are terminal, reverse the imbalance or hold; else Done check.
    fn simul_or_done(&mut self, i: usize, simul: bool, _now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if !simul || e.has(lf::REVERSING) {
            self.done_check(i, seq, out);
            return;
        }
        if !e.active()
            || !self.first_terminal(&e)
            || e.hedge_open.0 > 0
            || self.link_aux(e.link_id).unknown > 0
        {
            return;
        }
        if self.reverse_qty(i).is_none() {
            if let Some(x) = self.links.get_mut(i) {
                x.hedge_req = Qty(x.hedge_filled.0 + x.hedge_open.0);
            }
            self.done_check(i, seq, out);
            return;
        }
        let on_fail = self.plans.get(i).map_or(OnFail::Hold, |(_, p)| p.on_fail);
        if let Some(x) = self.links.get_mut(i) {
            x.hedge_req = Qty(x.hedge_filled.0 + x.hedge_open.0);
        }
        if on_fail == OnFail::Reverse {
            if let Some(x) = self.links.get_mut(i) {
                x.flags |= lf::REVERSING;
            }
            self.emit_reverse(i, seq, out);
        } else {
            self.set_state(i, LinkState::Done, LinkBroken::None, seq, out);
        }
    }

    /// Whether the deadline has passed (README "LegLink 与 HedgePlan" judging rule).
    fn deadline_passed(&self, i: usize, now: Tsc) -> bool {
        let Some(e) = self.links.get(i) else { return false };
        if e.first_fill_tsc.0 == 0 || e.has(lf::RESUME_HOLD | lf::RESUME_RETRY) {
            return false;
        }
        let exposed =
            e.owed().0 > 0 || e.hedge_open.0 > 0 || (e.has(lf::REVERSING) && self.reverse_qty(i).is_some());
        exposed && now.0.saturating_sub(e.first_fill_tsc.0) > self.p.hedge_deadline
    }

    /// Checks the deadline of a link (`on_unknown_query`, sweep).
    pub(crate) fn check_deadline(&mut self, link_id: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(i) = self.link_ix(link_id) else { return };
        if self.links.get(i).is_some_and(|e| e.active()) && self.deadline_passed(i, now) {
            self.set_broken(i, LinkBroken::Deadline, seq, out);
        }
    }

    /// OnFill step 6: second-leg (or reverse) failure.
    pub(crate) fn failure(
        &mut self,
        i: usize,
        counted: bool,
        retry_at: Tsc,
        now: Tsc,
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
            x.flags &= !lf::RESUME_RETRY;
        }
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        let reversing = e.has(lf::REVERSING);
        let needed = if reversing {
            self.reverse_qty(i).is_some()
        } else {
            e.owed().0 > 0
        };
        if !needed {
            self.done_check(i, seq, out);
            return;
        }
        if !reversing && p.on_fail == OnFail::Hold {
            self.set_state(i, LinkState::Done, LinkBroken::None, seq, out);
            return;
        }
        if counted && let Some(x) = self.links.get_mut(i) {
            x.retries = x.retries.saturating_add(1);
        }
        if self.deadline_passed(i, now) {
            self.set_broken(i, LinkBroken::Deadline, seq, out);
            return;
        }
        let retries = self.links.get(i).map_or(0, |e| e.retries);
        if retries > self.p.hedge_retry_max {
            let why = if reversing {
                LinkBroken::ReverseFailed
            } else {
                LinkBroken::RetryExhausted
            };
            self.set_broken(i, why, seq, out);
            return;
        }
        if p.on_fail == OnFail::Reverse && !reversing {
            if let Some(x) = self.links.get_mut(i) {
                x.flags |= lf::REVERSING;
                x.hedge_req = Qty(x.hedge_filled.0.saturating_add(x.hedge_open.0));
            }
            if !self.emit_reverse(i, seq, out) {
                self.done_check(i, seq, out);
            }
            return;
        }
        self.register_retry(i, retry_at, now, out);
    }

    fn register_retry(&mut self, i: usize, retry_at: Tsc, now: Tsc, out: &mut OmsOut) {
        let at = Tsc(now.0.saturating_add(self.p.hedge_retry_min).max(retry_at.0));
        let Some(x) = self.links.get_mut(i) else { return };
        x.flags |= lf::RETRY_PENDING;
        let id = x.link_id;
        self.push_timer(OmsTimerKind::HedgeRetry, id, at, false, out);
    }

    // ------------------------------------------------------------------ entries

    /// `Timer{Oms, HedgeRetry}`.
    pub fn on_hedge_retry(&mut self, link_id: u32, now: Tsc, cause_seq: u64, out: &mut OmsOut) {
        self.hedge_retry(link_id, now, cause_seq, out);
        self.flush(out);
    }

    fn hedge_retry(&mut self, link_id: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(i) = self.link_ix(link_id) else { return };
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if !e.active() || e.has(lf::RESUME_HOLD) {
            return;
        }
        if let Some(x) = self.links.get_mut(i) {
            x.flags &= !lf::RETRY_PENDING;
        }
        if self.deadline_passed(i, now) {
            self.set_broken(i, LinkBroken::Deadline, seq, out);
            return;
        }
        if self.link_aux(link_id).unknown > 0 {
            return;
        }
        if e.has(lf::REVERSING) {
            if !self.emit_reverse(i, seq, out) {
                self.done_check(i, seq, out);
            }
            return;
        }
        let (q, owed) = self.retry_qty(i, seq, out);
        if q.0 > 0 {
            self.emit_second_retry(i, q, owed, seq, out);
        } else {
            self.done_check(i, seq, out);
        }
    }

    /// The sequencer gated one of this crate's hedge requests (not counted; deadline applies).
    pub fn hedge_gated(
        &mut self,
        link_id: u32,
        why: HedgeGate,
        retry_at: Tsc,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) {
        if let Some(i) = self.link_ix(link_id)
            && let Some(e) = self.links.get(i).copied()
            && e.active()
        {
            let bit = match why {
                HedgeGate::Px(HedgePxErr::StaleQuote) => Some((lf::STALE_NOTED, notes::HEDGE_PX_STALE)),
                HedgeGate::Px(HedgePxErr::BeyondCross) => Some((lf::BEYOND_NOTED, notes::HEDGE_PX_GATED)),
                HedgeGate::Risk(_) => None,
            };
            if let Some((b, code)) = bit
                && !e.has(b)
            {
                if let Some(x) = self.links.get_mut(i) {
                    x.flags |= b;
                }
                let inst = self.plans.get(i).map_or(InstId(0), |(_, p)| p.inst);
                let n = notes::note(
                    code,
                    self.strategy_of(e.account),
                    e.group,
                    inst,
                    [u64::from(link_id), 0, 0, 0, 0, 0, 0, 0],
                );
                self.note(n, out);
            }
            self.failure(i, false, retry_at, now, cause_seq, out);
        }
        self.flush(out);
    }

    /// Risk rejected one of this crate's hedge requests with `HEDGE_LIMIT`: Broken, no retry.
    pub fn hedge_vetoed(&mut self, link_id: u32, _now: Tsc, cause_seq: u64, out: &mut OmsOut) {
        if let Some(i) = self.link_ix(link_id) {
            self.set_broken(i, LinkBroken::HedgeLimit, cause_seq, out);
        }
        self.flush(out);
    }

    /// The Unknown slot of a link converged: resume the link if it waited for that.
    pub(crate) fn link_unknown_converged(&mut self, link_id: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(i) = self.link_ix(link_id) else { return };
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if !e.has(lf::RESUME_HOLD) || !e.active() || self.link_aux(link_id).unknown > 0 {
            return;
        }
        let (v, a) = self.resume_venue(i);
        if self.table(v, a).is_some_and(|t| t.hedge_gate) {
            self.resume_link(i, now, seq, false, out);
        }
    }

    /// `pending_out` `Hedge` item: compute the resumption order now (README "输出扇出").
    pub(crate) fn emit_hedge_item(&mut self, it: &crate::types::OutItem, out: &mut OmsOut) {
        let Some(i) = self.link_ix(it.link_id) else { return };
        if let Some(x) = self.links.get_mut(i) {
            x.flags &= !lf::HEDGE_QUEUED;
        }
        self.resume_request(i, it.cause_seq, out);
    }

    /// Sends the resumption order of a link (SecondRetry or Reverse), or settles it at 0.
    pub(crate) fn resume_request(&mut self, i: usize, seq: u64, out: &mut OmsOut) {
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if !e.active() {
            return;
        }
        let reversing = e.has(lf::REVERSING);
        let owed = e.owed();
        let q = if reversing {
            let q = self.reverse_qty(i).map_or(Qty(0), |(_, q, _)| q);
            if q.0 > 0 {
                self.emit_reverse(i, seq, out);
            }
            q
        } else {
            let (q, owed) = self.retry_qty(i, seq, out);
            if q.0 > 0 {
                self.emit_second_retry(i, q, owed, seq, out);
            }
            q
        };
        if q.0 == 0
            && let Some(x) = self.links.get_mut(i)
        {
            x.flags &= !lf::RESUME_RETRY;
        }
        let inst = self.plans.get(i).map_or(InstId(0), |(_, p)| p.inst);
        let n = notes::note(
            notes::RESUME_RETRY,
            self.strategy_of(e.account),
            e.group,
            inst,
            [
                u64::from(e.link_id),
                u64::from(e.held_ms),
                owed.0 as u64,
                q.0 as u64,
                0,
                0,
                0,
                0,
            ],
        );
        self.note(n, out);
        if q.0 == 0 {
            self.done_check(i, seq, out);
        }
    }

    /// Clears `RESUME_HOLD` and resumes once (README "恢复" 4). `batch` = from `on_hedge_gate_open`.
    pub(crate) fn resume_link(&mut self, i: usize, now: Tsc, seq: u64, batch: bool, out: &mut OmsOut) {
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        let Some((_, p)) = self.plans.get(i).copied() else {
            return;
        };
        let held = u64::from(e.held_ms).saturating_mul(self.p.tpm);
        if let Some(x) = self.links.get_mut(i) {
            x.flags &= !lf::RESUME_HOLD;
            if x.first_fill_tsc.0 != 0 {
                x.first_fill_tsc = Tsc(now.0.saturating_sub(held).max(1));
            }
        }
        match p.on_fail {
            OnFail::Hold => {
                self.push_link(e.link_id, link_note::NONE, 0, seq, out);
                let n = notes::note(
                    notes::RESUME_RETRY,
                    self.strategy_of(e.account),
                    e.group,
                    p.inst,
                    [
                        u64::from(e.link_id),
                        u64::from(e.held_ms),
                        e.owed().0 as u64,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                );
                self.note(n, out);
            }
            OnFail::Reverse | OnFail::Retry => {
                if p.on_fail == OnFail::Reverse
                    && !e.has(lf::REVERSING)
                    && let Some(x) = self.links.get_mut(i)
                {
                    x.flags |= lf::REVERSING;
                    x.hedge_req = Qty(x.hedge_filled.0.saturating_add(x.hedge_open.0));
                }
                if let Some(x) = self.links.get_mut(i) {
                    x.flags |= lf::RESUME_RETRY;
                }
                if batch {
                    if let Some(x) = self.links.get_mut(i) {
                        x.flags |= lf::HEDGE_QUEUED;
                    }
                    let role = if p.on_fail == OnFail::Reverse {
                        LinkRole::Reverse
                    } else {
                        LinkRole::SecondRetry
                    };
                    self.push_item(
                        crate::types::OutItem {
                            kind: crate::types::out_kind::HEDGE,
                            action: role as u8,
                            from: 0,
                            cause: 0,
                            link_id: e.link_id,
                            cause_seq: seq,
                            slot: OrderRef::new(VenueId(0), AccountId(0), 0, 0),
                            _pad: 0,
                        },
                        out,
                    );
                } else {
                    self.resume_request(i, seq, out);
                }
                self.push_link(e.link_id, link_note::NONE, 0, seq, out);
            }
        }
    }
}
