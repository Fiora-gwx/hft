use hft_event::{AdviceHdr, EngineClock, EventKind, Note, NoteSource, SegmentRoll, SnapshotHdr};
use hft_journal::{RollMark, RollReason};
use hft_sequencer::advisor::checkpoint;
use hft_sequencer::driver::replay::Log;

use crate::fixture::{self, ModelRestart};
use crate::long_model::{execute, options, passed};

fn is_reset(r: &hft_sequencer::journal::MemRecord) -> Option<Note> {
    (r.kind == EventKind::NOTE)
        .then(|| r.read::<Note>())
        .flatten()
        .filter(|n| {
            n.code
                == hft_event::note_code(
                    NoteSource::Advisor,
                    hft_sequencer::codes::advisor_note::MODEL_PATH_RESET,
                )
        })
}

// Independent integer clock calculation, including boot prefixes before the new EngineClock.
fn timeline(log: &Log) -> Vec<i128> {
    let mut anchor = None;
    let mut stopped = 0i128;
    log.recs
        .iter()
        .enumerate()
        .map(|(ix, r)| {
            if r.kind == EventKind::SEGMENT_ROLL
                && r.read::<SegmentRoll>().is_some_and(|b| {
                    b.mark == RollMark::Begin as u8
                        && matches!(
                            RollReason::from_u8(b.reason),
                            Some(
                                RollReason::Startup | RollReason::Recovery | RollReason::RecoverFromSnapshot
                            )
                        )
                })
            {
                stopped = i128::from(
                    log.recs[ix..]
                        .iter()
                        .find_map(|r| {
                            (r.kind == EventKind::STATE_SNAPSHOT)
                                .then(|| r.read::<SnapshotHdr>())
                                .flatten()
                        })
                        .unwrap()
                        .engine_ns,
                );
                anchor = None;
            }
            if r.kind == EventKind::ENGINE_CLOCK {
                let c = r.read::<EngineClock>().unwrap();
                anchor = Some((c.base_engine_ns, r.tsc, c.tsc_hz_milli));
            }
            anchor.map_or(stopped, |(base, tsc, hz)| {
                i128::from(base) + (i128::from(r.tsc) - i128::from(tsc)) * 1_000_000_000_000 / i128::from(hz)
            })
        })
        .collect()
}

fn point(log: &Log, times: &[i128], last: u64, settle_ms: u64) -> u64 {
    let at = log.recs.iter().position(|r| r.seq == last).unwrap();
    log.recs
        .iter()
        .enumerate()
        .skip(at + 1)
        .find(|(i, _)| times[*i] >= times[at] + i128::from(settle_ms) * 1_000_000)
        .unwrap()
        .1
        .seq
}

