//! Suites and scenarios (README "6. DST 框架" 场景套件, docs/tests/README.md §6).
//!
//! A [`DstSuite`] is a named baseline: world, market source, fault rates, step count and the must-pass
//! case ids. A [`Scenario`] is one `dst` case registered by its id with `Scenario::new("<id>", …)`:
//! the suite it runs in (or every suite, chosen per seed), its seed set, a `configure` hook that
//! adds the case's `前置` and fault script to the suite baseline, and an `assert` hook over the
//! run record (the case's own `断言`; the invariants are checked by the run itself).
//!
//! `cargo xtask dst`, `hft-dst` and the cargo-test entry `tests/dst/` all take their suites and
//! scenarios from `crate::scenarios`, so PR, nightly and server runs execute the same code.

use crate::dst::invariants::InvariantSet;
use crate::dst::run::{RunRecord, run_record};
use crate::dst::sut::Sut;
use crate::dst::{DstConfig, DstFailure, DstOutcome, FailKind, FaultRates};
use crate::seed;

/// Baseline of one suite.
#[derive(Copy, Clone, Debug)]
pub struct DstSuite {
    pub name: &'static str,
    pub title: &'static str,
    /// Fault rates (`hft-dst list` prints them).
    pub faults: FaultRates,
    pub steps: u64,
    /// The suite's world and actors for a seed (faults and steps are applied afterwards).
    pub base: fn(u64) -> DstConfig,
    /// Case ids that must pass in this suite (testing §3.4, §4).
    pub must_pass: &'static [&'static str],
}

impl DstSuite {
    /// The suite's configuration of `seed`.
    pub fn config(&self, seed: u64) -> DstConfig {
        let mut c = (self.base)(seed);
        c.seed = seed;
        c.suite = self.name;
        c.faults = self.faults;
        c.steps = self.steps;
        c
    }
}

/// The suite a seed runs in when none is given (README "套件选择"): `suites` sorted by name,
/// index `xxh3_64(seed LE) mod n`.
pub fn suite_for_seed(suites: &[DstSuite], seed: u64) -> Option<&DstSuite> {
    let mut sorted: Vec<&DstSuite> = suites.iter().collect();
    sorted.sort_by_key(|s| s.name);
    sorted.get(seed::suite_index(seed, sorted.len())).copied()
}

/// Which suite a scenario runs in.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SuiteSel {
    /// Every seed in the suite chosen from the seed (the invariants' "全部 DST 套件").
    All,
    Named(&'static str),
}

/// Seed set of a scenario (case field `种子`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SeedSet {
    /// PR 200 (commit-derived) + regressions; nightly date-derived (scenarios §2.4).
    Standard,
    /// PR `n` consecutive seeds.
    Pr(u32),
    /// Fixed seeds (`hashtrail.toml`, regressions).
    Fixed(&'static [u64]),
}

impl SeedSet {
    /// Seeds of a run starting at `start` (`cargo xtask dst --seed S --seeds N` semantics for the
    /// counted sets).
    pub fn seeds(&self, start: u64) -> Vec<u64> {
        match self {
            SeedSet::Standard => (0..200).map(|i| start.wrapping_add(i)).collect(),
            SeedSet::Pr(n) => (0..u64::from(*n)).map(|i| start.wrapping_add(i)).collect(),
            SeedSet::Fixed(s) => s.to_vec(),
        }
    }
    pub fn count(&self) -> usize {
        match self {
            SeedSet::Standard => 200,
            SeedSet::Pr(n) => *n as usize,
            SeedSet::Fixed(s) => s.len(),
        }
    }
}

/// Hook adding a case's `前置` and fault script to the suite baseline.
pub type ConfigureFn = fn(&mut DstConfig);
/// The case's own assertions over the finished run.
pub type AssertFn = fn(&RunRecord) -> Result<(), String>;

