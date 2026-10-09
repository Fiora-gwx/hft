//! Framework scenarios (crates/hft-testkit/TESTS.md TK-043, TK-054, TK-055, TK-056): determinism,
//! cross-architecture conformance, crash and rebuild, poison repeat. They run on the engine the
//! caller instantiates (`ToyEngine` until the sequencer adapter exists).

use crate::dst::faults::{CrashFault, Fault};
use crate::dst::invariants::{InvId, InvariantSet};
use crate::dst::run::RunRecord;
use crate::dst::scenario::{Scenario, SeedSet};
use crate::dst::sut::CrashPoint;
use crate::dst::{DstConfig, RerunPolicy, ToyEngine};

/// Every framework scenario.
pub fn all() -> Vec<Scenario> {
    vec![
        tk_043_same_seed_same_result(),
        tk_054_cross_arch_conformance(),
        tk_055_crash_and_recover(),
        tk_056_poison_repeat(),
    ]
}

fn rerun_always(c: &mut DstConfig) {
    c.rerun = RerunPolicy::Always;
}

/// TK-043: two runs of one seed give the same outcome, hash trail and intent stream (the run's
/// INV-9 rerun is forced for every seed).
pub fn tk_043_same_seed_same_result() -> Scenario {
    Scenario::new("TK-043", "same seed, same result and hash trail")
        .suite("core")
        .seeds(SeedSet::Standard)
        .invariants(InvariantSet::all())
        .configure(rerun_always)
}

/// TK-054: the conformance seeds of `fixtures/dst/hashtrail.toml` reproduce their golden
/// `HashTrail` (`cargo xtask dst --hashtrail check` on x86_64 and aarch64).
pub fn tk_054_cross_arch_conformance() -> Scenario {
    Scenario::new(
        "TK-054",
        "conformance seeds match the HashTrail golden on every arch",
    )
    .suite("core")
    .seeds(SeedSet::Fixed(&[1, 2, 3]))
    .invariants(InvariantSet::only(InvId::I9))
    .configure(rerun_always)
}

fn crash_cfg(c: &mut DstConfig) {
    let at = 1_000 + crate::seed::derive(c.seed, "tk-055") % 4_001;
    c.scripted.push((
        0,
        Fault::Crash(CrashFault {
            at_step: at.min(c.steps.saturating_sub(1)),
            repeat: 1,
            point: CrashPoint::SeqInput,
        }),
    ));
    c.rebuild_every = 500;
}

fn crash_assert(r: &RunRecord) -> Result<(), String> {
    if r.steps_run == 0 {
        return Err("no steps ran".into());
    }
    Ok(())
}

/// TK-055: a crash at a seed-chosen step in [1000, 5000] is rebuilt from the journal; the rebuilt
/// state equals a replay from the snapshot (INV-6) and the run continues to the step limit.
pub fn tk_055_crash_and_recover() -> Scenario {
    Scenario::new("TK-055", "CrashFault rebuilds from the journal")
        .suite("storage")
        .seeds(SeedSet::Pr(200))
        .invariants(InvariantSet::all())
        .configure(crash_cfg)
        .assert(crash_assert)
}

fn poison_cfg(c: &mut DstConfig) {
    let at = 500 + crate::seed::derive(c.seed, "tk-056") % 1_000;
    c.scripted.push((
        0,
        Fault::Crash(CrashFault {
            at_step: at.min(c.steps.saturating_sub(1)),
            repeat: 2,
            point: CrashPoint::SeqInput,
        }),
    ));
}

/// TK-056: `repeat = 2` crashes again on the same Input after the first rebuild (the third
/// instance's `TailSkipped{Poison}` is asserted by the sequencer adapter's recovery).
pub fn tk_056_poison_repeat() -> Scenario {
    Scenario::new("TK-056", "CrashFault repeat = 2 crashes the same Input again")
        .suite("storage")
        .seeds(SeedSet::Pr(50))
        .invariants(InvariantSet::only(InvId::I6))
        .configure(poison_cfg)
        .assert(crash_assert)
}

/// Runs one scenario for `n` consecutive seeds from `start` on the toy engine (cargo-test smoke).
pub fn smoke(s: &Scenario, start: u64, n: u64, steps: u64) -> Result<(), String> {
    let suites = super::suites();
    for seed in start..start + n {
        let mut sc = *s;
        sc.steps = Some(steps.min(sc.steps.unwrap_or(u64::MAX)));
        let r = sc.run_seed::<ToyEngine>(&suites, seed);
        if let Some(f) = r.outcome.failure() {
            return Err(format!("{}: {}", crate::dst::report::repro_line(f), f.message));
        }
    }
    Ok(())
}
