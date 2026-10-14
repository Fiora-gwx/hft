//! `hft-exec`: the execution layer. Owns the per-(account, group) target cells and the execution
//! algorithms; turns `TargetIntent`s into order requests; repairs naked legs, handles residuals,
//! external fills, Broken disposal (auto bounded close-out) and margin-stress engine reduction.
//!
//! Pure state machine on the sequencer thread: no I/O, no clock, no allocation after `new`
//! (except `on_groups_changed` and `restore`), no `dyn`, no floating point; nothing panics.
//! Specification: `crates/hft-exec/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[cfg(target_endian = "big")]
compile_error!("hft-exec: snapshot rows are little-endian only");

mod advance;
#[cfg(feature = "testkit")]
pub mod audit;
mod config_blob;

pub use config_blob::{EXEC_CONFIG_ROW_VERSION, ExecConfigRow};
mod broken;
pub mod calc;
pub mod decision;
mod events;
mod external;
mod fix;
mod force;
mod out;
#[cfg(feature = "planted")]
pub mod planted;
mod snapshot;
mod targets;
pub mod types;

use hft_event::{ExecDecision, Note, NoteLevel, NoteSource};
use hft_refdata::{InstHot, InstKind, RefData};
use hft_strategy_sdk::{
    ExecUpdate, ExecUpdateKind, GroupTable, RefView, StuckReason, TargetOrigin, TargetState, TargetView,
};
use hft_types::{AccountId, GroupId, InstId, Qty, StrategyInstanceId, Tsc, VenueId};
use zerocopy::FromZeros;

pub use decision::{NOTE_CODES, pack, unpack};
pub use out::{ExecOut, FixedVec, OutCaps};
pub use types::*;

use decision::{action, algo, table};

/// Orders of one cell emitted by the latest `advance` (place results come back in the same event).
#[derive(Copy, Clone, Debug)]
pub(crate) struct PendOrder {
    pub n: u16,
    pub leg: u8,
    pub kind: ExecOrderKind,
    pub sell: bool,
    pub qty: Qty,
    pub px: hft_types::Px,
    pub mid: hft_types::Px,
    pub cap_n: hft_types::Notional,
    pub valid: bool,
}

const PEND_NONE: PendOrder = PendOrder {
    n: 0,
    leg: 0,
    kind: ExecOrderKind::Slice,
    sell: false,
    qty: Qty(0),
    px: hft_types::Px(0),
    mid: hft_types::Px(0),
    cap_n: hft_types::Notional(0),
    valid: false,
};

/// Static data of one account.
#[derive(Copy, Clone, Debug)]
pub(crate) struct AcctInfo {
    pub id: AccountId,
    pub sim: bool,
    pub params: ExecParams,
    pub owner: StrategyInstanceId,
    pub venues: u64,
}

/// The execution layer (README "入口").
#[derive(Debug)]
pub struct Exec {
    #[cfg(feature = "testkit")]
    pub(crate) audit: audit::Audit,
    pub(crate) accts: Box<[AcctInfo]>,
    pub(crate) acct: Box<[AcctState]>,
    /// Grouped cells `[0, n_grouped)` sorted by (account, group), then one ungrouped cell per account.
    pub(crate) cells: Box<[TargetCell]>,
    pub(crate) n_grouped: usize,
    pub(crate) max_cells: usize,
    /// (inst, cix) sorted by inst (grouped cells only).
    pub(crate) inst_ix: Vec<(InstId, u16)>,
    pub(crate) pending: Box<[u64]>,
    pub(crate) forced: Box<[u64]>,
    pub(crate) pend: Box<[[PendOrder; 4]]>,
    /// Hot inst rows (copy of refdata), indexed by `InstId`.
    pub(crate) insts: Box<[InstHot]>,
    pub(crate) none_inst: InstHot,
    /// Perp insts of each venue (ungrouped scan).
    pub(crate) perps: Box<[Box<[InstId]>]>,
    /// `VenueCaps.reduce_only` per venue.
    pub(crate) venue_ro: [bool; 64],
    pub(crate) continue_set: bool,
    /// TSC ticks per ms of the latest call (alarm planning without a `Now`).
    pub(crate) tsc_per_ms: u64,
}

