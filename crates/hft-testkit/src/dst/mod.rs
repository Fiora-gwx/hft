//! Deterministic simulation testing (README "6. DST 框架", "DST 的世界模型").
//!
//! One seed decides every interleaving: the fault plan is generated once from it, every actor
//! draws from its own derived ChaCha8 stream, and the engine under test ([`Sut`]) only sees
//! Inputs. A run is `run::<E>(&cfg)`; a failure carries the seed, step, invariant, the trace of
//! the last 200 events and the fault schedule, and prints one copyable repro line.
//!
//! Module map: `world` (virtual time, event heap, actor streams, world fixture W1), `driver`
//! (`DstDriver`, in-memory journal), `sut` (engine boundary), `obs` (debug-view rows), `faults`
//! (catalog, rates, plan), `invariants` (`I<n>` = docs/tests/invariants.md `INV-n`), `actors`
//! (market, control, reconciler, housekeeping, advisor, sim), `venue_behavior` (`MockVenue`:
//! matcher + venue behaviour layer), `run` (the loop), `shrink`, `report` (repro line and
//! artifacts), `scenario` (`Scenario`, `DstSuite`), `toy` (the framework's self-test engine).

pub mod actors;
pub mod driver;
pub mod faults;
pub mod invariants;
pub mod obs;
pub mod report;
pub mod run;
pub mod scenario;
#[cfg(feature = "seq")]
pub mod seq;
pub mod shrink;
pub mod sut;
pub mod toy;
pub mod venue_behavior;
pub mod world;

pub use driver::{DstDriver, MemJournal};
pub use faults::{
    AdvisorLagFault, CatRate, CleanRestartFault, ConnKind, CrashFault, Fault, FaultCategory, FaultPlan,
    FaultRates,
};
pub use invariants::{InvId, InvariantSet};
pub use run::{RunRecord, run, run_record};
pub use scenario::{DstSuite, Scenario, SeedSet, SuiteSel};
pub use shrink::shrink;
pub use sut::{BootCtx, BootFail, CrashPoint, FrameRec, InputRec, JRec, OutRing, Source, StepOut, Sut};
#[cfg(feature = "seq")]
pub use seq::{SeqSut, SimMatcher};
pub use toy::ToyEngine;
pub use venue_behavior::MockVenueCfg;
pub use world::{World, WorldSpec};

use crate::dst::sut::JRec as Rec;
use crate::hash::HashTrail;

/// Market data of a run.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum MarketSource {
    /// Seeded random walk on the world's insts (W1: ±`walk_bp`/10 000 per tick).
    Generated(MarketGen),
    /// A recorded journal fragment: market Inputs at their virtual times, replayed body for body.
    Recorded(Vec<(u64, InputRec)>),
}

/// Generated market data.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarketGen {
    /// Mean interval between two BBO updates of one inst (virtual ms; jittered ±50%).
    pub tick_ms: u64,
    /// Walk step in 0.01 bp (1 = ±0.01 bp per update; W1 uses 100 = ±0.01%).
    pub walk_centi_bp: i64,
}

impl Default for MarketGen {
    fn default() -> Self {
        MarketGen {
            tick_ms: 50,
            walk_centi_bp: 100,
        }
    }
}

/// Which optional actors run.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ActorSet {
    pub ctl: bool,
    pub recon: bool,
    pub hk: bool,
    pub advisor: bool,
}

impl ActorSet {
    pub const ALL: ActorSet = ActorSet {
        ctl: true,
        recon: true,
        hk: true,
        advisor: true,
    };
}

/// INV-9 in-process rerun policy.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum RerunPolicy {
    /// Seeds with `xxh3_64(seed) mod 100 < 5` (README I9).
    Sample5Pct,
    Always,
    Never,
}

