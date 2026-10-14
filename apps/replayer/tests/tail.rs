//! M2 journal acceptance through the real file reader, verifier and report formatter.
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};

use hft_event::{Bbo, EventKind, Note, NoteSource, OrderDecision, RecordFlags, Role};
use hft_sequencer::driver::replay::Log;
use hft_strategy_sdk::StrategySet;
use hft_testkit::strategies::{ReplayStrategies, TestStrategies};
use serde_json::Value;
use zerocopy::{FromBytes, IntoBytes};

use crate::fixture;

fn one_hour() -> Arc<fixture::Fixture> {
    static CACHE: Mutex<Weak<fixture::Fixture>> = Mutex::new(Weak::new());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(f) = cache.upgrade() {
        return f;
    }
    let f = Arc::new(fixture::path_model(1));
    *cache = Arc::downgrade(&f);
    f
}

fn verify<S: StrategySet>(path: &Path, extra: &[String]) -> (i32, Value, String) {
    let mut args = crate::args(path);
    args.extend_from_slice(extra);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = replayer::run::<S>(&args, &mut out, &mut err);
    (
        code,
        serde_json::from_slice(&out).unwrap_or(Value::Null),
        String::from_utf8(err).unwrap(),
    )
}

fn passed(r: &(i32, Value, String)) {
    assert_eq!(r.0, 0, "{r:?}");
    assert_eq!(r.1["summary"]["verify"], "pass");
    assert!(r.1["summary"]["first_divergent_seq"].is_null());
    assert!(r.1["divergences"].as_array().unwrap().is_empty());
    assert!(r.1["recovery_diffs"].as_array().unwrap().is_empty());
}

#[test]
fn rpl_010_testkit_1h_consistent() {
    let f = one_hour();
    let log = fixture::read(&f);
    let r = verify::<ReplayStrategies>(f.file.parent().unwrap(), &[]);
    passed(&r);
    let first = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let outputs = log
        .recs
        .iter()
        .filter(|r| r.seq > first && r.role() == Role::Output)
        .count();
    let snapshots = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .count();
    assert_eq!(r.1["summary"]["outputs_compared"], outputs);
    assert_eq!(r.1["summary"]["snapshots_checked"], snapshots);
    assert!(snapshots >= 12);
    assert!(log.seg_start.len() >= 2, "real hourly roll");
    assert!(log.recs.iter().any(|r| r.kind == EventKind::ORDER_EVENT));
    let meta: Value =
        serde_json::from_slice(&std::fs::read(f.root.join("generation.json")).unwrap()).unwrap();
    assert_eq!(meta["engine_ns"], 3_600_000_000_000u64);
}

// Keep the first decision's complete event. The prefix has no later embedded seqs to
// renumber when testing extra/missing Outputs; the exporter assigns and verifies real CRCs.
fn decision_prefix(f: &fixture::Fixture) -> (Log, usize) {
    let mut log = fixture::read(f);
    let ix = log
        .recs
        .iter()
        .enumerate()
        .find(|(_, r)| {
            r.kind == EventKind::ORDER_DECISION
                && r.read::<OrderDecision>().is_some_and(|d| {
                    d.strategy.0 != 0 && d.reject == 0 && d.action == hft_event::OrderAction::New
                })
        })
        .map(|(ix, _)| ix)
        .unwrap();
    let end = (ix..log.recs.len())
        .find(|&i| log.recs[i].flags.contains(RecordFlags::EVENT_END))
        .unwrap();
    assert_eq!(log.seg_of(end), 0);
    log.recs.truncate(end + 1);
    log.seg_start.truncate(1);
    log.segs.truncate(1);
    let seq = log.recs[end].seq;
    log.part_ends.retain(|p| p.1 <= seq);
    log.recovery_gaps.clear();
    (log, ix)
}

