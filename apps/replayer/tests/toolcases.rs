//! TOOL-039–045 exercise the xtask boundary with real journal files and the real verifier.
//! Only cargo process spawning is substituted; no exit code or report is scripted.
use hft_event::{BlobHdr, BlobKind, EventKind, OrderDecision, SnapshotHdr};
use hft_sequencer::driver::replay::Log;
use hft_testkit::TestStrategies;
use hft_testkit::dst::{DstOutcome, RerunPolicy, run_record};
use hft_testkit::scenarios::{SeqEngine, replay_config, suites};
use serde_json::Value;
use zerocopy::IntoBytes;

fn sample() -> (hft_testkit::tmp::TmpDir, Log) {
    let dir = hft_testkit::tmp::TmpDir::in_temp("tool-replay").unwrap();
    let suite = suites().into_iter().find(|s| s.name == "core").unwrap();
    let mut cfg = replay_config(&suite, 7);
    cfg.steps = 1500;
    cfg.faults = hft_testkit::dst::FaultRates::off();
    cfg.rerun = RerunPolicy::Never;
    cfg.rebuild_every = 200;
    cfg.confirmed_commands.push((
        1_000_000_000,
        hft_testkit::dst::actors::encode_cmd(
            &hft_event::ControlCmd::SnapshotNow,
            "toolcase",
            hft_event::CmdHdr::ROLE_ADMIN,
            hft_types::AccountId(1),
            hft_testkit::dst::world::UTC0 as u64 + 1_000_000_000,
            9999,
        ),
    ));
    let record = run_record::<SeqEngine>(&cfg);
    assert!(
        matches!(record.outcome, DstOutcome::Pass { .. }),
        "{:?}",
        record.outcome
    );
    let mut log = record.physical_journal.expect("physical DST journal");
    let mut meta = log.recs.last().unwrap().clone();
    meta.seq = log.record_end_seq(log.recs.len() - 1).unwrap() + 1;
    meta.kind = EventKind::LATENCY_SAMPLE;
    meta.flags = hft_event::RecordFlags(hft_event::RecordFlags::EVENT_END.0);
    meta.body = vec![0; size_of::<hft_event::LatencySample>()];
    log.recs.push(meta);
    (dir, log)
}

#[derive(Default)]
struct Verifier {
    output: xtask::exec::Output,
}
impl xtask::exec::Exec for Verifier {
    fn run(&mut self, cmd: &xtask::exec::Cmd) -> Result<xtask::exec::Output, xtask::EnvError> {
        assert_eq!(cmd.program, "cargo");
        assert!(cmd.args.windows(2).any(|x| x == ["-p", "replayer"]));
        let start = cmd.args.iter().position(|a| a == "--").unwrap() + 1;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let code = replayer::run::<TestStrategies>(&cmd.args[start..], &mut stdout, &mut stderr);
        self.output = xtask::exec::Output {
            code,
            stdout: String::from_utf8(stdout).unwrap(),
            stderr: String::from_utf8(stderr).unwrap(),
        };
        Ok(self.output.clone())
    }
}

fn verify(root: &std::path::Path, log: &Log, name: &str, extra: &[String]) -> (i32, Value, String) {
    let path = crate::fixture::dst::export(log, &root.join(name));
    let mut args = vec![path.display().to_string()];
    args.extend_from_slice(extra);
    let mut exec = Verifier::default();
    let code = xtask::replay_verify::run_with(root, &args, &mut exec);
    (
        code,
        serde_json::from_str(&exec.output.stdout).unwrap_or(Value::Null),
        exec.output.stderr,
    )
}

fn decision(log: &Log) -> usize {
    log.recs
        .iter()
        .position(|r| r.kind == EventKind::ORDER_DECISION)
        .expect("orders")
}

#[test]
fn tool_039_untouched_sample_passes() {
    let (dir, log) = sample();
    for kind in [
        EventKind::ORDER_DECISION,
        EventKind::EXEC_DECISION,
        EventKind::CMD_RESULT,
        EventKind::STATE_SNAPSHOT,
    ] {
        assert!(log.recs.iter().any(|r| r.kind == kind), "fixture lacks {kind:?}");
    }
    let r = verify(dir.path(), &log, "unchanged", &[]);
    assert_eq!(r.0, 0, "{r:?}");
    assert_eq!(r.1["divergences"], serde_json::json!([]));
    assert!(r.1["summary"]["outputs_compared"].as_u64().unwrap() > 0);
}

