//! The full-duration restart cases. Time advances through journaled Inputs; files are made by
//! the real sequencer and offline writer, and verified by the actual replayer executable.
use std::path::{Path, PathBuf};

use hft_event::{Bbo, EngineClock, EventKind, HostProfile, Note, NoteSource, Role, SnapshotHdr};
use hft_journal::RollReason;
use hft_sequencer::driver::replay::Log;
use hft_strategy_registry::Strategies;
use serde_json::Value;

use super::{args, fixture, run};

fn verify(path: &Path, keep_going: bool, expected: i32) -> Value {
    let mut a = args(path);
    if keep_going {
        a.push("--keep-going".into());
    }
    let (code, text, err) = run(&a);
    assert_eq!(code, expected, "{text} {err}");
    serde_json::from_str(&text).unwrap()
}

fn snapshots(log: &Log, from: usize, to: usize) -> Vec<(u64, SnapshotHdr)> {
    log.recs[from..to]
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .map(|r| (r.seq, r.read::<SnapshotHdr>().unwrap()))
        .collect()
}

fn sources(dir: &Path, log: &Log, restart: RollReason) -> Vec<(PathBuf, Vec<u8>)> {
    let rolls: Vec<_> = log
        .seg_start
        .iter()
        .map(|i| log.recs[*i].read::<hft_event::SegmentRoll>().unwrap())
        .collect();
    // UTC0 is 00:30. The new execution crosses 01:00, so it contains an ordinary
    // hourly roll as well as its recovery boot. Three files still describe two runs.
    assert_eq!(
        rolls.iter().map(|r| r.reason).collect::<Vec<_>>(),
        vec![RollReason::Startup as u8, restart as u8, RollReason::Hourly as u8]
    );
    rolls
        .iter()
        .flat_map(|r| {
            let file = dir.join(format!("{:010}.jrn", r.segment_no));
            [file.clone(), file.with_extension("seal")]
        })
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            (path, bytes)
        })
        .collect()
}

fn unchanged(sources: &[(PathBuf, Vec<u8>)]) {
    for (path, bytes) in sources {
        assert_eq!(std::fs::read(path).unwrap(), *bytes, "{}", path.display());
    }
}

fn verify_suffix(root: &Path, sources: &[(PathBuf, Vec<u8>)]) -> Value {
    let dir = root.join("restart-only");
    std::fs::create_dir(&dir).unwrap();
    let mut copied = Vec::new();
    // Omit the first run's .jrn and .seal. Retain both files of the restarted run.
    for (path, bytes) in sources.iter().skip(2) {
        let dest = dir.join(path.file_name().unwrap());
        std::fs::write(&dest, bytes).unwrap();
        copied.push((dest, bytes.clone()));
    }
    let result = verify(&dir, false, 0);
    unchanged(&copied);
    result
}

fn assert_run(v: &Value, ix: usize, snapshot: u64, minutes: u64) {
    let run = &v["summary"]["runs"][ix];
    assert_eq!(run["start_snapshot_seq"], snapshot);
    assert!(run["inputs_replayed"].as_u64().unwrap() >= minutes * 6);
    assert!(run["outputs_compared"].as_u64().unwrap() > 0);
    assert!(run["snapshots_checked"].as_u64().unwrap() > minutes / 5);
    assert_eq!(run["mismatches"], 0);
    assert_eq!(run["recovery_diffs"], 0);
}

// Recompute both transport checks after changing an Input. Headers and seqs are unchanged,
// so the next segment's previous-header chain hash remains valid.
fn change_bbo(path: &Path, seq: u64, tick: i64) {
    let mut bytes = std::fs::read(path).unwrap();
    let mut offset = 4096;
    loop {
        let h = hft_event::decode_record(&bytes[offset..]).unwrap().hdr;
        if h.seq == seq {
            assert_eq!(h.kind, EventKind::BBO.0);
            let at = offset + 24 + std::mem::offset_of!(Bbo, bid_px);
            let px = i64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
            bytes[at..at + 8].copy_from_slice(&(px + tick).to_le_bytes());
            let end = offset + h.len as usize;
            // Bbo normally has CrcPolicy::Never in a seq journal. Preserve the actual
            // transport flags, and only rewrite a CRC when that record has one.
            if hft_event::RecordFlags(h.flags).contains(hft_event::RecordFlags::CRC) {
                let crc = crc32c::crc32c(&bytes[offset..end - 4]);
                bytes[end - 4..end].copy_from_slice(&crc.to_le_bytes());
            }
            hft_event::decode_record(&bytes[offset..end]).unwrap();
            break;
        }
        offset += h.len as usize;
    }
    std::fs::write(path, &bytes).unwrap();
    fixture::seal(path);
}