pub(crate) fn empty_cell(account: AccountId, group: GroupId, owner: StrategyInstanceId) -> TargetCell {
    TargetCell {
        view: TargetView {
            intent: targets::zero_intent(account, group),
            origin: TargetOrigin::Strategy,
            state: TargetState::Idle,
            set_seq: 0,
            set_tsc: Tsc(0),
            last_slice_tsc: Tsc(0),
            n_slices: 0,
            filled: [Qty(0); 4],
        },
        account,
        group,
        owner,
        n_legs: 0,
        has_target: false,
        deferred: 0,
        def_from: 0,
        cap_block: 0,
        serial: 0,
        hold: 0,
        fail_streak: 0,
        nofill_streak: 0,
        fund_shrink: 0,
        force_steps: 0,
        aux16: 0,
        gated_bits: hft_types::RejectBits(0),
        legs: [CellLeg::ZERO; 4],
        pair_lot: Qty(0),
        active: ActiveSliceRow::new_zeroed(),
        wake_at: Tsc(0),
        gated_since: Tsc(0),
        stuck_since: Tsc(0),
        fix: FixState::ZERO,
        ext_deadline: Tsc(0),
        tolerance: hft_types::Rate(0),
        broken_since: Tsc(0),
        auto_fix: AutoFixState::new_zeroed(),
        aux: [Qty(0); 4],
        slot: [0; 4],
    }
}

fn check_slice(p: &hft_strategy_sdk::TakerSliceParams) -> bool {
    (1..=10).contains(&p.depth_levels) && p.depth_frac.0 > 0 && p.depth_frac.0 <= 1_000_000_000
}

fn check_params(p: &ExecParams) -> Result<(), ExecConfigError> {
    if !check_slice(&p.force_slice) {
        return Err(ExecConfigError::ForceSlice);
    }
    if p.fix_max == 0 || p.max_slice_fails == 0 || p.max_nofill == 0 || p.passive_cooldown_mult == 0 {
        return Err(ExecConfigError::ZeroLimit);
    }
    Ok(())
}

impl Exec {
    /// Preallocates every cell; all gates closed.
    pub fn new(cfg: &ExecConfig, cap: &Capacities) -> Result<Exec, ExecConfigError> {
        check_params(&cfg.default)?;
        for a in &cfg.accounts {
            if let Some(p) = &a.params {
                check_params(p)?;
            }
        }
        let n_acct = cfg.accounts.len();
        let total = (cap.max_cells_total as usize).saturating_add(n_acct);
        if total > 65_535 {
            return Err(ExecConfigError::Capacity);
        }
        let mut accts: Vec<AcctInfo> = cfg
            .accounts
            .iter()
            .map(|a| AcctInfo {
                id: a.account,
                sim: a.sim,
                params: a.params.unwrap_or(cfg.default),
                owner: StrategyInstanceId::default_none(),
                venues: 0,
            })
            .collect();
        accts.sort_by_key(|a| a.id);
        if accts
            .windows(2)
            .any(|w| w.first().map(|x| x.id) == w.get(1).map(|x| x.id))
        {
            return Err(ExecConfigError::Capacity);
        }
        let mut cells = Vec::with_capacity(total);
        for a in &accts {
            cells.push(empty_cell(a.id, GroupId(0), a.owner));
        }
        // Remaining slots are filled lazily by `on_groups_changed`; keep capacity.
        let words = total.div_ceil(64).max(1);
        let none_inst = none_hot();
        Ok(Exec {
            #[cfg(feature = "testkit")]
            audit: audit::Audit::new(total),
            acct: vec![AcctState::ZERO; accts.len()].into_boxed_slice(),
            accts: accts.into_boxed_slice(),
            cells: {
                let mut v = cells;
                v.resize(
                    total,
                    empty_cell(AccountId(0), GroupId(0), StrategyInstanceId::default_none()),
                );
                v.into_boxed_slice()
            },
            n_grouped: 0,
            max_cells: cap.max_cells_total as usize,
            inst_ix: Vec::with_capacity((cap.max_cells_total as usize).saturating_mul(4)),
            pending: vec![0u64; words].into_boxed_slice(),
            forced: vec![0u64; words].into_boxed_slice(),
            pend: vec![[PEND_NONE; 4]; total].into_boxed_slice(),
            insts: Vec::new().into_boxed_slice(),
            none_inst,
            perps: vec![Box::<[InstId]>::default(); 64].into_boxed_slice(),
            venue_ro: [false; 64],
            continue_set: false,
            tsc_per_ms: 1_000_000,
        })
    }

