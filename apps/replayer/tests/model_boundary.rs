use hft_event::{AdviceHdr, CmdHdr, CmdResult, ControlCmd, EventKind, Note, NoteSource, SnapshotHdr};
use hft_sequencer::advisor::checkpoint;
use hft_sequencer::driver::replay::Log;
use serde_json::Value;

use crate::fixture::{self, ModelBoundary, ModelRestart};
use crate::long_model::{execute, execute_input, options, passed};

fn advices(log: &Log) -> Vec<&hft_sequencer::MemRecord> {
    log.recs.iter().filter(|r| r.kind == EventKind::ADVICE).collect()
}
fn words(bytes: &[u8]) -> Vec<u64> {
    assert!(bytes.len().is_multiple_of(8));
    bytes
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
fn report(f: &fixture::Fixture, label: &str) -> Vec<String> {
    vec![
        "--report".into(),
        f.root.join(format!("{label}.json")).display().to_string(),
    ]
}
fn sources(f: &fixture::Fixture) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    std::fs::read_dir(f.file.parent().unwrap())
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            let b = std::fs::read(&p).unwrap();
            (p, b)
        })
        .collect()
}
fn unchanged(before: &[(std::path::PathBuf, Vec<u8>)]) {
    for (p, b) in before {
        assert_eq!(std::fs::read(p).unwrap(), *b);
    }
}
fn final_samples(f: &fixture::Fixture) -> Vec<u64> {
    let b = std::fs::read(f.root.join("final.ckpt")).unwrap();
    words(checkpoint::decode(&b, 1 << 20).unwrap().samples)
}
fn applied(log: &Log, command: &hft_sequencer::MemRecord) {
    let id = command.read::<CmdHdr>().unwrap().cmd_id;
    let results: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::CMD_RESULT)
        .filter_map(|r| r.read::<CmdResult>())
        .filter(|r| r.cmd_id == id)
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].status, hft_sequencer::codes::status::APPLIED);
}

#[test]
fn rpl_053_start_at_instance_creation() {
    let f = fixture::model_boundary(ModelBoundary::Created);
    let log = fixture::read(&f);
    let params = log
        .recs
        .iter()
        .find(|r| {
            r.kind == EventKind::CONTROL_CMD
                && matches!(
                    ControlCmd::decode(&r.body).unwrap().1,
                    ControlCmd::SetParams { .. }
                )
        })
        .unwrap();
    applied(&log, params);
    let before_create: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.seq < params.seq && r.kind == EventKind::BBO)
        .collect();
    assert_eq!(before_create.len(), 240);
    assert_eq!(params.tsc, 1_200_000_000_000);
    let after = advices(&log);
    assert_eq!(after.len(), 480);
    assert!(
        after
            .iter()
            .all(|r| r.read::<AdviceHdr>().unwrap().basis_seq > params.seq)
    );
    assert_eq!(words(&after[0].body[size_of::<AdviceHdr>()..])[0], 1);
    assert_eq!(final_samples(&f), [480]);
    assert_eq!(
        log.recs
            .iter()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .map(|r| r.read::<SnapshotHdr>().unwrap().engine_ns)
            .max(),
        Some(3_600_000_000_000)
    );
    let original = sources(&f);
    let r = execute(&f, false, &report(&f, "created"));
    passed(&r);
    assert_eq!(r.1["summary"]["model_advice_compared"], 480);
    assert_eq!(r.1["summary"]["coalesced"], 0);
    unchanged(&original);
}

#[test]
fn rpl_042_coalesced_not_divergence() {
    let f = fixture::model_boundary(ModelBoundary::Coalesced);
    let log = fixture::read(&f);
    let bbo: Vec<_> = log.recs.iter().filter(|r| r.kind == EventKind::BBO).collect();
    assert_eq!(bbo.len(), 39);
    assert_eq!(bbo.last().unwrap().tsc - bbo[0].tsc, 10_000_000_000);
    let advice = advices(&log);
    assert_eq!(advice.len(), 2);
    for (r, n) in advice.into_iter().zip([1, 39]) {
        assert_eq!(words(&r.body[size_of::<AdviceHdr>()..])[0], n);
        assert_eq!(r.read::<AdviceHdr>().unwrap().basis_seq, bbo[n as usize - 1].seq);
    }
    assert_eq!(final_samples(&f), [39]);
    let original = sources(&f);
    let r = execute(&f, false, &report(&f, "coalesced"));
    passed(&r);
    assert_eq!(r.1["summary"]["coalesced"], 37);
    assert_eq!(r.1["summary"]["model_advice_compared"], 2);
    unchanged(&original);
}

