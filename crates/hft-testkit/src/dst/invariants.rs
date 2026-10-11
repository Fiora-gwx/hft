//! Invariant checkers: `I<n>` is `INV-n` of docs/tests/invariants.md (the only definition; README
//! "6. DST 框架" 不变量). Every checker reads the step's journal records, frames, [`StepInfo`] and the
//! debug-view observations before and after the step (never the engine itself), so enabling or
//! disabling checkers never changes a run (TK-070).
//!
//! Each `inv_<n>_…` function registers the invariant's standing case (`Scenario::new("INV-n", …)`:
//! every suite, the standard seed set, only that invariant required) and names its checker.
//! INV-6 and INV-9 are checked by the run loop (rebuild from the journal, in-process rerun); their
//! step checkers only evaluate what the loop hands over.

use std::collections::{BTreeMap, BTreeSet};

use hft_event::{
    EventKind, ExternalCause, ExternalFill, GwOp, OrderAction, OrderDecision, OrderEvent, OrderEventKind,
    OrderFlags, SendReport, SendStatus, Timer, TimerCode,
};
use hft_types::{AccountId, AssetId, GroupId, InstId, RejectBits, RunMode, Side, SlotId, VenueId};

use crate::dst::obs::{
    AutoFixPhase, CapLevel, CapScope, DecClass, DecOrigin, DecisionInfo, EvidenceKind, HedgeKind, KillRow,
    LinkState, Obs, PlanKind, SlotState, StepInfo, TruthRow,
};
use crate::dst::scenario::{Scenario, SeedSet};
use crate::dst::sut::{FrameRec, JRec, OutRing};
use crate::hash::ComponentHashes;

/// Invariant number `n` of `I<n>` / `INV-n`.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct InvId(pub u8);

impl InvId {
    pub const I1: InvId = InvId(1);
    pub const I2: InvId = InvId(2);
    pub const I3: InvId = InvId(3);
    pub const I4: InvId = InvId(4);
    pub const I5: InvId = InvId(5);
    pub const I6: InvId = InvId(6);
    pub const I7: InvId = InvId(7);
    pub const I8: InvId = InvId(8);
    pub const I9: InvId = InvId(9);
    /// Optional extension (chain); never in a non-chain build.
    pub const I10: InvId = InvId(10);
    pub const I11: InvId = InvId(11);
    pub const I12: InvId = InvId(12);
    pub const I13: InvId = InvId(13);
    pub const I14: InvId = InvId(14);
    pub const I15: InvId = InvId(15);
    pub const I16: InvId = InvId(16);
    pub const I17: InvId = InvId(17);
    pub const I18: InvId = InvId(18);
    pub const I19: InvId = InvId(19);
    pub const I20: InvId = InvId(20);

    /// `I<n>`.
    pub fn code(self) -> String {
        format!("I{}", self.0)
    }
    /// Short name printed in the repro line (`I3(exposure)`).
    pub fn name(self) -> &'static str {
        CHECKERS.iter().find(|c| c.id == self).map_or("?", |c| c.name)
    }
}

/// A set of enabled checkers (bit n = `I<n>`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InvariantSet(u32);

impl InvariantSet {
    /// Every checker of this build (I10 excluded: optional chain extension).
    pub const fn all() -> InvariantSet {
        let mut bits = 0u32;
        let mut i = 0;
        while i < CHECKERS.len() {
            bits |= 1 << CHECKERS[i].id.0;
            i += 1;
        }
        InvariantSet(bits)
    }
    pub const fn none() -> InvariantSet {
        InvariantSet(0)
    }
    pub const fn only(i: InvId) -> InvariantSet {
        InvariantSet(1 << i.0)
    }
    pub const fn with(self, i: InvId) -> InvariantSet {
        InvariantSet(self.0 | 1 << i.0)
    }
    pub const fn without(self, i: InvId) -> InvariantSet {
        InvariantSet(self.0 & !(1 << i.0))
    }
    pub const fn contains(self, i: InvId) -> bool {
        i.0 < 32 && self.0 & (1 << i.0) != 0
    }
    pub fn iter(self) -> impl Iterator<Item = InvId> {
        (1..32u8).map(InvId).filter(move |i| self.contains(*i))
    }
}

/// Everything a checker sees at one step.
#[derive(Copy, Clone, Debug)]
pub struct StepView<'a> {
    pub step: u64,
    /// Virtual time = engine time (ns) of the step.
    pub now_ns: u64,
    /// This step's journal records, the Input first (with the live driver, Inputs the driver
    /// itself writes — due timers, `EngineClock`, keyframes — may come before the delivered one).
    pub records: &'a [JRec],
    /// Kind and body of the Input the world delivered this step (`None` = a timer step).
    pub delivered: Option<(EventKind, &'a [u8])>,
    pub frames: &'a [FrameRec],
    pub info: &'a StepInfo,
    pub before: &'a Obs,
    pub after: &'a Obs,
    /// Exchange truth (MockVenue / SimActor) after the step.
    pub truth: &'a [TruthRow],
    /// INV-6 due this step: (running, rebuilt from snapshot + journal).
    pub rebuild: Option<(ComponentHashes, ComponentHashes)>,
}

impl<'a> StepView<'a> {
    /// The record of the delivered Input (a timer step: the first record).
    pub fn input(&self) -> Option<&'a JRec> {
        let Some((k, body)) = self.delivered else {
            return self.records.first();
        };
        self.records
            .iter()
            .find(|r| r.kind == k && (k != EventKind::TIMER || r.body == body))
            .or_else(|| self.records.first())
    }
    pub fn input_kind(&self) -> Option<EventKind> {
        self.input().map(|r| r.kind)
    }
    /// Order decisions of the step with their debug-view facts.
    pub fn decisions(&self) -> Vec<(OrderDecision, Option<&'a DecisionInfo>)> {
        self.records
            .iter()
            .filter(|r| {
                r.kind == EventKind::ORDER_DECISION && r.flags.0 & hft_event::RecordFlags::SHADOW.0 == 0
            })
            .filter_map(|r| {
                let d = r.try_read::<OrderDecision>()?;
                Some((d, self.info.decisions.iter().find(|i| i.seq == r.seq)))
            })
            .collect()
    }
    pub fn has_note(&self, name: &str) -> bool {
        self.info.notes.iter().any(|n| n.name == name)
    }
    fn timer_code(&self) -> Option<TimerCode> {
        let t = self.input()?.read::<Timer>()?;
        TimerCode::decode(t.owner, t.timer_id, t.group).ok()
    }
    fn cmd_kind(&self) -> Option<hft_event::CmdKind> {
        let r = self.input()?;
        if r.kind != EventKind::CONTROL_CMD {
            return None;
        }
        let (h, _) = <hft_event::CmdHdr as zerocopy::FromBytes>::read_from_prefix(&r.body).ok()?;
        Some(h.cmd)
    }
}

/// End-of-run view.
#[derive(Copy, Clone, Debug)]
pub struct EndView<'a> {
    pub now_ns: u64,
    pub obs: &'a Obs,
    pub truth: &'a [TruthRow],
}

/// Cross-step state of the checkers.
#[derive(Clone, Debug, Default)]
pub struct InvState {
    /// I3: (account, asset) paused by an injected truth difference.
    i3_paused: BTreeSet<(u8, u16)>,
    /// I12: frames by decision seq: (oflags, account, venue, kill row at emission).
    i12_frames: BTreeMap<u64, (u16, AccountId, OutRing, Option<KillRow>)>,
    /// I19: external dedup keys already booked.
    i19_ext: BTreeSet<(u8, u64)>,
    /// I20: per (account, group) Broken period: (autofix starts, terminals).
    i20_period: BTreeMap<(u8, u32), (u32, u32)>,
}

