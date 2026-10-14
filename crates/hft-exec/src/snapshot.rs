//! Snapshot section 21 (README "快照与恢复"): row writing, `restore`, `recovery_end` and
//! `recovery_timers`.

use hft_strategy_sdk::{
    BrokenReason, ExecSpec, GroupTable, LegTarget, MakerParams, PairParams, RefView, RestoreError,
    SimulParams, SnapshotReader, SnapshotWriter, StuckReason, TakerSliceParams, TargetIntent, TargetOrigin,
    TargetReason, TargetState, Urgency,
};
use hft_types::{AccountId, GroupId, RejectBits, Tsc};
use zerocopy::{FromZeros, IntoBytes, TryFromBytes};

use crate::types::*;
use crate::{Exec, ExecOut};

const CELL_LEN: u32 = (size_of::<ExecCellRow>() + size_of::<ExecTargetRow>()) as u32;

fn spec_row(e: &ExecSpec) -> ExecSpecRow {
    let mut r = ExecSpecRow::new_zeroed();
    r.algo = e.algo();
    let b: &[u8] = match e {
        ExecSpec::TakerSlice(p) => p.as_bytes(),
        ExecSpec::PairOnFill(p) => p.as_bytes(),
        ExecSpec::Simultaneous(p) => p.as_bytes(),
        ExecSpec::MakerFirst(p) => p.as_bytes(),
    };
    for (d, s) in r.params.iter_mut().zip(b.iter()) {
        *d = *s;
    }
    r
}

fn spec_of(r: &ExecSpecRow) -> Result<ExecSpec, RestoreError> {
    if r._pad != [0; 7] {
        return Err(RestoreError::BadValue);
    }
    fn take<T: TryFromBytes + zerocopy::KnownLayout + zerocopy::Immutable>(
        p: &[u8; 128],
    ) -> Result<T, RestoreError> {
        let n = size_of::<T>();
        let (a, tail) = p.split_at(n);
        if tail.iter().any(|x| *x != 0) {
            return Err(RestoreError::BadValue);
        }
        T::try_read_from_bytes(a).map_err(|_| RestoreError::BadValue)
    }
    Ok(match r.algo {
        1 => ExecSpec::TakerSlice(take::<TakerSliceParams>(&r.params)?),
        2 => ExecSpec::PairOnFill(take::<PairParams>(&r.params)?),
        3 => ExecSpec::Simultaneous(take::<SimulParams>(&r.params)?),
        4 => ExecSpec::MakerFirst(take::<MakerParams>(&r.params)?),
        _ => return Err(RestoreError::BadValue),
    })
}

fn target_row(c: &TargetCell) -> ExecTargetRow {
    let mut r = ExecTargetRow::new_zeroed();
    if !c.has_target {
        return r;
    }
    let t = &c.view.intent;
    r.account = t.account.0;
    r.group = t.group.0;
    for (d, l) in r.legs.iter_mut().zip(t.legs.iter()) {
        *d = LegTargetRow::of(l);
    }
    r.n_legs = t.n_legs;
    r.reason = t.reason as u8;
    r.urgency = t.urgency as u8;
    r.band = t.band;
    r.exec = spec_row(&t.exec);
    r.tag = t.tag;
    r
}

fn reason_of(c: u8) -> Option<TargetReason> {
    Some(match c {
        1 => TargetReason::Rebalance,
        2 => TargetReason::Exit,
        3 => TargetReason::MarginStress,
        4 => TargetReason::Hard,
        5 => TargetReason::Manual,
        _ => return None,
    })
}

fn urgency_of(c: u8) -> Option<Urgency> {
    Some(match c {
        1 => Urgency::Passive,
        2 => Urgency::Normal,
        3 => Urgency::Urgent,
        _ => return None,
    })
}

fn origin_of(c: u8) -> Option<TargetOrigin> {
    hft_event::TargetOriginWire::try_read_from_bytes(&[c])
        .ok()
        .map(TargetOrigin::from)
}