#[test]
fn rpl_062_warmup_delivered() {
    let f = fixture::model_boundary(ModelBoundary::Warmup);
    let log = fixture::read(&f);
    let warmups: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::CONTROL_CMD)
        .filter(|r| {
            matches!(
                ControlCmd::decode(&r.body).unwrap().1,
                ControlCmd::InjectWarmup { .. }
            )
        })
        .collect();
    assert_eq!(warmups.len(), 3);
    for (no, r) in warmups.iter().enumerate() {
        applied(&log, r);
        let ControlCmd::InjectWarmup {
            local_kind,
            chunk_no,
            last,
            payload,
            ..
        } = ControlCmd::decode(&r.body).unwrap().1
        else {
            unreachable!()
        };
        assert_eq!((local_kind, chunk_no, last), (9, no as u32, u8::from(no == 2)));
        assert_eq!(words(payload), [[11], [23], [47]][no]);
        let advice = advices(&log)
            .into_iter()
            .find(|a| a.read::<AdviceHdr>().unwrap().basis_seq == r.seq)
            .unwrap();
        let expected = [[1, 1, 11, 0, 1], [2, 0x0102, 34, 0, 1], [3, 0x010203, 81, 4, 1]][no];
        assert_eq!(words(&advice.body[size_of::<AdviceHdr>()..]), expected);
    }
    assert_eq!(final_samples(&f), [3, 0x010203, 81, 4, 3]);
    let mut original = sources(&f);
    let cp = f.root.join("checkpoints/tsa.ckpt");
    original.push((cp.clone(), std::fs::read(&cp).unwrap()));
    for ckpt in [false, true] {
        let mut args = report(&f, if ckpt { "warmup-cp" } else { "warmup" });
        args.push("--fixture-warmup".into());
        if ckpt {
            args.extend(options(&f.root.join("checkpoints/tsa.ckpt"), None));
        }
        let r = execute(&f, false, &args);
        passed(&r);
        assert_eq!(r.1["summary"]["model_advice_compared"], if ckpt { 3 } else { 6 });
        assert_eq!(r.1["summary"]["coalesced"], 0);
    }
    unchanged(&original);
}

#[test]
fn rpl_054_start_after_checkpoint_discarded() {
    let f = fixture::model_restart(ModelRestart {
        count: 1,
        settle_ms: 60000,
        reason: 3,
    });
    let log = fixture::read(&f);
    let start = log.seg_start[1];
    let note = log.recs[start..]
        .iter()
        .find(|r| {
            r.kind == EventKind::NOTE
                && r.read::<Note>().is_some_and(|n| {
                    n.code
                        == hft_event::note_code(
                            NoteSource::Advisor,
                            hft_sequencer::codes::advisor_note::MODEL_CHECKPOINT_DISCARDED,
                        )
                })
        })
        .unwrap();
    assert_eq!(
        note.read::<Note>().unwrap().v[..2],
        [9, hft_sequencer::codes::discard::CORRUPT]
    );
    let advice: Vec<_> = advices(&log).into_iter().filter(|r| r.seq > note.seq).collect();
    assert_eq!(advice.len(), 60);
    assert_eq!(words(&advice[0].body[size_of::<AdviceHdr>()..])[0], 1);
    assert_eq!(final_samples(&f), [60]);
    let original = sources(&f);
    let file = f.file.parent().unwrap().join("0000000002.jrn");
    let r = execute_input(&f, &file, false, &report(&f, "discarded-suffix"));
    passed(&r);
    assert_eq!(r.1["summary"]["runs"].as_array().unwrap().len(), 1);
    assert_eq!(r.1["summary"]["model_advice_compared"], 60);
    assert_eq!(r.1["summary"]["coalesced"], 0);
    unchanged(&original);
}

#[test]
fn rpl_274_warmup_and_coalesced_payload_counterexamples() {
    for (id, mode) in [
        ("RPL-274.1", ModelBoundary::Warmup),
        ("RPL-274.2", ModelBoundary::Coalesced),
    ] {
        let f = fixture::model_boundary(mode);
        let log = fixture::read(&f);
        let (kind, seq, offset, first_advice) = if mode == ModelBoundary::Warmup {
            let r = log
                .recs
                .iter()
                .rev()
                .find(|r| {
                    r.kind == EventKind::CONTROL_CMD
                        && matches!(
                            ControlCmd::decode(&r.body).unwrap().1,
                            ControlCmd::InjectWarmup { .. }
                        )
                })
                .unwrap();
            let a = advices(&log)
                .into_iter()
                .find(|a| a.read::<AdviceHdr>().unwrap().basis_seq == r.seq)
                .unwrap();
            (r.kind, r.seq, r.body.len() - 8, a.seq)
        } else {
            let r = advices(&log).last().copied().unwrap();
            (r.kind, r.seq, size_of::<AdviceHdr>(), r.seq)
        };
        let bytes = std::fs::read(&f.file).unwrap();
        let (bad, _) = crate::model::mutate(&bytes, kind, Some(seq), offset);
        std::fs::write(&f.file, &bad).unwrap();
        fixture::seal(&f.file);
        let mut args = report(&f, "bad-payload");
        if mode == ModelBoundary::Warmup {
            args.push("--fixture-warmup".into());
        }
        let r = execute(&f, false, &args);
        assert_eq!(r.0, 1, "{id}: {r:?}");
        assert_eq!(r.1["summary"]["verify"], "pass", "{id}: Output");
        assert_eq!(r.1["summary"]["recompute"], "fail", "{id}: Model");
        assert_eq!(r.1["summary"]["first_model_divergent_seq"], first_advice, "{id}");
        assert_eq!(
            r.1["model_divergences"].as_array().unwrap().len(),
            if mode == ModelBoundary::Warmup { 3 } else { 1 },
            "{id}: difference count"
        );
        assert_eq!(std::fs::read(&f.file).unwrap(), bad, "{id}: read-only input");
        let summary: Value =
            serde_json::from_slice(&std::fs::read(f.root.join("bad-payload.json")).unwrap()).unwrap();
        assert_eq!(summary["recompute"], "fail", "{id}: saved report");
    }
}