/// One DST run (README "6. DST 框架" `DstConfig`). Engine-independent data: the engine type is
/// the type parameter of [`run`] (`SeqSut<S>` for a strategy set `S`).
#[derive(Clone, Debug)]
pub struct DstConfig {
    pub seed: u64,
    /// The suite this run belongs to (repro line `--suite`).
    pub suite: &'static str,
    /// Step limit (one step = one Input with all its Outputs).
    pub steps: u64,
    /// Virtual-time limit of the run and horizon of the fault plan.
    pub horizon_ns: u64,
    pub world: WorldSpec,
    /// Venue behaviour layer, one per world venue.
    pub venues: Vec<MockVenueCfg>,
    pub market: MarketSource,
    pub faults: FaultRates,
    /// Scenario fault script, merged into the generated plan.
    pub scripted: Vec<(u64, Fault)>,
    /// Explicit plan (`shrink`, repro of a recorded schedule); `None` = generate.
    pub plan: Option<FaultPlan>,
    pub invariants: InvariantSet,
    /// `HashTrail` sampling interval (Inputs).
    pub hash_every: u64,
    /// INV-6 rebuild check interval (steps).
    pub rebuild_every: u64,
    pub rerun: RerunPolicy,
    pub actors: ActorSet,
    /// Planted defects (framework self-tests).
    pub planted: Vec<&'static str>,
    /// Virtual TSC frequency.
    pub tsc_hz: u64,
    /// Keep the full per-step trace (repro `--trace`).
    pub trace: bool,
}

impl DstConfig {
    /// A W1 run with no faults and every invariant on.
    pub fn new(seed: u64, suite: &'static str) -> DstConfig {
        let world = WorldSpec::w1();
        let venues = world.venues.iter().map(|v| MockVenueCfg::w1(v.id)).collect();
        DstConfig {
            seed,
            suite,
            steps: crate::seed::DST_DEFAULT_STEPS,
            horizon_ns: 3_600 * 1_000_000_000,
            world,
            venues,
            market: MarketSource::Generated(MarketGen::default()),
            faults: FaultRates::off(),
            scripted: Vec::new(),
            plan: None,
            invariants: InvariantSet::all(),
            hash_every: 100,
            rebuild_every: 1_000,
            rerun: RerunPolicy::Sample5Pct,
            actors: ActorSet::ALL,
            planted: Vec::new(),
            tsc_hz: 1_000_000_000,
            trace: false,
        }
    }
    /// The run's fault plan: the explicit plan, or the seed's generated plan plus the script.
    pub fn fault_plan(&self) -> FaultPlan {
        match &self.plan {
            Some(p) => p.clone(),
            None => FaultPlan::generate(self.seed, &self.faults, &self.world, self.horizon_ns, self.steps)
                .with(&self.scripted),
        }
    }
}

/// What failed.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum FailKind {
    Inv(InvId),
    /// A scenario's own assertion (its case id).
    Scenario(&'static str),
    /// The engine panicked inside the step.
    Panic,
    /// The engine refused to boot (initially or after a crash).
    Boot,
}

impl FailKind {
    /// `I3(unhedged)`, `QF-1`, `panic`, `boot`.
    pub fn label(&self) -> String {
        match self {
            FailKind::Inv(i) => format!("{}({})", i.code(), i.name()),
            FailKind::Scenario(id) => (*id).to_string(),
            FailKind::Panic => "panic".into(),
            FailKind::Boot => "boot".into(),
        }
    }
}

/// One traced step.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct TraceItem {
    pub step: u64,
    pub at_ns: u64,
    pub src: Source,
    pub kind: hft_event::EventKind,
    /// Seq of the Input record (0 = none journaled).
    pub seq: u64,
}

/// A failed run.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct DstFailure {
    pub seed: u64,
    pub suite: &'static str,
    /// 0-based step of the violation (`--steps` of the repro = step + 1).
    pub step: u64,
    pub invariant: FailKind,
    pub message: String,
    /// The last ≤ 200 steps; the last item is the violating step.
    pub trace: Vec<TraceItem>,
    pub fault_schedule: FaultPlan,
    /// Journal records of the last ≤ 200 events (replayable fragment).
    pub journal_tail: Vec<Rec>,
}

/// Result of `run`.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum DstOutcome {
    Pass { state_hash_trail: HashTrail },
    Fail(Box<DstFailure>),
}

impl DstOutcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, DstOutcome::Pass { .. })
    }
    pub fn failure(&self) -> Option<&DstFailure> {
        match self {
            DstOutcome::Fail(f) => Some(f),
            DstOutcome::Pass { .. } => None,
        }
    }
}