fn check(spec: ModelRestart, path_v2: bool) {
    let f = fixture::model_restart(spec);
    let log = fixture::read(&f);
    let cp = f.root.join("checkpoints/tsa.ckpt");
    let bytes = std::fs::read(&cp).unwrap();
    let c = checkpoint::decode(&bytes, 1 << 20).unwrap();
    let times = timeline(&log);
    let resets: Vec<_> = log.recs.iter().filter_map(is_reset).collect();
    assert_eq!(resets.len(), usize::from(spec.count));
    assert!(
        resets
            .iter()
            .all(|n| n.strategy.0 == 9 && n.v[0] == spec.reason && n.v[2] == spec.settle_ms)
    );
    let points: Vec<_> = resets
        .iter()
        .map(|n| point(&log, &times, n.v[1], n.v[2]))
        .collect();
    assert!(points.windows(2).all(|p| p[0] < p[1]));
    let mut threshold = if path_v2 {
        point(&log, &times, c.key.last_seq, spec.settle_ms)
    } else {
        0
    };
    let mut compared = 0;
    let mut skipped = 0;
    for r in &log.recs {
        if let Some(n) = is_reset(r) {
            threshold = point(&log, &times, n.v[1], n.v[2]);
        }
        if r.kind == EventKind::ADVICE && r.seq > c.key.last_seq {
            let h = r.read::<AdviceHdr>().unwrap();
            if h.basis_seq >= threshold {
                compared += 1;
            } else {
                skipped += 1;
            }
        }
    }
    assert!(compared > 0 && skipped > 0);
    let r = execute(&f, path_v2, &options(&cp, None));
    passed(&r);
    assert_eq!(r.1["summary"]["model_advice_compared"], compared);
    assert_eq!(r.1["summary"]["recompute_from_seq"], *points.last().unwrap());
    assert_eq!(
        r.1["summary"]["runs"].as_array().unwrap().len(),
        usize::from(spec.count) + 1
    );
    assert_eq!(std::fs::read(&cp).unwrap(), bytes);
    // A checkpoint before the first selected process still supplies its Model origin.
    // Its reset Note is before the selected Input, but its rebuild interval remains active.
    let window = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::ENGINE_CLOCK)
        .nth(1)
        .unwrap()
        .seq
        + 2;
    let window_count = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::ADVICE && r.seq > window)
        .filter(|r| {
            let last_note = log
                .recs
                .iter()
                .take_while(|n| n.seq <= r.seq)
                .filter_map(is_reset)
                .last();
            let min_basis = last_note.map_or(0, |n| point(&log, &times, n.v[1], n.v[2]));
            r.read::<AdviceHdr>().unwrap().basis_seq >= min_basis
        })
        .count();
    let windowed = execute(&f, path_v2, &options(&cp, Some(window)));
    passed(&windowed);
    assert_eq!(windowed.1["summary"]["model_advice_compared"], window_count);
    assert_eq!(
        windowed.1["summary"]["recompute_from_seq"],
        *points.last().unwrap()
    );
    for n in resets {
        let l = log.recs.iter().position(|r| r.seq == n.v[1]).unwrap();
        let clock = log
            .recs
            .iter()
            .enumerate()
            .skip(l + 1)
            .find(|(_, r)| r.kind == EventKind::ENGINE_CLOCK)
            .unwrap()
            .0;
        assert_eq!(times[clock] - times[l], 30_000_000_000);
        let p = log
            .recs
            .iter()
            .position(|r| r.seq == point(&log, &times, n.v[1], n.v[2]))
            .unwrap();
        assert_eq!(
            times[p] - times[clock],
            i128::from(spec.settle_ms) * 1_000_000 - 30_000_000_000
        );
        assert!(log.recs[clock].tsc < log.recs[l].tsc);
    }
}

#[test]
fn rpl_058_path_reset_note() {
    for reason in [1, 2] {
        check(
            ModelRestart {
                count: 1,
                settle_ms: 60_000,
                reason,
            },
            false,
        );
    }
}

#[test]
fn rpl_059_last_rebuild_point() {
    check(
        ModelRestart {
            count: 2,
            settle_ms: 60_000,
            reason: 1,
        },
        false,
    );
}

#[test]
fn rpl_060_engine_time_excludes_downtime() {
    check(
        ModelRestart {
            count: 1,
            settle_ms: 120_000,
            reason: 1,
        },
        true,
    );
}

