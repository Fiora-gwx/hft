//! Margin-stress default reduction and `engine_flatten` (README "保证金危急的默认减仓"): engine
//! targets (proportional, closer to zero), resting-order cancellation, ungrouped perps, serial legs
//! (Broken and `SerialFallback`), the `HardAlign` target at L3 release and `flatten_lock`.

use hft_event::NoteLevel;
use hft_strategy_sdk::{
    EngineOrigin, ExecSpec, ExecUpdateKind, FirstLeg, LegTarget, Now, PairParams, PortfolioRead, PxMode,
    ResidualPolicy, StuckReason, TargetIntent, TargetOrigin, TargetReason, TargetState, Urgency, Views,
};
use hft_types::{AccountId, InstId, Qty, Rate, RejectBits, Tsc, VenueId};

use crate::advance::LegOut;
use crate::calc;
use crate::decision::{acct_group, action, engine, note, pack, table};
use crate::targets::ratio;
use crate::types::*;
use crate::{Exec, ExecOut};

impl Exec {
    /// `ForceDeleverage` (L3 step): registers the request; targets are set by the next `advance`.
    pub fn force_deleverage(
        &mut self,
        a: AccountId,
        venue: VenueId,
        frac: Rate,
        _now: Now,
        _out: &mut ExecOut,
    ) {
        let Some(k) = self.acct_ix(a) else { return };
        if let Some(st) = self.acct.get_mut(k) {
            st.force_active = true;
            if st.force_kind != FORCE_FLATTEN {
                st.force_kind = FORCE_VENUES;
                st.force_frac = frac;
            }
            st.force_venues |= 1u64 << (venue.0 & 63);
        }
        self.mark_account(k, true);
    }

    /// `Kill{Account, flatten}`: engine target 0 on every cell and ungrouped perp; sets
    /// `flatten_lock`. Idempotent.
    pub fn engine_flatten(&mut self, a: AccountId, _now: Now, _out: &mut ExecOut) {
        let Some(k) = self.acct_ix(a) else { return };
        if let Some(st) = self.acct.get_mut(k) {
            if !st.flatten_lock {
                st.force_flags |= FORCE_FULL;
            }
            st.force_kind = FORCE_FLATTEN;
            st.flatten_lock = true;
        }
        self.mark_account(k, true);
    }