    // ------------------------------------------------------------------ cell table plumbing

    /// Allocated cells (`max_cells_total` + accounts).
    pub fn capacity(&self) -> usize {
        self.cells.len()
    }

    /// Number of cells in use (grouped + one ungrouped per account).
    pub fn cell_count(&self) -> usize {
        self.n_grouped + self.accts.len()
    }

    pub(crate) fn acct_ix(&self, a: AccountId) -> Option<usize> {
        self.accts.binary_search_by_key(&a, |x| x.id).ok()
    }

    /// cix of (a, g); `GROUP_NONE` = the account's ungrouped cell.
    pub(crate) fn cix_of(&self, a: AccountId, g: GroupId) -> Option<usize> {
        if g.0 == 0 {
            return self.acct_ix(a).map(|k| self.n_grouped + k);
        }
        let grouped = self.cells.get(..self.n_grouped)?;
        grouped
            .binary_search_by(|c| (c.account, c.group).cmp(&(a, g)))
            .ok()
    }

    pub(crate) fn acct_of_cix(&self, cix: usize) -> usize {
        let a = self.cells.get(cix).map_or(AccountId(0), |c| c.account);
        self.acct_ix(a).unwrap_or(0)
    }

    pub(crate) fn params(&self, k: usize) -> ExecParams {
        self.accts.get(k).map_or_else(ExecParams::default, |a| a.params)
    }

    /// The parameters account `a` currently runs with (read-only DST observation).
    #[cfg(feature = "testkit")]
    pub fn account_params(&self, a: AccountId) -> Option<ExecParams> {
        self.acct_ix(a).map(|k| self.params(k))
    }

    pub(crate) fn inst(&self, i: InstId) -> &InstHot {
        self.insts.get(i.0 as usize).unwrap_or(&self.none_inst)
    }

    pub(crate) fn mark_pending(&mut self, cix: usize, forced: bool) {
        let (w, b) = (cix / 64, cix % 64);
        if let Some(x) = self.pending.get_mut(w) {
            *x |= 1 << b;
        }
        if forced && let Some(x) = self.forced.get_mut(w) {
            *x |= 1 << b;
        }
    }

    pub(crate) fn is_pending(&self, cix: usize) -> bool {
        self.pending
            .get(cix / 64)
            .is_some_and(|x| x & (1 << (cix % 64)) != 0)
    }

    pub(crate) fn take_pending(&mut self, cix: usize) -> bool {
        let (w, b) = (cix / 64, cix % 64);
        let f = self.forced.get(w).is_some_and(|x| x & (1 << b) != 0);
        if let Some(x) = self.pending.get_mut(w) {
            *x &= !(1 << b);
        }
        if let Some(x) = self.forced.get_mut(w) {
            *x &= !(1 << b);
        }
        f
    }

