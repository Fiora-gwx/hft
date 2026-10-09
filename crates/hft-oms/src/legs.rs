//! LegTracker: (account, group) → legs, fills and adjustments into the legs, `naked_groups` and
//! the residual write-off.

use hft_strategy_sdk::GroupTable;
use hft_types::{AccountId, GROUP_NONE, GroupId, InstId, Px, Qty, Tsc};

use crate::Oms;
use crate::table::GroupEntry;
use crate::types::{FixedVec, LegState, LinkState, OpError, SlotExt, WriteOff};

impl Oms {
    /// SetParams of one instance: rebuild its (account, inst) → (group, leg) map (capacity
    /// preallocated; only contents are rewritten).
    pub fn on_groups_changed(&mut self, groups: &GroupTable) {
        let specs = groups.specs();
        let Some(first) = specs.first() else { return };
        let strategy = first.id.strategy();
        let Some(aix) = self.acct_strategy.iter().position(|s| *s == strategy) else {
            return;
        };
        let account = AccountId((aix + 1) as u8);
        for e in self.groups.iter_mut() {
            if e.account == account && e.group.strategy() == strategy {
                e.in_spec = false;
            }
        }
        for sp in specs {
            let mut insts = [InstId(0); 4];
            for (d, l) in insts.iter_mut().zip(sp.legs.iter()) {
                *d = l.inst;
            }
            let n_legs = sp.n_legs.min(4);
            match self
                .groups
                .binary_search_by_key(&(account, sp.id), |e| (e.account, e.group))
            {
                Ok(i) => {
                    if let Some(e) = self.groups.get_mut(i) {
                        e.in_spec = true;
                        e.n_legs = n_legs;
                        e.insts = insts;
                        e.st.view.n_legs = n_legs;
                    }
                }
                Err(i) => {
                    if self.groups.len() >= self.groups.capacity() {
                        debug_assert!(false, "LegTracker capacity (max_groups) exceeded");
                        continue;
                    }
                    let mut st = LegState::EMPTY;
                    st.view.n_legs = n_legs;
                    self.groups.insert(
                        i,
                        GroupEntry {
                            account,
                            group: sp.id,
                            n_legs,
                            insts,
                            in_spec: true,
                            st,
                        },
                    );
                }
            }
        }
        // Drop removed groups whose state is all zero.
        self.groups
            .retain(|e| e.in_spec || !e.st.is_zero_ignoring_legs_def());
    }

    /// Leg index of `inst` in (a, g); `NO_LEG` when untracked.
    pub(crate) fn leg_ix_of(&self, a: AccountId, g: GroupId, inst: InstId) -> u8 {
        let Some(e) = self.group_ix(a, g).and_then(|i| self.groups.get(i)) else {
            return SlotExt::NO_LEG;
        };
        e.insts
            .iter()
            .take(usize::from(e.n_legs))
            .position(|x| *x == inst)
            .map_or(SlotExt::NO_LEG, |i| i as u8)
    }

    /// Booked fill into a leg (signed net), with the group's latest price.
    pub(crate) fn leg_fill(&mut self, a: AccountId, g: GroupId, leg_ix: u8, signed: Qty, px: Px, now: Tsc) {
        let Some(gi) = self.group_ix(a, g) else { return };
        if let Some(e) = self.groups.get_mut(gi) {
            if let Some(l) = e.st.view.leg.get_mut(usize::from(leg_ix)) {
                l.net = Qty(l.net.0.saturating_add(signed.0));
            }
            e.st.set_last_px(px);
            e.st.last_fill_tsc = now;
        }
        self.recompute_view(gi);
    }

    /// Signed adjustment of the leg of `inst` in (a, g).
    pub(crate) fn leg_add(&mut self, a: AccountId, g: GroupId, inst: InstId, delta: Qty) {
        let ix = self.leg_ix_of(a, g, inst);
        let Some(gi) = self.group_ix(a, g) else { return };
        if let Some(l) = self
            .groups
            .get_mut(gi)
            .and_then(|e| e.st.view.leg.get_mut(usize::from(ix)))
        {
            l.net = Qty(l.net.0.saturating_add(delta.0));
        }
        self.recompute_view(gi);
    }

    /// Adds to the residual of leg `ix` of (a, g).
    pub(crate) fn residual_add(&mut self, a: AccountId, g: GroupId, ix: u8, delta: i64) {
        let Some(gi) = self.group_ix(a, g) else { return };
        if let Some(l) = self
            .groups
            .get_mut(gi)
            .and_then(|e| e.st.view.leg.get_mut(usize::from(ix)))
        {
            l.residual = Qty(l.residual.0.saturating_add(delta));
        }
    }

