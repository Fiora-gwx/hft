use hft_event::{EventKind, Note, RecordFlags, Role};
use hft_rt::Heartbeat;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::verify::{VerifyError, VerifyOptions, VerifyResult, verify};
use hft_strategy_registry::Strategies;
use serde_json::Value;
use zerocopy::IntoBytes;

use super::{args, fixture, run};

fn notes(log: &Log) -> Vec<usize> {
    log.recs
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            (r.kind == EventKind::NOTE
                && r.read::<Note>().is_some_and(|n| {
                    n.code == hft_sequencer::codes::seq_note(hft_sequencer::codes::note::RECOVER_MISMATCH)
                }))
            .then_some(i)
        })
        .collect()
}

fn check(log: Log, keep_going: bool) -> Result<VerifyResult, VerifyError> {
    static HB: Heartbeat = Heartbeat::new();
    verify::<Strategies>(
        log,
        VerifyOptions {
            keep_going,
            ..Default::default()
        },
        &HB,
    )
}

#[test]
fn rpl_024_recovery_range_accepted() {
    let f = fixture::restarted::<Strategies>(fixture::Restart::Mismatch);
    let directory = f.file.parent().unwrap();
    let second = directory.join("0000000002.jrn");
    let originals = [std::fs::read(&f.file).unwrap(), std::fs::read(&second).unwrap()];
    let log = fixture::read(&f);
    let ns = notes(&log);
    assert_eq!(ns.len(), 1, "real recovery did not record one scoped mismatch");
    let n = log.recs[ns[0]].read::<Note>().unwrap();
    let target = log.recs.iter().find(|r| r.seq == n.v[1]).unwrap();
    assert_eq!(target.kind, EventKind::ORDER_DECISION);
    let report = f.root.join("recovery.json");
    let mut a = args(directory);
    a.extend(["--report".into(), report.display().to_string()]);
    let (code, text, err) = run(&a);
    assert_eq!(code, 0, "{text} {err}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["summary"]["verify"], "pass");
    assert_eq!(v["summary"]["mismatches"], 0);
    assert!(v["summary"]["first_divergent_seq"].is_null());
    assert_eq!(v["recovery_diffs"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["recovery_diffs"][0]["seq"], target.seq);
    assert!(
        v["recovery_diffs"][0]["field_diff"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["field"] == "qty")
    );
    let runs = v["summary"]["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["recovery_diffs"], 1);
    assert_eq!(runs[1]["recovery_diffs"], 0);
    assert!(runs[1]["inputs_replayed"].as_u64().unwrap() > 0);
    assert!(runs[1]["outputs_compared"].as_u64().unwrap() > 0);
    assert!(runs[1]["snapshots_checked"].as_u64().unwrap() >= 3);
    let sidecar: Value =
        serde_json::from_slice(&std::fs::read(report.with_extension("divergence.json")).unwrap()).unwrap();
    assert_eq!(sidecar["recovery_diffs"], v["recovery_diffs"]);
    assert_eq!(sidecar["divergences"], v["divergences"]);
    assert_eq!(std::fs::read(&f.file).unwrap(), originals[0]);
    assert_eq!(std::fs::read(&second).unwrap(), originals[1]);
    // A window may exclude the old difference, but must retain its evidence for validation.
    for (flag, at, count) in [("--to-seq", target.seq, 1), ("--from-seq", target.seq + 1, 0)] {
        let mut a = args(directory);
        a.extend([flag.into(), at.to_string()]);
        let r = run(&a);
        assert_eq!(r.0, 0, "window {flag}: {r:?}");
        let v: Value = serde_json::from_str(&r.1).unwrap();
        assert_eq!(v["recovery_diffs"].as_array().unwrap().len(), count);
    }
    let suffix = run(&args(&second));
    assert_eq!(suffix.0, 3, "missing old evidence accepted: {suffix:?}");
    assert!(suffix.2.contains("RecoveryRange"));
    // A new-run error remains an ordinary divergence, even without --keep-going.
    let changed = super::flip_first_command(&second, &originals[1]);
    let r = run(&a);
    assert_eq!(r.0, 1, "new run escaped comparison: {r:?}");
    let v: Value = serde_json::from_str(&r.1).unwrap();
    assert!(v["summary"]["first_divergent_seq"].as_u64().unwrap() > changed);
    assert_eq!(v["recovery_diffs"].as_array().unwrap().len(), 1);
}

#[test]
fn rpl_264_invalid_recovery_evidence_fails_closed() {
    let f = fixture::restarted::<Strategies>(fixture::Restart::Mismatch);
    let log = fixture::read(&f);
    let ni = notes(&log)[0];
    let original = log.recs[ni].read::<Note>().unwrap();
    let target = log.recs.iter().position(|r| r.seq == original.v[1]).unwrap();
    let snapshot = log.recs[..target]
        .iter()
        .rposition(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap();
    let input = log.recs[..target]
        .iter()
        .rposition(|r| r.role() == Role::Input)
        .unwrap();
    let global = log.recs[target + 1..log.seg_start[1]]
        .iter()
        .position(|r| r.kind == EventKind::CMD_RESULT)
        .unwrap()
        + target
        + 1;
    for row in 0..12 {
        let mut bad = log.clone();
        let mut n = original;
        match row {
            0 => n.v[1] = log.recs[snapshot].seq,
            1 => n.v[1] = log.recs[input].seq,
            2 => n.v[1] = log.recs[ni].seq,
            3 => n.v[1] = log.recs[global].seq, // global needs all account/venue pairs
            4 => n.v[0] = (n.v[0] & 0xff) | (2 << 8),
            5 => n.v[0] |= 1 << 32,
            6 => n.v[2] = 1,
            7 => n.level = hft_event::NoteLevel::P1,
            8 => n._pad = 1,
            9 => bad.recs[ni].flags.0 &= !RecordFlags::EVENT_END.0,
            10 => bad.recs[ni].flags.0 |= RecordFlags::SHADOW.0,
            11 => n.v[1] = u64::MAX,
            _ => unreachable!(),
        }
        bad.recs[ni].body = n.as_bytes().to_vec();
        assert!(
            matches!(check(bad, true), Err(VerifyError::RecoveryRange { .. })),
            "evidence mutation {row}"
        );
    }
    // Global mismatches are recorded once per configured pair. Duplicate or absent pairs
    // cannot be used to waive a global Output; normal scope acceptance still works.
    let f = fixture::restarted::<Strategies>(fixture::Restart::GlobalMismatch);
    let log = fixture::read(&f);
    let ns = notes(&log);
    assert_eq!(ns.len(), 4);
    let result = check(log.clone(), false).unwrap();
    assert!(result.sink.mismatches.is_empty(), "{:?}", result.sink.mismatches);
    assert_eq!(result.sink.recovery_diffs.len(), 1);
    let mut duplicate = log.clone();
    duplicate.recs[ns[1]].body = duplicate.recs[ns[0]].body.clone();
    assert!(matches!(
        check(duplicate, true),
        Err(VerifyError::RecoveryRange { .. })
    ));
    let mut missing = log;
    missing.recs.remove(ns[0]); // a missing logical Note, while physical seqs stay unchanged
    assert!(matches!(
        check(missing, true),
        Err(VerifyError::RecoveryRange { .. })
    ));
}

#[test]
fn rpl_265_recovery_does_not_waive_other_scope_or_earlier_outputs() {
    let f = fixture::restarted::<Strategies>(fixture::Restart::Mismatch);
    let log = fixture::read(&f);
    let ni = notes(&log)[0];
    let anchor = log.recs[ni].read::<Note>().unwrap().v[1];
    let old_end = log.seg_start[1];
    for earlier in [true, false] {
        let mut bad = log.clone();
        let ix = bad.recs[..old_end]
            .iter()
            .position(|r| r.kind == EventKind::CMD_RESULT && (r.seq < anchor) == earlier)
            .unwrap();
        let seq = bad.recs[ix].seq;
        bad.recs[ix].body[0] ^= 1;
        for keep in [true, false] {
            let result = check(bad.clone(), keep).unwrap();
            assert_eq!(result.sink.mismatches[0].seq, seq);
            assert!(!result.sink.recovery_diffs.iter().any(|d| d.mismatch.seq == seq));
            if keep {
                assert_eq!(result.runs.len(), 2);
            }
        }
    }
    // With no Note at all the old error remains a regular mismatch, never a success.
    let mut missing = log;
    missing.recs.remove(ni);
    let result = check(missing, false).unwrap();
    assert_eq!(result.sink.mismatches[0].seq, anchor);
    assert!(result.sink.recovery_diffs.is_empty());
}