/// `t` after `recovery_end` (README "快照与恢复": downtime not counted).
pub(crate) fn shift(t: Tsc, now: Tsc, stop: Tsc, hz_prev: u64, hz_cur: u64) -> Tsc {
    if t.0 == 0 {
        return t;
    }
    let conv = |d: u64| -> u64 {
        if hz_prev == 0 {
            return d;
        }
        let v = u128::from(d) * u128::from(hz_cur) / u128::from(hz_prev);
        u64::try_from(v).unwrap_or(u64::MAX)
    };
    if t.0 <= stop.0 {
        Tsc(now.0.saturating_sub(conv(stop.0 - t.0)).max(1))
    } else {
        Tsc(now.0.saturating_add(conv(t.0 - stop.0)))
    }
}

impl Exec {
    /// A cell gets a row when anything differs from a fresh cell (EXE-452), so that `restore`
    /// reproduces every later decision (cooldowns, order numbering, timer registrations).
    fn cell_nonempty(&self, cix: usize) -> bool {
        let Some(c) = self.cells.get(cix) else {
            return false;
        };
        if c.has_target || self.is_pending(cix) {
            return true;
        }
        let mut e = crate::empty_cell(c.account, c.group, c.owner);
        e.n_legs = c.n_legs;
        e.legs = c.legs;
        self.row_of(c, false).as_bytes() != self.row_of(&e, false).as_bytes()
    }

    fn cell_row(&self, cix: usize) -> ExecCellRow {
        match self.cells.get(cix) {
            Some(c) => {
                let mut row = self.row_of(c, false);
                row.flags |= self.work_flags(cix);
                row
            }
            None => ExecCellRow::new_zeroed(),
        }
    }

    fn row_of(&self, c: &TargetCell, pending: bool) -> ExecCellRow {
        let mut r = ExecCellRow::new_zeroed();
        r.account = c.account.0;
        r.state = c.view.state.code();
        match c.view.state {
            TargetState::Stuck(s) => r.stuck = s.code(),
            TargetState::Broken(b) => r.broken = b.code(),
            _ => {}
        }
        r.group = c.group.0;
        r.owner = c.owner.0;
        r.n_legs = c.n_legs;
        r.has_target = u8::from(c.has_target);
        r.origin = hft_event::TargetOriginWire::from(c.view.origin) as u8;
        let mut f = 0u8;
        if pending {
            f |= RF_PENDING;
        }
        if c.serial != 0 {
            f |= RF_SERIAL;
        }
        if c.hold & MANUAL_HOLD != 0 {
            f |= RF_MANUAL_HOLD;
        }
        if c.hold & LINK_HOLD != 0 {
            f |= RF_LINK_HOLD;
        }
        if c.hold & MANUAL_HOLD_DEFERRED != 0 {
            f |= RF_MANUAL_HOLD_DEFERRED;
        }
        r.flags = f;
        r.deferred = c.deferred;
        r.cap_block = c.cap_block;
        r.fix_count = c.fix.count;
        r.fail_streak = c.fail_streak;
        r.nofill_streak = c.nofill_streak;
        r.fund_shrink = c.fund_shrink;
        r.def_from = c.def_from;
        r.gated_bits = c.gated_bits.0;
        r.n_slices = c.view.n_slices;
        r.force_steps = c.force_steps;
        r.aux16 = c.aux16;
        r.set_seq = c.view.set_seq;
        r.set_tsc = c.view.set_tsc;
        r.last_slice_tsc = c.view.last_slice_tsc;
        r.wake_at = c.wake_at;
        r.gated_since = c.gated_since;
        r.stuck_since = c.stuck_since;
        r.fix_first_tsc = c.fix.first_tsc;
        r.fix_next_tsc = c.fix.next_tsc;
        r.ext_deadline = c.ext_deadline;
        r.broken_since = c.broken_since;
        r.tolerance = c.tolerance;
        r.filled = c.view.filled;
        r.own_infl = c.aux;
        r.active = c.active;
        r.auto_fix = c.auto_fix;
        if c.is_ungrouped() {
            for (i, (d, l)) in r
                .legs
                .iter_mut()
                .zip(c.legs.iter())
                .enumerate()
                .take(usize::from(c.n_legs))
            {
                *d = self.leg_row(l, c.slot.get(i).copied().unwrap_or(0));
            }
        }
        r
    }