    /// Marks every cell of account index `k` (grouped and ungrouped).
    pub(crate) fn mark_account(&mut self, k: usize, forced: bool) {
        let a = self.accts.get(k).map_or(AccountId(0), |x| x.id);
        for cix in 0..self.cell_count() {
            if self.cells.get(cix).is_some_and(|c| c.account == a) {
                self.mark_pending(cix, forced);
            }
        }
    }

    // ------------------------------------------------------------------ groups

    /// Rebuilds the cell table from the union of every instance's `GroupSpec`s (command path).
    /// Same (account, group) keeps its state; a cell whose leg definition changed loses its target
    /// (`TargetCleared` written by the next `advance`).
    pub fn on_groups_changed(&mut self, groups: &GroupTable, rd: &RefView, _out: &mut ExecOut) {
        self.load_refdata(rd.refdata());
        let rdd = rd.refdata();
        let mut keys: Vec<(AccountId, GroupId, usize)> = Vec::new();
        for (gix, s) in groups.iter() {
            if s.id.0 == 0 || s.id == hft_types::GROUP_ALL {
                continue;
            }
            let Some(a) = rdd.account_of(s.id.strategy()) else {
                continue;
            };
            if self.acct_ix(a).is_none() {
                continue;
            }
            keys.push((a, s.id, usize::from(gix)));
        }
        keys.sort_by_key(|k| (k.0, k.1));
        keys.dedup_by_key(|k| (k.0, k.1));
        keys.truncate(self.max_cells);
        let old: Vec<TargetCell> = self
            .cells
            .get(..self.cell_count())
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let old_pend: Vec<bool> = (0..self.cell_count()).map(|i| self.is_pending(i)).collect();
        let old_n = self.n_grouped;
        for w in self.pending.iter_mut() {
            *w = 0;
        }
        for w in self.forced.iter_mut() {
            *w = 0;
        }
        let n = keys.len();
        for (i, (a, g, gix)) in keys.iter().enumerate() {
            let spec = groups.get(*gix as u16);
            let mut legs = [CellLeg::ZERO; 4];
            let nl = usize::from(spec.n_legs).min(4);
            for (d, l) in legs.iter_mut().zip(spec.legs.iter()).take(nl) {
                *d = self.leg_ref(l.inst, l.venue, l.kind == hft_event::LegDef::CHAIN);
            }
            let owner = g.strategy();
            let prev = old
                .get(..old_n)
                .and_then(|o| o.binary_search_by(|c| (c.account, c.group).cmp(&(*a, *g))).ok());
            let mut c = match prev.and_then(|p| old.get(p)) {
                Some(p) => *p,
                None => empty_cell(*a, *g, owner),
            };
            let changed = prev.is_some() && (c.n_legs as usize != nl || c.legs.get(..nl) != legs.get(..nl));
            c.legs = legs;
            c.n_legs = nl as u8;
            c.owner = owner;
            if let Some(slot) = self.cells.get_mut(i) {
                *slot = c;
            }
            let was_pending = prev.and_then(|p| old_pend.get(p).copied()).unwrap_or(false);
            if changed {
                if let Some(c) = self.cells.get_mut(i)
                    && c.has_target
                {
                    c.has_target = false;
                    c.deferred |= D_CLEARED_BY_CMD;
                    c.pair_lot = Qty(0);
                    if !matches!(c.view.state, TargetState::Broken(_)) {
                        c.def_from = c.view.state.code();
                        c.view.state = TargetState::Idle;
                    }
                }
                self.mark_pending(i, true);
            } else if was_pending {
                self.mark_pending(i, true);
            }
            if let Some(slot) = self.pend.get_mut(i) {
                *slot = [PEND_NONE; 4];
            }
        }
        // Ungrouped cells after the grouped ones.
        for k in 0..self.accts.len() {
            let mut c = old
                .get(old_n + k)
                .copied()
                .unwrap_or_else(|| empty_cell(AccountId(0), GroupId(0), StrategyInstanceId::default_none()));
            let info = self.accts.get(k).copied();
            if let Some(info) = info {
                c.account = info.id;
                c.group = GroupId(0);
                let owner = rdd.account(info.id).bound_instance;
                c.owner = owner;
                if let Some(x) = self.accts.get_mut(k) {
                    x.owner = owner;
                    x.venues = rdd.account(info.id).venues;
                }
            }
            if let Some(slot) = self.cells.get_mut(n + k) {
                *slot = c;
            }
            if old_pend.get(old_n + k).copied().unwrap_or(false) {
                self.mark_pending(n + k, true);
            }
        }
        self.n_grouped = n;
        self.rebuild_inst_index();
        for cix in 0..n {
            self.recompute_pair_lot(cix);
        }
    }