#[test]
fn tool_040_reports_first_divergent_seq() {
    let (dir, mut log) = sample();
    let ix = decision(&log);
    let seq = log.recs[ix].seq;
    // Offset 8 is decided_tsc, a measurement. Change the semantic price field instead.
    log.recs[ix].body[std::mem::offset_of!(OrderDecision, px)] ^= 1;
    let r = verify(dir.path(), &log, "price", &[]);
    assert_eq!(r.0, 1, "{r:?}");
    assert_eq!(r.1["summary"]["first_divergent_seq"], seq);
    let d = &r.1["divergences"][0];
    assert_ne!(d["expected_hex"], d["actual_hex"]);
    assert!(
        d["field_diff"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["field"] == "px")
    );
    let bytes = std::fs::read(r.1["journal_fragment"].as_str().unwrap()).unwrap();
    let mut rest = bytes.as_slice();
    let mut seqs = Vec::new();
    while !rest.is_empty() {
        let record = hft_event::decode_record(rest).unwrap();
        seqs.push(record.hdr.seq);
        rest = &rest[record.hdr.len as usize..];
    }
    let want: Vec<_> = log.recs[..ix]
        .iter()
        .rev()
        .take(200)
        .map(|r| r.seq)
        .rev()
        .collect();
    assert_eq!(seqs, want);
}

#[test]
fn tool_041_masked_fields_ignored() {
    let (dir, log) = sample();
    let snap = log
        .recs
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == EventKind::STATE_SNAPSHOT)
        .nth(1)
        .map(|(i, _)| i)
        .expect("later snapshot");
    for row in 1..=4 {
        let mut changed = log.clone();
        let ix = match row {
            1 => decision(&changed),
            2 | 3 => snap,
            _ => changed
                .recs
                .iter()
                .position(|r| r.kind == EventKind::LATENCY_SAMPLE)
                .unwrap(),
        };
        let offset = match row {
            1 => std::mem::offset_of!(OrderDecision, decided_tsc),
            2 => std::mem::offset_of!(SnapshotHdr, snap_tsc),
            3 => std::mem::offset_of!(SnapshotHdr, engine_ns),
            _ => 0,
        };
        changed.recs[ix].body[offset] ^= 1;
        let r = verify(dir.path(), &changed, &format!("mask-{row}"), &[]);
        assert_eq!(r.0, i32::from(row == 3), "TOOL-041.{row}: {r:?}");
        if row == 3 {
            assert_eq!(r.1["summary"]["first_divergent_seq"], changed.recs[ix].seq);
        }
    }
}

#[test]
fn tool_043_not_self_contained() {
    let (dir, mut log) = sample();
    // Remove the Refdata kind without breaking physical seqs/CRCs or segment framing.
    // Its payload is deliberately left under a different blob kind; no external fallback exists.
    for r in &mut log.recs {
        if r.kind == EventKind::CONFIG_BLOB
            && let Some(mut h) = r.read::<BlobHdr>()
            && h.blob.kind == BlobKind::Refdata
        {
            h.blob.kind = BlobKind::RiskParams;
            r.body[..size_of::<BlobHdr>()].copy_from_slice(h.as_bytes());
        }
    }
    let r = verify(dir.path(), &log, "missing-refdata", &[]);
    assert_eq!(r.0, 1, "{r:?}");
    assert_eq!(r.1["missing_blob"], "Refdata");
    assert!(r.2.contains("journal 不自足") && r.2.contains("Refdata"));
}

#[test]
fn tool_044_seq_range() {
    let (dir, mut log) = sample();
    let ix = decision(&log);
    let seq = log.recs[ix].seq;
    log.recs[ix].body[std::mem::offset_of!(OrderDecision, px)] ^= 1;
    let boot = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let outside = verify(
        dir.path(),
        &log,
        "outside",
        &[
            "--from-seq".into(),
            boot.to_string(),
            "--to-seq".into(),
            (seq - 1).to_string(),
        ],
    );
    assert_eq!(outside.0, 0, "{outside:?}");
    let inside = verify(
        dir.path(),
        &log,
        "inside",
        &["--from-seq".into(), seq.to_string()],
    );
    assert_eq!(inside.0, 1, "{inside:?}");
    assert_eq!(inside.1["summary"]["first_divergent_seq"], seq);
}

#[test]
fn tool_045_decide_hist() {
    let (dir, log) = sample();
    let r = verify(dir.path(), &log, "hist", &["--record-decide-hist".into()]);
    assert_eq!(r.0, 0, "{r:?}");
    let hist = &r.1["decide_hist"];
    assert_eq!(
        hist["count"],
        log.recs
            .iter()
            .filter(|r| r.kind == EventKind::ORDER_DECISION)
            .count()
    );
    assert_eq!(hist["unit"], "ns");
    let values: Vec<_> = ["p50", "p90", "p99", "p99.9", "max"]
        .map(|k| hist[k].as_u64().unwrap())
        .into();
    assert!(values.windows(2).all(|w| w[0] <= w[1]));
    assert!(values[4] > 0);
    let plain = verify(dir.path(), &log, "plain", &[]);
    assert_eq!(plain.0, 0, "{plain:?}");
    assert!(plain.1.get("decide_hist").is_none());
    assert_eq!(
        plain.1["summary"]["outputs_compared"],
        r.1["summary"]["outputs_compared"]
    );
    assert_eq!(plain.1["divergences"], r.1["divergences"]);
}