    /// Writes section 21 (Cell rows, Acct rows, Gate rows; keys ascending).
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.begin_section(SECTION_EXEC, ROW_VERSION);
        // Cells sorted by (account, group): per account the ungrouped cell (GROUP_NONE) first.
        for (k, a) in self.accts.iter().enumerate() {
            let u = self.n_grouped + k;
            for cix in core::iter::once(u)
                .chain((0..self.n_grouped).filter(|&i| self.cells.get(i).is_some_and(|c| c.account == a.id)))
            {
                if !self.cell_nonempty(cix) {
                    continue;
                }
                let Some(c) = self.cells.get(cix) else { continue };
                w.put_bytes(
                    ExecRowHdr {
                        kind: ROW_CELL,
                        _pad: [0; 3],
                        len: CELL_LEN,
                    }
                    .as_bytes(),
                );
                w.put_bytes(self.cell_row(cix).as_bytes());
                w.put_bytes(target_row(c).as_bytes());
                w.add_rows(1);
            }
        }
        for (k, a) in self.accts.iter().enumerate() {
            let Some(s) = self.acct.get(k) else { continue };
            if !(s.force_active || s.flatten_lock || s.force_kind != FORCE_NONE) {
                continue;
            }
            let r = ExecAcctRow {
                account: a.id.0,
                force_active: u8::from(s.force_active),
                flatten_lock: u8::from(s.flatten_lock),
                force_kind: s.force_kind,
                force_flags: s.force_flags,
                _pad: [0; 3],
                force_venues: s.force_venues,
                force_frac: s.force_frac,
            };
            w.put_bytes(
                ExecRowHdr {
                    kind: ROW_ACCT,
                    _pad: [0; 3],
                    len: 24,
                }
                .as_bytes(),
            );
            w.put_bytes(r.as_bytes());
            w.add_rows(1);
        }
        for (k, a) in self.accts.iter().enumerate() {
            let Some(s) = self.acct.get(k) else { continue };
            let r = ExecGateRow {
                account: a.id.0,
                gate_open: u8::from(s.gate_open),
                _pad: [0; 6],
                venue_open: s.venue_open,
                gate_closed_tsc: s.gate_closed_tsc,
            };
            w.put_bytes(
                ExecRowHdr {
                    kind: ROW_GATE,
                    _pad: [0; 3],
                    len: 24,
                }
                .as_bytes(),
            );
            w.put_bytes(r.as_bytes());
            w.add_rows(1);
        }
        w.end_section();
    }

    /// Loads a snapshot (recovery, verification replay, advisor mirror); no TSC is changed.
    pub fn restore(
        r: &mut SnapshotReader,
        cfg: &ExecConfig,
        cap: &Capacities,
        groups: &GroupTable,
        rd: &RefView,
    ) -> Result<Exec, RestoreError> {
        let mut x = Exec::new(cfg, cap).map_err(|_| RestoreError::BadValue)?;
        let mut out = ExecOut::with_caps(crate::OutCaps::all(0));
        x.on_groups_changed(groups, rd, &mut out);
        let Some(sec) = r.find_section(SECTION_EXEC)? else {
            return Ok(x);
        };
        if sec.version != 1 && sec.version != ROW_VERSION {
            return Err(RestoreError::VersionUnsupported);
        }
        let mut s = sec.reader();
        let mut phase = 0u8;
        for _ in 0..sec.count {
            let h: ExecRowHdr = s.get_row()?;
            if h.kind < phase || !(1..=3).contains(&h.kind) {
                return Err(RestoreError::BadValue);
            }
            phase = h.kind;
            match h.kind {
                ROW_CELL => {
                    if h.len != CELL_LEN {
                        return Err(RestoreError::BadValue);
                    }
                    let cr: ExecCellRow = s.get_row()?;
                    let tr: ExecTargetRow = s.get_row()?;
                    x.load_cell(&cr, &tr, sec.version)?;
                }
                ROW_ACCT => {
                    if h.len != 24 {
                        return Err(RestoreError::BadValue);
                    }
                    let ar: ExecAcctRow = s.get_row()?;
                    if ar.force_kind > FORCE_FLATTEN || ar.force_active > 1 || ar.flatten_lock > 1 {
                        return Err(RestoreError::BadValue);
                    }
                    let k = x.acct_ix(AccountId(ar.account)).ok_or(RestoreError::BadValue)?;
                    if let Some(st) = x.acct.get_mut(k) {
                        st.force_active = ar.force_active != 0;
                        st.flatten_lock = ar.flatten_lock != 0;
                        st.force_kind = ar.force_kind;
                        st.force_flags = ar.force_flags;
                        st.force_venues = ar.force_venues;
                        st.force_frac = ar.force_frac;
                    }
                }
                _ => {
                    if h.len != 24 {
                        return Err(RestoreError::BadValue);
                    }
                    let gr: ExecGateRow = s.get_row()?;
                    let k = x.acct_ix(AccountId(gr.account)).ok_or(RestoreError::BadValue)?;
                    if let Some(st) = x.acct.get_mut(k) {
                        st.gate_open = gr.gate_open != 0;
                        st.venue_open = gr.venue_open;
                        st.gate_closed_tsc = gr.gate_closed_tsc;
                    }
                }
            }
        }
        x.continue_set = x.advancing.iter().any(|w| *w != 0);
        Ok(x)
    }

    fn load_cell(&mut self, r: &ExecCellRow, t: &ExecTargetRow, version: u16) -> Result<(), RestoreError> {
        let a = AccountId(r.account);
        let g = GroupId(r.group);
        let cix = self.cix_of(a, g).ok_or(RestoreError::BadValue)?;
        let state = match r.state {
            1 => TargetState::Idle,
            2 => TargetState::Working,
            3 => TargetState::Stuck(
                StuckReason::from_code(r.stuck, RejectBits(r.gated_bits)).ok_or(RestoreError::BadValue)?,
            ),
            4 => TargetState::Broken(BrokenReason::from_code(r.broken).ok_or(RestoreError::BadValue)?),
            _ => return Err(RestoreError::BadValue),
        };
        if r.stuck > 8 || r.broken > 4 {
            return Err(RestoreError::BadValue);
        }
        if r.active.kind > ACT_FORCE {
            return Err(RestoreError::BadValue);
        }
        if AutoFixPhase::from_code(r.auto_fix.phase).is_none()
            || !matches!(r.auto_fix.fail, 0 | 1 | 2 | 4 | 6 | 7 | 8 | 9)
        {
            return Err(RestoreError::BadValue);
        }
        if r.legs
            .iter()
            .any(|l| l.kind > LEG_CHAIN || l._pad != 0 || l._pad2 != 0)
        {
            return Err(RestoreError::BadValue);
        }
        let origin = origin_of(r.origin);
        let intent = if r.has_target != 0 {
            if origin.is_none() {
                return Err(RestoreError::BadValue);
            }
            let mut legs = [LegTarget::ZERO; 4];
            for (d, l) in legs.iter_mut().zip(t.legs.iter()) {
                if l._pad != [0; 7] {
                    return Err(RestoreError::BadValue);
                }
                *d = LegTarget::new(l.leg_ix, l.qty);
            }
            if t._pad0 != [0; 3] || t._pad1 != [0; 5] || t._pad2 != 0 {
                return Err(RestoreError::BadValue);
            }
            TargetIntent {
                account: AccountId(t.account),
                _pad0: [0; 3],
                group: GroupId(t.group),
                legs,
                n_legs: t.n_legs,
                reason: reason_of(t.reason).ok_or(RestoreError::BadValue)?,
                urgency: urgency_of(t.urgency).ok_or(RestoreError::BadValue)?,
                _pad1: [0; 5],
                band: t.band,
                exec: spec_of(&t.exec)?,
                tag: t.tag,
                _pad2: 0,
            }
        } else {
            if t.as_bytes().iter().any(|b| *b != 0) {
                return Err(RestoreError::BadValue);
            }
            crate::targets::zero_intent(a, g)
        };
        let pending = r.flags & RF_PENDING != 0;
        let forced = version == 1 || r.flags & RF_FORCED != 0;
        let advancing = version >= 2 && r.flags & RF_ADVANCING != 0;
        let advancing_forced = version >= 2 && r.flags & RF_ADVANCING_FORCED != 0;
        if (version == 1 && r.flags & (RF_FORCED | RF_ADVANCING | RF_ADVANCING_FORCED) != 0)
            || (version >= 2 && forced && !pending)
            || (advancing_forced && !advancing)
        {
            return Err(RestoreError::BadValue);
        }
        let Some(c) = self.cells.get_mut(cix) else {
            return Err(RestoreError::BadValue);
        };
        c.view.intent = intent;
        c.view.origin = origin.unwrap_or(TargetOrigin::Strategy);
        c.view.state = state;
        c.view.set_seq = r.set_seq;
        #[cfg(feature = "planted")]
        if crate::planted::on(crate::planted::SNAPSHOT_FIELD) {
            c.view.set_seq = 0;
        }
        c.view.set_tsc = r.set_tsc;
        c.view.last_slice_tsc = r.last_slice_tsc;
        c.view.n_slices = r.n_slices;
        c.view.filled = r.filled;
        c.has_target = r.has_target != 0;
        c.deferred = r.deferred;
        c.def_from = r.def_from;
        c.cap_block = r.cap_block;
        c.serial = u8::from(r.flags & RF_SERIAL != 0);
        let mut hold = 0;
        if r.flags & RF_MANUAL_HOLD != 0 {
            hold |= MANUAL_HOLD;
        }
        if r.flags & RF_LINK_HOLD != 0 {
            hold |= LINK_HOLD;
        }
        if r.flags & RF_MANUAL_HOLD_DEFERRED != 0 {
            hold |= MANUAL_HOLD_DEFERRED;
        }
        c.hold = hold;
        c.fail_streak = r.fail_streak;
        c.nofill_streak = r.nofill_streak;
        c.fund_shrink = r.fund_shrink;
        c.force_steps = r.force_steps;
        c.aux16 = r.aux16;
        c.gated_bits = RejectBits(r.gated_bits);
        c.active = r.active;
        c.wake_at = r.wake_at;
        c.gated_since = r.gated_since;
        c.stuck_since = r.stuck_since;
        c.fix = FixState {
            count: r.fix_count,
            first_tsc: r.fix_first_tsc,
            next_tsc: r.fix_next_tsc,
        };
        c.ext_deadline = r.ext_deadline;
        c.tolerance = r.tolerance;
        c.broken_since = r.broken_since;
        c.auto_fix = r.auto_fix;
        c.aux = r.own_infl;
        if c.is_ungrouped() {
            for (k, l) in r.legs.iter().enumerate() {
                if let Some(s) = c.slot.get_mut(k) {
                    *s = l.slot;
                }
            }
            for (d, l) in c.legs.iter_mut().zip(r.legs.iter()) {
                *d = CellLeg {
                    venue: l.venue,
                    kind: l.kind,
                    reduce_only_cap: l.reduce_only_cap,
                    inst: l.inst,
                    lot: l.lot,
                    min_qty: l.min_qty,
                };
            }
            c.n_legs = r.n_legs.min(4);
        }
        self.restore_work(cix, r.flags | if version == 1 && pending { RF_FORCED } else { 0 });
        self.recompute_pair_lot(cix);
        Ok(())
    }

    /// End of cold-start recovery: shifts every registered TSC (downtime not counted), closes all
    /// gates, holds links in flight.
    pub fn recovery_end(&mut self, now: Tsc, stop_tsc: Tsc, hz_prev: u64, hz_cur: u64) {
        let sh = |t: Tsc| shift(t, now, stop_tsc, hz_prev, hz_cur);
        self.tsc_per_ms = (hz_cur / 1_000_000).max(1);
        for st in self.acct.iter_mut() {
            st.gate_open = false;
            st.venue_open = 0;
            st.gate_closed_tsc = now;
        }
        let n = self.cell_count();
        for cix in 0..n {
            let Some(c) = self.cells.get_mut(cix) else {
                continue;
            };
            c.view.set_tsc = sh(c.view.set_tsc);
            c.view.last_slice_tsc = sh(c.view.last_slice_tsc);
            c.wake_at = sh(c.wake_at);
            c.gated_since = sh(c.gated_since);
            c.stuck_since = sh(c.stuck_since);
            c.fix.first_tsc = sh(c.fix.first_tsc);
            c.fix.next_tsc = sh(c.fix.next_tsc);
            c.ext_deadline = sh(c.ext_deadline);
            c.broken_since = sh(c.broken_since);
            c.auto_fix.due_tsc = sh(c.auto_fix.due_tsc);
            c.auto_fix.started_tsc = sh(c.auto_fix.started_tsc);
            c.auto_fix.run_mark_tsc = sh(c.auto_fix.run_mark_tsc);
            if c.active.link_id != 0 {
                c.hold |= LINK_HOLD;
            }
            c.aux16 &= !0xFF;
        }
        for cix in 0..n {
            let ts = self.cell_timers(cix);
            let mut m = 0u16;
            for t in ts.iter().flatten() {
                if let Some(k) = t.kind() {
                    m |= 1 << (k as u16);
                }
            }
            if let Some(c) = self.cells.get_mut(cix) {
                c.aux16 |= m;
            }
        }
    }

    fn cell_timers(&self, cix: usize) -> [Option<ExecTimerReq>; 5] {
        let mut r = [None; 5];
        let Some(c) = self.cells.get(cix) else { return r };
        let mk = |k: ExecTimerKind, at: Tsc| {
            Some(ExecTimerReq {
                cancel: false,
                timer_id: ExecTimerReq::id(k, c.account),
                group: c.group,
                at,
            })
        };
        if c.wake_at.0 != 0 {
            r[0] = mk(ExecTimerKind::Wake, c.wake_at);
        }
        if c.fix.next_tsc.0 != 0 {
            r[1] = mk(ExecTimerKind::Fix, c.fix.next_tsc);
        }
        if c.ext_deadline.0 != 0 {
            r[2] = mk(ExecTimerKind::External, c.ext_deadline);
        }
        if c.auto_fix.phase() == AutoFixPhase::Primed {
            r[3] = mk(ExecTimerKind::AutoFix, c.auto_fix.due_tsc);
        }
        if let Some((_, at, _)) = self.alarm_plan(cix) {
            r[4] = mk(ExecTimerKind::Alarm, at);
        }
        r
    }

    /// Timers to register after `recovery_end` (each cell at most one per kind).
    pub fn recovery_timers(&self) -> impl Iterator<Item = ExecTimerReq> + '_ {
        (0..self.cell_count()).flat_map(move |cix| self.cell_timers(cix).into_iter().flatten())
    }
}
