//! Batch cancels (`cancel_all`, `reissue_cancels`, `cancel_open`, `cancel_resting_on`), the Drain
//! readiness bits, `sim_reset` and slot recycling.

use hft_event::{OrderAction, OrderFlags};
use hft_types::{AccountId, GROUP_ALL, InstId, Tif, Tsc};

use crate::Oms;
use crate::engine::CLASS_OPEN;
use crate::types::{
    CancelScope, CellKey, LegState, LinkEntry, LinkState, OmsOut, OpError, OrderSlot, OrderState,
    QuiesceBits, SlotFlags, SlotStamps, Tomb, TransitionCause, link_flags, quiesce,
};

fn scope_table(sc: &CancelScope, t: &crate::table::Table) -> bool {
    match *sc {
        CancelScope::Global => true,
        CancelScope::Account(a) => t.account == a,
        CancelScope::Venue(v) => t.venue == v,
        CancelScope::VenueAccount(v, a) => t.venue == v && t.account == a,
        CancelScope::Cell(c) => c.account.0 == 0 || t.account == c.account,
        CancelScope::Inst(v, _) => t.venue == v,
    }
}

fn scope_slot(sc: &CancelScope, sl: &OrderSlot) -> bool {
    match *sc {
        CancelScope::Cell(c) => {
            (c.strategy.0 == 0 || sl.strategy == c.strategy) && (c.group == GROUP_ALL || sl.group == c.group)
        }
        CancelScope::Inst(_, i) => i.0 == 0 || sl.inst == i,
        _ => true,
    }
}

fn whole_table(sc: &CancelScope) -> bool {
    match *sc {
        CancelScope::Cell(c) => c.strategy.0 == 0 && c.group == GROUP_ALL,
        CancelScope::Inst(_, i) => i.0 == 0,
        _ => true,
    }
}

/// Cancellable: Live, PendingNew or PendingAmend without `CANCEL_SENT`.
#[inline]
pub(crate) fn cancellable(sl: &OrderSlot) -> bool {
    matches!(
        sl.state,
        OrderState::Live | OrderState::PendingNew | OrderState::PendingAmend
    ) && !sl.sflags.has(SlotFlags::CANCEL_SENT)
}

#[inline]
fn rr(sl: &OrderSlot) -> bool {
    sl.oflags & OrderFlags::RISK_REDUCING.0 != 0
}

#[inline]
fn resting_tif(t: Tif) -> bool {
    matches!(t, Tif::Gtc | Tif::PostOnly)
}

impl Oms {
    /// Kill / Drain / CancelAll (README "撤单范围与批量撤单").
    pub fn cancel_all(
        &mut self,
        scope: CancelScope,
        open_only: bool,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) {
        self.cancel_scope(scope, open_only, now, cause_seq, out);
        self.flush(out);
    }

    /// Standing cancel obligation: same rule as `cancel_all`.
    pub fn reissue_cancels(
        &mut self,
        scope: CancelScope,
        open_only: bool,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) {
        self.cancel_scope(scope, open_only, now, cause_seq, out);
        self.flush(out);
    }

    fn cancel_scope(&mut self, scope: CancelScope, open_only: bool, now: Tsc, seq: u64, out: &mut OmsOut) {
        for ti in 0..self.tables.len() {
            let Some(t) = self.t(ti) else { continue };
            if !scope_table(&scope, t) {
                continue;
            }
            let pick = |sl: &OrderSlot| cancellable(sl) && scope_slot(&scope, sl) && (!open_only || !rr(sl));
            let any = t.slots.iter().skip(1).any(pick);
            if !any {
                continue;
            }
            let has_rr = t.slots.iter().skip(1).any(|sl| sl.state.open() && rr(sl));
            let batch = t.batch_cancel && whole_table(&scope) && (!open_only || !has_rr);
            let n = t.n;
            for s in 1..=n {
                let hit = self.t(ti).and_then(|t| t.slot(s)).is_some_and(pick);
                if !hit {
                    continue;
                }
                if batch {
                    self.mark_cancel(ti, s, TransitionCause::CancelReq, None, now, seq, out);
                } else {
                    self.cancel_slot(ti, s, TransitionCause::CancelReq, None, now, seq, out);
                }
            }
            if batch {
                self.push_decision(ti, OrderAction::CancelAll, 0, seq, out);
            }
        }
    }

