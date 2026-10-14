//! Audit the capability change over the unchanged conformance seed set.
use hft_event::EventKind;
use hft_testkit::dst::world::VP;
use hft_testkit::dst::{RerunPolicy, run_record};
use hft_testkit::scenarios::{DstEngine, suites};

fn main() {
    let suite = suites().into_iter().find(|suite| suite.name == "core").unwrap();
    for seed in 1..=50 {
        let mut cfg = suite.config(seed);
        cfg.steps = 2_000;
        cfg.rerun = RerunPolicy::Always;
        let legacy = run_record::<DstEngine>(&cfg);
        assert!(legacy.outcome.is_pass(), "legacy {seed}: {:?}", legacy.outcome);
        cfg.world
            .venues
            .iter_mut()
            .find(|venue| venue.id == VP)
            .unwrap()
            .perp_cash = true;
        let strict = run_record::<DstEngine>(&cfg);
        if let Some(failure) = strict.outcome.failure() {
            let artifacts = hft_testkit::dst::report::write_artifacts(
                std::path::Path::new("audit/conformance-failures"),
                failure,
            )
            .unwrap();
            std::fs::write(
                artifacts.dir.join("full-journal.bin"),
                hft_testkit::dst::driver::encode_records(&strict.journal),
            )
            .unwrap();
            std::fs::write(
                artifacts.dir.join("detail.txt"),
                format!(
                    "{}\n{:#?}\n{:#?}",
                    failure.message,
                    strict.matched_tail,
                    strict.deliveries.iter().rev().take(24).collect::<Vec<_>>()
                ),
            )
            .unwrap();
            panic!("strict {seed}, step {}: {}", failure.step, failure.message);
        }
        assert!(legacy.snapshot_checks > 0 && strict.snapshot_checks > 0);
        assert_eq!(legacy.frames, strict.frames, "gateway frames {seed}");
        assert_eq!(
            legacy.matched_tail, strict.matched_tail,
            "original matching facts {seed}"
        );
        assert_eq!(legacy.truth, strict.truth, "exchange truth {seed}");
        let business = |kind: EventKind| {
            matches!(
                kind,
                EventKind::ORDER_EVENT | EventKind::ORDER_DECISION | EventKind::EXTERNAL_FILL
            )
        };
        assert_eq!(
            legacy
                .journal
                .iter()
                .filter(|j| business(j.kind))
                .collect::<Vec<_>>(),
            strict
                .journal
                .iter()
                .filter(|j| business(j.kind))
                .collect::<Vec<_>>(),
            "cash inputs and order decisions {seed}"
        );
        assert_eq!(legacy.trail.points.len(), strict.trail.points.len());
        let mut ledger_diffs = 0;
        let mut oms_diffs = 0;
        for ((a_seq, a), (b_seq, b)) in legacy.trail.points.iter().zip(&strict.trail.points) {
            assert_eq!(a_seq, b_seq);
            assert_eq!(
                (a.book, a.risk, a.exec, a.strategies),
                (b.book, b.risk, b.exec, b.strategies),
                "other components {seed}/{a_seq}"
            );
            ledger_diffs += usize::from(a.ledger != b.ledger);
            oms_diffs += usize::from(a.oms != b.oms);
        }
        assert_eq!(legacy.obs.positions, strict.obs.positions);
        assert_eq!(legacy.obs.inventory, strict.obs.inventory);
        assert_eq!(legacy.obs.balances, strict.obs.balances);
        println!(
            "{seed}\t{:032x}\t{:032x}\t{}\t{}\t{ledger_diffs}\t{oms_diffs}",
            xxhash_rust::xxh3::xxh3_128(&legacy.trail.to_bytes()),
            xxhash_rust::xxh3::xxh3_128(&strict.trail.to_bytes()),
            strict.trail.points.len(),
            strict.snapshot_checks
        );
    }
}
