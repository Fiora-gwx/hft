//! The DST loop (README "DST 的世界模型"): one world, one engine, every actor, the fault plan,
//! the invariant checkers after every step, the hash trail, the INV-6 rebuild checks and the INV-9
//! in-process rerun.

use std::panic::{AssertUnwindSafe, catch_unwind};

use hft_event::GwCmd;
use hft_types::InstId;
use zerocopy::{FromBytes, IntoBytes};

use crate::dst::actors::{AdvisorActor, CtlActor, HkActor, MarketActor, MarketOut, ReconActor};
use crate::dst::driver::{last_events, tsc_at};
use crate::dst::faults::{Fault, FaultPlan};
use crate::dst::invariants::{EndView, InvId, InvState, InvariantSet, StepView, check_end, check_step};
use crate::dst::obs::{Obs, TruthRow};
use crate::dst::sut::{BootCtx, FrameRec, InputRec, JRec, OutRing, Source, StepOut, Sut};
use crate::dst::venue_behavior::{
    CancelFilledReport, ExternalReceipt, LateRecycleReport, Matcher, MockVenue, StaleGenReport,
};
use crate::dst::world::{AccountKind, World};
use crate::dst::{DstConfig, DstFailure, DstOutcome, FailKind, RerunPolicy, TraceItem};
use crate::hash::{ComponentHashes, HashTrail};

/// Everything a finished run leaves for scenario assertions and framework tests.
#[derive(Clone, Debug)]
pub struct RunRecord {
    pub seed: u64,
    pub suite: &'static str,
    pub steps_run: u64,
    pub outcome: DstOutcome,
    /// The engine's journal at the end.
    pub journal: Vec<JRec>,
    /// Complete committed segment metadata, only retained when physical_journal was requested.
    #[cfg(feature = "seq")]
    pub physical_journal: Option<hft_sequencer::driver::replay::Log>,
    /// Frames with their emission time.
    pub frames: Vec<(u64, FrameRec)>,
    /// Inputs handed to the engine with their delivery time (timer steps excluded).
    pub deliveries: Vec<(u64, InputRec)>,
    pub plan: FaultPlan,
    pub trail: HashTrail,
    /// The last ≤ 200 steps (all steps with `cfg.trace`).
    pub trace: Vec<TraceItem>,
    pub obs: Obs,
    pub end_ns: u64,
    /// Successful mandatory INV-6 checks, one per journal snapshot (including boot / restart).
    pub snapshot_checks: u64,
    /// End-of-run full replay; absent when disabled, unavailable or failed.
    pub journal_verification: Option<crate::dst::sut::JournalVerification>,
    /// Input seqs whose exact hot/cold ledger sections were compared at that event's end.
    pub ledger_checks: Vec<u64>,
    /// Bounded raw matching evidence for diagnosis; not an invariant reference.
    pub matched_tail: Vec<(u64, hft_event::OrderEvent)>,
    /// Complete bounded original external execution history, independent of delivery copies.
    pub external_receipts: Vec<ExternalReceipt>,
    /// Default stale-generation requests, including pending requests that created no fill.
    pub stale_gen: Vec<StaleGenReport>,
    /// Delayed copies of real fills, with the committed boundary that proved slot recycling.
    pub late_recycle: Vec<LateRecycleReport>,
    /// Original fills delayed across real cancel commands, including requests not yet matched.
    pub cancel_filled: Vec<CancelFilledReport>,
    /// Final exchange-side view, independently of the engine observations.
    pub truth: Vec<TruthRow>,
    /// Non-vacuity: items each invariant checker compared, index n = `I<n>` (TK-176). INV-6
    /// includes the run's snapshot checks and end-of-run journal comparisons, INV-9 the hash-trail
    /// points and Outputs an in-process rerun compared.
    pub compared: [u64; 32],
}