    /// Step 0 of `advance`: sets the engine targets of an account's pending request.
    pub(crate) fn apply_force_req<V: Views>(&mut self, k: usize, v: &V, now: Now) {
        let Some(st) = self.acct.get(k).copied() else {
            return;
        };
        let Some(info) = self.accts.get(k).copied() else {
            return;
        };
        let flatten = st.force_kind == FORCE_FLATTEN;
        let full = st.force_flags & FORCE_FULL != 0;
        if let Some(s) = self.acct.get_mut(k) {
            s.force_kind = FORCE_NONE;
            s.force_flags = 0;
        }
        for cix in 0..self.n_grouped {
            let Some(c) = self.cells.get(cix).copied() else {
                continue;
            };
            if c.account != info.id {
                continue;
            }
            let ln = self.legs_now(cix, v);
            let n = usize::from(c.n_legs);
            let mut tg = [Qty(0); 4];
            let engine_now = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
            if flatten {
                if !full {
                    let zero_engine = engine_now && c.view.intent.legs().iter().all(|l| l.qty.0 == 0);
                    let flat = !c.has_target
                        && (0..n)
                            .all(|i| c.legs[i].kind == LEG_CHAIN || self.in_band_dust(cix, i, ln[i].pos, v));
                    if zero_engine || flat {
                        continue;
                    }
                }
            } else {
                let hit = (0..n).any(|i| {
                    let l = c.legs[i];
                    l.kind == LEG_PERP
                        && st.force_venues & (1u64 << (l.venue.0 & 63)) != 0
                        && ln[i].pos.0 != 0
                });
                if !hit {
                    continue;
                }
                for (i, t) in tg.iter_mut().enumerate().take(n) {
                    let l = c.legs[i];
                    let base = if engine_now {
                        c.target_of(i).unwrap_or(ln[i].pos)
                    } else {
                        ln[i].pos
                    };
                    let keep = Rate(1_000_000_000 - st.force_frac.0.clamp(0, 1_000_000_000));
                    let p = calc::floor_lot(calc::mul_rate(base, keep), l.lot.0.max(1));
                    let cur = c.target_of(i).unwrap_or(ln[i].pos);
                    *t = closer(p, cur);
                }
            }
            let keep_hard = engine_now && c.view.intent.reason == TargetReason::Hard;
            let (reason, origin) = if flatten {
                (
                    TargetReason::Hard,
                    TargetOrigin::Engine(EngineOrigin::KillFlatten),
                )
            } else if keep_hard {
                (TargetReason::Hard, c.view.origin)
            } else {
                (
                    TargetReason::MarginStress,
                    TargetOrigin::Engine(EngineOrigin::MarginStress),
                )
            };
            self.set_engine(cix, &tg, reason, origin, v, now);
            #[cfg(feature = "testkit")]
            for (i, leg) in ln.iter().enumerate().take(n) {
                if c.legs[i].kind == LEG_CHAIN {
                    continue;
                }
                let Some(target) = self.cells.get(cix).and_then(|cell| cell.target_of(i)) else {
                    self.audit.fail();
                    continue;
                };
                self.audit.push(crate::audit::ForceFacts {
                    account: c.account,
                    group: c.group,
                    inst: c.legs[i].inst,
                    position: leg.pos,
                    previous_target: c.target_of(i),
                    previous_engine: engine_now,
                    fraction: if flatten {
                        Rate(1_000_000_000)
                    } else {
                        st.force_frac
                    },
                    target,
                });
            }
        }
        // Ungrouped perps.
        let venues = if flatten { info.venues } else { st.force_venues };
        self.load_ungrouped(k, venues, flatten || st.flatten_lock, st.force_frac, v, now);
    }

    fn in_band_dust<V: Views>(&self, cix: usize, i: usize, pos: Qty, v: &V) -> bool {
        self.in_band(cix, i, Qty(0), pos, v)
    }

    /// Installs an engine target (state edit; records are written when the cell is advanced).
    fn set_engine<V: Views>(
        &mut self,
        cix: usize,
        tg: &[Qty; 4],
        reason: TargetReason,
        origin: TargetOrigin,
        v: &V,
        now: Now,
    ) {
        let k = self.acct_of_cix(cix);
        let p = self.params(k);
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let n = usize::from(c.n_legs);
        let mut legs = [LegTarget::ZERO; 4];
        let mut nl = 0u8;
        for (i, t) in tg.iter().enumerate().take(n) {
            if c.legs[i].kind == LEG_CHAIN {
                continue;
            }
            if let Some(slot) = legs.get_mut(usize::from(nl)) {
                *slot = LegTarget::new(i as u8, *t);
            }
            nl += 1;
        }
        let broken = matches!(c.view.state, TargetState::Broken(_));
        let spec = if broken || n < 2 || c.is_ungrouped() {
            ExecSpec::TakerSlice(p.force_slice)
        } else {
            let (num, den) = if c.has_target && crate::targets::is_pair(&c.view.intent.exec) {
                ratio(&c.view.intent.exec)
            } else {
                (1, 1)
            };
            ExecSpec::PairOnFill(PairParams {
                slice: p.force_slice,
                first_leg: FirstLeg::PerpFirst,
                first_leg_ix: 0,
                hedge_px_mode: PxMode::Cross,
                _pad0: [0; 5],
                hedge_cross: p.force_slice.max_cross,
                hedge_max_slip: p.force_slice.max_cross,
                ratio_num: num,
                ratio_den: den,
                fix_tolerance: if c.tolerance.0 != 0 {
                    c.tolerance
                } else {
                    p.fix_tolerance_default
                },
                residual: ResidualPolicy::TrimExcess,
                _pad1: [0; 7],
            })
        };
        let intent = TargetIntent {
            account: c.account,
            _pad0: [0; 3],
            group: c.group,
            legs,
            n_legs: nl,
            reason,
            urgency: Urgency::Urgent,
            _pad1: [0; 5],
            band: Rate(0),
            exec: spec,
            tag: if c.has_target { c.view.intent.tag } else { 0 },
            _pad2: 0,
        };
        let replaced_strategy = c.has_target && c.view.origin == TargetOrigin::Strategy;
        let _ = v;
        let Some(cm) = self.cells.get_mut(cix) else { return };
        if !broken {
            let new = if replaced_strategy
                || matches!(cm.view.state, TargetState::Stuck(StuckReason::EngineOverride))
            {
                TargetState::Stuck(StuckReason::EngineOverride)
            } else {
                TargetState::Working
            };
            if cm.view.state != new {
                if cm.deferred & D_STATE == 0 {
                    cm.def_from = cm.view.state.code();
                }
                cm.deferred |= D_STATE;
                cm.view.state = new;
            }
        }
        cm.view.intent = intent;
        cm.view.origin = origin;
        cm.view.set_seq = now.seq;
        cm.view.set_tsc = now.tsc;
        cm.view.filled = if cm.is_ungrouped() {
            cm.view.filled
        } else {
            [Qty(0); 4]
        };
        cm.has_target = true;
        cm.fail_streak = 0;
        cm.nofill_streak = 0;
        cm.fund_shrink = 0;
        cm.gated_since = Tsc(0);
        cm.gated_bits = RejectBits(0);
        cm.deferred &= !D_ENGINE_CLEARED;
        cm.deferred |= D_ENGINE_SET;
        cm.force_steps = cm.force_steps.saturating_add(1);
        if cm.hold & MANUAL_HOLD != 0 {
            cm.hold &= !MANUAL_HOLD;
            cm.fix.first_tsc = Tsc(0);
            cm.deferred |= D_HOLD_REL;
        }
        if cm.ext_deadline.0 != 0 {
            cm.ext_deadline = Tsc(0);
            cm.deferred |= D_EXT_CLR;
        }
        self.recompute_pair_lot(cix);
        self.mark_pending(cix, true);
    }

