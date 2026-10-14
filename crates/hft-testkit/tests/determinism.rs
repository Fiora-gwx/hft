//! TK-181: the same seed run twice in one process gives the same event digest, journal, intents,
//! deliveries and state-hash trail. Nothing is compared with a committed digest: determinism is a
//! property of two runs of the same build, not of a golden file.
use hft_testkit::dst::{InvId, RerunPolicy, RunRecord, run_record};
use hft_testkit::scenarios::{DstEngine, suites};

const STEPS: u64 = 2_000;

fn once(suite: &str, seed: u64, rerun: RerunPolicy) -> RunRecord {
    let all = suites();
    let s = all.iter().find(|s| s.name == suite).expect("suite");
    let mut cfg = s.config(seed);
    cfg.steps = STEPS;
    cfg.rerun = rerun;
    run_record::<DstEngine>(&cfg)
}

#[test]
fn tk_181_same_seed_twice_in_process_same_event_digest() {
    let mut digests = Vec::new();
    for (suite, seed) in [("core", 1u64), ("core", 2), ("storage", 3)] {
        let a = once(suite, seed, RerunPolicy::Never);
        let b = once(suite, seed, RerunPolicy::Never);
        for r in [&a, &b] {
            assert!(
                r.outcome.is_pass(),
                "{suite} seed {seed}: {:?}",
                r.outcome.failure()
            );
            assert_eq!(r.steps_run, STEPS, "{suite} seed {seed}: ended early");
        }
        // Non-vacuity: the digest covers a non-trivial journal with intents and deliveries.
        assert!(
            a.journal.len() > 100,
            "{suite} seed {seed}: only {} records",
            a.journal.len()
        );
        assert!(!a.outputs().is_empty(), "{suite} seed {seed}: no intents");
        assert!(!a.deliveries.is_empty(), "{suite} seed {seed}: no deliveries");
        assert_eq!(
            a.event_digest(),
            b.event_digest(),
            "{suite} seed {seed}: event digest differs"
        );
        assert!(a.journal == b.journal, "{suite} seed {seed}: journal differs");
        assert!(a.frames == b.frames, "{suite} seed {seed}: intents differ");
        assert!(
            a.deliveries == b.deliveries,
            "{suite} seed {seed}: deliveries differ"
        );
        assert!(
            a.trail == b.trail,
            "{suite} seed {seed}: state-hash trail differs"
        );
        assert!(a.truth == b.truth, "{suite} seed {seed}: exchange truth differs");
        digests.push(a.event_digest());
    }
    // The digest separates runs: different seeds give different digests.
    let mut unique = digests.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), digests.len(), "event digests collide: {digests:x?}");

    // The run loop's own INV-9 rerun compares the same digest and counts what it compared.
    let forced = once("core", 1, RerunPolicy::Always);
    assert!(forced.outcome.is_pass(), "{:?}", forced.outcome.failure());
    assert!(forced.compared_of(InvId::I9) as usize >= forced.journal.len());
}