/// Step checker.
pub type StepCheck = fn(&mut InvState, &StepView<'_>) -> Result<(), String>;
/// End-of-run checker.
pub type EndCheck = fn(&mut InvState, &EndView<'_>) -> Result<(), String>;

/// One checker of the table.
#[derive(Copy, Clone, Debug)]
pub struct CheckerDesc {
    pub id: InvId,
    pub name: &'static str,
    pub step: StepCheck,
    pub end: Option<EndCheck>,
}

/// Every checker of a non-chain build, by number (TK-100: one per `INV-n` of invariants.md
/// except the optional INV-10).
pub const CHECKERS: &[CheckerDesc] = &[
    CheckerDesc {
        id: InvId(1),
        name: "fill_evidence",
        step: check_1,
        end: None,
    },
    CheckerDesc {
        id: InvId(2),
        name: "report_matched",
        step: check_2,
        end: None,
    },
    CheckerDesc {
        id: InvId(3),
        name: "exposure",
        step: check_3,
        end: None,
    },
    CheckerDesc {
        id: InvId(4),
        name: "unknown_no_open",
        step: check_4,
        end: None,
    },
    CheckerDesc {
        id: InvId(5),
        name: "kill",
        step: check_5,
        end: None,
    },
    CheckerDesc {
        id: InvId(6),
        name: "rebuild",
        step: check_6,
        end: None,
    },
    CheckerDesc {
        id: InvId(7),
        name: "slot_leak",
        step: check_7,
        end: Some(end_7),
    },
    CheckerDesc {
        id: InvId(8),
        name: "hedge_net",
        step: check_8,
        end: None,
    },
    CheckerDesc {
        id: InvId(9),
        name: "determinism",
        step: check_9,
        end: None,
    },
    CheckerDesc {
        id: InvId(11),
        name: "mode_recompute",
        step: check_11,
        end: None,
    },
    CheckerDesc {
        id: InvId(12),
        name: "risk_reducing",
        step: check_12,
        end: None,
    },
    CheckerDesc {
        id: InvId(13),
        name: "mode_no_open",
        step: check_13,
        end: None,
    },
    CheckerDesc {
        id: InvId(14),
        name: "gate_not_counted",
        step: check_14,
        end: None,
    },
    CheckerDesc {
        id: InvId(15),
        name: "hedge_cap",
        step: check_15,
        end: None,
    },
    CheckerDesc {
        id: InvId(16),
        name: "sim_truth",
        step: check_16,
        end: Some(end_16),
    },
    CheckerDesc {
        id: InvId(17),
        name: "l3_reduce",
        step: check_17,
        end: None,
    },
    CheckerDesc {
        id: InvId(18),
        name: "drain",
        step: check_18,
        end: None,
    },
    CheckerDesc {
        id: InvId(19),
        name: "naked_bound",
        step: check_19,
        end: None,
    },
    CheckerDesc {
        id: InvId(20),
        name: "broken_bounded",
        step: check_20,
        end: None,
    },
];

/// The checker of `I<n>`.
pub fn checker(i: InvId) -> Option<&'static CheckerDesc> {
    CHECKERS.iter().find(|c| c.id == i)
}

/// Runs every enabled step checker; the first violation (by number) is returned.
pub fn check_step(set: InvariantSet, st: &mut InvState, v: &StepView<'_>) -> Result<(), (InvId, String)> {
    for c in CHECKERS {
        if set.contains(c.id) {
            observation_complete(c.id, v).map_err(|m| (c.id, m))?;
            (c.step)(st, v).map_err(|m| (c.id, m))?;
        }
    }
    Ok(())
}

/// A real adapter declares its applicable cells and accounts independently of the observed rows.
/// Synthetic checker tests may supply only the rows relevant to their counterexample.
fn observation_complete(id: InvId, v: &StepView<'_>) -> Result<(), String> {
    for o in [v.before, v.after] {
        if let Some(err) = &o.observation_error {
            return Err(err.clone());
        }
        let Some(req) = &o.required else { continue };
        if o.engine_ns.is_none() {
            return Err("missing engine clock observation".into());
        }
        for &(a, s, g) in &req.cells {
            if id == InvId::I11
                && o.cells
                    .iter()
                    .filter(|c| (c.account, c.strategy, c.group) == (a, s, g))
                    .count()
                    != 1
            {
                return Err(format!("missing or duplicate mode observation for {a:?}/{g:?}"));
            }
            if [InvId::I17, InvId::I18, InvId::I20].contains(&id)
                && o.exec.iter().filter(|c| (c.account, c.group) == (a, g)).count() != 1
            {
                return Err(format!(
                    "missing or duplicate execution observation for {a:?}/{g:?}"
                ));
            }
            if [InvId::I4, InvId::I8, InvId::I19].contains(&id)
                && o.legs.iter().filter(|c| (c.account, c.group) == (a, g)).count() != 1
            {
                return Err(format!("missing or duplicate leg observation for {a:?}/{g:?}"));
            }
        }
        for a in &req.accounts {
            if [InvId::I17, InvId::I20].contains(&id)
                && o.margin.iter().filter(|m| m.account == *a).count() != 1
            {
                return Err(format!("missing or duplicate margin observation for {a:?}"));
            }
        }
        for venue in &req.venues {
            if [InvId::I5, InvId::I11, InvId::I12].contains(&id)
                && o.kill.iter().filter(|k| k.venue == *venue).count() != 1
            {
                return Err(format!("missing or duplicate Kill observation for {venue:?}"));
            }
        }
        if [InvId::I3, InvId::I19].contains(&id) {
            for &(a, inst) in &req.instruments {
                for side in [Side::Buy, Side::Sell] {
                    if o.inflight
                        .iter()
                        .filter(|r| (r.account, r.inst, r.side) == (a, inst, side))
                        .count()
                        != 1
                    {
                        return Err(format!(
                            "missing or duplicate in-flight observation for {a:?}/{inst:?}/{side:?}"
                        ));
                    }
                }
            }
        }
    }
    if v.after.required.is_some()
        && [
            InvId::I3,
            InvId::I4,
            InvId::I8,
            InvId::I13,
            InvId::I15,
            InvId::I17,
            InvId::I19,
        ]
        .contains(&id)
    {
        for r in v.records {
            if r.try_read::<OrderDecision>()
                .is_some_and(|d| d.action == OrderAction::New)
            {
                if v.info.decisions.iter().filter(|i| i.seq == r.seq).count() != 1 {
                    return Err(format!(
                        "missing or duplicate decision observation at seq {}",
                        r.seq
                    ));
                }
                if v.info
                    .decision_inflight
                    .iter()
                    .filter(|(seq, _)| *seq == r.seq)
                    .count()
                    != 1
                {
                    return Err(format!("missing decision-time in-flight at seq {}", r.seq));
                }
                if v.info
                    .decision_limits
                    .iter()
                    .filter(|(seq, _, _)| *seq == r.seq)
                    .count()
                    != 1
                {
                    return Err(format!("missing decision-time limits at seq {}", r.seq));
                }
                if v.info
                    .decision_perp_pending
                    .iter()
                    .filter(|(seq, _)| *seq == r.seq)
                    .count()
                    != 1
                {
                    return Err(format!("missing decision-time perp target at seq {}", r.seq));
                }
            }
        }
        if v.info.force_targets.len() != v.info.force_fractions.len() {
            return Err("missing force-target fraction observation".into());
        }
        for &(a, g, i) in &v.info.required_force {
            if v.info
                .force_targets
                .iter()
                .filter(|f| (f.account, f.group, f.inst) == (a, g, i))
                .count()
                != 1
            {
                return Err(format!(
                    "missing or duplicate force-target observation for {a:?}/{g:?}/{i:?}"
                ));
            }
        }
    }
    Ok(())
}

/// Runs every enabled end checker.
pub fn check_end(set: InvariantSet, st: &mut InvState, v: &EndView<'_>) -> Result<(), (InvId, String)> {
    for c in CHECKERS {
        if set.contains(c.id)
            && let Some(f) = c.end
        {
            f(st, v).map_err(|m| (c.id, m))?;
        }
    }
    Ok(())
}

/// The standing case of an invariant: every suite, the standard seed set, only that invariant.
fn standing(s: Scenario, i: InvId) -> Scenario {
    s.seeds(SeedSet::Standard).invariants(InvariantSet::only(i))
}

const fn sign(s: Side) -> i128 {
    match s {
        Side::Buy => 1,
        Side::Sell => -1,
    }
}

fn is_new(d: &OrderDecision) -> bool {
    d.action == OrderAction::New
}

const LINK_SECOND: u8 = 2;
const LINK_SECOND_RETRY: u8 = 3;
const LINK_HEDGE_FIX: u8 = 4;
const LINK_REVERSE: u8 = 5;

// ------------------------------------------------------------------------------------------- I1

/// INV-1: every position and inventory change is explained by fill evidence of the same Input;
/// transport Inputs change nothing and move in-flight slots only to `Unknown`.
pub fn inv_1_position_change_has_fill_evidence() -> Scenario {
    standing(
        Scenario::new("INV-1", "position changes trace to fill evidence"),
        InvId::I1,
    )
}

fn is_transport(v: &StepView<'_>) -> bool {
    match v.input_kind() {
        Some(EventKind::CONN_STATE) | Some(EventKind::SEND_REPORT) => true,
        Some(EventKind::TIMER) => matches!(v.timer_code(), Some(TimerCode::Oms(_)) | Some(TimerCode::Sys(_))),
        _ => false,
    }
}

fn check_1(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let mut change: BTreeMap<(u8, u8, u32), i128> = BTreeMap::new();
    for p in &v.after.positions {
        *change.entry((p.account.0, p.venue.0, p.inst.0)).or_default() += p.qty;
    }
    for p in &v.before.positions {
        *change.entry((p.account.0, p.venue.0, p.inst.0)).or_default() -= p.qty;
    }
    let mut inv_change: BTreeMap<(u8, u8, u16), i128> = BTreeMap::new();
    for p in &v.after.inventory {
        *inv_change.entry((p.account.0, p.venue.0, p.asset.0)).or_default() += p.qty;
    }
    for p in &v.before.inventory {
        *inv_change.entry((p.account.0, p.venue.0, p.asset.0)).or_default() -= p.qty;
    }
    let transport = is_transport(v);
    let mut ev: BTreeMap<(u8, u8, u32), i128> = BTreeMap::new();
    let mut inv_ev: BTreeMap<(u8, u8, u16), i128> = BTreeMap::new();
    for e in &v.info.evidence {
        *ev.entry((e.account.0, e.venue.0, e.inst.0)).or_default() += e.delta;
        if let Some(r) = v.after.inst(e.inst)
            && !r.perp
        {
            *inv_ev.entry((e.account.0, e.venue.0, r.asset.0)).or_default() += e.delta;
        }
    }
    for (k, d) in &change {
        if *d == 0 {
            continue;
        }
        if transport {
            return Err(format!(
                "transport input {:?} changed position {k:?} by {d}",
                v.input_kind()
            ));
        }
        let e = ev.get(k).copied().unwrap_or(0);
        if e != *d {
            return Err(format!("position {k:?} changed by {d}, evidence {e}"));
        }
    }
    for (k, e) in &ev {
        if change.get(k).copied().unwrap_or(0) != *e {
            return Err(format!("evidence {e} on {k:?} not reflected in the position"));
        }
    }
    for (k, d) in &inv_change {
        if *d == 0 {
            continue;
        }
        if transport {
            return Err(format!("transport input changed inventory {k:?} by {d}"));
        }
        let e = inv_ev.get(k).copied().unwrap_or(0);
        if e != *d {
            return Err(format!("inventory {k:?} changed by {d}, evidence {e}"));
        }
    }
    if transport {
        let place_unsent = v
            .input()
            .and_then(|r| r.try_read::<SendReport>())
            .is_some_and(|s| {
                s.op == GwOp::Place
                    && matches!(
                        s.status,
                        SendStatus::KillBlocked
                            | SendStatus::RateLimited
                            | SendStatus::NoConn
                            | SendStatus::EncodeError
                            | SendStatus::RingFull
                    )
            });
        for b in v.before.slots.iter().filter(|s| s.state.is_inflight()) {
            let a = v.after.slots.iter().find(|s| {
                s.venue == b.venue && s.account == b.account && s.slot == b.slot && s.r#gen == b.r#gen
            });
            let to = a.map_or(SlotState::Free, |s| s.state);
            let ok = to == b.state || to == SlotState::Unknown || (place_unsent && to == SlotState::Rejected);
            if !ok {
                return Err(format!(
                    "transport input moved slot {}/{} from {:?} to {to:?}",
                    b.account.0, b.slot, b.state
                ));
            }
        }
    }
    // Group attribution: LegTracker nets change only with evidence or a write-off.
    let has_ev = |a: AccountId| v.info.evidence.iter().any(|e| e.account == a);
    for r in &v.after.leg_net {
        let old = v
            .before
            .leg_net
            .iter()
            .find(|o| o.account == r.account && o.group == r.group && o.leg == r.leg)
            .map_or(0, |o| o.net);
        if r.net != old
            && !has_ev(r.account)
            && !v
                .info
                .writeoffs
                .iter()
                .any(|w| w.account == r.account && w.group == r.group && w.leg == r.leg)
        {
            return Err(format!(
                "group {:#x} leg {} net changed {old} -> {} without evidence or write-off",
                r.group.0, r.leg, r.net
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------- I2

/// INV-2: every order report matches a slot or tombstone, or produces an alarm.
pub fn inv_2_every_report_matched_or_alarmed() -> Scenario {
    standing(
        Scenario::new("INV-2", "every report matched or alarmed"),
        InvId::I2,
    )
}

fn check_2(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let Some(inp) = v.input() else { return Ok(()) };
    if let Some(e) = inp.try_read::<OrderEvent>() {
        if e.what == OrderEventKind::FeeOnly {
            return check_fee_only(v, &e);
        }
        let has_fill =
            matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill) || e.cum_filled.0 > 0;
        if e.slot == SlotId::UNRESOLVED {
            let want = if has_fill {
                "OmsForeignFillViaOrderEvent"
            } else {
                "OmsForeignOrder"
            };
            return if v.has_note(want) {
                Ok(())
            } else {
                Err(format!("unresolved report without Note{{{want}}}"))
            };
        }
        let cur = v.before.slots.iter().find(|s| {
            s.venue == e.venue
                && s.account == e.account
                && s.slot == e.slot.0
                && s.r#gen == e.r#gen
                && s.epoch == e.epoch
        });
        if let Some(c) = cur {
            let dup = v.after.oms_dup > v.before.oms_dup;
            return if v.has_note("OmsTransition") || dup || v.has_note("OmsAnomaly") || keep_cell(v, &e, c) {
                Ok(())
            } else {
                Err(format!(
                    "report {:?} on slot {} gen {} matched but left no transition, dup or anomaly",
                    e.what, e.slot.0, e.r#gen
                ))
            };
        }
        let tomb = v.before.tombs.iter().find(|t| {
            t.venue == e.venue
                && t.account == e.account
                && t.slot == e.slot.0
                && t.r#gen == e.r#gen
                && t.epoch == e.epoch
        });
        if let Some(t) = tomb {
            let late = i128::from(e.cum_filled.0) > t.filled;
            return if late {
                if v.has_note("OmsLateFill") {
                    Ok(())
                } else {
                    Err("late fill on a tombstone without Note{OmsLateFill}".into())
                }
            } else if v.after.oms_dup > v.before.oms_dup {
                Ok(())
            } else {
                Err(format!(
                    "tombstone {:?} report on venue {} account {} slot {} gen {} epoch {} \
                     (cum {} <= {}) not counted as dup ({} -> {})",
                    e.what,
                    e.venue.0,
                    e.account.0,
                    e.slot.0,
                    e.r#gen,
                    e.epoch,
                    e.cum_filled.0,
                    t.filled,
                    v.before.oms_dup,
                    v.after.oms_dup
                ))
            };
        }
        // StaleGen.
        return if has_fill {
            if v.has_note("OmsStaleGenFill") {
                Ok(())
            } else {
                Err(format!(
                    "stale-gen fill on slot {} without Note{{OmsStaleGenFill}}",
                    e.slot.0
                ))
            }
        } else if v.after.oms_stale_gen > v.before.oms_stale_gen {
            Ok(())
        } else {
            Err(format!("stale-gen report on slot {} silently dropped", e.slot.0))
        };
    }
    if let Some(x) = inp.try_read::<ExternalFill>() {
        let key = if x.trade_id != 0 { x.trade_id } else { x.fill_seq };
        let seen = v
            .before
            .ext_seen
            .iter()
            .any(|(a, k)| *a == x.account && *k == key);
        if v.has_note("ExternalFill") || seen || v.has_note("LedgerAutoHeldOffset") {
            return Ok(());
        }
        return Err("ExternalFill neither booked, deduplicated nor offset".into());
    }
    Ok(())
}

/// FeeOnly does not move an order's state or cumulative fill. Observe its actual ledger effect
/// independently of the Input: accepting the report or incrementing dup is not evidence of a
/// booked fee. The OMS component tests additionally check the exact FeeOut count/attribution.
fn check_fee_only(v: &StepView<'_>, e: &OrderEvent) -> Result<(), String> {
    let cur = v.before.slots.iter().find(|s| {
        s.venue == e.venue
            && s.account == e.account
            && s.slot == e.slot.0
            && s.r#gen == e.r#gen
            && s.epoch == e.epoch
    });
    let tomb = v.before.tombs.iter().find(|s| {
        s.venue == e.venue
            && s.account == e.account
            && s.slot == e.slot.0
            && s.r#gen == e.r#gen
            && s.epoch == e.epoch
    });
    let inst = cur
        .map(|s| s.inst)
        .or_else(|| tomb.map(|s| s.inst))
        .unwrap_or(e.inst);
    if cur.is_none() && tomb.is_none() && !v.has_note("OmsFeeUnattributed") {
        return Err("unmatched FeeOnly without Note{OmsFeeUnattributed}".into());
    }
    if let Some(s) = cur
        && let Some(after) = v.after.slot(e.venue, e.account, e.slot.0)
        && after.r#gen == s.r#gen
        && after.epoch == s.epoch
        && (after.state != s.state || after.filled != s.filled)
    {
        return Err("FeeOnly changed order state or cumulative fill".into());
    }
    let spec = v.before.inst(inst).ok_or("FeeOnly instrument is not observed")?;
    let fee = if e.fee_asset.0 == 0 {
        0
    } else {
        i128::from(e.fee.0)
    };
    let base = !spec.perp && e.fee_asset == spec.asset;
    if e.fee_asset.0 != 0 {
        let rows = |o: &Obs| {
            let rows = if base { &o.inventory } else { &o.balances };
            rows.iter()
                .filter(|r| r.account == e.account && r.venue == e.venue && r.asset == e.fee_asset)
                .map(|r| r.qty)
                .sum::<i128>()
        };
        let delta = rows(v.after) - rows(v.before);
        if delta != -fee {
            return Err(format!(
                "FeeOnly asset {} delta {delta}, expected {}",
                e.fee_asset.0, -fee
            ));
        }
    }
    let delta = v.after.pos(e.account, e.venue, inst) - v.before.pos(e.account, e.venue, inst);
    let want = if base { -fee } else { 0 };
    if delta != want {
        return Err(format!("FeeOnly position delta {delta}, expected {want}"));
    }
    Ok(())
}

/// The "保持" cells of hft-oms's report table: the slot stays in its state and the OMS writes
/// nothing (a late new-order receipt, a reject of an operation already superseded, a query that
/// confirms the current state).
fn keep_cell(v: &StepView<'_>, e: &OrderEvent, before: &crate::dst::obs::SlotRow) -> bool {
    use OrderEventKind as K;
    use SlotState as S;
    let st = before.state;
    let after = v
        .after
        .slots
        .iter()
        .find(|s| s.venue == before.venue && s.account == before.account && s.slot == before.slot)
        .map(|s| (s.state, s.filled));
    let unchanged = after.is_some_and(|(a, f)| a == st && f == before.filled);
    unchanged
        && match e.what {
            K::Ack => matches!(st, S::PendingCancel | S::PendingAmend),
            K::CancelReject => st == S::Unknown,
            K::AmendAck => st == S::PendingCancel,
            K::AmendReject => matches!(st, S::PendingCancel | S::Unknown),
            K::OutcomeUnknown => st == S::Unknown,
            K::QueryResult => i128::from(e.cum_filled.0) <= before.filled,
            _ => false,
        }
}

// ------------------------------------------------------------------------------------------- I3

/// INV-3: per account and asset, |Σ positions| ≤ the position limit; naked ≤ the group limit;
/// position-increasing orders stay within `max_pos_qty`.
pub fn inv_3_unhedged_within_risk_limits() -> Scenario {
    standing(Scenario::new("INV-3", "unhedged within risk limits"), InvId::I3)
}

fn check_3(st: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    // Exceptions: injected truth differences pause the first two clauses for that (a, X).
    let inj = v.input().and_then(|r| r.try_read::<ExternalFill>());
    if let Some(x) = inj
        && matches!(x.cause, ExternalCause::Manual | ExternalCause::Unknown)
        && let Some(r) = v.after.inst(x.inst)
    {
        st.i3_paused.insert((x.account.0, r.asset.0));
    }
    for e in &v.info.evidence {
        if e.kind == EvidenceKind::Adjust
            && let Some(r) = v.after.inst(e.inst)
        {
            st.i3_paused.insert((e.account.0, r.asset.0));
        }
    }
    let mut sum: BTreeMap<(u8, u16), i128> = BTreeMap::new();
    for p in &v.after.positions {
        *sum.entry((p.account.0, p.asset.0)).or_default() += p.qty;
    }
    for ((a, x), s) in &sum {
        let lim = v
            .after
            .limits
            .iter()
            .find(|l| l.account == AccountId(*a) && l.asset == AssetId(*x))
            .map(|l| l.max_pos_qty);
        let Some(lim) = lim else { continue };
        if st.i3_paused.contains(&(*a, *x)) {
            if s.abs() <= lim {
                st.i3_paused.remove(&(*a, *x));
            }
            continue;
        }
        if s.abs() > lim {
            return Err(format!("account {a} asset {x}: |Σ pos| = {} > {lim}", s.abs()));
        }
    }
    for l in &v.after.legs {
        let paused = st.i3_paused.iter().any(|(a, _)| *a == l.account.0);
        if !paused && l.max_pos_qty > 0 && l.naked_qty > l.max_pos_qty {
            return Err(format!(
                "group {:#x}: naked {} > max_pos_qty {}",
                l.group.0, l.naked_qty, l.max_pos_qty
            ));
        }
    }
    for (d, info) in v.decisions() {
        let Some(i) = info else { continue };
        if !is_new(&d) || d.reject != 0 {
            continue;
        }
        let reduce = OrderFlags(d.oflags).contains(OrderFlags::REDUCE_ONLY);
        let counted = i.class == DecClass::Open || (i.class == DecClass::Hedge && !reduce);
        if !counted || i.max_pos_qty <= 0 {
            continue;
        }
        let after = i.pos + sign(d.side) * (i.same_dir_inflight + i128::from(d.qty.0));
        if after.abs() > i.pos.abs() && after.abs() > i.max_pos_qty {
            return Err(format!(
                "place seq {}: |pos + inflight + qty| = {} > max_pos_qty {}",
                i.seq,
                after.abs(),
                i.max_pos_qty
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------- I4

/// INV-4: a group with an `Unknown` slot opens nothing and reverses nothing until it converges.
pub fn inv_4_unknown_group_no_open_no_reverse() -> Scenario {
    standing(
        Scenario::new("INV-4", "unknown group: no open, no reverse"),
        InvId::I4,
    )
}

fn check_4(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let unknown: BTreeSet<(u8, u32)> = v
        .before
        .legs
        .iter()
        .filter(|l| l.unknown_slots > 0)
        .map(|l| (l.account.0, l.group.0))
        .collect();
    let unknown_slots: BTreeSet<(u8, u8, u32, u32)> = v
        .before
        .slots
        .iter()
        .filter(|s| s.state == SlotState::Unknown)
        .map(|s| (s.venue.0, s.account.0, s.slot, s.r#gen))
        .collect();
    for (d, info) in v.decisions() {
        if !is_new(&d) {
            continue;
        }
        if d.reject == 0 && unknown_slots.contains(&(d.venue.0, d.account.0, d.slot.0, d.r#gen)) {
            return Err(format!("New on Unknown slot {} gen {}", d.slot.0, d.r#gen));
        }
        if !unknown.contains(&(d.account.0, d.group.0)) {
            continue;
        }
        let Some(i) = info else { continue };
        if i.margin_stress && i.class == DecClass::Flatten {
            continue;
        }
        if i.class == DecClass::Open && d.reject == 0 {
            return Err(format!(
                "Open released in Unknown group {:#x} (seq {})",
                d.group.0, i.seq
            ));
        }
        if i.link_unknown && d.reject == 0 && matches!(d.link_role, LINK_SECOND_RETRY | LINK_REVERSE) {
            return Err(format!(
                "link role {} on a link with an Unknown slot",
                d.link_role
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------- I5

/// INV-5: under kill, non-risk-reducing places and amends never leave the gw; under HARD nothing does.
pub fn inv_5_kill_blocks_non_risk_reducing() -> Scenario {
    standing(Scenario::new("INV-5", "kill blocks non-risk-reducing"), InvId::I5)
}

/// Kill word recomputed from the remaining `Kill` caps.
pub fn kill_from_caps(o: &Obs, venue: VenueId) -> KillRow {
    let mut k = KillRow::clear(venue);
    for c in o.caps.iter().filter(|c| c.kill) {
        match c.scope {
            CapScope::Global => {
                k.global = true;
                k.hard |= c.hard;
            }
            CapScope::Venue(v) if v == venue => k.venue_bit = true,
            CapScope::Account(a) => {
                k.accounts |= 1u64 << (a.0 & 63);
                if c.hard {
                    k.accounts_hard |= 1u64 << (a.0 & 63);
                }
            }
            _ => {}
        }
    }
    k
}

fn kill_blocks(k: &KillRow, a: AccountId, oflags: u16) -> Option<&'static str> {
    let bit = 1u64 << (a.0 & 63);
    if k.hard || k.accounts_hard & bit != 0 {
        return Some("HARD");
    }
    if (k.global || k.venue_bit || k.accounts & bit != 0)
        && !OrderFlags(oflags).contains(OrderFlags::RISK_REDUCING)
    {
        return Some("kill without RISK_REDUCING");
    }
    None
}

fn check_5(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    for k in &v.after.kill {
        if kill_from_caps(v.after, k.venue) != *k {
            return Err(format!(
                "kill bits of venue {} differ from the Kill caps",
                k.venue.0
            ));
        }
    }
    for f in v.frames {
        if !matches!(f.cmd.op, GwOp::Place | GwOp::Amend) {
            continue;
        }
        let k = match f.ring {
            OutRing::Venue(vn) => v
                .after
                .kill
                .iter()
                .find(|k| k.venue == vn)
                .copied()
                .unwrap_or(KillRow::clear(vn)),
            OutRing::Sim => kill_from_caps(v.after, VenueId(0)),
        };
        if let Some(why) = kill_blocks(&k, f.cmd.account, f.cmd.oflags) {
            return Err(format!(
                "{:?} frame of account {} left under {why} (decision {})",
                f.cmd.op, f.cmd.account.0, f.cmd.decision_seq
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------- I6

/// INV-6: the state rebuilt from the latest snapshot plus the following Inputs equals the running
/// state component by component (checked every `rebuild_every` steps and at the end).
pub fn inv_6_state_is_function_of_journal() -> Scenario {
    standing(
        Scenario::new("INV-6", "state is a function of the journal"),
        InvId::I6,
    )
}

fn check_6(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    match v.rebuild {
        Some((run, rebuilt)) => match run.first_diff(&rebuilt) {
            Some(c) => Err(format!("rebuilt state differs in component {}", c.name())),
            None => Ok(()),
        },
        None => Ok(()),
    }
}

// ------------------------------------------------------------------------------------------- I7

/// INV-7: slots do not leak; counters match; terminal slots return to `Free` in time.
pub fn inv_7_slots_do_not_leak() -> Scenario {
    standing(Scenario::new("INV-7", "OMS slots do not leak"), InvId::I7)
}

fn slot_deadline_ns(o: &Obs, venue: VenueId, account: AccountId) -> i64 {
    let nonfree = o
        .slots
        .iter()
        .filter(|s| s.venue == venue && s.account == account)
        .count() as i64;
    let batch = i64::from(o.risk.sweep_batch.max(1));
    (o.risk.reuse_quarantine_ms + (nonfree + batch - 1) / batch * 1_000 + 1_000) * 1_000_000
}

fn check_7(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let o = v.after;
    for t in &o.tables {
        let mine = o
            .slots
            .iter()
            .filter(|s| s.venue == t.venue && s.account == t.account);
        let unknown = mine.clone().filter(|s| s.state == SlotState::Unknown).count() as u32;
        let nonfree = mine.count() as u32;
        if unknown != t.unknown_count {
            return Err(format!(
                "table {}/{}: unknown_count {} != Unknown slots {unknown}",
                t.venue.0, t.account.0, t.unknown_count
            ));
        }
        if !t.free_list_consistent || t.free + nonfree != t.capacity {
            return Err(format!(
                "table {}/{}: free {} + non-free {nonfree} != capacity {}",
                t.venue.0, t.account.0, t.free, t.capacity
            ));
        }
    }
    let now = v.after.engine_ns.unwrap_or(v.now_ns as i64);
    for s in o.slots.iter().filter(|s| s.state.is_terminal()) {
        let limit = slot_deadline_ns(o, s.venue, s.account);
        if s.terminal_ens > 0 && now - s.terminal_ens > limit {
            return Err(format!(
                "slot {}/{} terminal ({:?}) for {} ms, limit {} ms",
                s.account.0,
                s.slot,
                s.state,
                (now - s.terminal_ens) / 1_000_000,
                limit / 1_000_000
            ));
        }
    }
    Ok(())
}

fn end_7(_: &mut InvState, v: &EndView<'_>) -> Result<(), String> {
    let now = v.obs.engine_ns.unwrap_or(v.now_ns as i64);
    for s in v.obs.slots.iter().filter(|s| s.state.is_terminal()) {
        let limit = slot_deadline_ns(v.obs, s.venue, s.account);
        if s.terminal_ens > 0 && now - s.terminal_ens > limit {
            return Err(format!("slot {}/{} never returned to Free", s.account.0, s.slot));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------- I8

/// INV-8: OnFill hedge quantities sum to the first leg's cumulative net fill × ratio.
pub fn inv_8_second_leg_equals_net_fill() -> Scenario {
    standing(Scenario::new("INV-8", "second leg equals net fill"), InvId::I8)
}

fn check_8(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    if let Some(error) = &v.after.observation_error {
        return Err(error.clone());
    }
    let mut per_group: BTreeMap<(u8, u32), (i128, i128, i64)> = BTreeMap::new();
    for l in v
        .after
        .links
        .iter()
        .filter(|l| l.plan == PlanKind::OnFill && !l.reversed)
    {
        let e = per_group
            .entry((l.account.0, l.group.0))
            .or_insert((0, 0, l.ratio_den.max(1)));
        e.0 += l.hedge_req * i128::from(l.ratio_den.max(1));
        e.1 += l.first_net * i128::from(l.ratio_num);
        e.2 = l.ratio_den.max(1);
    }
    for h in &v.after.hedge_history {
        let e = per_group
            .entry((h.account.0, h.group.0))
            .or_insert((0, 0, h.ratio_den.max(1)));
        if e.2 != h.ratio_den.max(1) {
            return Err("OnFill history has incompatible ratio denominators".into());
        }
        e.0 += h.hedge_req * i128::from(h.ratio_den.max(1));
        e.1 += h.first_net * i128::from(h.ratio_num);
    }
    for ((a, g), (req_den, net_num, den)) in per_group {
        let Some(leg) = v.after.leg(AccountId(a), GroupId(g)) else {
            continue;
        };
        let lhs = req_den + (leg.naked_capped + leg.residual - leg.fee_adjust) * i128::from(den);
        if lhs != net_num {
            return Err(format!(
                "group {g:#x}: Σ hedge_req + capped + residual − fee ({}) != Σ first_net × ratio ({})",
                lhs, net_num
            ));
        }
    }
    for (d, info) in v.decisions() {
        let Some(i) = info else { continue };
        if i.hedge == Some(HedgeKind::Second) && is_new(&d) {
            let lot = v.after.inst(d.inst).map_or(1, |r| r.lot.max(1));
            let cap = i.basis / lot * lot;
            if i128::from(d.qty.0) > cap {
                return Err(format!(
                    "Second seq {}: qty {} > basis {} rounded to lot",
                    i.seq, d.qty.0, i.basis
                ));
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------- I9

/// INV-9: the same seed twice gives byte-identical intent streams and hash trails (the run loop
/// reruns 5% of seeds in process; the version-tag job compares `fixtures/dst/hashtrail.toml`).
pub fn inv_9_same_seed_same_trail() -> Scenario {
    standing(Scenario::new("INV-9", "same seed, same trail"), InvId::I9)
}

fn check_9(_: &mut InvState, _: &StepView<'_>) -> Result<(), String> {
    Ok(())
}

// ------------------------------------------------------------------------------------------ I11

/// INV-11: the cached effective mode equals the recomputation from desired mode, caps and flatness.
pub fn inv_11_effective_mode_recomputes() -> Scenario {
    standing(Scenario::new("INV-11", "effective mode recomputes"), InvId::I11)
}

/// `E(c)` of hft-risk "有效模式": `(mode, deferred_shadow)`.
pub fn effective_mode(
    o: &Obs,
    a: AccountId,
    s: hft_types::StrategyId,
    g: GroupId,
    desired: RunMode,
    flat: bool,
) -> (RunMode, bool) {
    if desired == RunMode::Shadow {
        return if flat {
            (RunMode::Shadow, false)
        } else {
            (RunMode::CloseOnly, true)
        };
    }
    let level = o
        .caps
        .iter()
        .filter(|c| !c.venue_level)
        .filter(|c| match c.scope {
            CapScope::Global => true,
            CapScope::Account(x) => x == a,
            CapScope::Cell(x, y, z) => {
                (x.0 == 0 || x == a) && (y.0 == 0 || y == s) && (z == hft_types::GROUP_ALL || z == g)
            }
            CapScope::Venue(_) => false,
        })
        .map(|c| c.level)
        .max()
        .unwrap_or(CapLevel::None);
    if level >= CapLevel::CloseOnly {
        (RunMode::CloseOnly, false)
    } else {
        (desired, false)
    }
}

fn check_11(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    for c in &v.after.cells {
        let (m, deferred) = effective_mode(v.after, c.account, c.strategy, c.group, c.desired, c.flat);
        if m != c.effective || deferred != c.deferred_shadow {
            return Err(format!(
                "cell {}/{}/{:#x}: effective {:?} (deferred {}) != recomputed {m:?} (deferred {deferred})",
                c.account.0, c.strategy.0, c.group.0, c.effective, c.deferred_shadow
            ));
        }
    }
    for k in &v.after.kill {
        if kill_from_caps(v.after, k.venue) != *k {
            return Err(format!("kill bits of venue {} != recomputed", k.venue.0));
        }
    }
    let cmd = v.cmd_kind();
    if cmd == Some(hft_event::CmdKind::SET_MODE) && v.before.caps != v.after.caps {
        return Err("SetMode changed the cap set".into());
    }
    if v.before.caps != v.after.caps
        && !matches!(
            cmd,
            Some(hft_event::CmdKind::SET_MODE | hft_event::CmdKind::SET_PARAMS)
        )
    {
        for c in &v.after.cells {
            if let Some(b) = v
                .before
                .cells
                .iter()
                .find(|b| b.account == c.account && b.strategy == c.strategy && b.group == c.group)
                && b.desired != c.desired
            {
                return Err(format!("cap change altered the desired mode of {:#x}", c.group.0));
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I12

/// INV-12: risk-reducing orders are blocked only by HARD.
pub fn inv_12_risk_reducing_only_hard_blocks() -> Scenario {
    standing(
        Scenario::new("INV-12", "risk-reducing orders only blocked by HARD"),
        InvId::I12,
    )
}

const GATE_BITS: u32 = RejectBits::CLOCK.0
    | RejectBits::DRAINING.0
    | RejectBits::KILLED.0
    | RejectBits::DAILY_LOSS.0
    | RejectBits::RECON_DEVIATION.0
    | RejectBits::CLOSE_ONLY.0
    | RejectBits::VENUE_NOT_READY.0
    | RejectBits::UNKNOWN_PENDING.0
    | RejectBits::EXPOSURE_GATE.0
    | RejectBits::LINK_STALE.0
    | RejectBits::LINK_FUNDS.0;

fn check_12(st: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    for (d, info) in v.decisions() {
        let rr = OrderFlags(d.oflags).contains(OrderFlags::RISK_REDUCING);
        if rr && d.reject & GATE_BITS != 0 {
            return Err(format!(
                "risk-reducing decision (slot {}) rejected by gate bits {:#x}",
                d.slot.0,
                d.reject & GATE_BITS
            ));
        }
        if let Some(i) = info
            && rr
            && matches!(i.origin, DecOrigin::Exec | DecOrigin::Oms)
            && d.reject & (RejectBits::MODE_SHADOW.0 | RejectBits::CLOSE_ONLY.0) != 0
        {
            return Err(format!(
                "engine risk-reducing order seq {} rejected by mode",
                i.seq
            ));
        }
    }
    for f in v.frames {
        let k = match f.ring {
            OutRing::Venue(vn) => v.after.kill.iter().find(|k| k.venue == vn).copied(),
            OutRing::Sim => None,
        };
        st.i12_frames
            .insert(f.cmd.decision_seq, (f.cmd.oflags, f.cmd.account, f.ring, k));
    }
    if st.i12_frames.len() > 65_536
        && let Some(&first) = st.i12_frames.keys().next()
    {
        st.i12_frames.remove(&first);
    }
    if let Some(s) = v.input().and_then(|r| r.try_read::<SendReport>())
        && s.status == SendStatus::KillBlocked
        && let Some((oflags, a, _, k)) = st.i12_frames.get(&s.decision_seq)
        && OrderFlags(*oflags).contains(OrderFlags::RISK_REDUCING)
    {
        let bit = 1u64 << (a.0 & 63);
        let hard = k.is_some_and(|k| k.hard || k.accounts_hard & bit != 0);
        if !hard {
            return Err(format!(
                "risk-reducing frame (decision {}) KillBlocked without HARD",
                s.decision_seq
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I13

/// INV-13: Resume, SetMode and cap changes never emit opening or hedge orders by themselves.
pub fn inv_13_mode_changes_emit_no_open_or_hedge() -> Scenario {
    standing(
        Scenario::new("INV-13", "mode changes emit no open or hedge"),
        InvId::I13,
    )
}

const fn mode_rank(m: RunMode) -> u8 {
    match m {
        RunMode::Shadow => 0,
        RunMode::CloseOnly => 1,
        RunMode::Live => 2,
    }
}

fn check_13(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    use hft_event::{CmdKind, SysTimer};
    let mode_input = matches!(
        v.cmd_kind(),
        Some(CmdKind::SET_MODE | CmdKind::RESUME | CmdKind::KILL | CmdKind::VENUE_NOTICE)
    ) || matches!(
        v.timer_code(),
        Some(TimerCode::Sys(
            SysTimer::ClockOk
                | SysTimer::ClockBad
                | SysTimer::PersistLag { .. }
                | SysTimer::DiskLow { .. }
                | SysTimer::StorageOk
        ))
    );
    if !mode_input {
        return Ok(());
    }
    let mut lowered = v.after.caps.len() > v.before.caps.len();
    let mut raised = false;
    for c in &v.after.cells {
        if let Some(b) = v
            .before
            .cells
            .iter()
            .find(|b| b.account == c.account && b.strategy == c.strategy && b.group == c.group)
        {
            lowered |= mode_rank(c.effective) < mode_rank(b.effective);
            raised |= mode_rank(c.effective) > mode_rank(b.effective);
        }
    }
    for (d, info) in v.decisions() {
        if is_new(&d)
            && matches!(
                d.link_role,
                LINK_SECOND | LINK_SECOND_RETRY | LINK_REVERSE | LINK_HEDGE_FIX
            )
        {
            return Err(format!(
                "mode input emitted link role {} (slot {})",
                d.link_role, d.slot.0
            ));
        }
        let Some(i) = info else { continue };
        if matches!(
            i.hedge,
            Some(
                HedgeKind::HedgeFix
                    | HedgeKind::ResidualTrim
                    | HedgeKind::ExternalDefault
                    | HedgeKind::AutoFix
            )
        ) && is_new(&d)
        {
            return Err(format!("mode input emitted engine order {:?}", i.hedge));
        }
        let open = i.class == DecClass::Open && matches!(d.action, OrderAction::New | OrderAction::Amend);
        if lowered && open {
            return Err(format!(
                "mode-lowering input emitted an Open order (seq {})",
                i.seq
            ));
        }
        if raised && open && i.origin != DecOrigin::Strategy {
            return Err(format!(
                "mode-raising input emitted a non-strategy Open (seq {})",
                i.seq
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I14

/// INV-14: gate rejects never count as execution failures.
pub fn inv_14_gate_rejects_not_counted() -> Scenario {
    standing(Scenario::new("INV-14", "gate rejects not counted"), InvId::I14)
}

fn check_14(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let gate_send = v
        .input()
        .and_then(|r| r.try_read::<SendReport>())
        .is_some_and(|s| {
            matches!(
                s.status,
                SendStatus::KillBlocked | SendStatus::RateLimited | SendStatus::NoConn | SendStatus::RingFull
            )
        });
    let exchange_result = v
        .input()
        .and_then(|r| r.try_read::<OrderEvent>())
        .is_some_and(|e| {
            matches!(
                e.what,
                OrderEventKind::Reject
                    | OrderEventKind::Expired
                    | OrderEventKind::CancelAck
                    | OrderEventKind::Fill
            )
        })
        || v.input()
            .and_then(|r| r.try_read::<SendReport>())
            .is_some_and(|s| s.status == SendStatus::EncodeError);
    for l in &v.after.links {
        let old = v
            .before
            .links
            .iter()
            .find(|b| b.id == l.id)
            .map_or(l.retries, |b| b.retries);
        if l.retries > old && (gate_send || !exchange_result) {
            return Err(format!(
                "link {} retries {old} -> {} on a gate input",
                l.id, l.retries
            ));
        }
    }
    for c in &v.after.exec {
        if let Some(b) = v.before.exec_cell(c.account, c.group) {
            let grew = c.fail_streak > b.fail_streak || c.fix_count > b.fix_count;
            if grew && gate_send {
                return Err(format!(
                    "cell {:#x} failure count grew on a gate SendReport",
                    c.group.0
                ));
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I15

/// INV-15: hedge orders ≤ basis + 1 lot and ≤ the hedge notional cap; Flatten ≤ the reduce cap.
pub fn inv_15_hedge_qty_and_notional_capped() -> Scenario {
    standing(
        Scenario::new("INV-15", "hedge quantity and notional capped"),
        InvId::I15,
    )
}

fn check_15(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let r = v.after.risk;
    let sent: BTreeSet<u64> = v.frames.iter().map(|f| f.cmd.decision_seq).collect();
    for (d, info) in v.decisions() {
        let Some(i) = info else { continue };
        if d.reject & RejectBits::HEDGE_LIMIT.0 != 0 && sent.contains(&i.seq) {
            return Err(format!("HEDGE_LIMIT decision seq {} left the ring", i.seq));
        }
        if !is_new(&d) || d.reject != 0 {
            continue;
        }
        let qty = i128::from(d.qty.0);
        let (_, max_hedge, max_reduce) = v
            .info
            .decision_limits
            .iter()
            .find(|(seq, _, _)| *seq == i.seq)
            .copied()
            .unwrap_or((i.seq, r.max_hedge_notional, r.max_reduce_notional));
        match i.class {
            DecClass::Hedge => {
                let lot = v.after.inst(d.inst).map_or(0, |x| x.lot);
                if qty > i.basis + lot {
                    return Err(format!(
                        "hedge seq {} ({:?}): qty {qty} > basis {} + 1 lot {lot}",
                        i.seq, i.hedge, i.basis
                    ));
                }
                if max_hedge > 0 && i.notional > max_hedge {
                    return Err(format!(
                        "hedge seq {}: notional {} > max_hedge_notional",
                        i.seq, i.notional
                    ));
                }
                if i.hedge == Some(HedgeKind::AutoFix) && max_reduce > 0 && i.notional > max_reduce {
                    return Err(format!("AutoFix seq {}: notional > max_reduce_notional", i.seq));
                }
            }
            DecClass::Flatten if max_reduce > 0 && i.notional > max_reduce => {
                return Err(format!("Flatten seq {}: notional > max_reduce_notional", i.seq));
            }
            _ => {}
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I16

/// INV-16: with nothing in flight, ledger positions, inventory and balances equal the exchange truth.
pub fn inv_16_ledger_matches_sim_truth() -> Scenario {
    standing(
        Scenario::new("INV-16", "ledger matches the exchange truth"),
        InvId::I16,
    )
}

fn compare_truth(o: &Obs, truth: &[TruthRow]) -> Result<(), String> {
    for t in truth {
        let inflight = o
            .slots
            .iter()
            .any(|s| s.account == t.account && s.venue == t.venue && !s.state.is_terminal());
        if t.undelivered || inflight {
            continue;
        }
        for (i, q) in &t.positions {
            if t.excluded.contains(&i.0) {
                continue;
            }
            let l = o.pos(t.account, t.venue, *i);
            if l != *q {
                return Err(format!(
                    "account {} venue {} inst {}: ledger {l} != truth {q}",
                    t.account.0, t.venue.0, i.0
                ));
            }
        }
        let find = |rows: &[crate::dst::obs::AssetRow], a: AssetId| {
            rows.iter()
                .find(|r| r.account == t.account && r.venue == t.venue && r.asset == a)
                .map_or(0, |r| r.qty)
        };
        for (a, q) in &t.inventory {
            if !t.excluded.contains(&u32::from(a.0)) && find(&o.inventory, *a) != *q {
                return Err(format!(
                    "account {} venue {} asset {}: inventory {} != truth {q}",
                    t.account.0,
                    t.venue.0,
                    a.0,
                    find(&o.inventory, *a)
                ));
            }
        }
        for (a, q) in &t.balances {
            if !t.excluded.contains(&u32::from(a.0)) && find(&o.balances, *a) != *q {
                return Err(format!(
                    "account {} venue {} asset {}: balance {} != truth {q}",
                    t.account.0,
                    t.venue.0,
                    a.0,
                    find(&o.balances, *a)
                ));
            }
        }
    }
    Ok(())
}

fn check_16(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    compare_truth(v.after, v.truth)
}

fn end_16(_: &mut InvState, v: &EndView<'_>) -> Result<(), String> {
    compare_truth(v.obs, v.truth)
}

// ------------------------------------------------------------------------------------------ I17

/// INV-17: L3 forced reduction only reduces, perp first, targets shrink by `force_step`.
pub fn inv_17_l3_reduces_toward_zero_perp_first() -> Scenario {
    standing(
        Scenario::new("INV-17", "L3 reduces toward zero, perp first"),
        InvId::I17,
    )
}

fn check_17(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let mut first_engine: BTreeMap<(u8, u32), InstId> = BTreeMap::new();
    for (d, info) in v.decisions() {
        let Some(i) = info else { continue };
        if !(i.margin_stress && i.class == DecClass::Flatten && is_new(&d) && d.reject == 0) {
            continue;
        }
        let q = i128::from(d.qty.0);
        if i.pos == 0 || sign(d.side) == i.pos.signum() {
            return Err(format!(
                "MarginStress order seq {} increases |pos| (pos {})",
                i.seq, i.pos
            ));
        }
        if q > i.pos.abs() - i.same_dir_inflight {
            return Err(format!(
                "MarginStress order seq {}: qty {q} > |pos| {} − inflight {}",
                i.seq,
                i.pos.abs(),
                i.same_dir_inflight
            ));
        }
        let pending = v
            .info
            .decision_perp_pending
            .iter()
            .find(|(seq, _)| *seq == i.seq)
            .is_none_or(|(_, p)| *p);
        if pending {
            first_engine.entry((d.account.0, d.group.0)).or_insert(d.inst);
        }
    }
    let step_bp = i128::from(v.after.risk.force_step_bp);
    for (ix, t) in v.info.force_targets.iter().enumerate() {
        let lot = v.after.inst(t.inst).map_or(1, |r| r.lot.max(1));
        if let Some(frac) = v.info.force_fractions.get(ix) {
            let keep = 1_000_000_000 - i128::from((*frac).clamp(0, 1_000_000_000));
            let mut expected = (t.base * keep / 1_000_000_000) / lot * lot;
            if let Some(previous) = t.strategy_target {
                if previous.signum() != expected.signum() {
                    expected = 0;
                } else if previous.abs() < expected.abs() {
                    expected = previous;
                }
            }
            if t.target != expected {
                return Err(format!(
                    "force target {} != independently recomputed {expected}",
                    t.target
                ));
            }
            continue;
        }
        if let Some(s) = t.strategy_target
            && s.abs() <= t.target.abs()
            && s.signum() * t.base.signum() >= 0
        {
            if t.target != s {
                return Err(format!(
                    "force target {} ignores the closer strategy target {s}",
                    t.target
                ));
            }
            continue;
        }
        if t.target != 0 && t.target.signum() != t.base.signum() {
            return Err(format!(
                "force target {} flips the sign of base {}",
                t.target, t.base
            ));
        }
        let frac = v
            .info
            .force_fractions
            .get(ix)
            .map_or(step_bp * 100_000, |f| i128::from(*f));
        let bound = (t.base.abs() * (1_000_000_000 - frac) / 1_000_000_000) / lot * lot;
        if t.target.abs() > bound {
            return Err(format!(
                "force target |{}| > |base {}| × (1 − force_step) = {bound}",
                t.target, t.base
            ));
        }
    }
    for ((a, g), inst) in first_engine {
        let cell = v.after.exec_cell(AccountId(a), GroupId(g));
        let skip = cell.is_none_or(|c| c.broken || c.serial_fallback);
        let perp = v.after.inst(inst).is_some_and(|r| r.perp);
        if !skip && !perp {
            return Err(format!(
                "first MarginStress order of cell {g:#x} is not on the perp leg"
            ));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I18

/// INV-18: a Drain that ends `Done` without `DrainTimeout` leaves no naked leg and no unreached
/// engine reduction.
pub fn inv_18_drain_leaves_no_naked_leg() -> Scenario {
    standing(Scenario::new("INV-18", "drain leaves no naked leg"), InvId::I18)
}

fn check_18(_: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let d = v.after.drain;
    if !d.done || d.timed_out || v.has_note("DrainTimeout") {
        return Ok(());
    }
    if let Some(l) = v
        .after
        .links
        .iter()
        .find(|l| matches!(l.state, LinkState::Open | LinkState::Hedging))
    {
        return Err(format!("Drain Done with link {} {:?}", l.id, l.state));
    }
    if let Some(s) = v.after.slots.iter().find(|s| {
        matches!(
            s.state,
            SlotState::PendingNew | SlotState::PendingCancel | SlotState::PendingAmend
        ) || (s.immediate && !s.state.is_terminal())
    }) {
        return Err(format!("Drain Done with slot {} {:?}", s.slot, s.state));
    }
    if d.hedge_retry_timers > 0 {
        return Err(format!(
            "Drain Done with {} HedgeRetry timers",
            d.hedge_retry_timers
        ));
    }
    if !d.exec_drain_ready {
        return Err("Drain Done while exec.drain_ready() is false".into());
    }
    if v.after.exec.iter().any(|c| c.engine_target_pending) {
        return Err("Drain Done with an unreached MarginStress/Hard engine target".into());
    }
    if d.snapshot_open_links > 0 {
        return Err("Drain snapshot holds Open/Hedging links".into());
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I19

/// INV-19: hedges never exceed the naked quantity, reductions never exceed the position, one fill
/// is never hedged twice.
pub fn inv_19_no_hedge_beyond_naked() -> Scenario {
    standing(Scenario::new("INV-19", "no hedge beyond naked"), InvId::I19)
}

fn check_19(st: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    for (d, info) in v.decisions() {
        let Some(i) = info else { continue };
        if !is_new(&d) || d.reject != 0 {
            continue;
        }
        let q = i128::from(d.qty.0);
        match i.hedge {
            Some(HedgeKind::Second | HedgeKind::SecondRetry) => {
                if q > i.naked_dir {
                    return Err(format!(
                        "{:?} seq {}: qty {q} > naked {}",
                        i.hedge, i.seq, i.naked_dir
                    ));
                }
            }
            Some(
                HedgeKind::HedgeFix
                | HedgeKind::ResidualTrim
                | HedgeKind::ExternalDefault
                | HedgeKind::CmdHedgeFix
                | HedgeKind::AutoFix,
            ) => {
                if v.after.required.is_some() && q > i.naked_dir {
                    return Err(format!(
                        "{:?} seq {} increases or crosses naked exposure",
                        i.hedge, i.seq
                    ));
                }
                if q > i.basis {
                    return Err(format!(
                        "{:?} seq {}: qty {q} > |Σ net| {}",
                        i.hedge, i.seq, i.basis
                    ));
                }
                let immediate = v
                    .info
                    .decision_inflight
                    .iter()
                    .find(|(s, _)| *s == i.seq)
                    .map(|(_, x)| x)
                    .or_else(|| v.before.leg(d.account, d.group).map(|l| &l.inflight_immediate));
                if immediate.is_some_and(|xs| xs.iter().any(|x| *x != 0)) {
                    return Err(format!(
                        "{:?} seq {} sent with immediate in-flight",
                        i.hedge, i.seq
                    ));
                }
            }
            _ => {}
        }
        if i.class == DecClass::Flatten && q > i.pos.abs() - i.same_dir_inflight {
            return Err(format!("Flatten seq {}: qty {q} > |pos| − inflight", i.seq));
        }
        if v.after.required.is_none()
            && i.class == DecClass::Hedge
            && let Some(l) = v.after.leg(d.account, d.group)
        {
            let opposite = match d.side {
                Side::Sell => l.naked_buy,
                Side::Buy => l.naked_sell,
            };
            if opposite != 0 {
                return Err(format!(
                    "hedge seq {} crossed the naked quantity (opposite naked {opposite})",
                    i.seq
                ));
            }
        }
    }
    for e in v
        .info
        .evidence
        .iter()
        .filter(|e| e.kind == EvidenceKind::External && e.ext_key != 0)
    {
        if !st.i19_ext.insert((e.account.0, e.ext_key)) {
            return Err(format!("external fill key {} booked twice", e.ext_key));
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ I20

/// INV-20: each Broken period starts at most one auto-fix, on time, with at most one terminal.
pub fn inv_20_broken_auto_fix_bounded() -> Scenario {
    standing(Scenario::new("INV-20", "Broken auto-fix bounded"), InvId::I20)
}

fn check_20(st: &mut InvState, v: &StepView<'_>) -> Result<(), String> {
    let now = v.after.engine_ns.unwrap_or(v.now_ns as i64);
    for c in &v.after.exec {
        let key = (c.account.0, c.group.0);
        let was = v.before.exec_cell(c.account, c.group).is_some_and(|b| b.broken);
        if c.broken && !was {
            st.i20_period.insert(key, (0, 0));
        }
        if !c.broken {
            st.i20_period.remove(&key);
        }
        if c.autofix_filled > c.budget && c.budget >= 0 && c.phase != AutoFixPhase::Idle {
            return Err(format!(
                "cell {:#x}: auto-fix filled {} > budget {}",
                c.group.0, c.autofix_filled, c.budget
            ));
        }
    }
    for (a, g) in &v.info.autofix_starts {
        let b = v.before.exec_cell(*a, *g);
        let Some(b) = b else {
            return Err(format!("AutoFix/Start on unknown cell {:#x}", g.0));
        };
        if now < b.due_ens {
            return Err(format!(
                "AutoFix/Start of {:#x} before due ({} < {})",
                g.0, now, b.due_ens
            ));
        }
        if b.due_ens != b.due_ens_recomputed {
            return Err(format!(
                "cell {:#x}: due {} != recomputed {}",
                g.0, b.due_ens, b.due_ens_recomputed
            ));
        }
        let hard = v.after.margin_of(*a).is_some_and(|m| m.hard);
        if hard {
            return Err(format!("AutoFix/Start of {:#x} under HARD", g.0));
        }
        let e = st.i20_period.entry((a.0, g.0)).or_insert((0, 0));
        e.0 += 1;
        if e.0 > 1 {
            return Err(format!("second AutoFix/Start in one Broken period of {:#x}", g.0));
        }
    }
    for n in v
        .info
        .notes
        .iter()
        .filter(|n| matches!(n.name, "ExecAutoFixDone" | "ExecAutoFixFailed"))
    {
        let e = st.i20_period.entry((n.account.0, n.group.0)).or_insert((0, 0));
        e.1 += 1;
        if e.1 > 1 {
            return Err(format!(
                "second auto-fix terminal in one Broken period of {:#x}",
                n.group.0
            ));
        }
    }
    // Missed start: Primed, due, not held, still Primed after the step without a start.
    for b in v
        .before
        .exec
        .iter()
        .filter(|b| b.broken && b.phase == AutoFixPhase::Primed)
    {
        let hard = v.after.margin_of(b.account).is_some_and(|m| m.hard);
        let started = v
            .info
            .autofix_starts
            .iter()
            .any(|(a, g)| *a == b.account && *g == b.group);
        let still = v
            .after
            .exec_cell(b.account, b.group)
            .is_some_and(|c| c.broken && c.phase == AutoFixPhase::Primed);
        let advanced_ready =
            v.after.required.is_none() || v.info.autofix_ready.contains(&(b.account, b.group));
        if now >= b.due_ens && !b.manual_hold && !hard && !started && still && advanced_ready {
            return Err(format!("cell {:#x}: due auto-fix not started", b.group.0));
        }
    }
    // HARD: no AutoFix orders.
    for (d, info) in v.decisions() {
        if let Some(i) = info
            && i.hedge == Some(HedgeKind::AutoFix)
            && is_new(&d)
            && d.reject == 0
            && v.after.margin_of(d.account).is_some_and(|m| m.hard)
        {
            return Err(format!("AutoFix order seq {} under HARD", i.seq));
        }
    }
    Ok(())
}
