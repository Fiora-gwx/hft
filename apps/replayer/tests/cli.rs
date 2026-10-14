mod dst;
mod fixture;
mod long_model;
mod model;
mod model_boundary;
mod receipt;
mod recompute;
mod recovery;
mod recovery_gap;
mod verify;
/// Tests over the generated 1 h / 3 h fixtures (feature `long-fixtures`; see `long_model::heavy`).
#[cfg(feature = "long-fixtures")]
// `path = "."`: the nested modules are this directory's files.
#[path = "."]
mod heavy {
    mod fixture_bundle;
    mod snapshot_diff;
}

use hft_event::{EventKind, RecordFlags, RecordHeader};
use hft_strategy_registry::Strategies;
use serde_json::Value;
use std::process::Command;
use zerocopy::FromBytes;

fn run(args: &[String]) -> (i32, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_replayer"))
        .args(args)
        .output()
        .unwrap();
    (
        o.status.code().unwrap(),
        String::from_utf8(o.stdout).unwrap(),
        String::from_utf8(o.stderr).unwrap(),
    )
}
fn args(path: &std::path::Path) -> Vec<String> {
    vec!["verify".into(), "--journal".into(), path.display().to_string()]
}

#[test]
fn rpl_260_real_file_verify_report_and_divergence() {
    let f = fixture::create::<Strategies>();
    let original = std::fs::read(&f.file).unwrap();
    let report = f.root.join("out.json");
    let mut a = args(&f.file);
    a.extend(["--report".into(), report.display().to_string()]);
    let (code, text, err) = run(&a);
    assert_eq!(code, 0, "{text} {err}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["summary"]["verify"], "pass");
    assert_eq!(value["summary"]["date"], "2026-10-01");
    assert_eq!(value["summary"]["recompute"], "skipped");
    assert!(value["summary"]["outputs_compared"].as_u64().unwrap() > 0);
    assert!(
        value["summary"]["snapshots_checked"].as_u64().unwrap() >= 3,
        "{value}"
    );
    assert_eq!(std::fs::read(&f.file).unwrap(), original);
    let summary: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    assert_eq!(summary, value["summary"]);
    let mut changed = original.clone();
    let mut offset = 4096;
    let seq = loop {
        let (h, _) = RecordHeader::read_from_prefix(&changed[offset..]).unwrap();
        assert!(h.len > 0);
        if h.kind == EventKind::CMD_RESULT.0 {
            changed[offset + 24] ^= 1;
            if RecordFlags(h.flags).contains(RecordFlags::CRC) {
                let end = offset + h.len as usize;
                let crc = crc32c::crc32c(&changed[offset..end - 4]);
                changed[end - 4..end].copy_from_slice(&crc.to_le_bytes());
            }
            break h.seq;
        }
        offset += h.len as usize;
    };
    let _ = std::fs::remove_file(f.file.with_extension("seal"));
    std::fs::write(&f.file, &changed).unwrap();
    let (code, text, err) = run(&a);
    assert_eq!(code, 1, "{text} {err}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["summary"]["first_divergent_seq"], seq);
    assert_eq!(v["divergences"][0]["seq"], seq);
    assert!(!v["divergences"][0]["field_diff"].as_array().unwrap().is_empty());
    let sidecar: Value =
        serde_json::from_slice(&std::fs::read(report.with_extension("divergence.json")).unwrap()).unwrap();
    assert_eq!(sidecar["divergences"], v["divergences"]);
    assert_eq!(sidecar["recovery_diffs"], v["recovery_diffs"]);
    assert_eq!(std::fs::read(&f.file).unwrap(), changed);
    let mut window = args(&f.file);
    window.extend(["--to-seq".into(), (seq - 1).to_string()]);
    let result = run(&window);
    assert_eq!(result.0, 0, "{result:?}");
    std::fs::write(&f.file, &original).unwrap();
    assert_eq!(run(&a).0, 0);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(report.with_extension("divergence.json")).unwrap())
            .unwrap(),
        serde_json::json!({"divergences": [], "recovery_diffs": []})
    );
    // A change in a variable-length snapshot section is not a change in its last header
    // field (engine_ns). Exercise the actual JSON formatter through the release binary.
    let mut state = original.clone();
    let mut offset = 4096;
    let snapshot_seq = loop {
        let (h, _) = RecordHeader::read_from_prefix(&state[offset..]).unwrap();
        assert!(h.len > 0);
        if h.kind == EventKind::STATE_SNAPSHOT.0 {
            let (part, _) = hft_event::PartHdr::read_from_prefix(&state[offset + 24..]).unwrap();
            let body = offset + 24 + size_of::<hft_event::PartHdr>();
            if part.part == 0 && state[body] != hft_sequencer::snapshot::PURPOSE_BOOT {
                state[body + size_of::<hft_event::SnapshotHdr>() + size_of::<hft_event::SectionHdr>()] ^= 1;
                let end = offset + h.len as usize;
                let crc = crc32c::crc32c(&state[offset..end - 4]);
                assert!(RecordFlags(h.flags).contains(RecordFlags::CRC));
                state[end - 4..end].copy_from_slice(&crc.to_le_bytes());
                break h.seq;
            }
        }
        offset += h.len as usize;
    };
    std::fs::write(&f.file, state).unwrap();
    let (code, text, err) = run(&args(&f.file));
    assert_eq!(code, 1, "{text} {err}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["summary"]["first_divergent_seq"], snapshot_seq);
    let fields = v["divergences"][0]["field_diff"].as_array().unwrap();
    assert!(fields.iter().any(|f| f["field"] == "payload"));
    assert!(!fields.iter().any(|f| f["field"] == "engine_ns"));
    let mut corrupt = original;
    corrupt[0] ^= 1;
    std::fs::write(&f.file, corrupt).unwrap();
    let result = run(&args(&f.file));
    assert_eq!(result.0, 3, "{result:?}");
    assert!(result.1.is_empty());
}

#[test]
fn rpl_261_invalid_and_unimplemented_options_fail_loud() {
    let rows = [
        vec![],
        vec!["verify"],
        vec!["verify", "--journal", "missing", "--advice", "foo"],
        vec!["verify", "--journal", "missing", "--model-checkpoint", "a.ckpt"],
        vec!["verify", "--journal", "missing", "--instance", "live"],
        vec![
            "verify",
            "--journal",
            "missing",
            "--from-seq",
            "100",
            "--to-seq",
            "99",
        ],
        vec!["verify", "--journal", "missing", "--max-records", "0"],
        vec!["verify", "--journal", "missing", "--report-dir", "out"],
        vec!["sim", "--journal", "missing"],
    ];
    for row in rows {
        let a: Vec<_> = row.into_iter().map(str::to_owned).collect();
        let result = run(&a);
        assert_eq!(result.0, 2, "{a:?} {result:?}");
        assert!(result.1.is_empty());
        assert!(
            !result.2.contains("No such file"),
            "invalid arguments read the journal"
        );
    }
    let (code, v, _) = run(&["--version".into()]);
    assert_eq!(code, 0);
    assert!(v.starts_with(replayer::VERSION));
}

#[test]
fn rpl_262_limits_instance_and_strategy_incompatibility() {
    let f = fixture::create::<hft_testkit::TestStrategies>();
    let a = args(&f.file);
    let r = run(&a);
    assert_eq!(r.0, 4, "{r:?}");
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = replayer::run::<hft_testkit::TestStrategies>(&a, &mut out, &mut err);
    assert_eq!(
        code,
        0,
        "{} {}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    );
    for (extra, expected) in [
        (vec!["--max-records", "1"], 3),
        (vec!["--max-body-bytes", "1"], 3),
        (vec!["--reassembly-bytes", "1"], 3),
        (vec!["--instance", "sim"], 2),
    ] {
        let mut a = a.clone();
        a.extend(extra.into_iter().map(str::to_owned));
        let r = run(&a);
        assert_eq!(r.0, expected, "{a:?} {r:?}");
    }
}

fn flip_first_command(path: &std::path::Path, original: &[u8]) -> u64 {
    let mut bytes = original.to_vec();
    let mut offset = 4096;
    loop {
        let record = hft_event::decode_record(&bytes[offset..]).unwrap();
        let h = record.hdr;
        if h.kind == EventKind::CONTROL_CMD.0 {
            // Every Multi record carries PartHdr, including a one-part command. PART
            // only means another part follows; it is clear on the final/only part.
            let (part, _) = hft_event::PartHdr::read_from_prefix(record.body).unwrap();
            assert_eq!(part.part, 0);
            assert!(part.parts > 0);
            assert!(part.chunk_len as usize >= size_of::<hft_event::CmdHdr>());
            assert_eq!(RecordFlags(h.flags).contains(RecordFlags::PART), part.parts > 1);
            let body = offset + 24 + size_of::<hft_event::PartHdr>();
            bytes[body + std::mem::offset_of!(hft_event::CmdHdr, cmd_id)] ^= 0x40;
            assert!(RecordFlags(h.flags).contains(RecordFlags::CRC));
            let end = offset + h.len as usize;
            let crc = crc32c::crc32c(&bytes[offset..end - 4]);
            bytes[end - 4..end].copy_from_slice(&crc.to_le_bytes());
            hft_event::decode_record(&bytes[offset..end]).unwrap();
            let _ = std::fs::remove_file(path.with_extension("seal"));
            std::fs::write(path, bytes).unwrap();
            return h.seq;
        }
        offset += h.len as usize;
    }
}

#[test]
fn rpl_263_real_restart_runs_are_independent_and_strict() {
    use fixture::Restart;
    for restart in [Restart::Recovery, Restart::FromSnapshot, Restart::Version] {
        let f = fixture::restarted::<Strategies>(restart);
        let directory = f.file.parent().unwrap();
        let second = directory.join("0000000002.jrn");
        let a = args(directory);
        let originals = [std::fs::read(&f.file).unwrap(), std::fs::read(&second).unwrap()];
        let log = fixture::read(&f);
        assert_eq!(log.seg_start.len(), 2);
        let begin = log.recs[log.seg_start[1]]
            .read::<hft_event::SegmentRoll>()
            .unwrap();
        let expected_reason = if matches!(restart, Restart::FromSnapshot) {
            6
        } else {
            4
        };
        assert_eq!(begin.reason, expected_reason);
        let second_snapshot = log.recs[log.seg_start[1]..]
            .iter()
            .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .unwrap()
            .seq;
        let (code, text, err) = run(&a);
        assert_eq!(code, 0, "{restart:?}: {text} {err}");
        let v: Value = serde_json::from_str(&text).unwrap();
        let runs = v["summary"]["runs"].as_array().unwrap();
        assert_eq!(runs.len(), 2, "{v}");
        assert_eq!(runs[1]["start_snapshot_seq"], second_snapshot);
        for r in runs {
            assert!(r["inputs_replayed"].as_u64().unwrap() > 0, "{v}");
            assert!(r["outputs_compared"].as_u64().unwrap() > 0, "{v}");
            assert!(r["snapshots_checked"].as_u64().unwrap() >= 3, "{v}");
            assert_eq!(r["mismatches"], 0);
        }
        assert_eq!(std::fs::read(&f.file).unwrap(), originals[0]);
        assert_eq!(std::fs::read(&second).unwrap(), originals[1]);
        let suffix = run(&args(&second));
        assert_eq!(suffix.0, 0, "recovery-only suffix {restart:?}: {suffix:?}");
        let suffix: Value = serde_json::from_str(&suffix.1).unwrap();
        assert_eq!(suffix["summary"]["runs"].as_array().unwrap().len(), 1);
        assert_eq!(suffix["summary"]["start_snapshot_seq"], second_snapshot);
        for extra in [
            vec!["--from-seq".into(), second_snapshot.to_string()],
            vec![
                "--to-seq".into(),
                (log.recs[log.seg_start[1]].seq - 1).to_string(),
            ],
        ] {
            let mut window = a.clone();
            window.extend(extra);
            let r = run(&window);
            assert_eq!(r.0, 0, "{restart:?}: {r:?}");
            let v: Value = serde_json::from_str(&r.1).unwrap();
            assert_eq!(v["summary"]["runs"].as_array().unwrap().len(), 1);
        }
        let first_changed = flip_first_command(&f.file, &originals[0]);
        let r = run(&a);
        assert_eq!(r.0, 1, "{r:?}");
        let v: Value = serde_json::from_str(&r.1).unwrap();
        assert_eq!(v["summary"]["runs"].as_array().unwrap().len(), 1, "stop on first");
        assert!(v["summary"]["first_divergent_seq"].as_u64().unwrap() > first_changed);
        let mut keep = a.clone();
        keep.push("--keep-going".into());
        let r = run(&keep);
        assert_eq!(r.0, 1, "{r:?}");
        let v: Value = serde_json::from_str(&r.1).unwrap();
        assert_eq!(v["summary"]["runs"].as_array().unwrap().len(), 2, "{v}");
        assert!(v["summary"]["runs"][0]["mismatches"].as_u64().unwrap() > 0);
        assert_eq!(
            v["summary"]["runs"][1]["mismatches"], 0,
            "old state crossed restart: {v}"
        );
        std::fs::write(&f.file, &originals[0]).unwrap();
        flip_first_command(&second, &originals[1]);
        let r = run(&a);
        assert_eq!(r.0, 1, "{r:?}");
        let v: Value = serde_json::from_str(&r.1).unwrap();
        assert_eq!(v["summary"]["runs"][0]["mismatches"], 0);
        assert!(
            v["summary"]["runs"][1]["mismatches"].as_u64().unwrap() > 0,
            "new outputs escaped comparison: {v}"
        );
    }
}