#[test]
fn rpl_272_restart_origin_and_reset_counterexamples() {
    let f = fixture::model_restart(ModelRestart {
        count: 1,
        settle_ms: 60_000,
        reason: 1,
    });
    let log = fixture::read(&f);
    let note_seq = log.recs.iter().find(|r| is_reset(r).is_some()).unwrap().seq;
    let note_ix = log.recs.iter().position(|r| r.seq == note_seq).unwrap();
    let mut files: Vec<_> = std::fs::read_dir(f.file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "jrn"))
        .collect();
    files.sort();
    let path = &files[log.seg_of(note_ix)];
    let original = std::fs::read(path).unwrap();
    let cp = f.root.join("checkpoints/tsa.ckpt");
    for (name, field, want) in [
        ("bad-reason", std::mem::offset_of!(Note, v), 3),
        ("bad-reserved", std::mem::offset_of!(Note, v) + 5 * 8, 3),
        ("missing-reset", std::mem::offset_of!(Note, code), 1),
    ] {
        let (changed, _) = crate::model::mutate(&original, EventKind::NOTE, Some(note_seq), field);
        std::fs::write(path, changed).unwrap();
        fixture::seal(path);
        let mut args = options(&cp, None);
        args.extend([
            "--report".into(),
            f.root.join(format!("{name}.json")).display().to_string(),
        ]);
        let r = execute(&f, false, &args);
        assert_eq!(r.0, want, "{name}: {r:?}");
        if want == 1 {
            assert_eq!(r.1["summary"]["verify"], "pass");
            assert_eq!(r.1["summary"]["recompute"], "fail");
            assert!(!r.1["model_divergences"].as_array().unwrap().is_empty());
        }
    }
    std::fs::write(path, &original).unwrap();
    fixture::seal(path);
    let window = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::ENGINE_CLOCK)
        .nth(1)
        .unwrap()
        .seq;
    let missing = execute(&f, false, &["--from-seq".into(), window.to_string()]);
    assert_eq!(missing.0, 0, "{missing:?}");
    assert_eq!(missing.1["summary"]["verify"], "pass");
    assert_eq!(missing.1["summary"]["recompute"], "skipped");
    assert_eq!(missing.1["model_no_start"], serde_json::json!(["tsa"]));
    assert_eq!(missing.1["summary"]["model_advice_compared"], 0);
}

#[test]
fn rpl_273_checkpoint_catchup_reemits_prior_run_advice() {
    let f = fixture::model_restart(ModelRestart {
        count: 2,
        settle_ms: 120_000,
        reason: 0,
    });
    let log = fixture::read(&f);
    assert!(log.recs.iter().all(|r| is_reset(r).is_none()));
    let cp = f.root.join("checkpoints/tsa.ckpt");
    let bytes = std::fs::read(&cp).unwrap();
    let c = checkpoint::decode(&bytes, 1 << 20).unwrap();
    let mut seen = std::collections::BTreeSet::new();
    let repeated: Vec<_> = log
        .recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == EventKind::ADVICE)
        .map(|(ix, r)| (ix, r.read::<AdviceHdr>().unwrap()))
        .filter(|(_, a)| !seen.insert((a.strategy.0, a.account.0, a.group.0, a.local_kind, a.basis_seq)))
        .collect();
    assert_eq!(repeated.len(), 12, "six prior-run Advice per checkpoint catch-up");
    let args = options(&cp, None);
    let r = execute(&f, false, &args);
    passed(&r);
    assert_eq!(r.1["summary"]["runs"].as_array().unwrap().len(), 3);
    assert_eq!(
        r.1["summary"]["model_advice_compared"],
        log.recs
            .iter()
            .filter(|r| r.kind == EventKind::ADVICE && r.seq > c.key.last_seq)
            .count()
    );
    assert_eq!(r.1["summary"]["coalesced"], 0);
    let ix = repeated[0].0;
    let mut files: Vec<_> = std::fs::read_dir(f.file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "jrn"))
        .collect();
    files.sort();
    let path = &files[log.seg_of(ix)];
    let original = std::fs::read(path).unwrap();
    let (changed, _) = crate::model::mutate(
        &original,
        EventKind::ADVICE,
        Some(log.recs[ix].seq),
        std::mem::size_of::<AdviceHdr>() + 16,
    );
    std::fs::write(path, changed).unwrap();
    fixture::seal(path);
    let mut args = args;
    args.extend([
        "--report".into(),
        f.root.join("bad-catchup.json").display().to_string(),
    ]);
    let wrong = execute(&f, false, &args);
    assert_eq!(wrong.0, 1, "{wrong:?}");
    assert_eq!(wrong.1["summary"]["verify"], "pass");
    assert_eq!(wrong.1["summary"]["recompute"], "fail");
    assert_eq!(wrong.1["summary"]["first_model_divergent_seq"], log.recs[ix].seq);
    std::fs::write(path, original).unwrap();
    fixture::seal(path);
}