impl RunRecord {
    pub fn empty(seed: u64) -> RunRecord {
        RunRecord {
            seed,
            suite: "",
            steps_run: 0,
            outcome: DstOutcome::Pass {
                state_hash_trail: HashTrail::default(),
            },
            journal: Vec::new(),
            #[cfg(feature = "seq")]
            physical_journal: None,
            frames: Vec::new(),
            deliveries: Vec::new(),
            plan: FaultPlan::default(),
            trail: HashTrail::default(),
            trace: Vec::new(),
            obs: Obs::default(),
            end_ns: 0,
            snapshot_checks: 0,
            journal_verification: None,
            ledger_checks: Vec::new(),
            matched_tail: Vec::new(),
            external_receipts: Vec::new(),
            stale_gen: Vec::new(),
            late_recycle: Vec::new(),
            cancel_filled: Vec::new(),
            truth: Vec::new(),
            compared: [0; 32],
        }
    }
    /// Items `I<n>` compared in this run (see [`RunRecord::compared`]).
    pub fn compared_of(&self, i: InvId) -> u64 {
        self.compared.get(usize::from(i.0)).copied().unwrap_or(0)
    }
    /// xxh3-128 over every journal record (seq, end seq, tsc, kind, flags, body length and bytes) in
    /// order: the run's event digest. INV-9 compares it between two in-process runs of one seed; it
    /// is never committed to the repository (TK-181).
    pub fn event_digest(&self) -> u128 {
        let mut h = xxhash_rust::xxh3::Xxh3::new();
        for r in &self.journal {
            h.update(&r.seq.to_le_bytes());
            h.update(&r.end_seq.to_le_bytes());
            h.update(&r.tsc.to_le_bytes());
            h.update(&r.kind.0.to_le_bytes());
            h.update(&r.flags.0.to_le_bytes());
            h.update(&(r.body.len() as u64).to_le_bytes());
            h.update(&r.body);
        }
        h.digest128()
    }
    /// Output records (`role = Output` by kind) of the journal: the intent stream compared by INV-9.
    pub fn outputs(&self) -> Vec<&JRec> {
        self.journal
            .iter()
            .filter(|r| {
                hft_event::classify(r.kind, &r.body).is_ok_and(|role| role == hft_event::Role::Output)
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
enum Ev {
    Deliver(InputRec),
    ConfirmedCommand(InputRec),
    SutTimer,
    Market,
    Venue(usize),
    Frame { venue: usize, sim: bool, cmd: GwCmd },
    Fault(usize),
    Hk,
    Recon,
    Advisor,
}

/// Runs `cfg` with engine `E`.
pub fn run<E: Sut>(cfg: &DstConfig) -> DstOutcome {
    run_record::<E>(cfg).outcome
}

/// Runs `cfg` and keeps the full record; applies the INV-9 rerun policy.
pub fn run_record<E: Sut>(cfg: &DstConfig) -> RunRecord {
    let mut rec = run_once::<E>(cfg);
    let rerun = match cfg.rerun {
        RerunPolicy::Always => true,
        RerunPolicy::Never => false,
        RerunPolicy::Sample5Pct => xxhash_rust::xxh3::xxh3_64(&cfg.seed.to_le_bytes()) % 100 < 5,
    };
    if rerun && cfg.invariants.contains(InvId::I9) && rec.outcome.is_pass() {
        let mut c2 = cfg.clone();
        c2.rerun = RerunPolicy::Never;
        let r2 = run_once::<E>(&c2);
        let same_out = rec.outputs() == r2.outputs();
        let (d1, d2) = (rec.event_digest(), r2.event_digest());
        rec.compared[usize::from(InvId::I9.0)] += (rec.trail.points.len().max(r2.trail.points.len())
            + rec.outputs().len()
            + rec.journal.len()) as u64;
        if r2.trail != rec.trail || !same_out || d1 != d2 {
            let step = rec.steps_run.saturating_sub(1);
            rec.outcome = DstOutcome::Fail(Box::new(DstFailure {
                seed: cfg.seed,
                suite: cfg.suite,
                step,
                invariant: FailKind::Inv(InvId::I9),
                message: format!(
                    "in-process rerun differs: trail divergence {:?}, outputs equal {same_out},                      event digest {d1:032x} vs {d2:032x}",
                    crate::hash::diff_trails(&rec.trail, &r2.trail)
                ),
                trace: rec.trace.clone(),
                fault_schedule: rec.plan.clone(),
                journal_tail: last_events(&rec.journal, 200).to_vec(),
            }));
        }
    }
    rec
}

struct Loop<'c, E: Sut> {
    cfg: &'c DstConfig,
    sut: E,
    world: World<Ev>,
    venues: Vec<MockVenue<E::Matcher>>,
    sims: Vec<MockVenue<E::Matcher>>,
    market: MarketActor,
    ctl: CtlActor,
    recon: ReconActor,
    advisor: AdvisorActor,
    plan: FaultPlan,
    inv: InvState,
    rec: RunRecord,
    out: StepOut,
    before: Obs,
    after: Obs,
    timer_at: u64,
    venue_wake: Vec<u64>,
    fail: Option<(u64, FailKind, String)>,
    snapshots_checked_through: u64,
}

fn venue_ix<M: crate::dst::venue_behavior::Matcher>(
    v: &[MockVenue<M>],
    id: hft_types::VenueId,
) -> Option<usize> {
    v.iter().position(|x| x.cfg.venue == id)
}

fn run_once<E: Sut>(cfg: &DstConfig) -> RunRecord {
    let mut rec = RunRecord::empty(cfg.seed);
    rec.suite = cfg.suite;
    rec.trail = HashTrail::new(cfg.hash_every);
    let plan = cfg.fault_plan();
    rec.plan = plan.clone();
    let boot = BootCtx {
        seed: cfg.seed,
        world: &cfg.world,
        planted: &cfg.planted,
        tsc_hz: cfg.tsc_hz,
    };
    // The planted defects stay switched on until this run (rebuilds and verifiers included) ends.
    let _plant = match E::plant(&cfg.planted) {
        Ok(p) => p,
        Err(e) => {
            rec.outcome = DstOutcome::Fail(Box::new(DstFailure {
                seed: cfg.seed,
                suite: cfg.suite,
                step: 0,
                invariant: FailKind::Boot,
                message: e.0,
                trace: Vec::new(),
                fault_schedule: plan,
                journal_tail: Vec::new(),
            }));
            return rec;
        }
    };
    let sut = match E::boot(&boot) {
        Ok(s) => s,
        Err(e) => {
            rec.outcome = DstOutcome::Fail(Box::new(DstFailure {
                seed: cfg.seed,
                suite: cfg.suite,
                step: 0,
                invariant: FailKind::Boot,
                message: e.0,
                trace: Vec::new(),
                fault_schedule: plan,
                journal_tail: Vec::new(),
            }));
            return rec;
        }
    };
    let venues = cfg
        .venues
        .iter()
        .map(|c| MockVenue::new(c.clone(), &cfg.world, cfg.seed))
        .collect::<Vec<_>>();
    let sims = cfg
        .venues
        .iter()
        .map(|c| {
            let mut m = MockVenue::new(c.clone(), &cfg.world, cfg.seed ^ 0x5_1A);
            m.clean = true;
            m
        })
        .collect::<Vec<_>>();
    let n = venues.len();
    let advisor_strategy = cfg
        .world
        .strategies
        .first()
        .map_or(hft_types::StrategyId(1), |s| s.instance);
    let mut lp = Loop {
        cfg,
        sut,
        world: World::new(cfg.seed),
        venues,
        sims,
        market: MarketActor::new(cfg.seed, &cfg.world, &cfg.market),
        ctl: CtlActor::new(cfg.seed),
        recon: ReconActor::new(30_000_000_000),
        advisor: AdvisorActor {
            period_ns: 100_000_000,
            strategy: advisor_strategy,
            lags: Vec::new(),
        },
        plan,
        inv: InvState::default(),
        rec,
        out: StepOut::default(),
        before: Obs::default(),
        after: Obs::default(),
        timer_at: u64::MAX,
        venue_wake: vec![u64::MAX; n],
        fail: None,
        snapshots_checked_through: 0,
    };
    lp.go();
    lp.finish()
}

impl<E: Sut> Loop<'_, E> {
    fn go(&mut self) {
        let first = self.market.first();
        self.world.schedule(first, Ev::Market);
        for (i, (t, f)) in self.plan.items.iter().enumerate() {
            if !matches!(f, Fault::Crash(_) | Fault::CleanRestart(_)) {
                self.world.schedule(*t, Ev::Fault(i));
            }
        }
        for (at, input) in &self.cfg.confirmed_commands {
            self.world.schedule(*at, Ev::ConfirmedCommand(input.clone()));
        }
        for i in 0..self.venues.len() {
            self.wake_venue(i);
        }
        let a = self.cfg.actors;
        if a.hk {
            self.world.schedule(1_000_000_000, Ev::Hk);
        }
        if a.recon {
            self.world.schedule(self.recon.period_ns, Ev::Recon);
        }
        if a.advisor {
            self.world.schedule(self.advisor.period_ns, Ev::Advisor);
        }
        self.observe_into_after();
        self.sut.drain_observations(&mut self.out);
        self.check_snapshot_boundaries();
        while self.rec.steps_run < self.cfg.steps && self.fail.is_none() {
            let Some(s) = self.world.pop() else { break };
            if s.at > self.cfg.horizon_ns {
                break;
            }
            let now = s.at;
            match s.ev {
                Ev::Deliver(i) => self.step(now, Some(i)),
                Ev::ConfirmedCommand(mut input) => {
                    let prepared = (|| {
                        if !self.cfg.actors.ctl
                            || input.kind != hft_event::EventKind::CONTROL_CMD
                            || input.src != Source::Ctl
                        {
                            return Err("invalid confirmed command or disabled control actor".into());
                        }
                        let (mut hdr, _) = hft_event::CmdHdr::read_from_prefix(&input.body)
                            .map_err(|_| "truncated confirmed command")?;
                        hdr.expect_state_version = self.sut.control_state_version()?;
                        hdr.confirmed = 1;
                        input.body[..size_of::<hft_event::CmdHdr>()].copy_from_slice(hdr.as_bytes());
                        Ok::<_, String>(())
                    })();
                    match prepared {
                        Ok(()) => self.step(now, Some(input)),
                        Err(msg) => self.fail = Some((self.rec.steps_run, FailKind::Scenario("TK-142"), msg)),
                    }
                }
                Ev::SutTimer => {
                    if self.sut.next_deadline().is_some_and(|d| d <= now) {
                        self.step(now, None);
                    }
                    self.timer_at = u64::MAX;
                    self.arm_timer();
                }
                Ev::Market => {
                    let (outs, next) = self.market.tick(now);
                    for o in outs {
                        match o {
                            MarketOut::Bbo {
                                venue,
                                inst,
                                bid,
                                ask,
                            } => {
                                if let Some(v) = venue_ix(&self.venues, venue) {
                                    if let Some((at, inp)) = self.venues[v].on_market(inst, bid, ask, now) {
                                        self.world.schedule(at, Ev::Deliver(inp));
                                    }
                                    self.sims[v].m.on_bbo(inst, bid, ask, now);
                                    self.wake_venue(v);
                                }
                            }
                            MarketOut::Recorded(i) => {
                                // The exchange truth follows the recorded quotes too.
                                if let Some(b) = i.read::<hft_event::Bbo>()
                                    && let Some(v) = venue_ix(&self.venues, b.h.venue)
                                {
                                    let (bid, ask) = ((b.bid_px.0, b.bid_qty.0), (b.ask_px.0, b.ask_qty.0));
                                    self.venues[v].m.on_bbo(b.h.inst, bid, ask, now);
                                    self.sims[v].m.on_bbo(b.h.inst, bid, ask, now);
                                }
                                // So do recorded mark prices (margin and liquidation truth).
                                if let Some(m) = i.read::<hft_event::MarkPrice>()
                                    && let Some(v) = venue_ix(&self.venues, m.h.venue)
                                {
                                    self.venues[v].m.on_mark(m.h.inst, m.px.0, now);
                                    self.sims[v].m.on_mark(m.h.inst, m.px.0, now);
                                    self.wake_venue(v);
                                }
                                self.world.schedule(now, Ev::Deliver(i));
                            }
                        }
                    }
                    if next != u64::MAX {
                        self.world.schedule(next, Ev::Market);
                    }
                }
                Ev::Venue(v) => {
                    self.venue_wake[v] = u64::MAX;
                    let ins = self.venues[v].tick(now);
                    self.deliver_all(ins);
                    let ins = self.sims[v].tick(now);
                    self.deliver_all(ins);
                    self.wake_venue(v);
                }
                Ev::Frame { venue, sim, cmd } => {
                    let ins = if sim {
                        self.sims[venue].on_frame(&cmd, now)
                    } else {
                        self.venues[venue].on_frame(&cmd, now)
                    };
                    self.deliver_all(ins);
                    self.wake_venue(venue);
                }
                Ev::Fault(i) => self.apply_fault(i, now),
                Ev::Hk => {
                    self.world
                        .schedule(now, Ev::Deliver(HkActor::clock_status(now, self.cfg.tsc_hz)));
                    self.world.schedule(now + 1_000_000_000, Ev::Hk);
                }
                Ev::Recon => {
                    let truth = self.truth(now);
                    let history: Vec<_> = self
                        .venues
                        .iter()
                        .flat_map(|v| v.m.external_history().iter().cloned())
                        .collect();
                    let inputs = match self.sut.reconcile_inputs(&mut self.recon, &truth, &history) {
                        Ok(Some(inputs)) => inputs,
                        Ok(None) if self.after.required.is_none() => {
                            let obs = &self.after;
                            self.recon
                                .round(now, &truth, &|a, v, i| obs.pos(a, v, i))
                                .iter()
                                .map(|r| InputRec::of(r, Source::Recon))
                                .collect()
                        }
                        other => {
                            let msg = match other {
                                Err(e) => e,
                                _ => "reconcile: real adapter did not provide a committed view".into(),
                            };
                            self.fail = Some((self.rec.steps_run, FailKind::Scenario("TK-135"), msg));
                            continue;
                        }
                    };
                    for input in inputs {
                        self.world.schedule(now, Ev::Deliver(input));
                    }
                    self.world.schedule(now + self.recon.period_ns, Ev::Recon);
                }
                Ev::Advisor => {
                    let inputs = if self.cfg.world.model_advice {
                        match self.sut.advisor_inputs() {
                            Ok(Some(inputs)) => inputs,
                            other => {
                                self.fail = Some((
                                    self.rec.steps_run,
                                    FailKind::Scenario("TK-152"),
                                    match other {
                                        Err(e) => e,
                                        _ => "real Model advisor unavailable".into(),
                                    },
                                ));
                                continue;
                            }
                        }
                    } else {
                        vec![self.advisor.advice(self.sut.seq(), now, self.cfg.tsc_hz)]
                    };
                    let at = self.advisor.delivery(now);
                    for input in inputs {
                        self.world.schedule(at, Ev::Deliver(input));
                    }
                    self.world.schedule(now + self.advisor.period_ns, Ev::Advisor);
                }
            }
            // Queued venue faults may fail when an Ack arrives or the stream reconnects,
            // after apply_fault has returned. Never let that failure become an empty pass.
            if self.fail.is_none()
                && let Some(error) = self.venues.iter().find_map(|v| v.fault_error())
            {
                self.fail = Some((self.rec.steps_run, FailKind::Scenario("TK-161"), error.to_owned()));
            }
        }
    }

    fn deliver_all(&mut self, ins: Vec<(u64, InputRec)>) {
        for (at, i) in ins {
            self.world.schedule(at, Ev::Deliver(i));
        }
    }

    fn wake_venue(&mut self, v: usize) {
        let now = self.world.now();
        let t = self.venues[v].next_wake(now).min(self.sims[v].next_wake(now));
        if t < self.venue_wake[v] && t != u64::MAX {
            self.venue_wake[v] = t;
            self.world.schedule(t, Ev::Venue(v));
        }
    }

    fn arm_timer(&mut self) {
        if let Some(d) = self.sut.next_deadline()
            && d < self.timer_at
        {
            self.timer_at = d;
            self.world.schedule(d, Ev::SutTimer);
        }
    }

    fn inst_venue(&self, i: InstId) -> Option<usize> {
        let v = self.cfg.world.inst(i)?.venue;
        venue_ix(&self.venues, v)
    }

    fn apply_fault(&mut self, ix: usize, now: u64) {
        let f = self.plan.items[ix].1;
        use Fault::*;
        let target = match f {
            ConnDown { venue, .. }
            | GrayConn { venue, .. }
            | ReportReorder { venue, .. }
            | ReportDup { venue, .. }
            | ReportDelay { venue, .. }
            | FillBeforeAck { venue }
            | CancelRejectFilled { venue, .. }
            | PartialFills { venue, .. }
            | LateFeeOnly { venue, .. }
            | StaleGenFill { venue, .. }
            | LateFillAfterRecycle { venue, .. }
            | RateLimit { venue, .. }
            | ServerError { venue }
            | Timeout { venue }
            | UnknownCode { venue }
            | NotFoundNoGuard { venue }
            | SecondLegReject { venue }
            | SecondLegSilent { venue, .. }
            | PersistentSecondReject { venue, .. }
            | L3OtherLegSilent { venue, .. }
            | BaseFee { venue }
            | NoClientIdFill { venue, .. }
            | MarginPath { venue, .. } => venue_ix(&self.venues, venue),
            QuietBook { inst, .. }
            | SelfLiquidation { inst, .. }
            | ManualFill { inst, .. }
            | Adl { inst, .. }
            | ReconciledAdl { inst, .. }
            | ForeignOrderFill { inst, .. } => self.inst_venue(inst),
            _ => None,
        };
        if let Some(v) = target {
            let ins = self.venues[v].apply_fault(&f, now);
            if let Some(e) = self.venues[v].fault_error() {
                let case = if matches!(f, ForeignOrderFill { .. }) {
                    "LED-080"
                } else {
                    "TK-138"
                };
                self.fail = Some((self.rec.steps_run, FailKind::Scenario(case), e.to_string()));
                return;
            }
            self.deliver_all(ins);
            self.wake_venue(v);
        }
        match f {
            OneSidedMove { .. } => self.market.apply_fault(&f, now),
            AdvisorLag(l) => self.advisor.lags.push(l),
            PersistLag { .. } | DiskLow => {
                if let Some(i) = HkActor::apply_fault(&f) {
                    self.world.schedule(now, Ev::Deliver(i));
                }
            }
            _ if f.category() == crate::dst::FaultCategory::Control && self.cfg.actors.ctl => {
                let ins = self.ctl.apply_fault(&f, now);
                self.deliver_all(ins);
            }
            _ => {}
        }
    }

    fn truth(&self, now: u64) -> Vec<TruthRow> {
        let kind = |a: hft_types::AccountId| self.cfg.world.account(a).map(|x| x.kind);
        let mut t: Vec<TruthRow> = Vec::new();
        for v in &self.venues {
            t.extend(
                v.truth(now)
                    .into_iter()
                    .filter(|r| kind(r.account) == Some(AccountKind::Live)),
            );
        }
        for v in &self.sims {
            t.extend(
                v.truth(now)
                    .into_iter()
                    .filter(|r| kind(r.account) == Some(AccountKind::Sim)),
            );
        }
        t
    }

    fn observe_into_after(&mut self) {
        // Actors such as Recon consume this view too. Disabling assertions must not change
        // their inputs by leaving the engine's positions at the empty default.
        self.after = Obs::default();
        self.sut.observe(&mut self.after);
        self.after.normalize();
    }

    fn step(&mut self, now: u64, input: Option<InputRec>) {
        let step = self.rec.steps_run;
        // Step-based storage faults.
        let crash = self.plan.items.iter().find_map(|(_, f)| match f {
            Fault::Crash(c) if c.at_step == step => Some(*c),
            _ => None,
        });
        let clean = self
            .plan
            .items
            .iter()
            .any(|(_, f)| matches!(f, Fault::CleanRestart(c) if c.at_step == step));
        std::mem::swap(&mut self.before, &mut self.after);
        self.out.clear();
        let tsc = tsc_at(0, self.cfg.tsc_hz, now);
        let sut = &mut self.sut;
        let out = &mut self.out;
        let inp = input.as_ref();
        let r = catch_unwind(AssertUnwindSafe(|| match inp {
            Some(i) => {
                sut.process(now, tsc, i, out);
                true
            }
            None => sut.fire_timers(now, tsc, out),
        }));
        let (src, kind) = match &input {
            Some(i) => (i.src, i.kind),
            None => (Source::Engine, hft_event::EventKind::TIMER),
        };
        let seq = self.out.records.first().map_or(0, |r| r.seq);
        let item = TraceItem {
            step,
            at_ns: now,
            src,
            kind,
            seq,
        };
        self.rec.trace.push(item);
        if !self.cfg.trace && self.rec.trace.len() > 400 {
            self.rec.trace.drain(..200);
        }
        let delivered = input.as_ref().map(|i| (i.kind, i.body.clone()));
        if let Some(i) = input {
            self.rec.deliveries.push((now, i));
        }
        match r {
            Err(p) => {
                let msg = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
                    .unwrap_or_default();
                self.rec.steps_run += 1;
                self.fail = Some((step, FailKind::Panic, msg));
                return;
            }
            Ok(false) => {
                // No timer was due: not a step.
                self.rec.trace.pop();
                std::mem::swap(&mut self.before, &mut self.after);
                return;
            }
            Ok(true) => {}
        }
        self.rec.steps_run += 1;
        if let Some(c) = crash {
            let mut res = self.sut.crash_and_recover(c.point, None);
            for _ in 1..c.repeat {
                if res.is_ok() {
                    res = self.sut.crash_and_recover(c.point, None);
                }
            }
            if let Err(e) = res {
                self.fail = Some((step, FailKind::Boot, e.0));
                return;
            }
        }
        if clean && let Err(e) = self.sut.clean_restart() {
            self.fail = Some((step, FailKind::Boot, e.0));
            return;
        }
        self.sut.drain_observations(&mut self.out);
        // A batch merely queued by the actor cannot release a gap. Ask after actual commit,
        // even with all checkers disabled, so observation choices never alter delivery.
        if self
            .out
            .records
            .iter()
            .any(|r| r.kind == hft_event::EventKind::RECONCILE_BATCH)
        {
            for v in 0..self.venues.len() {
                match self.venues[v].acknowledge_reconciled(&self.sut, now) {
                    Ok(true) => self.wake_venue(v),
                    Ok(false) => {}
                    Err(e) => {
                        self.fail = Some((step, FailKind::Scenario("TK-138"), e));
                        return;
                    }
                }
            }
        }
        // Frames to venues and the sim.
        let frames: Vec<FrameRec> = self.out.frames.clone();
        for f in frames {
            self.rec.frames.push((now, f));
            let (vix, sim) = match f.ring {
                OutRing::Venue(v) => (venue_ix(&self.venues, v), false),
                OutRing::Sim => (self.inst_venue(f.cmd.inst), true),
            };
            if let Some(v) = vix {
                self.world.schedule(
                    now + 20_000,
                    Ev::Frame {
                        venue: v,
                        sim,
                        cmd: f.cmd,
                    },
                );
            }
        }
        self.arm_timer();
        self.observe_into_after();
        // Venue transport consumes committed observations even when assertions are disabled.
        // Recovery has already trimmed/rebuilt the journal and refreshed this view.
        for v in 0..self.venues.len() {
            if self.venues[v].observe_recycled(&self.after, self.sut.journal(), now) {
                self.wake_venue(v);
            }
        }
        let ordinal = self.rec.steps_run;
        if self.cfg.hash_every > 0 && ordinal.is_multiple_of(self.cfg.hash_every) {
            self.rec.trail.sample(ordinal, self.sut.hashes());
        }
        if self.cfg.invariants == InvariantSet::none() {
            return;
        }
        self.check_snapshot_boundaries();
        if self.fail.is_some() {
            return;
        }
        let rebuild = if self.cfg.invariants.contains(InvId::I6)
            && self.cfg.rebuild_every > 0
            && ordinal.is_multiple_of(self.cfg.rebuild_every)
        {
            match self.rebuild() {
                Ok(b) => b.map(|b| (self.sut.hashes(), b)),
                Err(m) => {
                    self.fail = Some((step, FailKind::Inv(InvId::I6), m));
                    return;
                }
            }
        } else {
            None
        };
        let truth = self.truth(now);
        let v = StepView {
            step,
            now_ns: now,
            records: &self.out.records,
            delivered: delivered.as_ref().map(|(k, b)| (*k, b.as_slice())),
            frames: &self.out.frames,
            info: &self.out.info,
            before: &self.before,
            after: &self.after,
            truth: &truth,
            rebuild,
        };
        let set = self.cfg.invariants;
        if self.after.required.is_some() {
            let err = self
                .after
                .observation_error
                .clone()
                .map_or_else(|| self.out.verify_observations(), Err);
            if let Err(err) = err {
                self.fail = Some((step, FailKind::Inv(InvId::I8), err));
                return;
            }
        }
        let mut result = Ok(());
        if self.out.events.is_empty() {
            result = check_step(set, &mut self.inv, &v);
        } else {
            // Exchange truth and replay compare the whole world step. Business checks run on
            // each actual Input, before later Inputs and automatic restarts can hide state.
            for event in &self.out.events {
                if set.contains(InvId::I6)
                    && event
                        .records
                        .first()
                        .is_some_and(|r| crate::dst::sut::ledger_input(r.kind))
                {
                    if let Err(e) = self.sut.verify_ledger_event(event) {
                        result = Err((InvId::I6, e));
                        break;
                    }
                    if self.rec.ledger_checks.len() == crate::dst::venue_behavior::EXTERNAL_HISTORY_CAP {
                        result = Err((InvId::I6, "ledger event evidence capacity exceeded".into()));
                        break;
                    }
                    self.rec.ledger_checks.push(event.records[0].seq);
                }
                let ev = StepView {
                    records: &event.records,
                    frames: &event.frames,
                    info: &event.info,
                    before: &event.before,
                    after: &event.after,
                    delivered: None,
                    truth: &[],
                    rebuild: None,
                    ..v
                };
                result = check_step(set.without(InvId::I6).without(InvId::I16), &mut self.inv, &ev);
                if result.is_err() {
                    break;
                }
            }
            if result.is_ok() {
                let final_set = InvariantSet::none();
                let final_set = if set.contains(InvId::I6) {
                    final_set.with(InvId::I6)
                } else {
                    final_set
                };
                let final_set = if set.contains(InvId::I16) {
                    final_set.with(InvId::I16)
                } else {
                    final_set
                };
                result = check_step(final_set, &mut self.inv, &v);
            }
        }
        if let Err((i, m)) = result {
            self.fail = Some((step, FailKind::Inv(i), m));
        }
    }

    fn check_snapshot_boundaries(&mut self) {
        if !self.cfg.invariants.contains(InvId::I6) {
            return;
        }
        let result = (|| {
            self.out
                .verify_snapshot_observations(self.sut.journal(), self.snapshots_checked_through)?;
            for p in &self.out.snapshots {
                let b = self.sut.rebuilt_snapshot(p)?;
                if let Some(c) = p.live.first_diff(&b) {
                    return Err(format!(
                        "snapshot {} at seq {}: rebuilt state differs in {}",
                        p.snapshot_seq,
                        p.through_seq,
                        c.name()
                    ));
                }
                self.rec.snapshot_checks += 1;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.snapshots_checked_through = self.sut.seq(),
            Err(m) => self.fail = Some((self.rec.steps_run.saturating_sub(1), FailKind::Inv(InvId::I6), m)),
        }
    }

    fn rebuild(&self) -> Result<Option<ComponentHashes>, String> {
        let rebuilt = self.sut.rebuilt_hashes()?;
        if rebuilt.is_none()
            && (self.after.required.is_some()
                || self
                    .sut
                    .journal()
                    .iter()
                    .any(|r| r.kind == hft_event::EventKind::STATE_SNAPSHOT))
        {
            return Err("journal rebuild unavailable or output verification failed".into());
        }
        Ok(rebuilt)
    }

    fn finish(mut self) -> RunRecord {
        let now = self.world.now();
        self.rec.truth = self.truth(now);
        if self.fail.is_none() && self.cfg.invariants != InvariantSet::none() {
            let truth = self.truth(now);
            let end = EndView {
                now_ns: now,
                obs: &self.after,
                truth: &truth,
            };
            if let Err((i, m)) = check_end(self.cfg.invariants, &mut self.inv, &end) {
                self.fail = Some((self.rec.steps_run.saturating_sub(1), FailKind::Inv(i), m));
            }
            if self.fail.is_none() && self.cfg.invariants.contains(InvId::I6) {
                let err = match self.rebuild() {
                    Err(m) => Some(m),
                    Ok(Some(b)) => self
                        .sut
                        .hashes()
                        .first_diff(&b)
                        .map(|c| format!("end: rebuilt state differs in {}", c.name())),
                    Ok(None) => None,
                };
                if let Some(m) = err {
                    self.fail = Some((self.rec.steps_run.saturating_sub(1), FailKind::Inv(InvId::I6), m));
                }
                if self.fail.is_none() {
                    let result = self.sut.verify_journal().and_then(|v| {
                        if v.is_none()
                            && (self.after.required.is_some()
                                || self
                                    .sut
                                    .journal()
                                    .iter()
                                    .any(|r| r.kind == hft_event::EventKind::STATE_SNAPSHOT))
                        {
                            Err("full journal verification unavailable".into())
                        } else {
                            Ok(v)
                        }
                    });
                    match result {
                        Ok(v) => self.rec.journal_verification = v,
                        Err(m) => {
                            self.fail =
                                Some((self.rec.steps_run.saturating_sub(1), FailKind::Inv(InvId::I6), m))
                        }
                    }
                }
            }
        }
        self.rec.journal = self.sut.journal().to_vec();
        self.rec.compared = self.inv.compared;
        if self.cfg.invariants.contains(InvId::I6) {
            let i6 = &mut self.rec.compared[usize::from(InvId::I6.0)];
            *i6 += self.rec.snapshot_checks + self.rec.ledger_checks.len() as u64;
            if let Some(j) = self.rec.journal_verification {
                *i6 += j.inputs + j.outputs + j.snapshots;
            }
        }
        #[cfg(feature = "seq")]
        if self.cfg.world.physical_journal {
            match self.sut.physical_journal() {
                Ok(Some(log)) => self.rec.physical_journal = Some(log),
                other if self.fail.is_none() => {
                    self.fail = Some((
                        self.rec.steps_run.saturating_sub(1),
                        FailKind::Scenario("TK-153"),
                        match other {
                            Err(e) => e,
                            _ => "physical journal unavailable".into(),
                        },
                    ));
                }
                _ => {}
            }
        }
        for v in self.venues.iter().chain(&self.sims) {
            if !v.clean {
                self.rec.stale_gen.push(v.stale_gen_report());
                self.rec.late_recycle.push(v.late_recycle_report());
                self.rec.cancel_filled.push(v.cancel_filled_report());
            }
            self.rec.matched_tail.extend(v.m.matched_tail());
            self.rec
                .external_receipts
                .extend_from_slice(v.m.external_history());
        }
        if let Some((_, FailKind::Inv(InvId::I16), message)) = &mut self.fail {
            for (generated, e) in &self.rec.matched_tail {
                message.push_str(&format!(
                    "\nmatch gen_seq={generated} account={} venue={} epoch={} slot={} gen={} inst={} px={} qty={} cum={}",
                    e.account.0, e.venue.0, e.epoch, e.slot.0, e.r#gen, e.inst.0, e.px.0, e.qty.0, e.cum_filled.0
                ));
            }
        }
        self.rec.obs = std::mem::take(&mut self.after);
        self.rec.end_ns = now;
        let keep = if self.cfg.trace { usize::MAX } else { 200 };
        let n = self.rec.trace.len();
        if n > keep {
            self.rec.trace.drain(..n - keep);
        }
        self.rec.outcome = match self.fail.take() {
            None => DstOutcome::Pass {
                state_hash_trail: self.rec.trail.clone(),
            },
            Some((step, invariant, message)) => DstOutcome::Fail(Box::new(DstFailure {
                seed: self.cfg.seed,
                suite: self.cfg.suite,
                step,
                invariant,
                message,
                trace: self.rec.trace.clone(),
                fault_schedule: self.rec.plan.clone(),
                journal_tail: last_events(&self.rec.journal, 200).to_vec(),
            })),
        };
        self.rec
    }
}