#[test]
fn rpl_025_restart_at_recover_from_snapshot() {
    let f = fixture::long_restart::<Strategies>(fixture::Restart::FromSnapshot);
    let dir = f.file.parent().unwrap();
    let log = fixture::read(&f);
    let original = sources(dir, &log, RollReason::RecoverFromSnapshot);
    let boundary = log.seg_start[1];
    let old = snapshots(&log, 0, boundary);
    let new = snapshots(&log, boundary, log.recs.len());
    assert_eq!(old.last().unwrap().1.engine_ns, 1_200_000_000_000);
    assert_eq!(new[0].1.engine_ns, 600_000_000_000);
    assert_eq!(new.last().unwrap().1.engine_ns, 1_800_000_000_000);
    let boot_done = log.recs[boundary..]
        .iter()
        .position(|r| {
            r.kind == EventKind::SEGMENT_ROLL
                && r.read::<hft_event::SegmentRoll>().unwrap().mark == hft_journal::RollMark::BootDone as u8
        })
        .unwrap()
        + boundary;
    let notes: Vec<_> = log.recs[boundary..boot_done]
        .iter()
        .filter(|r| r.kind == EventKind::NOTE)
        .map(|r| (r, r.read::<Note>().unwrap()))
        .filter(|(_, n)| {
            n.code
                == hft_event::note_code(
                    NoteSource::Boot,
                    hft_sequencer::codes::boot_note::RECOVER_FROM_SNAPSHOT,
                )
        })
        .collect();
    assert_eq!(notes.len(), 1, "missing or duplicated recovery evidence");
    let (record, note) = notes[0];
    assert_eq!(record.role(), Role::Meta);
    assert_eq!(note.level, hft_event::NoteLevel::P1);
    let selected = log.recs.iter().position(|r| r.seq == note.v[0]).unwrap();
    assert_eq!(
        log.recs[selected].read::<SnapshotHdr>().unwrap().engine_ns,
        600_000_000_000
    );
    assert_eq!(note.v[1], log.record_end_seq(selected).unwrap() + 1);
    assert!(note.v[1] < note.v[2]);
    // Exclude the End appended by the real roll, and independently find the published event.
    let old_committed = log.recs[..boundary - 1]
        .iter()
        .rposition(|r| r.flags.contains(hft_event::RecordFlags::EVENT_END))
        .unwrap();
    assert_eq!(note.v[2], log.record_end_seq(old_committed).unwrap());
    assert_eq!(note.v[3], 3);
    assert_eq!(note.v[4].to_le_bytes(), *b"cli:syst");
    let clocks: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::ENGINE_CLOCK)
        .collect();
    assert_eq!(clocks.len(), 2);
    let last_old_input = log.recs[..boundary]
        .iter()
        .rev()
        .find(|r| r.role() == Role::Input)
        .unwrap();
    assert!(clocks[1].tsc < last_old_input.tsc);
    assert_eq!(
        clocks[1].read::<EngineClock>().unwrap().tsc_hz_milli,
        log.segs[0].tsc_hz_milli * 2
    );
    assert_eq!(clocks[1].read::<EngineClock>().unwrap().source, 3);
    assert_eq!(
        clocks[1].read::<EngineClock>().unwrap().base_engine_ns,
        600_000_000_000
    );
    let baseline = verify(dir, false, 0);
    assert_eq!(baseline["summary"]["runs"].as_array().unwrap().len(), 2);
    assert_run(&baseline, 0, old[0].0, 20);
    assert_run(&baseline, 1, new[0].0, 20);
    unchanged(&original);
    let changed_record = log.recs[..boundary]
        .iter()
        .rev()
        .find(|r| r.kind == EventKind::BBO)
        .unwrap();
    let changed_seq = changed_record.seq;
    let inst = changed_record.read::<Bbo>().unwrap().h.inst;
    let tick = hft_testkit::dst::world::WorldSpec::w1()
        .insts
        .iter()
        .find(|i| i.id == inst)
        .unwrap()
        .tick;
    assert!((note.v[1]..=note.v[2]).contains(&changed_seq));
    change_bbo(&f.file, changed_seq, tick);
    let changed = sources(dir, &log, RollReason::RecoverFromSnapshot);
    let result = verify(dir, true, 1);
    assert_eq!(result["summary"]["runs"].as_array().unwrap().len(), 2);
    assert!(result["summary"]["runs"][0]["mismatches"].as_u64().unwrap() > 0);
    assert_eq!(result["summary"]["runs"][1], baseline["summary"]["runs"][1]);
    let suffix = verify_suffix(&f.root, &original);
    assert_eq!(suffix["summary"]["runs"].as_array().unwrap().len(), 1);
    assert_eq!(suffix["summary"]["runs"][0], baseline["summary"]["runs"][1]);
    unchanged(&changed);
    unchanged(&original[2..]);
}

#[test]
fn rpl_026_restart_at_version_change() {
    let f = fixture::long_restart::<Strategies>(fixture::Restart::Version);
    let dir = f.file.parent().unwrap();
    let log = fixture::read(&f);
    let original = sources(dir, &log, RollReason::Recovery);
    let boundary = log.seg_start[1];
    let old = snapshots(&log, 0, boundary);
    let new = snapshots(&log, boundary, log.recs.len());
    assert_eq!(
        old.last().unwrap().1.purpose,
        hft_sequencer::snapshot::PURPOSE_DRAIN
    );
    assert_eq!(old.last().unwrap().1.engine_ns, 1_200_000_000_000);
    assert_eq!(new[0].1.engine_ns, 1_200_000_000_000);
    assert_eq!(new.last().unwrap().1.engine_ns, 1_800_000_000_000);
    let builds: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::HOST_PROFILE)
        .map(|r| r.read::<HostProfile>().unwrap().build_git)
        .collect();
    assert_eq!(builds, vec![[0; 20], [1; 20], [1; 20]]);
    let result = verify(dir, false, 0);
    assert_eq!(result["summary"]["runs"].as_array().unwrap().len(), 2);
    assert_run(&result, 0, old[0].0, 20);
    assert_run(&result, 1, new[0].0, 10);
    let suffix = verify_suffix(&f.root, &original);
    assert_eq!(suffix["summary"]["runs"].as_array().unwrap().len(), 1);
    assert_eq!(suffix["summary"]["runs"][0], result["summary"]["runs"][1]);
    unchanged(&original);
}