    pub(crate) fn rebuild_inst_index(&mut self) {
        self.inst_ix.clear();
        for (cix, c) in self.cells.iter().take(self.n_grouped).enumerate() {
            for l in c.legs.iter().take(usize::from(c.n_legs)) {
                self.inst_ix.push((l.inst, cix as u16));
            }
        }
        self.inst_ix.sort_unstable();
    }

    pub(crate) fn load_refdata(&mut self, rd: &RefData) {
        let mut max_id = 0u32;
        for v in rd.venues_in_order() {
            for i in rd.insts_of_venue(*v) {
                max_id = max_id.max(i.0);
            }
        }
        let mut insts = vec![self.none_inst; max_id as usize + 1];
        let mut perps: Vec<Box<[InstId]>> = vec![Box::default(); 64];
        for v in rd.venues_in_order() {
            let mut p = Vec::new();
            for i in rd.insts_of_venue(*v) {
                let h = rd.inst(*i);
                if let Some(s) = insts.get_mut(i.0 as usize) {
                    *s = *h;
                }
                if h.kind == InstKind::Perp {
                    p.push(*i);
                }
            }
            if let Some(s) = perps.get_mut(usize::from(v.0)) {
                *s = p.into_boxed_slice();
            }
            if let Some(r) = self.venue_ro.get_mut(usize::from(v.0)) {
                *r = rd.caps(*v).reduce_only;
            }
        }
        self.insts = insts.into_boxed_slice();
        self.perps = perps.into_boxed_slice();
    }

    /// Snapshot row form of a cell leg.
    pub(crate) fn leg_row(&self, l: &CellLeg, slot: u32) -> LegRef {
        LegRef {
            venue: l.venue,
            kind: l.kind,
            reduce_only_cap: l.reduce_only_cap,
            _pad: 0,
            inst: l.inst,
            lot: l.lot,
            min_qty: l.min_qty,
            slot,
            _pad2: 0,
        }
    }

    pub(crate) fn leg_ref(&self, inst: InstId, venue: VenueId, chain: bool) -> CellLeg {
        let h = *self.inst(inst);
        let kind = if chain {
            LEG_CHAIN
        } else if h.kind == InstKind::Perp {
            LEG_PERP
        } else {
            LEG_SPOT
        };
        CellLeg {
            venue,
            kind,
            reduce_only_cap: u8::from(self.venue_ro.get(usize::from(venue.0)).copied().unwrap_or(false)),
            inst,
            lot: Qty(h.lot),
            min_qty: h.min_qty,
        }
    }

    // ------------------------------------------------------------------ read-only queries

    /// Projection of a cell (`Views::target` data source).
    pub fn view(&self, a: AccountId, g: GroupId) -> Option<&TargetView> {
        let cix = self.cix_of(a, g)?;
        self.cells.get(cix).map(|c| &c.view)
    }

    /// Some cell is pending.
    pub fn has_pending(&self) -> bool {
        self.pending.iter().any(|w| *w != 0)
    }

    /// Drain may proceed for every account.
    pub fn drain_ready(&self) -> bool {
        self.accts.iter().all(|a| self.drain_ready_acct(a.id))
    }