    /// Loads at most four ungrouped perp positions of `venues` into the account's ungrouped cell.
    fn load_ungrouped<V: Views>(
        &mut self,
        k: usize,
        venues: u64,
        flatten: bool,
        frac: Rate,
        v: &V,
        now: Now,
    ) {
        let Some(info) = self.accts.get(k).copied() else {
            return;
        };
        let ucix = self.n_grouped + k;
        let p = info.params;
        let Some(u) = self.cells.get(ucix).copied() else {
            return;
        };
        let pf = v.portfolio(info.id);
        // Candidates: (notional, venue, inst, u).
        let mut best: [(i128, VenueId, InstId, Qty); 4] = [(0, VenueId(0), InstId(0), Qty(0)); 4];
        for vb in 0..64u8 {
            if venues & (1u64 << vb) == 0 {
                continue;
            }
            let venue = VenueId(vb);
            let Some(list) = self.perps.get(usize::from(vb)) else {
                continue;
            };
            for &i in list.iter() {
                let acct = pf.perp(venue, i).qty;
                if acct.0 == 0 {
                    continue;
                }
                let mut uq = self.ungrouped_pos(info.id, venue, i, v);
                if uq.0 == 0 || uq.0.signum() != acct.0.signum() {
                    continue;
                }
                if uq.0.unsigned_abs() > acct.0.unsigned_abs() {
                    uq = acct;
                }
                let h = self.inst(i);
                let mark = self.mark(v, i);
                if uq.0 == 0 || (!flatten && calc::notional_le(uq, mark, h, p.dust_notional)) {
                    continue;
                }
                let nv = calc::notional_ceil(uq, mark, h).max(1);
                if let Some(pos) = best.iter().position(|b| nv > b.0) {
                    let mut j = 3;
                    while j > pos {
                        best[j] = best[j - 1];
                        j -= 1;
                    }
                    best[pos] = (nv, venue, i, uq);
                }
            }
        }
        let n = best.iter().filter(|b| b.0 > 0).count();
        if n == 0 && !u.has_target {
            return;
        }
        let mut legs = [CellLeg::ZERO; 4];
        let mut tg = [Qty(0); 4];
        let mut aux = [Qty(0); 4];
        let mut released = [Qty(0); 4];
        let mut act = <ActiveSliceRow as zerocopy::FromZeros>::new_zeroed();
        let mut unk = 0u16;
        let mut slots = [0u32; 4];
        #[cfg(feature = "testkit")]
        let mut force_facts = [None; 4];
        for (ix, b) in best.iter().enumerate().take(n) {
            legs[ix] = self.leg_ref(b.2, b.1, false);
            // Previous unfinished step target of the same inst, else u.
            let prev = (0..usize::from(u.n_legs)).find(|&j| u.legs[j].inst == b.2);
            let base = match prev {
                Some(j) => {
                    aux[ix] = u.aux[j];
                    released[ix] = u.view.filled[j];
                    slots[ix] = u.slot[j];
                    // Slice tracking follows the inst to its new index.
                    let (ob, nb) = (1u8 << j, 1u8 << ix);
                    for m in 1..4 {
                        if u.active.ext[m] & ob != 0 {
                            act.ext[m] |= nb;
                        }
                    }
                    if u.aux16 & (1 << (AUX_UNKNOWN_SHIFT as usize + j)) != 0 {
                        unk |= 1 << (AUX_UNKNOWN_SHIFT as usize + ix);
                    }
                    if usize::from(u.active.leg_ix) == j {
                        act.leg_ix = ix as u8;
                    }
                    u.target_of(j).unwrap_or(b.3)
                }
                None => b.3,
            };
            let keep = Rate(1_000_000_000 - frac.0.clamp(0, 1_000_000_000));
            tg[ix] = if flatten {
                Qty(0)
            } else {
                calc::floor_lot(calc::mul_rate(base, keep), legs[ix].lot.0.max(1))
            };
            #[cfg(feature = "testkit")]
            {
                force_facts[ix] = Some(crate::audit::ForceFacts {
                    account: info.id,
                    group: hft_types::GroupId(0),
                    inst: b.2,
                    position: b.3,
                    previous_target: prev.and_then(|j| u.target_of(j)),
                    previous_engine: true,
                    fraction: if flatten { Rate(1_000_000_000) } else { frac },
                    target: tg[ix],
                });
            }
        }
        if let Some(cm) = self.cells.get_mut(ucix) {
            cm.legs = legs;
            cm.n_legs = n as u8;
            cm.aux = aux;
            cm.view.filled = released;
            cm.slot = slots;
            if act.ext[1] != 0 {
                act.kind = u.active.kind;
                act.n = u.active.n;
                act.link_id = u.active.link_id;
                act.first = u.active.first;
                act.planned = u.active.planned;
                act.ext[0] = u.active.ext[0];
            } else {
                act.leg_ix = 0;
            }
            cm.active = act;
            let mask = 0xF << AUX_UNKNOWN_SHIFT;
            cm.aux16 = (cm.aux16 & !mask) | unk;
        }
        let reason = if flatten {
            TargetReason::Hard
        } else {
            TargetReason::MarginStress
        };
        let origin = TargetOrigin::Engine(if flatten {
            EngineOrigin::KillFlatten
        } else {
            EngineOrigin::MarginStress
        });
        if n == 0 {
            if let Some(cm) = self.cells.get_mut(ucix) {
                cm.has_target = false;
                cm.view.state = TargetState::Idle;
            }
            return;
        }
        self.set_engine(ucix, &tg, reason, origin, v, now);
        #[cfg(feature = "testkit")]
        for (ix, fact) in force_facts.into_iter().enumerate() {
            if let Some(mut fact) = fact {
                if let Some(target) = self.cells.get(ucix).and_then(|cell| cell.target_of(ix)) {
                    fact.target = target;
                    self.audit.push(fact);
                } else {
                    self.audit.fail();
                }
            }
        }
    }