    pub(crate) fn residual_get(&self, a: AccountId, g: GroupId, ix: u8) -> i64 {
        self.legs(a, g)
            .view
            .leg
            .get(usize::from(ix))
            .map_or(0, |l| l.residual.0)
    }

    /// Late base-asset fee of the latest link's first leg: `delta × ratio` into the hedge leg's residual.
    pub(crate) fn residual_from_fee(&mut self, a: AccountId, g: GroupId, inst: InstId, delta: Qty) {
        let last = self.legs(a, g).last_link;
        let Some(i) = self.link_ix(last) else { return };
        let Some(e) = self.links.get(i).copied() else {
            return;
        };
        if e.state == LinkState::Broken || self.first_inst(i) != inst {
            return;
        }
        let Some((_, plan)) = self.plans.get(i).copied() else {
            return;
        };
        let side = self.first_side(i);
        let num = i128::from(plan.ratio_num);
        let den = i128::from(plan.ratio_den.max(1));
        let r = (i128::from(delta.0) * i128::from(side.sign()) * num / den) as i64;
        let hix = self.leg_ix_of(a, g, plan.inst);
        self.residual_add(a, g, hix, r);
    }

    /// HARD release candidates: (a, group) with filled naked quantity ≠ 0, ascending, in batches.
    pub fn naked_groups(&self, a: AccountId, out: &mut FixedVec<GroupId, 64>, cursor: &mut u32) -> bool {
        let start = self.groups.partition_point(|e| e.account < a);
        let mut i = (start as u32).max(*cursor) as usize;
        while let Some(e) = self.groups.get(i) {
            if e.account != a {
                break;
            }
            let n = usize::from(e.n_legs).min(4);
            let sum: i128 = e.st.view.leg.iter().take(n).map(|l| i128::from(l.net.0)).sum();
            if sum != 0 && out.push(e.group).is_err() {
                *cursor = i as u32;
                return true;
            }
            i += 1;
        }
        *cursor = i as u32;
        false
    }

    /// Whether (a, g) is quiescent (README "LegTracker").
    pub(crate) fn group_quiescent(&self, a: AccountId, g: GroupId) -> bool {
        let Some(e) = self.group_ix(a, g).and_then(|i| self.groups.get(i)) else {
            return false;
        };
        let v = &e.st.view;
        v.open_links == 0
            && !v.has_unknown
            && !v.has_pending
            && v.leg
                .iter()
                .take(usize::from(e.n_legs))
                .all(|l| l.inflight_immediate.0 == 0)
    }

    /// `WriteOffResidual{account, group}` (README "残余核销").
    pub fn write_off_residual(&mut self, a: AccountId, g: GroupId) -> Result<WriteOff, OpError> {
        if g == GROUP_NONE || !self.group_quiescent(a, g) {
            return Err(OpError::NotQuiescent);
        }
        let gi = self.group_ix(a, g).ok_or(OpError::NotQuiescent)?;
        let e = self.groups.get(gi).ok_or(OpError::NotQuiescent)?;
        let n = usize::from(e.n_legs).min(4);
        let sum: i64 =
            e.st.view
                .leg
                .iter()
                .take(n)
                .map(|l| l.net.0)
                .fold(0i64, i64::saturating_add);
        let mut wo = WriteOff {
            venue: hft_types::VenueId(0),
            inst: InstId(0),
            leg_ix: 0,
            qty: Qty(0),
        };
        let mut excess: Option<usize> = None;
        if sum != 0 {
            let mut best = 0i64;
            for (i, l) in e.st.view.leg.iter().take(n).enumerate() {
                if l.net.0.signum() == sum.signum() && l.net.0.saturating_abs() > best {
                    best = l.net.0.saturating_abs();
                    excess = Some(i);
                }
            }
        }
        let insts = e.insts;
        if let Some(ix) = excess {
            let inst = insts.get(ix).copied().unwrap_or(InstId(0));
            wo = WriteOff {
                venue: self.inst(inst).venue,
                inst,
                leg_ix: ix as u8,
                qty: Qty(sum),
            };
        }
        if let Some(e) = self.groups.get_mut(gi) {
            if let Some(ix) = excess
                && let Some(l) = e.st.view.leg.get_mut(ix)
            {
                l.net = Qty(l.net.0.saturating_sub(sum));
            }
            for l in e.st.view.leg.iter_mut() {
                l.residual = Qty(0);
            }
        }
        self.recompute_view(gi);
        Ok(wo)
    }
}

impl LegState {
    /// All dynamic state zero (`n_legs` is a definition, not state).
    pub(crate) fn is_zero_ignoring_legs_def(&self) -> bool {
        let mut c = *self;
        c.view.n_legs = 0;
        c.is_zero()
    }
}
