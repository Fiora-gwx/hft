use std::path::Path;

use hft_event::{EventKind, RecordFlags, RecordHeader};
use hft_sequencer::advisor::checkpoint;
use hft_testkit::TestStrategies;
use serde_json::Value;
use zerocopy::FromBytes;

use crate::fixture;

fn execute(path: &Path, extra: &[String]) -> (i32, Value, String) {
    let mut args = crate::args(path);
    args.extend_from_slice(extra);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = replayer::run::<TestStrategies>(&args, &mut out, &mut err);
    (
        code,
        serde_json::from_slice(&out).unwrap_or(Value::Null),
        String::from_utf8(err).unwrap(),
    )
}
fn options(f: &fixture::Fixture, ckpt: bool) -> Vec<String> {
    let mut a = vec!["--advice".into(), "recompute".into()];
    if ckpt {
        a.extend([
            "--model-checkpoint".into(),
            f.root.join("checkpoints").display().to_string(),
        ]);
    }
    a
}
fn checkpoint_files(f: &fixture::Fixture) -> Vec<std::path::PathBuf> {
    let mut p: Vec<_> = std::fs::read_dir(f.root.join("checkpoints"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    p.sort();
    p
}
fn passed(r: &(i32, Value, String)) {
    assert_eq!(r.0, 0, "{r:?}");
    assert_eq!(r.1["summary"]["verify"], "pass", "{r:?}");
    assert_eq!(r.1["summary"]["recompute"], "pass", "{r:?}");
    assert!(r.1["model_divergences"].as_array().unwrap().is_empty());
    assert!(r.1["model_no_start"].as_array().unwrap().is_empty());
}

#[test]
fn rpl_266_real_advisor_files_recompute_with_and_without_checkpoints() {
    let f = fixture::with_model();
    let original = std::fs::read(&f.file).unwrap();
    let checkpoints: Vec<_> = checkpoint_files(&f)
        .into_iter()
        .map(|p| {
            let b = std::fs::read(&p).unwrap();
            (p, b)
        })
        .collect();
    let mut counts = Vec::new();
    for ckpt in [false, true] {
        let report = f.root.join("model.json");
        let mut a = options(&f, ckpt);
        a.extend(["--report".into(), report.display().to_string()]);
        let r = execute(&f.file, &a);
        passed(&r);
        let count = r.1["summary"]["model_advice_compared"].as_u64().unwrap();
        assert!(count > 0, "{r:?}");
        counts.push(count);
        assert_eq!(
            r.1["summary"]["checkpoint_key"].as_array().unwrap().len(),
            if ckpt { checkpoints.len() } else { 0 }
        );
        let detail: Value =
            serde_json::from_slice(&std::fs::read(report.with_extension("divergence.json")).unwrap())
                .unwrap();
        assert_eq!(detail["model_divergences"], r.1["model_divergences"]);
        assert_eq!(detail["model_no_start"], r.1["model_no_start"]);
        assert_eq!(std::fs::read(&f.file).unwrap(), original);
        for (p, b) in &checkpoints {
            assert_eq!(&std::fs::read(p).unwrap(), b);
        }
    }
    assert!(counts[0] > counts[1]); // the consumed prefix is not delivered twice
    let r = execute(&f.file, &[]);
    assert_eq!(r.0, 0, "{r:?}");
    assert_eq!(r.1["summary"]["skipped_reason"], "advice_journal");
    assert!(r.1.get("model_divergences").is_none());
    let f = fixture::create::<TestStrategies>(); // ProbeTaker uses NoModel
    let r = execute(&f.file, &options(&f, false));
    passed(&r);
    assert_eq!(r.1["summary"]["model_advice_compared"], 0);
    assert_eq!(r.1["summary"]["coalesced"], 0);
}

fn mutate(bytes: &[u8], kind: EventKind, seq: Option<u64>, body_offset: usize) -> (Vec<u8>, u64) {
    let mut out = bytes.to_vec();
    let mut at = 4096;
    loop {
        let (h, _) = RecordHeader::read_from_prefix(&out[at..]).unwrap();
        assert!(h.len > 0);
        if h.kind == kind.0 && seq.is_none_or(|s| s == h.seq) {
            out[at + 24 + body_offset] ^= 1;
            assert!(RecordFlags(h.flags).contains(RecordFlags::CRC));
            let end = at + h.len as usize;
            let crc = crc32c::crc32c(&out[at..end - 4]);
            out[end - 4..end].copy_from_slice(&crc.to_le_bytes());
            return (out, h.seq);
        }
        at += h.len as usize;
    }
}

#[test]
fn rpl_267_advice_header_and_payload_divergences_are_reported() {
    let f = fixture::with_model();
    let original = std::fs::read(&f.file).unwrap();
    let _ = std::fs::remove_file(f.file.with_extension("seal"));
    // All five pairing fields, basis_tsc and payload are checked independently.
    for offset in [0, 1, 2, 4, 16, 24, 32] {
        let (bytes, seq) = mutate(&original, EventKind::ADVICE, None, offset);
        std::fs::write(&f.file, &bytes).unwrap();
        let r = execute(&f.file, &options(&f, false));
        assert_eq!(r.0, 1, "offset={offset} {r:?}");
        assert_eq!(r.1["summary"]["recompute"], "fail", "{r:?}");
        assert_eq!(r.1["summary"]["first_model_divergent_seq"], seq, "{r:?}");
        assert_eq!(r.1["model_divergences"][0]["seq"], seq);
        assert!(
            !r.1["model_divergences"][0]["field_diff"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(std::fs::read(&f.file).unwrap(), bytes);
    }
}

#[test]
fn rpl_268_checkpoint_identity_origin_bounds_and_readonly_paths() {
    let f = fixture::with_model();
    let log = fixture::read(&f);
    let files = checkpoint_files(&f);
    let bytes = std::fs::read(&files[0]).unwrap();
    let d = checkpoint::decode(&bytes, 1 << 20).unwrap();
    let advice: Vec<_> = log.recs.iter().filter(|r| r.kind == EventKind::ADVICE).collect();
    let from = advice[advice.len() - 3].seq;
    assert!(from > d.key.last_seq);
    let mut window = options(&f, false);
    window.extend(["--from-seq".into(), from.to_string()]);
    let missing = execute(&f.file, &window);
    assert_eq!(missing.0, 0, "{missing:?}");
    assert_eq!(missing.1["summary"]["recompute"], "skipped");
    assert_eq!(missing.1["model_no_start"].as_array().unwrap().len(), files.len());
    assert!(missing.1["summary"].get("coalesced").is_none());
    window.extend([
        "--model-checkpoint".into(),
        f.root.join("checkpoints").display().to_string(),
    ]);
    let r = execute(&f.file, &window);
    passed(&r);
    assert!(r.1["summary"]["model_advice_compared"].as_u64().unwrap() > 0);
    assert!(r.1["summary"]["start_snapshot_seq"].as_u64().unwrap() < d.key.last_seq);
    for field in 0..5 {
        let mut h = d.header(0);
        match field {
            0 => h.kind_id ^= 1,
            1 => h.layout_version ^= 1,
            2 => h.schema_id ^= 1,
            3 => h.strategy.0 ^= 1,
            _ => h.account.0 ^= 1,
        }
        std::fs::write(&files[0], checkpoint::encode(&h, d.body, 1 << 20).unwrap()).unwrap();
        let r = execute(&f.file, &window);
        assert_eq!(r.0, 2, "field={field} {r:?}");
        assert!(r.1.is_null());
    }
    for last in log.recs[0]
        .seq
        .checked_sub(2)
        .into_iter()
        .chain([log.recs.last().unwrap().seq + 1, u64::MAX])
    {
        let mut h = d.header(0);
        h.last_seq = last;
        std::fs::write(&files[0], checkpoint::encode(&h, d.body, 1 << 20).unwrap()).unwrap();
        let r = execute(&f.file, &window);
        assert_eq!(r.0, 0, "{r:?}");
        assert_eq!(r.1["summary"]["recompute"], "skipped");
        assert_eq!(r.1["model_no_start"].as_array().unwrap().len(), 1);
    }
    let mut corrupt = bytes.clone();
    corrupt[40] ^= 1;
    std::fs::write(&files[0], corrupt).unwrap();
    assert_eq!(execute(&f.file, &window).0, 3);
    std::fs::write(&files[0], &bytes).unwrap();
    // A single file has an arbitrary name; it still cannot become an output report.
    let named = f.root.join("checkpoint.json");
    std::fs::write(&named, &bytes).unwrap();
    let mut a = options(&f, false);
    a.extend([
        "--model-checkpoint".into(),
        named.display().to_string(),
        "--report".into(),
        named.display().to_string(),
    ]);
    assert_eq!(execute(&f.file, &a).0, 2);
    assert_eq!(std::fs::read(&named).unwrap(), bytes);
    for flag in ["--max-model-records", "--max-model-bytes"] {
        let mut a = options(&f, false);
        a.extend([flag.into(), "1".into()]);
        let r = execute(&f.file, &a);
        assert_eq!(r.0, 3, "{flag} {r:?}");
    }
}

#[test]
fn rpl_269_checkpoint_prefix_outputs_are_verified_before_requested_window() {
    let f = fixture::with_model();
    let log = fixture::read(&f);
    let last = log
        .recs
        .iter()
        .rev()
        .find(|r| r.kind == EventKind::ADVICE)
        .unwrap()
        .seq;
    let original = std::fs::read(&f.file).unwrap();
    let cmd = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::CMD_RESULT)
        .unwrap()
        .seq;
    assert!(cmd < last);
    let (bytes, seq) = mutate(&original, EventKind::CMD_RESULT, Some(cmd), 0);
    let _ = std::fs::remove_file(f.file.with_extension("seal"));
    std::fs::write(&f.file, bytes).unwrap();
    let mut a = options(&f, true);
    a.extend(["--from-seq".into(), last.to_string()]);
    let r = execute(&f.file, &a);
    assert_eq!(r.0, 1, "{r:?}");
    assert_eq!(r.1["summary"]["first_output_divergent_seq"], seq);
    assert_eq!(r.1["summary"]["recompute"], "pass");
}
