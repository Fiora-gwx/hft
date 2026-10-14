//! TK-173: preserve and reproduce the old no-op golden before accepting a new fault trace.
use hft_testkit::dst::faults::Fault;
use hft_testkit::dst::{InvariantSet, RerunPolicy, RunRecord, run_record};
use hft_testkit::scenarios::{DstEngine, suites};
use std::fmt::Write;
use zerocopy::IntoBytes;

fn value<'a>(row: &'a str, name: &str) -> &'a str {
    row.lines()
        .find_map(|l| {
            l.split_once(" = ")
                .filter(|(k, _)| *k == name)
                .map(|(_, v)| v.trim_matches('"'))
        })
        .unwrap()
}

fn digest(r: &RunRecord) -> String {
    format!("{:032x}", xxhash_rust::xxh3::xxh3_128(&r.trail.to_bytes()))
}

#[test]
fn tk_173_cancel_migration_reproduces_prior_goldens_and_preserves_prefix() {
    let old = include_str!("../../../fixtures/dst/cancel-before.toml");
    let suites = suites();
    let mut rows = Vec::new();
    let mut changed = 0;
    for row in old.split("[[seed]]").skip(1) {
        let seed = u64::from_str_radix(value(row, "seed").trim_start_matches("0x"), 16).unwrap();
        let suite = suites.iter().find(|s| s.name == value(row, "suite")).unwrap();
        let steps = value(row, "steps").parse::<u64>().unwrap();
        let expected = format!("{:032x}", u128::from_str_radix(value(row, "trail"), 16).unwrap());
        let mut cfg = suite.config(seed);
        cfg.steps = steps;
        cfg.rerun = RerunPolicy::Never;
        let old_cfg = crate::cancel_filled::without_cancel(&cfg);
        let before = run_record::<DstEngine>(&old_cfg);
        let after = run_record::<DstEngine>(&cfg);
        for r in [&before, &after] {
            assert!(
                r.outcome.is_pass(),
                "seed {seed}: {:?}",
                r.outcome.failure().map(|f| (&f.invariant, f.message.as_str()))
            );
            assert_eq!(r.steps_run, steps);
        }
        assert_eq!(
            digest(&before),
            expected,
            "seed {seed}: former no-op no longer reproduces the reviewed golden"
        );
        let first = before
            .journal
            .iter()
            .zip(&after.journal)
            .position(|(a, b)| a != b)
            .or_else(|| {
                (before.journal.len() != after.journal.len())
                    .then_some(before.journal.len().min(after.journal.len()))
            });
        let mut first_seq = 0;
        let mut first_tsc = 0;
        if let Some(ix) = first {
            changed += 1;
            let start = cfg
                .fault_plan()
                .items
                .iter()
                .filter_map(|(t, f)| {
                    matches!(f, Fault::CancelRejectFilled { count, .. } if *count > 0).then_some(*t)
                })
                .min()
                .expect("trace changes without a cancel-filled fault");
            for j in [before.journal.get(ix), after.journal.get(ix)]
                .into_iter()
                .flatten()
            {
                assert!(
                    j.tsc >= start,
                    "seed {seed}: trace changes at {} before fault {start}",
                    j.tsc
                );
                first_seq = j.seq;
                first_tsc = j.tsc;
            }
            assert!(before.journal[..ix] == after.journal[..ix]);
        } else {
            assert_eq!(digest(&before), digest(&after));
        }
        cfg.invariants = InvariantSet::none();
        let off = run_record::<DstEngine>(&cfg);
        assert!(off.outcome.is_pass(), "seed {seed}: checker-off run failed");
        assert_eq!(off.steps_run, steps);
        assert_eq!(after.journal.len(), off.journal.len());
        for (ix, (a, b)) in after.journal.iter().zip(&off.journal).enumerate() {
            assert!(a == b, "seed {seed}: checker changes record {ix}");
        }
        assert_eq!(after.frames, off.frames);
        assert_eq!(after.deliveries, off.deliveries);
        assert_eq!(after.trail.to_bytes(), off.trail.to_bytes());
        assert_eq!(after.truth, off.truth);
        let cases: Vec<_> = after.cancel_filled.iter().flat_map(|v| &v.cases).collect();
        let held = cases.len();
        let rejects = cases
            .iter()
            .filter(|x| {
                x.rejection.is_some_and(|e| {
                    after
                        .journal
                        .iter()
                        .any(|j| j.kind == hft_event::EventKind::ORDER_EVENT && j.body == e.as_bytes())
                })
            })
            .count();
        let delivered = cases
            .iter()
            .filter(|x| {
                x.report.is_some_and(|e| {
                    after
                        .journal
                        .iter()
                        .any(|j| j.kind == hft_event::EventKind::ORDER_EVENT && j.body == e.as_bytes())
                })
            })
            .count();
        if first.is_some() {
            assert!(held > 0, "trace changed without an actual held fill");
        }
        let mut line = String::new();
        write!(&mut line,
            "{{\"seed\":\"{seed:#018x}\",\"suite\":\"{}\",\"steps\":{steps},\"before\":\"{}\",\"after\":\"{}\",\"first_seq\":{first_seq},\"first_tsc\":{first_tsc},\"cancel_held\":{held},\"cancel_rejects\":{rejects},\"delivered_fills\":{delivered},\"before_records\":{},\"after_records\":{},\"prefix_equal\":true,\"checker_equal\":true}}",
            suite.name, digest(&before), digest(&after), before.journal.len(), after.journal.len()).unwrap();
        rows.push(line);
    }
    assert_eq!(rows.len(), 50);
    assert!(changed > 0, "default faults did not change any trace");
    let path = hft_testkit::tmp::per_process_file(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/dst"),
        "cancel-filled-migration",
        "json",
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let run_id = std::env::var("GITHUB_RUN_ID")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    std::fs::write(path, format!(
        "{{\"baseline\":\"02318351620fb71f438faa9cc7fe01c37e6e6af4\",\"run_id\":{run_id},\"changed\":{changed},\"seeds\":[{}]}}\n", rows.join(","))).unwrap();
}
