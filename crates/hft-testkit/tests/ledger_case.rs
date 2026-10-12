//! The acceptance case must reject incomplete execution and missing or wrong observations.
use hft_event::{CmdHdr, CmdResult, EventKind, Note, NoteSource};
use hft_testkit::dst::{DstOutcome, InvariantSet, RerunPolicy, Source, run_record};
use hft_testkit::scenarios::{DstEngine, ledger, suites};
use zerocopy::FromBytes;

#[test]
fn tk_142_confirmed_case_executes_and_rejects_missing_evidence() {
    let case = ledger::led_081_auto_external_cold_hot_equal();
    let suites = suites();
    for seed in 0..4 {
        let mut cfg = case.config(&suites, seed).unwrap();
        cfg.rerun = RerunPolicy::Never;
        let r = case.run_config::<DstEngine>(&cfg);
        assert!(r.outcome.is_pass(), "seed {seed}: {:?}", r.outcome);
        let commands: Vec<_> = r
            .journal
            .iter()
            .filter(|j| j.kind == EventKind::CONTROL_CMD)
            .filter_map(|j| CmdHdr::read_from_prefix(&j.body).ok().map(|(h, _)| h))
            .filter(|h| h.issuer.starts_with(b"ledger-case"))
            .collect();
        assert_eq!(commands.len(), 4);
        for h in commands {
            assert_eq!(h.confirmed, 1);
            assert!(
                r.journal
                    .iter()
                    .filter_map(|j| j.read::<CmdResult>())
                    .any(|c| c.cmd_id == h.cmd_id && c.status == 0),
                "confirmed command must apply"
            );
        }
        let mut bad = r.clone();
        bad.ledger_checks.clear();
        assert!(ledger::assert_record(&bad).is_err(), "missing ledger checks");
        let mut bad = r.clone();
        bad.journal.retain(|j| j.kind != EventKind::EXEC_DECISION);
        assert!(ledger::assert_record(&bad).is_err(), "missing paired execution");
        let mut bad = r.clone();
        bad.truth[0].undelivered = true;
        assert!(ledger::assert_record(&bad).is_err(), "skipped final truth");
        let mut bad = r.clone();
        bad.obs.balances[0].qty += 1;
        assert!(ledger::assert_record(&bad).is_err(), "one-unit wallet mismatch");
        let mut bad = r.clone();
        bad.obs.required = None;
        assert!(
            ledger::assert_record(&bad).is_err(),
            "missing observation requirements"
        );
        if seed & 1 == 0 {
            let mut bad = r.clone();
            bad.journal.retain(|j| {
                !j.try_read::<Note>().is_some_and(|n| {
                    n.source == NoteSource::Ledger && n.code == hft_ledger::notes::code::AUTO_HELD_OFFSET
                })
            });
            assert!(ledger::assert_record(&bad).is_err(), "missing offset");
        }
        cfg.invariants = InvariantSet::none();
        let off = run_record::<DstEngine>(&cfg);
        assert!(off.outcome.is_pass(), "{:?}", off.outcome);
        assert_eq!(r.journal, off.journal);
        assert_eq!(r.frames, off.frames);
        assert_eq!(r.deliveries, off.deliveries);
        assert_eq!(r.trail.to_bytes(), off.trail.to_bytes());
        assert_eq!(r.truth, off.truth);
    }
    let mut truncated = case.config(&suites, 0).unwrap();
    truncated.steps = 1;
    assert!(!case.run_config::<DstEngine>(&truncated).outcome.is_pass());
    for invalid in [false, true] {
        let mut cfg = case.config(&suites, 0).unwrap();
        if invalid {
            cfg.confirmed_commands[0].1.src = Source::Market;
        } else {
            cfg.actors.ctl = false;
        }
        let failed = run_record::<DstEngine>(&cfg);
        assert!(matches!(failed.outcome, DstOutcome::Fail(_)));
        assert!(
            failed
                .outcome
                .failure()
                .unwrap()
                .message
                .contains("confirmed command")
        );
    }
}