    /// Effective mode fell to CloseOnly/Shadow: cancel the cell's Open resting orders.
    pub fn cancel_open(&mut self, cell: CellKey, now: Tsc, cause_seq: u64, out: &mut OmsOut) {
        for ti in 0..self.tables.len() {
            let Some(t) = self.t(ti) else { continue };
            if t.account != cell.account {
                continue;
            }
            for s in 1..=t.n {
                let hit = self.t(ti).is_some_and(|t| {
                    let (Some(sl), Some(ex)) = (t.slot(s), t.ext.get(s as usize)) else {
                        return false;
                    };
                    cancellable(sl)
                        && sl.group == cell.group
                        && (cell.strategy.0 == 0 || sl.strategy == cell.strategy)
                        && ex.class == CLASS_OPEN
                        && !rr(sl)
                        && resting_tif(sl.tif)
                });
                if hit {
                    self.cancel_slot(ti, s, TransitionCause::CancelReq, None, now, cause_seq, out);
                }
            }
        }
        self.flush(out);
    }

    /// Before an engine MarginStress/Hard target: cancel every non-EXEC resting order of (a, inst).
    pub fn cancel_resting_on(
        &mut self,
        a: AccountId,
        inst: InstId,
        now: Tsc,
        cause_seq: u64,
        out: &mut OmsOut,
    ) {
        let v = self.inst(inst).venue;
        if let Some(ti) = self.tix(v, a) {
            let n = self.t(ti).map_or(0, |t| t.n);
            for s in 1..=n {
                let hit = self.t(ti).and_then(|t| t.slot(s)).is_some_and(|sl| {
                    cancellable(sl)
                        && sl.inst == inst
                        && !sl.sflags.has(SlotFlags::EXEC)
                        && resting_tif(sl.tif)
                });
                if hit {
                    self.cancel_slot(ti, s, TransitionCause::CancelReq, None, now, cause_seq, out);
                }
            }
        }
        self.flush(out);
    }

    /// Drain readiness bits of an account (README "Drain 静止判定").
    pub fn quiescent(&self, a: AccountId) -> QuiesceBits {
        let mut b = 0;
        if let Some(aix) = self.aix(a) {
            let lpa = self.p.lpa as usize;
            for e in self.links.iter().skip(aix * lpa).take(lpa) {
                if e.active() {
                    b |= quiesce::LINKS_ACTIVE;
                    if e.has(link_flags::RETRY_PENDING) {
                        b |= quiesce::HEDGE_RETRY_PENDING;
                    }
                }
            }
        }
        for t in self.tables.iter().flatten() {
            if t.account != a {
                continue;
            }
            if t.unknown_count > 0 {
                b |= quiesce::UNKNOWN;
            }
            if t.slots
                .iter()
                .skip(1)
                .any(|sl| sl.state.open() && matches!(sl.tif, Tif::Ioc | Tif::Fok))
            {
                b |= quiesce::IOC_NONTERMINAL;
            }
        }
        b
    }

