//! SEQ-344 exercises the complete engine through the shared fault suites.
use crate::dst::{DstConfig, InvariantSet, RerunPolicy, RunRecord, Scenario, SeedSet};

fn configure(c: &mut DstConfig) {
    c.rerun = RerunPolicy::Always;
    c.rebuild_every = 500;
    c.world.physical_journal = true;
}

fn evidence(r: &RunRecord) -> Result<(), String> {
    if r.snapshot_checks == 0 || r.journal_verification.is_none() {
        return Err("full DST did not verify snapshots and the complete journal".into());
    }
    Ok(())
}

pub fn seq_344_full_dst() -> Scenario {
    Scenario::new(
        "SEQ-344",
        "all components, suite faults, snapshots and deterministic rerun",
    )
    .seeds(SeedSet::Standard)
    .invariants(InvariantSet::all())
    .configure(configure)
    .assert(evidence)
}