/// One registered `dst` case.
#[derive(Copy, Clone, Debug)]
pub struct Scenario {
    pub id: &'static str,
    pub title: &'static str,
    pub suite: SuiteSel,
    pub seeds: SeedSet,
    pub steps: Option<u64>,
    pub invariants: Option<InvariantSet>,
    pub configure: Option<ConfigureFn>,
    pub assert: Option<AssertFn>,
}

impl Scenario {
    /// Registers case `id` (test-trace locates the case at the function containing this call).
    pub fn new(id: &'static str, title: &'static str) -> Scenario {
        Scenario {
            id,
            title,
            suite: SuiteSel::All,
            seeds: SeedSet::Standard,
            steps: None,
            invariants: None,
            configure: None,
            assert: None,
        }
    }
    pub fn suite(mut self, name: &'static str) -> Scenario {
        self.suite = SuiteSel::Named(name);
        self
    }
    pub fn seeds(mut self, s: SeedSet) -> Scenario {
        self.seeds = s;
        self
    }
    pub fn steps(mut self, n: u64) -> Scenario {
        self.steps = Some(n);
        self
    }
    pub fn invariants(mut self, set: InvariantSet) -> Scenario {
        self.invariants = Some(set);
        self
    }
    pub fn configure(mut self, f: ConfigureFn) -> Scenario {
        self.configure = Some(f);
        self
    }
    pub fn assert(mut self, f: AssertFn) -> Scenario {
        self.assert = Some(f);
        self
    }

    /// The configuration of `seed`: suite baseline, then steps, invariants and `configure`.
    pub fn config(&self, suites: &[DstSuite], seed: u64) -> Option<DstConfig> {
        let suite = match self.suite {
            SuiteSel::All => suite_for_seed(suites, seed)?,
            SuiteSel::Named(n) => suites.iter().find(|s| s.name == n)?,
        };
        let mut c = suite.config(seed);
        if let Some(n) = self.steps {
            c.steps = n;
        }
        if let Some(i) = self.invariants {
            c.invariants = i;
        }
        if let Some(f) = self.configure {
            f(&mut c);
        }
        Some(c)
    }

    /// Runs one seed with engine `E`; the case's assertion turns a passing run into
    /// `Fail{invariant = <case id>}` when it does not hold.
    pub fn run_seed<E: Sut>(&self, suites: &[DstSuite], seed: u64) -> RunRecord {
        let Some(cfg) = self.config(suites, seed) else {
            let mut r = RunRecord::empty(seed);
            r.outcome = DstOutcome::Fail(Box::new(DstFailure {
                seed,
                suite: "?",
                step: 0,
                invariant: FailKind::Scenario(self.id),
                message: format!("{}: unknown suite {:?}", self.id, self.suite),
                trace: Vec::new(),
                fault_schedule: Default::default(),
                journal_tail: Vec::new(),
            }));
            return r;
        };
        let mut rec = run_record::<E>(&cfg);
        if rec.outcome.is_pass()
            && let Some(a) = self.assert
            && let Err(msg) = a(&rec)
        {
            let step = rec.steps_run.saturating_sub(1);
            rec.outcome = DstOutcome::Fail(Box::new(DstFailure {
                seed,
                suite: cfg.suite,
                step,
                invariant: FailKind::Scenario(self.id),
                message: msg,
                trace: rec.trace.clone(),
                fault_schedule: rec.plan.clone(),
                journal_tail: crate::dst::driver::last_events(&rec.journal, 200).to_vec(),
            }));
        }
        rec
    }

    /// Runs every seed of `seeds`; the first failure is returned.
    pub fn check<E: Sut>(&self, suites: &[DstSuite], seeds: &[u64]) -> Result<(), Box<DstFailure>> {
        for &s in seeds {
            if let DstOutcome::Fail(f) = self.run_seed::<E>(suites, s).outcome {
                return Err(f);
            }
        }
        Ok(())
    }
}