    /// Execution-layer Drain condition of one account.
    pub fn drain_ready_acct(&self, a: AccountId) -> bool {
        self.cells
            .iter()
            .take(self.cell_count())
            .filter(|c| c.account == a)
            .all(|c| {
                c.active.kind == ACT_NONE
                    && c.fix.first_tsc.0 == 0
                    && c.ext_deadline.0 == 0
                    && c.auto_fix.phase() != AutoFixPhase::Running
                    && !Self::engine_unreached(c)
            })
    }

    /// Some account has an unreached `MarginStress`/`Hard` engine target.
    pub fn engine_reduce_pending(&self) -> bool {
        self.cells
            .iter()
            .take(self.cell_count())
            .any(Self::engine_unreached)
    }

    pub(crate) fn engine_unreached(c: &TargetCell) -> bool {
        c.has_target
            && matches!(c.view.origin, TargetOrigin::Engine(_))
            && matches!(
                c.view.intent.reason,
                hft_strategy_sdk::TargetReason::MarginStress | hft_strategy_sdk::TargetReason::Hard
            )
            && c.view.state != TargetState::Idle
    }

    // ------------------------------------------------------------------ record helpers

    /// A decision pre-filled from cell `cix`.
    pub(crate) fn dec(&self, cix: usize, act: u8, now_seq: u64) -> ExecDecision {
        let mut d = ExecDecision::new_zeroed();
        let Some(c) = self.cells.get(cix) else {
            return d;
        };
        d.trigger_seq = now_seq;
        d.set_seq = c.view.set_seq;
        d.group = c.group;
        d.tag = if c.has_target { c.view.intent.tag } else { 0 };
        d.owner = c.owner;
        d.account = c.account;
        d.action = act;
        d.algo = if c.has_target {
            if matches!(c.view.origin, TargetOrigin::Engine(_)) && (c.serial != 0 || self.is_broken(cix)) {
                algo::ENGINE
            } else {
                c.view.intent.exec.algo()
            }
        } else {
            algo::NONE
        };
        d.origin = hft_event::TargetOriginWire::from(c.view.origin) as u8;
        d.reason = if c.has_target {
            c.view.intent.reason as u8
        } else {
            0
        };
        d.urgency = if c.has_target {
            self.urgency_of(cix) as u8
        } else {
            0
        };
        d.state_from = c.view.state.code();
        d.state_to = c.view.state.code();
        d.leg_ix = 0xFF;
        d.first_leg_ix = 0xFF;
        d
    }

    pub(crate) fn note(&self, cix: usize, n: u32, level: NoteLevel, v: [u64; 8]) -> Note {
        let c = self.cells.get(cix);
        Note {
            code: decision::note_code(n),
            level,
            source: NoteSource::Exec,
            strategy: c.map_or(hft_types::StrategyId(0), |c| c.owner),
            _pad: 0,
            group: c.map_or(GroupId(0), |c| c.group),
            inst: InstId(0),
            v,
        }
    }

    pub(crate) fn is_broken(&self, cix: usize) -> bool {
        self.cells
            .get(cix)
            .is_some_and(|c| matches!(c.view.state, TargetState::Broken(_)))
    }

    /// Effective urgency (MarginStress/Hard targets and accounts in force/flatten are Urgent).
    pub(crate) fn urgency_of(&self, cix: usize) -> hft_strategy_sdk::Urgency {
        use hft_strategy_sdk::{TargetReason, Urgency};
        let Some(c) = self.cells.get(cix) else {
            return Urgency::Normal;
        };
        let k = self.acct_of_cix(cix);
        let st = self.acct.get(k).copied().unwrap_or(AcctState::ZERO);
        if matches!(
            c.view.intent.reason,
            TargetReason::MarginStress | TargetReason::Hard
        ) || st.force_active
            || st.flatten_lock
        {
            Urgency::Urgent
        } else {
            c.view.intent.urgency
        }
    }