    /// Records of a newly set engine target (`D_ENGINE_SET`).
    pub(crate) fn engine_set_records<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        let ag = acct_group(c.account.0, c.group.0);
        // ForceStep record.
        let lead = c
            .view
            .intent
            .legs()
            .iter()
            .find(|l| c.legs[usize::from(l.leg_ix)].kind == LEG_PERP)
            .copied()
            .or_else(|| c.view.intent.legs().first().copied());
        let mut d = self.dec(cix, action::FORCE_STEP, now.seq);
        d.detail = pack(
            table::ENGINE,
            if c.is_ungrouped() {
                engine::UNGROUPED
            } else {
                engine::STEP
            },
            0,
        );
        if let Some(l) = lead {
            d.leg_ix = l.leg_ix;
            d.target_qty = l.qty;
        }
        if c.deferred & D_STATE != 0 {
            d.state_from = c.def_from;
        }
        out.decisions.push(d);
        // Cancel the strategy's resting orders on every exchange leg.
        let mut bitmap = 0u16;
        for l in c.view.intent.legs() {
            let leg = c.legs[usize::from(l.leg_ix)];
            if leg.kind == LEG_CHAIN {
                continue;
            }
            bitmap |= 1 << l.leg_ix;
            let req = CancelResting {
                account: c.account,
                inst: leg.inst,
            };
            if !out.cancel_resting.iter().any(|x| *x == req) {
                out.cancel_resting.push(req);
            }
        }
        let mut d = self.dec(cix, action::FORCE_STEP, now.seq);
        d.detail = pack(table::ENGINE, engine::CANCEL_RESTING, bitmap);
        out.decisions.push(d);
        // Notes.
        if c.is_ungrouped() {
            let mut total = 0i128;
            for i in 0..usize::from(c.n_legs) {
                let l = c.legs[i];
                let pos = self.ungrouped_pos(c.account, l.venue, l.inst, v);
                total =
                    total.saturating_add(calc::notional_ceil(pos, self.mark(v, l.inst), self.inst(l.inst)));
            }
            let flatten = c.view.intent.reason == TargetReason::Hard;
            let venue = if flatten {
                0xFF
            } else {
                u64::from(st.force_venues.trailing_zeros().min(63))
            };
            let n = self.note(
                cix,
                note::EXEC_UNGROUPED_REDUCE,
                NoteLevel::P0,
                [
                    u64::from(c.account.0),
                    venue,
                    u64::from(c.n_legs),
                    u64::try_from(total.max(0) / 1_000_000).unwrap_or(u64::MAX),
                    u64::from(c.n_legs),
                    c.view.intent.reason as u64,
                    0,
                    0,
                ],
            );
            out.notes.push(n);
        } else {
            let n = self.note(
                cix,
                note::EXEC_FORCE_STEP,
                NoteLevel::P1,
                [ag, u64::from(c.force_steps), 0, 0, 0, 0, 0, 0],
            );
            out.notes.push(n);
        }
        if c.deferred & D_STATE != 0 {
            let mut r = self.dec(cix, action::STATE, now.seq);
            r.state_from = c.def_from;
            let (det, gate) = crate::state_detail(c.view.state);
            r.detail = det;
            r.gate = gate;
            out.decisions.push(r);
            if let TargetState::Stuck(s) = c.view.state {
                self.notify(cix, ExecUpdateKind::Stuck(s), None, out);
            }
        }
        if c.deferred & D_HOLD_REL != 0 {
            let n = self.note(
                cix,
                note::EXEC_MANUAL_HOLD,
                NoteLevel::Info,
                [ag, 0, crate::decision::hold_release::ENGINE_TARGET, 0, 0, 0, 0, 0],
            );
            out.notes.push(n);
        }
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.deferred &= !(D_ENGINE_SET | D_STATE | D_HOLD_REL);
        }
    }

    /// L3 released (`D_ENGINE_CLEARED`, no slice in flight): serial cells get the `HardAlign`
    /// target, others lose their engine target.
    pub(crate) fn engine_clear<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.deferred &= !D_ENGINE_CLEARED;
        }
        if !(c.has_target
            && matches!(c.view.origin, TargetOrigin::Engine(_))
            && c.view.intent.reason == TargetReason::MarginStress)
        {
            return;
        }
        if c.serial != 0 && !c.is_ungrouped() {
            let ln = self.legs_now(cix, v);
            let (num, den) = ratio(&c.view.intent.exec);
            let perp = (0..usize::from(c.n_legs))
                .find(|&i| c.legs[i].kind == LEG_PERP)
                .unwrap_or(0);
            let pnet = ln[perp].pos;
            let mut legs = [LegTarget::ZERO; 4];
            let mut nl = 0usize;
            for (i, (l, x)) in c
                .legs
                .iter()
                .zip(ln.iter())
                .enumerate()
                .take(usize::from(c.n_legs))
            {
                let t = if l.kind == LEG_PERP {
                    x.pos
                } else {
                    let s = x.pos;
                    let h =
                        (i128::from(pnet.0.unsigned_abs()) * i128::from(num) / i128::from(den.max(1))) as i64;
                    let h = h.min(s.0.unsigned_abs() as i64);
                    calc::floor_lot(Qty(h * s.0.signum()), l.lot.0.max(1))
                };
                if let Some(slot) = legs.get_mut(nl) {
                    *slot = LegTarget::new(i as u8, t);
                }
                nl += 1;
            }
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.view.intent.legs = legs;
                cm.view.intent.n_legs = nl as u8;
                cm.view.intent.reason = TargetReason::Hard;
                cm.view.intent.urgency = Urgency::Urgent;
                cm.view.intent.band = Rate(0);
                cm.view.set_seq = now.seq;
            }
            let mut d = self.dec(cix, action::FORCE_STEP, now.seq);
            d.detail = pack(table::ENGINE, engine::HARD_ALIGN, 0);
            d.leg_ix = perp as u8;
            d.target_qty = pnet;
            out.decisions.push(d);
            return;
        }
        let mut d = self.dec(cix, action::TARGET_CLEARED, now.seq);
        d.eflags |= crate::decision::eflags::CLEARED_BY_ENGINE;
        let broken = matches!(c.view.state, TargetState::Broken(_));
        if !broken {
            d.state_to = TargetState::Idle.code();
        }
        out.decisions.push(d);
        self.notify(
            cix,
            ExecUpdateKind::Done {
                cleared_by_engine: true,
            },
            None,
            out,
        );
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.has_target = false;
            cm.serial = 0;
            cm.pair_lot = Qty(0);
            if !broken {
                cm.view.state = TargetState::Idle;
            }
        }
        self.convert_deferred_hold(cix, v, now, out);
    }

    /// Switches a pair engine target to serial legs.
    pub(crate) fn serial_fallback<V: Views>(
        &mut self,
        cix: usize,
        perp: usize,
        reason: u16,
        v: &V,
        now: Now,
        out: &mut ExecOut,
    ) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let ln = self.legs_now(cix, v);
        if let Some(cm) = self.cells.get_mut(cix) {
            cm.serial = SERIAL_FALLBACK;
            if cm.active.kind == ACT_PAIR || cm.active.kind == ACT_SIMUL {
                cm.active.kind = ACT_NONE;
                cm.active.ext[1] = 0;
                cm.active.link_id = 0;
            }
        }
        let h = 1 - perp;
        let mut d = self.dec(cix, action::FORCE_STEP, now.seq);
        d.detail = pack(table::ENGINE, engine::SERIAL_FALLBACK, reason);
        d.leg_ix = h as u8;
        out.decisions.push(d);
        let n = self.note(
            cix,
            note::EXEC_SERIAL_FALLBACK,
            NoteLevel::P0,
            [
                u64::from(c.account.0),
                h as u64,
                u64::from(reason),
                ln[perp].pos.0 as u64,
                ln[h].pos.0 as u64,
                c.view.intent.reason as u64,
                0,
                0,
            ],
        );
        out.notes.push(n);
    }

    /// Serial legs of an engine target: perp legs (reduce-only) first, spot legs after.
    pub(crate) fn serial_step<V: Views>(&mut self, cix: usize, v: &V, now: Now, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let p = self.params(self.acct_of_cix(cix));
        if c.active.kind != ACT_NONE && c.active.ext[1] == 0 {
            return;
        }
        let ln = self.legs_now(cix, v);
        let mut perp_pending = false;
        for pass in [LEG_PERP, LEG_SPOT] {
            if pass == LEG_SPOT && perp_pending {
                break;
            }
            for lt in c.view.intent.legs() {
                let i = usize::from(lt.leg_ix);
                if c.legs[i].kind != pass {
                    continue;
                }
                let x = ln[i];
                let eff = Qty(x.pos.0.saturating_add(x.infl.0));
                let tq = self.leg_target(cix, lt.qty, eff);
                let done_leg =
                    self.in_band(cix, i, self.leg_target(cix, lt.qty, x.pos), x.pos, v) && x.infl.0 == 0;
                if done_leg {
                    continue;
                }
                if pass == LEG_PERP {
                    perp_pending = true;
                }
                if c.active.ext[1] & (1 << i) != 0 {
                    continue;
                }
                let ro_perp = c.legs[i].kind == LEG_PERP && c.legs[i].reduce_only_cap != 0;
                if x.infl.0 != 0 && !ro_perp {
                    continue;
                }
                if self.in_band(cix, i, tq, eff, v) {
                    continue;
                }
                if !self.time_ok_engine(cix, &now) {
                    self.wake_engine(cix, now, out);
                    return;
                }
                match self.leg_slice(cix, i, tq, x, &p.force_slice, ExecOrderKind::ForceStep, v, now) {
                    LegOut::Emit(mut e) => {
                        e.act = ACT_SERIAL;
                        e.algo = crate::decision::algo::ENGINE;
                        self.emit(cix, e, v, now, out);
                    }
                    LegOut::Gate(b) => {
                        self.gate(cix, b, true, !self.is_broken(cix), now, out);
                    }
                    LegOut::MinOrder | LegOut::NoLiq | LegOut::Wait | LegOut::Nothing => {
                        if pass == LEG_PERP {
                            // Remainder below the minimum order: the spot legs may start.
                            let below = matches!(
                                self.leg_slice(
                                    cix,
                                    i,
                                    tq,
                                    x,
                                    &p.force_slice,
                                    ExecOrderKind::ForceStep,
                                    v,
                                    now
                                ),
                                LegOut::MinOrder
                            );
                            if below {
                                if self
                                    .acct
                                    .get(self.acct_of_cix(cix))
                                    .is_some_and(|a| a.flatten_lock)
                                {
                                    self.min_order(cix, i, tq, x.pos, v, now, out);
                                } else {
                                    perp_pending = false;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn time_ok_engine(&self, cix: usize, now: &Now) -> bool {
        let p = self.params(self.acct_of_cix(cix));
        let Some(c) = self.cells.get(cix) else {
            return false;
        };
        if now.tsc.0 < c.wake_at.0 {
            return false;
        }
        let cd = self.cooldown_eff(cix, p.force_slice.cooldown_ms);
        c.view.last_slice_tsc.0 == 0 || now.tsc.0 >= c.view.last_slice_tsc.0.saturating_add(Self::ms(now, cd))
    }

    fn wake_engine(&mut self, cix: usize, now: Now, out: &mut ExecOut) {
        let p = self.params(self.acct_of_cix(cix));
        let Some(c) = self.cells.get(cix).copied() else {
            return;
        };
        let cd = self.cooldown_eff(cix, p.force_slice.cooldown_ms);
        let t = c
            .view
            .last_slice_tsc
            .0
            .saturating_add(Self::ms(&now, cd))
            .max(c.wake_at.0);
        if t > now.tsc.0 {
            self.reg_timer(cix, ExecTimerKind::Wake, Tsc(t), c.wake_at, out);
            if let Some(cm) = self.cells.get_mut(cix) {
                cm.wake_at = Tsc(t);
            }
        }
    }
}

/// The value closer to zero of a proportional target `p` and the current target `cur` (opposite
/// signs → 0).
fn closer(p: Qty, cur: Qty) -> Qty {
    if p.0 == 0 || cur.0 == 0 || p.0.signum() != cur.0.signum() {
        return Qty(0);
    }
    if p.0.unsigned_abs() <= cur.0.unsigned_abs() {
        p
    } else {
        cur
    }
}