fn changed_quote(bit: bool) {
    let f = one_hour();
    let (mut log, ix) = decision_prefix(&f);
    let decision = log.recs[ix].read::<OrderDecision>().unwrap();
    let first = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let b = log.recs[..ix]
        .iter_mut()
        .rev()
        .find(|r| {
            r.kind == EventKind::BBO
                && r.read::<Bbo>()
                    .is_some_and(|b| b.h.inst == decision.inst && b.bid_px == decision.req_px)
        })
        .unwrap();
    let before = b.read::<Bbo>().unwrap();
    assert!(b.seq > first);
    // This fixture's taker prices from the bid. Skip duplicate update IDs whose
    // prices were not consumed. The changed bit survives the instrument's tick rounding.
    let offset = std::mem::offset_of!(Bbo, bid_px);
    if bit {
        let bit = (10usize..16)
            .find(|bit| before.bid_px.0 & (1 << bit) != 0)
            .unwrap();
        b.body[offset + bit / 8] ^= 1 << (bit % 8);
    } else {
        let tick = hft_testkit::dst::world::WorldSpec::w1()
            .inst(decision.inst)
            .unwrap()
            .tick;
        b.body[offset..offset + 8].copy_from_slice(&(before.bid_px.0 - tick).to_le_bytes());
    }
    let dir = fixture::dst::export(&log, &f.root.join(if bit { "rpl021" } else { "rpl013" }));
    let r = verify::<ReplayStrategies>(&dir, &[]);
    assert_eq!(r.0, 1, "{r:?}");
    assert_eq!(r.1["summary"]["start_snapshot_seq"], first);
    assert_eq!(r.1["summary"]["first_divergent_seq"], log.recs[ix].seq, "{r:?}");
    assert_eq!(r.1["divergences"].as_array().unwrap().len(), 1);
    let d = &r.1["divergences"][0];
    assert_eq!(d["nearest_snapshot_seq"], first);
    assert_eq!(d["kind"], EventKind::ORDER_DECISION.0);
    assert!(
        d["field_diff"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["field"] == "px"),
        "{d}"
    );
}

#[test]
fn rpl_013_default_starts_at_first_snapshot() {
    changed_quote(false);
}

#[test]
fn rpl_021_bbo_byte_first_affected_decision() {
    changed_quote(true);
}

#[test]
fn rpl_018_extra_logged_output_at_its_seq() {
    let f = one_hour();
    let (mut log, ix) = decision_prefix(&f);
    let mut extra = log.recs[ix].clone();
    extra.seq = log.recs.last().unwrap().seq + 1;
    extra.kind = EventKind::NOTE;
    extra.flags = RecordFlags::EVENT_END;
    let note = Note {
        code: (NoteSource::Strategy as u32) << 24,
        level: hft_event::NoteLevel::Info,
        source: NoteSource::Strategy,
        strategy: log.recs[ix].read::<OrderDecision>().unwrap().strategy,
        _pad: 0,
        group: hft_types::GroupId(0),
        inst: hft_types::InstId(0),
        v: [0; 8],
    };
    extra.body = note.as_bytes().to_vec();
    let seq = extra.seq;
    log.recs.last_mut().unwrap().flags.0 &= !RecordFlags::EVENT_END.0;
    log.recs.push(extra);
    let dir = fixture::dst::export(&log, &f.root.join("rpl018"));
    let r = verify::<ReplayStrategies>(&dir, &[]);
    assert_eq!(r.0, 1, "{r:?}");
    assert_eq!(r.1["summary"]["first_divergent_seq"], seq);
    assert_eq!(r.1["divergences"][0]["actual_hex"], "");
    assert!(!r.1["divergences"][0]["expected_hex"].as_str().unwrap().is_empty());
}

