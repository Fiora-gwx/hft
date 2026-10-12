//! Audit-only attribution of HashTrail changes to the appended business Output.
use hft_event::{Note, NoteSource};
use hft_testkit::dst::invariants::InvariantSet;
use hft_testkit::dst::{RerunPolicy, run_record};
use hft_testkit::scenarios::{DstEngine, suites};

fn main() {
    let suite = suites().into_iter().find(|s| s.name == "core").unwrap();
    for seed in 1..=50 {
        let mut cfg = suite.config(seed);
        cfg.steps = 2_000;
        cfg.rerun = RerunPolicy::Never;
        // The normal candidate was generated separately with every checker enabled. This
        // experiment also verifies that turning them off preserves that complete digest.
        cfg.invariants = InvariantSet::none();
        let rec = run_record::<DstEngine>(&cfg);
        assert!(rec.outcome.is_pass(), "seed {seed}: {:?}", rec.outcome);
        assert_eq!(rec.steps_run, cfg.steps);
        let notes = rec.journal.iter().filter_map(|r| r.try_read::<Note>()).filter(|n| {
            n.source == NoteSource::Ledger && n.code == hft_ledger::notes::code::BALANCE_CHECK
        }).count();
        println!("{seed}\t{:032x}\t{}\t{notes}",
            xxhash_rust::xxh3::xxh3_128(&rec.trail.to_bytes()), rec.trail.points.len());
    }
}
