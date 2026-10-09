//! Test hooks (feature `testing`, only enabled from this crate's dev-dependencies): put slots,
//! links and tables into a given state while keeping the derived books consistent.

use hft_strategy_sdk::OrderRef;
use hft_types::{AccountId, GroupId, InstId, Qty, Tsc, VenueId};

use crate::Oms;
use crate::types::{LegState, LinkEntry, OmsOut, OrderSlot, OrderState, OutItem, SlotExt, SlotStamps, Tomb};

impl Oms {
    /// Mutates a slot (any state, any generation) and re-derives its contributions.
    pub fn test_slot_mut(
        &mut self,
        v: VenueId,
        a: AccountId,
        s: u32,
        f: impl FnOnce(&mut OrderSlot, &mut SlotExt),
    ) {
        let Some(ti) = self.tix(v, a) else { return };
        let before = self.contrib(ti, s);
        if let Some(t) = self.tm(ti)
            && let (Some(sl), Some(ex)) = (t.slots.get_mut(s as usize), t.ext.get_mut(s as usize))
        {
            f(sl, ex);
        }
        let after = self.contrib(ti, s);
        self.apply(ti, &before, -1);
        self.apply(ti, &after, 1);
    }

    /// Sets the state of a live slot (and `prev_state` when entering Unknown); flags untouched.
    pub fn test_set_state(&mut self, r: OrderRef, st: OrderState, prev: OrderState) {
        self.test_slot_mut(r.venue, r.account, r.slot, |sl, _| {
            if st == OrderState::Unknown {
                sl.prev_state = prev;
            }
            sl.state = st;
        });
    }

    pub fn test_stamps_mut(&mut self, v: VenueId, a: AccountId, s: u32, f: impl FnOnce(&mut SlotStamps)) {
        if let Some(ti) = self.tix(v, a)
            && let Some(st) = self.tm(ti).and_then(|t| t.stamps.get_mut(s as usize))
        {
            f(st);
        }
    }

    pub fn test_link_mut(&mut self, link_id: u32, f: impl FnOnce(&mut LinkEntry)) {
        if let Some(e) = (link_id as usize)
            .checked_sub(1)
            .and_then(|i| self.links.get_mut(i))
        {
            f(e);
        }
    }

    /// Mutates a LegState (net/residual etc.) and recomputes the derived view fields.
    pub fn test_leg_mut(&mut self, a: AccountId, g: GroupId, f: impl FnOnce(&mut LegState)) {
        if let Some(gi) = self.group_ix(a, g) {
            if let Some(e) = self.groups.get_mut(gi) {
                f(&mut e.st);
            }
            self.recompute_view(gi);
        }
    }

    /// Sets the net of the leg of `inst` in (a, g).
    pub fn test_set_net(&mut self, a: AccountId, g: GroupId, inst: InstId, net: Qty) {
        let ix = self.leg_ix_of(a, g, inst);
        self.test_leg_mut(a, g, |st| {
            if let Some(l) = st.view.leg.get_mut(usize::from(ix)) {
                l.net = net;
            }
        });
    }

    pub fn test_set_residual(&mut self, a: AccountId, g: GroupId, inst: InstId, r: Qty) {
        let ix = self.leg_ix_of(a, g, inst);
        self.test_leg_mut(a, g, |st| {
            if let Some(l) = st.view.leg.get_mut(usize::from(ix)) {
                l.residual = r;
            }
        });
    }

    pub fn test_tomb_mut(&mut self, v: VenueId, a: AccountId, s: u32, r#gen: u32, f: impl FnOnce(&mut Tomb)) {
        let tg = self.p.tomb_gens;
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            let ix = t.tomb_ix(s, r#gen, tg);
            if let Some(tb) = t.tombs.get_mut(ix) {
                f(tb);
            }
        }
    }

    pub fn test_set_conn_up(&mut self, v: VenueId, a: AccountId, up: bool) {
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            t.conn_up = up;
        }
    }

    pub fn test_set_hedge_gate(&mut self, v: VenueId, a: AccountId, open: bool) {
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            t.hedge_gate = open;
        }
    }

    pub fn test_set_smoke_slot(&mut self, v: VenueId, a: AccountId, s: u32) {
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            t.smoke_slot = s;
        }
    }

    pub fn test_set_decode_fails(&mut self, v: VenueId, a: AccountId, n: u8) {
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            t.decode_fails = n;
        }
    }

    pub fn test_set_ext_held(&mut self, v: VenueId, a: AccountId, eid: u64, q: Qty) {
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            t.ext_held.set(eid, q);
        }
    }

    pub fn test_set_foreign(&mut self, v: VenueId, a: AccountId, eid: u64, q: Qty) {
        if let Some(ti) = self.tix(v, a)
            && let Some(t) = self.tm(ti)
        {
            t.foreign.set(eid, q);
        }
    }

    pub fn test_set_resume_hold_until(&mut self, t: Tsc) {
        self.resume_hold_until = t;
    }

    /// Queues a decision item of slot `r` without handing anything out.
    pub fn test_enqueue_decision(&mut self, r: OrderRef, action: hft_event::OrderAction, seq: u64) {
        if let Some(ti) = self.tix(r.venue, r.account) {
            self.enqueue_decision(
                ti,
                OutItem {
                    kind: crate::types::out_kind::DECISION,
                    action: action as u8,
                    from: 0,
                    cause: 0,
                    link_id: 0,
                    cause_seq: seq,
                    slot: r,
                    _pad: 0,
                },
            );
        }
    }

    /// Hands out queued outputs (same as `on_continue`).
    pub fn test_flush(&mut self, out: &mut OmsOut) {
        self.flush(out);
    }

    /// Recomputes the derived link bookkeeping of every link from the slots.
    pub fn test_link_aux(&self, link_id: u32) -> (u16, u16) {
        let a = self.link_aux(link_id);
        (a.rev_open, a.unknown)
    }

    /// `naked_dir` of a link (README "LegTracker"); `None` = untracked.
    pub fn test_naked_dir(&self, link_id: u32) -> Option<i64> {
        self.link_ix(link_id).and_then(|i| self.naked_dir(i))
    }
}