    pub(crate) fn notify(
        &self,
        cix: usize,
        kind: ExecUpdateKind,
        origin: Option<TargetOrigin>,
        out: &mut ExecOut,
    ) {
        let Some(c) = self.cells.get(cix) else { return };
        if c.is_ungrouped() {
            return;
        }
        let mut target = [Qty(0); 4];
        if c.has_target {
            for l in c.view.intent.legs() {
                if let Some(t) = target.get_mut(usize::from(l.leg_ix)) {
                    *t = l.qty;
                }
            }
        }
        let u = ExecUpdate {
            group: c.group,
            tag: if c.has_target { c.view.intent.tag } else { 0 },
            origin: origin.unwrap_or(c.view.origin),
            kind,
            target,
            filled: c.view.filled,
            // Filled by the sequencer from the OMS leg tracker when it queues the event.
            inflight: [Qty(0); 4],
        };
        out.notify.push(ExecNotify {
            account: c.account,
            owner: c.owner,
            u,
        });
    }

    /// State transition with its `State` record and notification (`detail` built from the new state).
    pub(crate) fn set_state(&mut self, cix: usize, new: TargetState, seq: u64, out: &mut ExecOut) {
        let Some(old) = self.cells.get(cix).map(|c| c.view.state) else {
            return;
        };
        if old == new {
            return;
        }
        if let Some(c) = self.cells.get_mut(cix) {
            c.view.state = new;
        }
        let mut d = self.dec(cix, action::STATE, seq);
        d.state_from = old.code();
        d.state_to = new.code();
        let (det, gate) = state_detail(new);
        d.detail = det;
        d.gate = gate;
        out.decisions.push(d);
        match new {
            TargetState::Stuck(r) => self.notify(cix, ExecUpdateKind::Stuck(r), None, out),
            TargetState::Broken(r) => self.notify(cix, ExecUpdateKind::Broken(r), None, out),
            _ => {}
        }
    }

    // ------------------------------------------------------------------ timers

    /// Registers timer `kind` of cell `cix` at `at`; `saved` = the time currently registered
    /// (if the mask bit is set). Moving later cancels first; earlier or new only sets.
    pub(crate) fn reg_timer(
        &mut self,
        cix: usize,
        kind: ExecTimerKind,
        at: Tsc,
        saved: Tsc,
        out: &mut ExecOut,
    ) {
        let Some(c) = self.cells.get(cix) else { return };
        let bit = 1u16 << (kind as u16);
        let has = c.aux16 & bit != 0;
        if has && saved == at {
            return;
        }
        if out.timers.remaining() < 2 {
            if let Some(c) = self.cells.get_mut(cix) {
                c.deferred |= D_TIMERS;
            }
            self.mark_pending(cix, true);
            return;
        }
        let id = ExecTimerReq::id(kind, c.account);
        let group = c.group;
        if has && at.0 > saved.0 {
            out.timers.push(ExecTimerReq {
                cancel: true,
                timer_id: id,
                group,
                at: Tsc(0),
            });
        }
        out.timers.push(ExecTimerReq {
            cancel: false,
            timer_id: id,
            group,
            at,
        });
        if let Some(c) = self.cells.get_mut(cix) {
            c.aux16 |= bit;
        }
    }

    pub(crate) fn cancel_timer(&mut self, cix: usize, kind: ExecTimerKind, out: &mut ExecOut) {
        let Some(c) = self.cells.get(cix) else { return };
        let bit = 1u16 << (kind as u16);
        if c.aux16 & bit == 0 {
            return;
        }
        out.timers.push(ExecTimerReq {
            cancel: true,
            timer_id: ExecTimerReq::id(kind, c.account),
            group: c.group,
            at: Tsc(0),
        });
        if let Some(c) = self.cells.get_mut(cix) {
            c.aux16 &= !bit;
        }
    }

