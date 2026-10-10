//! The DST loop (README "DST 的世界模型"): one world, one engine, every actor, the fault plan,
//! the invariant checkers after every step, the hash trail, the INV-6 rebuild checks and the INV-9
//! in-process rerun.

use std::panic::{AssertUnwindSafe, catch_unwind};

use hft_event::GwCmd;
use hft_types::InstId;

use crate::dst::actors::{AdvisorActor, CtlActor, HkActor, MarketActor, MarketOut, ReconActor};
use crate::dst::driver::{last_events, tsc_at};
use crate::dst::faults::{Fault, FaultPlan};
use crate::dst::invariants::{EndView, InvId, InvState, InvariantSet, StepView, check_end, check_step};
use crate::dst::obs::{Obs, TruthRow};
use crate::dst::sut::{BootCtx, FrameRec, InputRec, JRec, OutRing, Source, StepOut, Sut};
use crate::dst::venue_behavior::{Matcher, MockVenue};
use crate::dst::world::{AccountKind, World};
use crate::dst::{DstConfig, DstFailure, DstOutcome, FailKind, RerunPolicy, TraceItem};
use crate::hash::HashTrail;

/// Everything a finished run leaves for scenario assertions and framework tests.
#[derive(Clone, Debug)]
pub struct RunRecord {
    pub seed: u64,
    pub suite: &'static str,
    pub steps_run: u64,
    pub outcome: DstOutcome,
    /// The engine's journal at the end.
    pub journal: Vec<JRec>,
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
            frames: Vec::new(),
            deliveries: Vec::new(),
            plan: FaultPlan::default(),
            trail: HashTrail::default(),
            trace: Vec::new(),
            obs: Obs::default(),
            end_ns: 0,
        }
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
        if r2.trail != rec.trail || !same_out {
            let step = rec.steps_run.saturating_sub(1);
            rec.outcome = DstOutcome::Fail(Box::new(DstFailure {
                seed: cfg.seed,
                suite: cfg.suite,
                step,
                invariant: FailKind::Inv(InvId::I9),
                message: format!(
                    "in-process rerun differs: trail divergence {:?}, outputs equal {same_out}",
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
        while self.rec.steps_run < self.cfg.steps && self.fail.is_none() {
            let Some(s) = self.world.pop() else { break };
            if s.at > self.cfg.horizon_ns {
                break;
            }
            let now = s.at;
            match s.ev {
                Ev::Deliver(i) => self.step(now, Some(i)),
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
                    let obs = self.after.clone();
                    let reps = self.recon.round(now, &truth, &|a, v, i| obs.pos(a, v, i));
                    for r in reps {
                        self.world
                            .schedule(now, Ev::Deliver(InputRec::of(&r, Source::Recon)));
                    }
                    self.world.schedule(now + self.recon.period_ns, Ev::Recon);
                }
                Ev::Advisor => {
                    let adv = self.advisor.advice(self.sut.seq(), now, self.cfg.tsc_hz);
                    let at = self.advisor.delivery(now);
                    self.world.schedule(at, Ev::Deliver(adv));
                    self.world.schedule(now + self.advisor.period_ns, Ev::Advisor);
                }
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
            | CancelRejectFilled { venue }
            | PartialFills { venue, .. }
            | LateFeeOnly { venue }
            | StaleGenFill { venue }
            | LateFillAfterRecycle { venue }
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
            | Adl { inst, .. } => self.inst_venue(inst),
            _ => None,
        };
        if let Some(v) = target {
            let ins = self.venues[v].apply_fault(&f, now);
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
        if self.cfg.invariants == InvariantSet::none() {
            return;
        }
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
        let ordinal = self.rec.steps_run;
        if self.cfg.hash_every > 0 && ordinal.is_multiple_of(self.cfg.hash_every) {
            self.rec.trail.sample(ordinal, self.sut.hashes());
        }
        if self.cfg.invariants == InvariantSet::none() {
            return;
        }
        let rebuild = if self.cfg.invariants.contains(InvId::I6)
            && self.cfg.rebuild_every > 0
            && ordinal.is_multiple_of(self.cfg.rebuild_every)
        {
            self.sut.rebuilt_hashes().map(|b| (self.sut.hashes(), b))
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
        if let Err((i, m)) = check_step(self.cfg.invariants, &mut self.inv, &v) {
            self.fail = Some((step, FailKind::Inv(i), m));
        }
    }

    fn finish(mut self) -> RunRecord {
        let now = self.world.now();
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
            if self.fail.is_none()
                && self.cfg.invariants.contains(InvId::I6)
                && let Some(b) = self.sut.rebuilt_hashes()
                && let Some(c) = self.sut.hashes().first_diff(&b)
            {
                self.fail = Some((
                    self.rec.steps_run.saturating_sub(1),
                    FailKind::Inv(InvId::I6),
                    format!("end: rebuilt state differs in {}", c.name()),
                ));
            }
        }
        self.rec.journal = self.sut.journal().to_vec();
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
