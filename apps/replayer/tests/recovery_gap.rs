//! Flagged power-loss boundaries in immutable files; no synthesized replay outputs.
use std::path::{Path, PathBuf};

use hft_event::{AdviceHdr, EventKind, SegmentRoll};
use hft_journal::JournalReader;
use hft_rt::Heartbeat;
use hft_sequencer::advisor::checkpoint;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::replay_io::LogReadLimits;
use hft_sequencer::driver::verify::{VerifyError, VerifyOptions, verify};
use hft_strategy_registry::Strategies;
use serde_json::{Value, json};
use zerocopy::IntoBytes;

use super::fixture::{self, ModelRestart};
use super::long_model::{execute, execute_input, options, passed};

fn files(f: &fixture::Fixture) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(f.file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jrn"))
        .collect();
    paths.sort();
    paths
}
fn read(f: &fixture::Fixture) -> Log {
    let mut r = JournalReader::open_files_recoverable(&files(f), 16 << 20).unwrap();
    Log::from_reader(
        &mut r,
        LogReadLimits {
            records: 200_000,
            body_bytes: 256 << 20,
        },
    )
    .unwrap()
}
fn plain(f: &fixture::Fixture, path: &Path, label: &str, extra: &[String]) -> (i32, Value, String) {
    let mut a = super::args(path);
    a.extend_from_slice(extra);
    a.extend([
        "--report".into(),
        f.root.join(format!("{label}.json")).display().to_string(),
    ]);
    let r = super::run(&a);
    if let Some(root) = std::env::var_os("HFT_REPLAY_EVIDENCE_DIR") {
        let dest = PathBuf::from(root).join(f.root.file_name().unwrap());
        std::fs::write(dest.join(format!("gap-{label}.json")), &r.1).unwrap();
        std::fs::write(dest.join(format!("gap-{label}.stderr.txt")), &r.2).unwrap();
    }
    (r.0, serde_json::from_str(&r.1).unwrap_or(Value::Null), r.2)
}
fn label(f: &fixture::Fixture, name: &str, mut a: Vec<String>) -> Vec<String> {
    a.extend([
        "--report".into(),
        f.root.join(format!("{name}.json")).display().to_string(),
    ]);
    a
}

#[test]
fn rpl_275_recovery_gap_files_restore_each_run_and_report_unknown_tail() {
    for skip in [0, 4] {
        let f = fixture::recovery_gap(skip);
        let paths = files(&f);
        let originals: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
        let log = read(&f);
        assert_eq!(log.recovery_gaps.len(), 1);
        let gap = log.recovery_gaps[0];
        assert_eq!(gap.first, log.seg_start[1]);
        assert_eq!(gap.first_seq - gap.prev_last_seq - 1, skip);
        let clock = log.recs[gap.first..]
            .iter()
            .find(|r| r.kind == EventKind::ENGINE_CLOCK)
            .unwrap()
            .seq;
        for (name, input, extra, included, runs) in [
            ("full", f.file.parent().unwrap(), vec![], true, 2),
            (
                "window",
                f.file.parent().unwrap(),
                vec!["--from-seq".into(), clock.to_string()],
                true,
                1,
            ),
            ("suffix", paths[1].as_path(), vec![], false, 1),
        ] {
            let r = plain(&f, input, name, &extra);
            assert_eq!(r.0, 0, "{name}: {r:?}");
            let s = &r.1["summary"];
            assert_eq!(s["verify"], "pass");
            assert_eq!(s["input_continuity"], "recovery_gap");
            assert_eq!(s["runs"].as_array().unwrap().len(), runs);
            assert!(s["outputs_compared"].as_u64().unwrap() > 0);
            assert!(s["snapshots_checked"].as_u64().unwrap() > 0);
            assert_eq!(
                s["recovery_gaps"],
                json!([{
                    "segment_no": 2, "first_seq": gap.first_seq, "prev_last_seq": gap.prev_last_seq,
                    "previous_segment_included": included, "unavailable_seq_count": skip, "tail_loss_unknown": true
                }])
            );
        }
        for (p, original) in paths.iter().zip(&originals) {
            assert_eq!(std::fs::read(p).unwrap(), *original);
        }
        let (changed, seq) = super::model::mutate(&originals[1], EventKind::CMD_RESULT, None, 0);
        std::fs::write(&paths[1], changed).unwrap();
        fixture::seal(&paths[1]);
        let r = plain(&f, f.file.parent().unwrap(), "changed-output", &[]);
        assert_eq!(r.0, 1, "{r:?}");
        assert_eq!(r.1["summary"]["first_divergent_seq"], seq);
        assert!(r.1["recovery_diffs"].as_array().unwrap().is_empty());
    }
}

