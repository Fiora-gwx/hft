mod fixture;

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
    assert!(value["summary"]["snapshots_checked"].as_u64().unwrap() >= 3);
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
    assert_eq!(sidecar, v["divergences"]);
    assert_eq!(std::fs::read(&f.file).unwrap(), changed);
    let mut window = args(&f.file);
    window.extend(["--to-seq".into(), (seq - 1).to_string()]);
    let result = run(&window);
    assert_eq!(result.0, 0, "{result:?}");
    std::fs::write(&f.file, &original).unwrap();
    assert_eq!(run(&a).0, 0);
    assert_eq!(
        std::fs::read_to_string(report.with_extension("divergence.json"))
            .unwrap()
            .trim(),
        "[]"
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
        vec!["verify", "--journal", "missing", "--advice", "recompute"],
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