    /// Writes the tombstone of slot `s` and returns it to the tail of the free list.
    pub(crate) fn recycle(&mut self, ti: usize, s: u32, now: Tsc) {
        let tg = self.p.tomb_gens;
        let c = self.contrib(ti, s);
        self.apply(ti, &c, -1);
        let Some(t) = self.tm(ti) else { return };
        let (Some(sl), Some(ex)) = (t.slots.get(s as usize).copied(), t.ext.get(s as usize).copied()) else {
            return;
        };
        let ix = t.tomb_ix(s, sl.r#gen, tg);
        if let Some(tb) = t.tombs.get_mut(ix) {
            *tb = Tomb {
                r#gen: sl.r#gen,
                epoch: sl.epoch,
                oflags: sl.oflags,
                inst: sl.inst,
                group: sl.group,
                side: sl.side,
                leg_ix: ex.leg_ix,
                strategy: sl.strategy,
                role: ex.role,
                link_id: sl.link_id,
                filled: sl.filled,
                net_filled: ex.net_filled,
                exch_order_id: sl.exch_order_id,
                freed_tsc: now,
            };
        }
        if let Some(x) = t.slots.get_mut(s as usize) {
            let r#gen = x.r#gen;
            let (venue, account) = (x.venue, x.account);
            *x = OrderSlot::FREE;
            x.r#gen = r#gen;
            x.slot_id = s;
            x.venue = venue;
            x.account = account;
        }
        if let Some(st) = t.stamps.get_mut(s as usize) {
            *st = SlotStamps::EMPTY;
        }
        if t.smoke_slot == s {
            t.smoke_slot = 0;
        }
        t.free.push(s);
        self.stats.recycled += 1;
    }

    /// Sim accounts only (`SimReset`): see README "入口".
    pub fn sim_reset(&mut self, a: AccountId) -> Result<(), OpError> {
        let idle = self.tables.iter().flatten().filter(|t| t.account == a).all(|t| {
            t.unknown_count == 0
                && !t.slots.iter().skip(1).any(|sl| {
                    sl.state == OrderState::Unknown
                        || (sl.state.open() && matches!(sl.tif, Tif::Ioc | Tif::Fok))
                })
        });
        if !idle {
            return Err(OpError::NotIdle);
        }
        for ti in 0..self.tables.len() {
            let Some(t) = self.tm(ti) else { continue };
            if t.account != a {
                continue;
            }
            for tb in t.tombs.iter_mut() {
                *tb = Tomb::EMPTY;
            }
            let n = t.n;
            for s in 1..=n {
                let open = self
                    .t(ti)
                    .and_then(|t| t.slot(s))
                    .is_some_and(|sl| sl.state.open());
                if !open {
                    continue;
                }
                let before = self.contrib(ti, s);
                self.apply(ti, &before, -1);
                if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
                    x.state = OrderState::Cancelled;
                    x.sflags.0 &= !(SlotFlags::CANCEL_SENT
                        | SlotFlags::AMEND_SENT
                        | SlotFlags::VERIFY
                        | SlotFlags::QUEUED);
                }
                // Already removed from the books; recycle without a second removal.
                self.recycle_terminal(ti, s);
            }
            if let Some(t) = self.tm(ti) {
                t.smoke_slot = 0;
            }
        }
        for e in self.groups.iter_mut() {
            if e.account == a {
                let n = e.st.view.n_legs;
                e.st = LegState::EMPTY;
                e.st.view.n_legs = n;
            }
        }
        if let Some(aix) = self.aix(a) {
            let lpa = self.p.lpa as usize;
            for e in self.links.iter_mut().skip(aix * lpa).take(lpa) {
                *e = LinkEntry::FREE;
            }
            for x in self.aux.iter_mut().skip(aix * lpa).take(lpa) {
                *x = crate::LinkAux::default();
            }
            let n_insts = self.inflight.n_insts;
            for p in self
                .inflight
                .pos
                .iter_mut()
                .skip(aix * (n_insts + 1))
                .take(n_insts + 1)
            {
                *p = crate::types::InflightPos::ZERO;
            }
        }
        for ti in 0..self.tables.len() {
            if self.t(ti).is_some_and(|t| t.account == a)
                && let Some(vb) = self.inflight.venue.get_mut(ti)
            {
                *vb = crate::table::VenueBook::default();
            }
        }
        self.inflight.group.retain(|(x, _, _)| *x != a);
        Ok(())
    }

    /// Recycle of a terminal slot whose contribution is already zero (tombstone, free list).
    fn recycle_terminal(&mut self, ti: usize, s: u32) {
        self.recycle(ti, s, Tsc(0));
    }

    #[allow(dead_code)]
    pub(crate) fn link_states(&self) -> impl Iterator<Item = &LinkEntry> {
        self.links.iter().filter(|e| e.state != LinkState::Free)
    }
}