#[test]
fn rpl_276_recovery_gap_metadata_cannot_hide_an_invalid_run() {
    static HB: Heartbeat = Heartbeat::new();
    let f = fixture::recovery_gap(4);
    let log = read(&f);
    for row in 0..7 {
        let mut bad = log.clone();
        match row {
            0 => bad.recovery_gaps[0].first += 1,
            1 => bad.recovery_gaps[0].first_seq += 1,
            2 => bad.recovery_gaps[0].prev_last_seq -= 1,
            3 => bad.recovery_gaps[0].segment_no += 1,
            4 => bad.recovery_gaps.push(bad.recovery_gaps[0]),
            5 => bad.recovery_gaps.clear(),
            _ => {
                let r = &mut bad.recs[log.seg_start[1]];
                let mut roll = r.read::<SegmentRoll>().unwrap();
                roll.reason = hft_journal::RollReason::Hourly as u8;
                r.body = roll.as_bytes().to_vec();
            }
        }
        assert!(
            matches!(
                verify::<Strategies>(bad, VerifyOptions::default(), &HB),
                Err(VerifyError::RunBoundary { .. })
            ),
            "row {row}"
        );
    }
}

#[test]
fn rpl_277_model_origins_and_history_stop_at_each_marked_gap() {
    let f = fixture::model_restart(ModelRestart {
        count: 2,
        settle_ms: 60_000,
        reason: 0,
        recovery_gap: true,
    });
    let log = read(&f);
    assert_eq!(log.recovery_gaps.len(), 2);
    assert!(
        log.recovery_gaps
            .iter()
            .all(|g| g.first_seq == g.prev_last_seq + 1)
    );
    let first = log.recovery_gaps[0];
    let last = log.recovery_gaps[1];
    let cp = f.root.join("checkpoints/tsa.ckpt");
    for (name, a, count) in [
        ("gap-no-cp", vec![], 66),
        ("gap-old-cp", options(&cp, None), 6),
        (
            "gap-window",
            vec!["--from-seq".into(), first.first_seq.to_string()],
            0,
        ),
        ("gap-window-old-cp", options(&cp, Some(first.first_seq)), 0),
    ] {
        let r = execute(&f, false, &label(&f, name, a));
        assert_eq!(r.0, 0, "{name}: {r:?}");
        assert_eq!(r.1["summary"]["verify"], "pass");
        assert_eq!(r.1["summary"]["recompute"], "skipped");
        assert_eq!(r.1["model_no_start"], json!(["tsa"]));
        assert_eq!(r.1["summary"]["model_advice_compared"], count);
        assert_eq!(r.1["summary"]["recovery_gaps"].as_array().unwrap().len(), 2);
    }
    // An adjacent previous End still belongs to the uncertain side of the flag.
    let bytes = std::fs::read(&cp).unwrap();
    let decoded = checkpoint::decode(&bytes, 1 << 20).unwrap();
    let mut header = decoded.header(0);
    header.last_seq = last.prev_last_seq;
    let adjacent = f.root.join("adjacent.ckpt");
    std::fs::write(
        &adjacent,
        checkpoint::encode(&header, decoded.body, 1 << 20).unwrap(),
    )
    .unwrap();
    let paths = files(&f);
    let r = execute_input(
        &f,
        paths.last().unwrap(),
        false,
        &label(&f, "gap-adjacent-cp", options(&adjacent, None)),
    );
    assert_eq!(r.0, 0, "{r:?}");
    assert_eq!(r.1["model_no_start"], json!(["tsa"]));
    assert_eq!(r.1["summary"]["model_advice_compared"], 0);
    assert!(r.1["summary"]["checkpoint_key"].as_array().unwrap().is_empty());
    // A real checkpoint saved after the last gap resumes only the following 30 samples.
    let after = f.root.join("after-gap-1.ckpt");
    let bytes = std::fs::read(&after).unwrap();
    let d = checkpoint::decode(&bytes, 1 << 20).unwrap();
    assert!(d.key.last_seq > last.first_seq);
    for (name, path) in [
        ("gap-after-cp-full", f.file.parent().unwrap()),
        ("gap-after-cp-suffix", paths.last().unwrap().as_path()),
    ] {
        let r = execute_input(
            &f,
            path,
            false,
            &label(&f, name, options(&after, Some(d.key.last_seq + 1))),
        );
        passed(&r);
        assert_eq!(r.1["summary"]["model_advice_compared"], 30);
    }
}

#[test]
fn rpl_278_discarded_checkpoint_after_gap_starts_a_new_model_comparison() {
    let f = fixture::model_restart(ModelRestart {
        count: 1,
        settle_ms: 60_000,
        reason: 3,
        recovery_gap: true,
    });
    let log = read(&f);
    let gap = log.recovery_gaps[0];
    let r = execute(&f, false, &label(&f, "gap-discarded", vec![]));
    passed(&r);
    assert_eq!(r.1["summary"]["model_advice_compared"], 126);
    let paths = files(&f);
    let original = std::fs::read(&paths[1]).unwrap();
    let advice = log.recs[gap.first..]
        .iter()
        .find(|r| r.kind == EventKind::ADVICE)
        .unwrap();
    let (changed, seq) = super::model::mutate(
        &original,
        EventKind::ADVICE,
        Some(advice.seq),
        size_of::<AdviceHdr>(),
    );
    std::fs::write(&paths[1], changed).unwrap();
    fixture::seal(&paths[1]);
    let r = execute(&f, false, &label(&f, "gap-changed-advice", vec![]));
    assert_eq!(r.0, 1, "{r:?}");
    assert_eq!(r.1["summary"]["verify"], "pass");
    assert_eq!(r.1["summary"]["recompute"], "fail");
    assert_eq!(r.1["summary"]["first_model_divergent_seq"], seq);
}