#[test]
fn rpl_019_missing_logged_output_at_its_seq() {
    let f = one_hour();
    let (mut log, ix) = decision_prefix(&f);
    let removed = log.recs.remove(ix);
    assert!(!removed.flags.contains(RecordFlags::EVENT_END));
    for r in &mut log.recs[ix..] {
        r.seq -= 1;
    }
    let dir = fixture::dst::export(&log, &f.root.join("rpl019"));
    let r = verify::<ReplayStrategies>(&dir, &[]);
    assert_eq!(r.0, 1, "{r:?}");
    assert_eq!(r.1["summary"]["first_divergent_seq"], removed.seq);
    assert!(
        r.1["divergences"][0]["actual_hex"].as_str().unwrap().starts_with(
            &EventKind::ORDER_DECISION
                .0
                .to_le_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    );
}

#[test]
fn rpl_222_chain_gap_hash_and_valid_suffix() {
    use hft_strategy_registry::Strategies;
    let f = fixture::long_restart::<Strategies>(fixture::Restart::FromSnapshot);
    let dir = f.file.parent().unwrap();
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jrn"))
        .collect();
    files.sort();
    assert_eq!(files.len(), 3);
    passed(&verify::<Strategies>(dir, &[]));
    let suffix = f.root.join("suffix");
    std::fs::create_dir(&suffix).unwrap();
    for file in &files[1..] {
        for path in [file.clone(), file.with_extension("seal")] {
            std::fs::copy(&path, suffix.join(path.file_name().unwrap())).unwrap();
        }
    }
    passed(&verify::<Strategies>(&suffix, &[]));
    let saved = f.root.join("middle.jrn");
    std::fs::rename(&files[1], &saved).unwrap();
    let r = verify::<Strategies>(dir, &[]);
    assert_eq!(r.0, 3, "{r:?}");
    assert!(r.1.is_null(), "preflight must not compare Outputs");
    std::fs::rename(&saved, &files[1]).unwrap();
    let mut bytes = std::fs::read(&files[2]).unwrap();
    let (mut header, _) = hft_journal::SegmentHeaderFixed::read_from_prefix(&bytes).unwrap();
    header.prev_header_sha256[0] ^= 1;
    header.header_crc32c = header.compute_crc();
    bytes[..header.as_bytes().len()].copy_from_slice(header.as_bytes());
    std::fs::write(&files[2], bytes).unwrap();
    fixture::seal(&files[2]);
    assert_eq!(verify::<Strategies>(dir, &[]).0, 3);
}

fn scenario_files(scenario: hft_testkit::dst::Scenario, label: &str, recover: bool) {
    use hft_testkit::dst::faults::{CrashFault, Fault};
    use hft_testkit::dst::{CrashPoint, RerunPolicy};
    use hft_testkit::scenarios::{SeqEngine, suites};
    let mut cfg = scenario.config(&suites(), 1).unwrap();
    cfg.world.physical_journal = true;
    cfg.rerun = RerunPolicy::Always;
    cfg.rebuild_every = 500;
    if recover {
        cfg.scripted.push((
            0,
            Fault::Crash(CrashFault {
                at_step: 250,
                repeat: 1,
                point: CrashPoint::SeqInput,
            }),
        ));
    }
    // Recovery changes timing of the source scenario's coverage assertions; the full
    // invariant/replay checks still run, and required journal evidence is checked below.
    let record = if recover {
        hft_testkit::dst::run_record::<SeqEngine>(&cfg)
    } else {
        scenario.run_config::<SeqEngine>(&cfg)
    };
    assert!(record.outcome.is_pass(), "{:?}", record.outcome);
    assert!(record.snapshot_checks > 0);
    let log = record.physical_journal.as_ref().unwrap();
    assert!(log.recs.iter().any(|r| r.kind == EventKind::ORDER_DECISION));
    assert!(log.recs.iter().any(|r| r.kind == EventKind::ORDER_EVENT));
    if recover {
        assert!(log.segs.len() > 1);
    }
    if label == "exec" {
        assert!(log.recs.iter().any(|r| r.kind == EventKind::EXEC_DECISION));
    }
    let owner = fixture::create::<hft_strategy_registry::Strategies>();
    let dir = fixture::dst::export(log, &owner.root.join(label));
    let result = verify::<TestStrategies>(&dir, &[]);
    passed(&result);
    assert!(result.1["summary"]["outputs_compared"].as_u64().unwrap() > 0);
    if recover {
        assert!(result.1["summary"]["runs"].as_array().unwrap().len() > 1);
    }
}

#[test]
fn oms_410_replay_verify() {
    scenario_files(
        hft_testkit::scenarios::oms::oms_400_faults_reconcile_zero(),
        "oms",
        false,
    );
    scenario_files(
        hft_testkit::scenarios::oms::oms_400_faults_reconcile_zero(),
        "oms-recover",
        true,
    );
}

#[test]
fn exe_464_replay_verify() {
    scenario_files(
        hft_testkit::scenarios::exec::exe_500_pair_open_partial_fills(),
        "exec",
        false,
    );
    scenario_files(
        hft_testkit::scenarios::exec::exe_505_common_checker(),
        "exec",
        true,
    );
}