    /// Ticks per ms of `now`.
    pub(crate) fn ms(now: &hft_strategy_sdk::Now, ms: u32) -> u64 {
        u64::from(ms).saturating_mul(now.tsc_per_ms.max(1))
    }

    // ------------------------------------------------------------------ test access

    /// Cell of (a, g) (`GROUP_NONE` = ungrouped cell).
    pub fn cell(&self, a: AccountId, g: GroupId) -> Option<&TargetCell> {
        self.cix_of(a, g).and_then(|i| self.cells.get(i))
    }

    /// Account state.
    pub fn acct_state(&self, a: AccountId) -> Option<&AcctState> {
        self.acct_ix(a).and_then(|k| self.acct.get(k))
    }

    /// cix of (a, g).
    pub fn cix(&self, a: AccountId, g: GroupId) -> Option<u16> {
        self.cix_of(a, g).map(|x| x as u16)
    }
}

/// Test-only state access (feature `testing`; dev-dependencies only).
#[cfg(feature = "testing")]
impl Exec {
    pub fn cell_mut(&mut self, a: AccountId, g: GroupId) -> Option<&mut TargetCell> {
        let i = self.cix_of(a, g)?;
        self.cells.get_mut(i)
    }
    pub fn acct_mut(&mut self, a: AccountId) -> Option<&mut AcctState> {
        let k = self.acct_ix(a)?;
        self.acct.get_mut(k)
    }
    /// Marks (a, g) pending as if an event touched it.
    pub fn touch_cell(&mut self, a: AccountId, g: GroupId) {
        if let Some(i) = self.cix_of(a, g) {
            self.mark_pending(i, true);
        }
    }
    /// Recomputes `pair_lot` after a test edited a target in place.
    pub fn refresh(&mut self, a: AccountId, g: GroupId) {
        if let Some(i) = self.cix_of(a, g) {
            self.recompute_pair_lot(i);
        }
    }
}

/// The NONE inst row (all zero, `status = Invalid`).
pub(crate) fn none_hot() -> InstHot {
    InstHot {
        venue: VenueId(0),
        status: hft_refdata::InstStatus::Invalid,
        kind: InstKind::Spot,
        flags: 0,
        px_exp: 0,
        qty_exp: 0,
        px_wire_exp: 0,
        qty_wire_exp: 0,
        asset: hft_types::AssetId(0),
        quote: hft_types::AssetId(0),
        book_window: 0,
        tick: 0,
        lot: 0,
        min_qty: Qty(0),
        max_qty: Qty(0),
        min_notional: hft_types::Notional(0),
        l2_depth: 0,
        crossed_tolerance: 0,
        _pad: 0,
    }
}

/// `(detail, gate)` of a `State` record entering `s`.
pub(crate) fn state_detail(s: TargetState) -> (u32, u32) {
    match s {
        TargetState::Stuck(r) => {
            let gate = if let StuckReason::Gated(b) = r { b.0 } else { 0 };
            (pack(table::STUCK, r.code(), 0), gate)
        }
        TargetState::Broken(r) => (pack(table::BROKEN, r.code(), 0), 0),
        _ => (0, 0),
    }
}

/// Extension used for the placeholder owner of cells without a binding.
pub(crate) trait NoneOwner {
    fn default_none() -> Self;
}

impl NoneOwner for StrategyInstanceId {
    fn default_none() -> Self {
        hft_types::StrategyId(0)
    }
}

/// `TargetState::Stuck` with `Gated(bits)` or `StaleQuote` (quote-only gate).
pub(crate) fn gated_reason(bits: hft_types::RejectBits) -> StuckReason {
    if bits.0 == hft_types::RejectBits::STALE_MID.0 {
        StuckReason::StaleQuote
    } else {
        StuckReason::Gated(hft_types::RejectBits(
            bits.0 & !hft_types::RejectBits::STALE_MID.0,
        ))
    }
}
