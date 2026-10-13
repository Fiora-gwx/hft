//! The real writer/reader path, including multipart snapshots and physical sequence numbers.
use crate::common::*;
use hft_event::{BlobRef, ClockStatus, EventKind, FeedCmd, GwCmd, HostProfile, RecordFlags, Role};
use hft_journal::{
    EndAt, InstanceName, JournalConfig, JournalKind, JournalReader, JournalWriter, PageSize, ReaderConfig,
};
use hft_sequencer::driver::RollInfo;
use hft_sequencer::driver::live::{LiveDriver, LiveIo, RingId};
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::driver::replay_io::{LogReadError, LogReadLimits};
use hft_sequencer::driver::replay_snapshot::{SnapshotCheckError, check_snapshot};
use hft_sequencer::{
    BootSink, BootSource, EngineSetup, InputSlot, JournalSink, MemJournal, OutRing, ParsedConfig, RollMeta,
    Sequencer, SnapshotSel, StepOutcome,
};
use hft_types::{HostClass, InstId, KillBits, Tsc, VenueId};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("hft-seq-file-{}-{n}", std::process::id()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct FileIo {
    io: TestIo<TestSet>,
    writer: PairedJournal,
}
impl LiveIo<TestSet> for FileIo {
    type J = PairedJournal;
    fn journal(&mut self) -> &mut Self::J {
        &mut self.writer
    }
    fn tsc(&mut self) -> Tsc {
        self.io.tsc()
    }
    fn n_rep(&self) -> u8 {
        self.io.n_rep()
    }
    fn n_bbo(&self) -> u8 {
        self.io.n_bbo()
    }
    fn n_md(&self) -> u8 {
        self.io.n_md()
    }
    fn len(&mut self, r: RingId) -> usize {
        self.io.len(r)
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        self.io.peek_len(r)
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        self.io.peek_inst(r)
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        self.io.pop(r, out)
    }
    fn push_out(&mut self, ring: OutRing, c: &GwCmd) -> bool {
        self.io.push_out(ring, c)
    }
    fn push_feed(&mut self, v: VenueId, c: &FeedCmd) -> bool {
        self.io.push_feed(v, c)
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<TestSet>> {
        self.io.take_parsed(r)
    }
    fn retire(&mut self, p: ParsedConfig<TestSet>) {
        self.io.retire(p);
    }
    fn write_kill(&mut self, bits: &[KillBits]) {
        self.io.write_kill(bits);
    }
    fn roll_info(&mut self) -> RollInfo {
        self.io.roll_info()
    }
    fn host_profile(&self) -> HostProfile {
        self.io.host_profile()
    }
    fn clock_status(&self) -> ClockStatus {
        self.io.clock_status()
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        self.io.utc_of(t)
    }
}

// Every seq is checked at append time, before the engine can embed it in another record.
struct PairedJournal {
    file: JournalWriter,
    mem: MemJournal,
}
struct PairedBoot<'a> {
    file: hft_journal::BootWriter<'a>,
    mem: hft_sequencer::journal::MemBoot<'a>,
}
impl BootSink for PairedBoot<'_> {
    fn append_var(
        &mut self,
        kind: EventKind,
        tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<hft_types::Seq, hft_journal::AppendErr> {
        let seq = self.file.append_var(kind, tsc, pieces)?;
        assert_eq!(self.mem.append_var(kind, tsc, pieces)?, seq);
        Ok(seq)
    }
    fn finish(self) -> Result<hft_types::Seq, hft_journal::AppendErr> {
        let seq = self.file.finish()?;
        assert_eq!(self.mem.finish()?, seq);
        Ok(seq)
    }
}
impl JournalSink for PairedJournal {
    type Boot<'a> = PairedBoot<'a>;
    fn append_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<hft_types::Seq, hft_journal::AppendErr> {
        let seq = self.file.append_var(kind, flags, tsc, pieces)?;
        assert_eq!(self.mem.append_var(kind, flags, tsc, pieces)?, seq);
        Ok(seq)
    }
    fn end_event(&mut self) {
        self.file.end_event();
        self.mem.end_event();
    }
    fn publish(&mut self) {
        self.file.publish();
        self.mem.publish();
    }
    fn roll(
        &mut self,
        reason: hft_journal::RollReason,
        info: &RollMeta<'_>,
    ) -> Result<PairedBoot<'_>, hft_journal::RollErr> {
        Ok(PairedBoot {
            file: JournalSink::roll(&mut self.file, reason, info)?,
            mem: self.mem.roll(reason, info)?,
        })
    }
    fn needs_roll(&self, now: Tsc) -> Option<(hft_journal::RollReason, hft_journal::RollAt)> {
        self.file.needs_roll(now)
    }
}

type FileSeq = Sequencer<LiveDriver<TestSet, FileIo>, TestSet>;
fn run(s: &mut FileSeq) {
    for _ in 0..10_000 {
        match s.step() {
            StepOutcome::Busy => {}
            StepOutcome::Idle => return,
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("live input did not finish");
}
fn limits() -> LogReadLimits {
    LogReadLimits {
        records: 50_000,
        body_bytes: 64 << 20,
    }
}
fn reader_cfg(t: &Temp) -> ReaderConfig {
    let mut c = ReaderConfig::new(
        t.0.join("out"),
        t.0.join("absent"),
        InstanceName::Prod,
        "seq".into(),
    );
    c.end = EndAt::LastCompleteEvent;
    c.reassembly_cap = 16 << 20;
    c
}
fn fixture() -> Temp {
    paired_fixture().0
}
fn paired_fixture() -> (Temp, MemJournal) {
    reset_script();
    let mut source = b0().boot();
    source.steady();
    source.cmd(AC1, &hft_event::ControlCmd::SnapshotNow);
    source.run();
    source.run();
    source.quote(A1, "50001", "50002");
    source.run();
    source.keyframe();
    source.sys(2); // segment roll
    source.keyframe();
    let inputs: Vec<_> = source
        .j()
        .records()
        .filter(|r| r.role() == Role::Input && r.kind != EventKind::ENGINE_CLOCK)
        .cloned()
        .collect();
    drop(source);
    reset_script();
    let t = Temp::new();
    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: t.0.join("hot-unused"),
        cold_root: t.0.join("cold-unused"),
        segment_bytes: 64 << 20,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 1,
    };
    let writer = JournalWriter::open_offline(&cfg, &t.0.join("out")).unwrap();
    let b = b0();
    let rd = b.rd.load();
    let mut setup = b.setup(&rd);
    // Counter rows retain every slot generation, even for free slots. Two tables alone now
    // exceed PART_MAX_CHUNK, so multipart coverage cannot disappear with other state changes.
    setup.oms.slots = hft_oms::PerTable::new(4096);
    // The startup invariant reserves timers for every order slot (24,698 for this fixture).
    setup.caps.max_timers = 32_768;
    let mut configs = b.configs(&rd);
    configs.engine_blob = setup.encode_blob(1, HostClass::Dev2, UTC0);
    configs.risk_blob = configs.risk.encode_blob().unwrap();
    configs.risk_sha256 = sha(&configs.risk_blob);
    let io = FileIo {
        io: TestIo::new(setup.clock),
        writer: PairedJournal {
            file: writer,
            mem: MemJournal::with_physical_seq(setup.clock),
        },
    };
    let mut s = Sequencer::boot(
        BootSource::Empty { configs },
        &setup,
        LiveDriver::new(io, b.live.clone()),
        hb(),
    )
    .unwrap();
    run(&mut s);
    for r in inputs {
        let io = &mut s.driver_mut().io.io;
        io.tsc = r.tsc;
        let ring = if r.kind == EventKind::BBO {
            RingId::Bbo(0)
        } else {
            RingId::Ctl
        };
        io.ring(ring).push_back(Elem {
            kind: r.kind,
            flags: RecordFlags(0),
            body: r.body,
        });
        run(&mut s);
    }
    let mut driver = s.into_driver();
    driver.io.writer.publish();
    let PairedJournal { file, mem } = driver.io.writer;
    file.seal_and_close().unwrap();
    (t, mem)
}

#[test]
fn seq_422_memory_physical_sequences_match_real_writer_and_reader() {
    let (t, mem) = paired_fixture();
    let mut reader = JournalReader::open(&reader_cfg(&t)).unwrap();
    let file = Log::from_reader(&mut reader, limits()).unwrap();
    let memory = Log::from_segments(&mem.segments);
    assert_eq!(file.recs.len(), memory.recs.len() + 1, "file adds final End");
    assert_eq!(file.part_ends, memory.part_ends);
    assert!(!memory.part_ends.is_empty());
    assert_eq!(file.seg_start, memory.seg_start);
    assert_eq!(file.segs, memory.segs);
    let wire_flags = RecordFlags::CRC.0 | RecordFlags::PART.0;
    for (ix, (a, b)) in file.recs.iter().zip(&memory.recs).enumerate() {
        assert_eq!((a.seq, a.kind, a.tsc), (b.seq, b.kind, b.tsc));
        assert_eq!(a.body, b.body, "logical body at seq {}", a.seq);
        assert_eq!(a.flags.0 & !wire_flags, b.flags.0 & !wire_flags, "seq {}", a.seq);
        assert_eq!(file.record_end_seq(ix), memory.record_end_seq(ix));
        assert_eq!(mem.record_end_seq(b.seq), file.record_end_seq(ix).unwrap());
    }
    for s in &mem.segments {
        let done = s
            .records
            .iter()
            .find(|r| {
                r.kind == EventKind::SEGMENT_ROLL && r.read::<hft_event::SegmentRoll>().unwrap().mark == 3
            })
            .unwrap();
        let count = done.seq - s.records[0].seq + 1;
        assert_eq!(
            u64::from(done.read::<hft_event::SegmentRoll>().unwrap().boot_records),
            count
        );
        assert!(s.records.iter().take_while(|r| r.seq <= done.seq).count() < count as usize);
    }
    verify_snapshots(&memory);
    verify_snapshots(&file);
}

#[test]
fn seq_385_file_journal_independent_verify() {
    let t = fixture();
    let cfg = reader_cfg(&t);
    let mut reader = JournalReader::open(&cfg).unwrap();
    let log = Log::from_reader(&mut reader, limits()).unwrap();
    assert!(
        log.recs
            .iter()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .count()
            >= 4
    );
    assert_eq!(log.seg_start.len(), 2);
    assert!(
        log.recs.windows(2).any(|p| p[1].seq > p[0].seq + 1),
        "multipart physical seq was lost"
    );
    assert!(
        log.recs
            .iter()
            .any(|r| r.kind == EventKind::STATE_SNAPSHOT && r.body.len() > hft_event::PART_MAX_CHUNK)
    );
    assert!(log.recs.iter().any(|r| r.kind == EventKind::CMD_RESULT));
    assert!(!log.part_ends.is_empty());
    for (ix, records) in log.recs.windows(2).enumerate() {
        assert_eq!(log.record_end_seq(ix), Some(records[1].seq - 1));
    }
    let tail = log.recs.len() - 1;
    assert_eq!(log.record_end_seq(tail), Some(log.recs[tail].seq));
    assert_eq!(log.record_end_seq(tail + 1), None);
    verify_snapshots(&log);
}

fn verify_snapshots(log: &Log) {
    let snapshots: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .map(|r| r.seq)
        .collect();
    assert!(!snapshots.is_empty());
    for at in snapshots {
        let checked = check_snapshot::<TestSet>(log, at).unwrap_or_else(|e| panic!("snapshot {at}: {e:?}"));
        assert_ne!(checked.state_hash, [0; 16]);
        let setup = EngineSetup::from_snapshot(log, at).unwrap();
        let driver = ReplayDriver::new(ReplayMode::Verify, log.clone(), 0, setup.seq.replay_per_round);
        let mut s: Sequencer<ReplayDriver, TestSet> = Sequencer::boot(
            BootSource::Journal(plan(SnapshotSel::AtSeq(at), true, 0, 0)),
            &setup,
            driver,
            hb(),
        )
        .unwrap();
        for _ in 0..100_000 {
            if s.step() != StepOutcome::Busy {
                break;
            }
        }
        let d = s.driver();
        let detail: Vec<_> = d
            .sink
            .detail
            .iter()
            .map(|(seq, a, b)| (*seq, a.len(), b.len(), a.iter().zip(b).position(|(x, y)| x != y)))
            .collect();
        assert!(d.finished(), "snapshot {at}: replay unfinished");
        assert!(
            d.mismatches().is_empty(),
            "snapshot {at}: {:?}, {detail:?}",
            d.mismatches()
        );
        assert!(d.leftover().is_empty(), "snapshot {at}: {:?}", d.leftover());
    }
}

#[test]
fn seq_390_original_file_order_and_selected_segment_verify() {
    let t = fixture();
    let mut original = JournalReader::open(&reader_cfg(&t)).unwrap();
    let expected = Log::from_reader(&mut original, limits()).unwrap();
    drop(original);
    let mut renamed = Vec::new();
    let mut before = Vec::new();
    for (i, name) in ["z-first.data", "a-second.data"].iter().enumerate() {
        let p = t.0.join(name);
        // Move the source, leaving no canonical path for a hidden fallback or directory scan.
        std::fs::rename(t.0.join(format!("out/prod/seq/{:010}.jrn", i + 1)), &p).unwrap();
        before.push(std::fs::read(&p).unwrap());
        renamed.push(p);
    }
    for paths in [
        vec![renamed[1].clone(), renamed[0].clone()],
        vec![renamed[1].clone()],
    ] {
        let mut reader = JournalReader::open_files(&paths, 16 << 20).unwrap();
        let log = Log::from_reader(&mut reader, limits()).unwrap();
        let start = if paths.len() == 2 {
            0
        } else {
            expected.seg_start[1]
        };
        assert_eq!(log.recs, expected.recs[start..]);
        assert_eq!(log.segs, expected.segs[if paths.len() == 2 { 0 } else { 1 }..]);
        if paths.len() == 2 {
            assert_eq!(log.seg_start, expected.seg_start);
        } else {
            assert_eq!(log.seg_start, vec![0]);
        }
        verify_snapshots(&log);
    }
    for (i, p) in renamed.iter().enumerate() {
        assert_eq!(std::fs::read(p).unwrap(), before[i]);
    }
}

#[test]
fn seq_386_file_read_failures_do_not_return_partial_log() {
    let t = fixture();
    let cfg = reader_cfg(&t);
    for lim in [
        LogReadLimits {
            records: 1,
            ..limits()
        },
        LogReadLimits {
            body_bytes: 1,
            ..limits()
        },
    ] {
        let mut r = JournalReader::open(&cfg).unwrap();
        assert!(matches!(Log::from_reader(&mut r, lim), Err(LogReadError::Limit)));
    }
    let mut small = cfg.clone();
    small.reassembly_cap = 32;
    let mut r = JournalReader::open(&small).unwrap();
    assert!(matches!(
        Log::from_reader(&mut r, limits()),
        Err(LogReadError::Read(hft_journal::ReadErr::ReassemblyCap))
    ));
    // A corrupt first segment must not masquerade as a successfully read journal.
    let path = t.0.join("out/prod/seq/0000000001.jrn");
    let before = std::fs::read(&path).unwrap();
    let mut corrupt = before.clone();
    corrupt[0] ^= 1;
    std::fs::write(&path, &corrupt).unwrap();
    if let Ok(mut r) = JournalReader::open(&cfg) {
        assert!(Log::from_reader(&mut r, limits()).is_err());
    }
    std::fs::write(&path, &before).unwrap();
    let mut r = JournalReader::open(&cfg).unwrap();
    assert!(Log::from_reader(&mut r, limits()).is_ok());
    assert_eq!(
        std::fs::read(path).unwrap(),
        before,
        "reading modified the source"
    );
}

#[test]
fn seq_391_offline_runner_snapshot_and_window() {
    use hft_sequencer::driver::verify::{VerifyOptions, verify};
    let t = fixture();
    let mut reader = JournalReader::open(&reader_cfg(&t)).unwrap();
    let log = Log::from_reader(&mut reader, limits()).unwrap();
    let snaps: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .map(|r| r.seq)
        .collect();
    let got = verify::<TestSet>(log.clone(), VerifyOptions::default(), hb()).unwrap();
    assert_eq!(got.start_snapshot, snaps[0]);
    assert_eq!(got.runs.len(), 1, "ordinary roll restarted the engine");
    assert!(got.sink.mismatches.is_empty(), "{:?}", got.sink.detail);
    assert_eq!(got.snapshots, snaps.len() as u64);
    assert!(got.inputs > 0 && got.sink.compared > 0);
    for at in snaps {
        let got = verify::<TestSet>(
            log.clone(),
            VerifyOptions {
                from_seq: Some(at),
                ..Default::default()
            },
            hb(),
        )
        .unwrap();
        assert_eq!(got.start_snapshot, at);
        assert!(got.sink.mismatches.is_empty());
    }
    let ix = log
        .recs
        .iter()
        .position(|r| r.kind == EventKind::CMD_RESULT)
        .unwrap();
    let at = log.recs[ix].seq;
    let mut bad = log.clone();
    bad.recs[ix].body[0] ^= 1;
    let got = verify::<TestSet>(bad.clone(), VerifyOptions::default(), hb()).unwrap();
    assert_eq!(got.sink.mismatches.len(), 1);
    assert_eq!(got.sink.mismatches[0].seq, at);
    assert!(got.sink.halted);
    assert_eq!(got.sink.detail.len(), 1);
    let before = verify::<TestSet>(
        bad.clone(),
        VerifyOptions {
            to_seq: Some(at - 1),
            ..Default::default()
        },
        hb(),
    )
    .unwrap();
    assert!(before.sink.mismatches.is_empty());
    let after = verify::<TestSet>(
        bad.clone(),
        VerifyOptions {
            from_seq: Some(at + 1),
            ..Default::default()
        },
        hb(),
    )
    .unwrap();
    assert!(after.sink.mismatches.is_empty());
    let inside = verify::<TestSet>(
        bad,
        VerifyOptions {
            from_seq: Some(at),
            to_seq: Some(at),
            ..Default::default()
        },
        hb(),
    )
    .unwrap();
    assert_eq!(inside.sink.mismatches[0].seq, at);
}

#[test]
fn seq_393_runner_snapshot_declarations_and_failures() {
    use hft_sequencer::driver::verify::{VerifyError, VerifyOptions, verify};
    let t = fixture();
    let mut reader = JournalReader::open(&reader_cfg(&t)).unwrap();
    let log = Log::from_reader(&mut reader, limits()).unwrap();
    let ix: Vec<_> = log
        .recs
        .iter()
        .enumerate()
        .filter_map(|(i, r)| (r.kind == EventKind::STATE_SNAPSHOT).then_some(i))
        .collect();
    for zero in [false, true] {
        let mut changed = log.clone();
        for i in &ix {
            rehash_snapshot(&mut changed.recs[*i].body);
            if zero {
                changed.recs[*i].body[16..32].fill(0);
            }
        }
        assert!(
            verify::<TestSet>(changed, VerifyOptions::default(), hb())
                .unwrap()
                .sink
                .mismatches
                .is_empty()
        );
    }
    let mut bad = log.clone();
    bad.recs[ix[0]].body[16] ^= 1;
    assert!(matches!(
        verify::<TestSet>(bad, VerifyOptions::default(), hb()),
        Err(VerifyError::Snapshot(_))
    ));
    let mut bad = log.clone();
    rehash_snapshot(&mut bad.recs[ix[1]].body);
    bad.recs[ix[1]].body[16] ^= 1;
    let got = verify::<TestSet>(bad, VerifyOptions::default(), hb()).unwrap();
    assert_eq!(got.sink.mismatches[0].seq, log.recs[ix[1]].seq);
    assert!(matches!(
        verify::<TestSet>(
            log.clone(),
            VerifyOptions {
                from_seq: Some(log.recs[0].seq),
                ..Default::default()
            },
            hb()
        ),
        Err(VerifyError::NoSnapshot)
    ));
    let mut bad = log.clone();
    bad.recs[ix[1]].body[0] = hft_sequencer::snapshot::PURPOSE_RECOVERY;
    assert!(matches!(
        verify::<TestSet>(bad, VerifyOptions::default(), hb()),
        Err(VerifyError::RunBoundary { .. })
    ));
    let mut bad = log;
    for r in &mut bad.recs {
        if r.kind == EventKind::CMD_RESULT {
            r.body[0] ^= 1;
        }
    }
    let got = verify::<TestSet>(
        bad,
        VerifyOptions {
            keep_going: true,
            ..Default::default()
        },
        hb(),
    )
    .unwrap();
    assert!(!got.sink.halted);
    assert!(got.sink.mismatches.len() >= 2);
    assert_eq!(got.sink.detail.len(), got.sink.mismatches.len());
}

#[test]
fn seq_394_invalid_run_boundaries_fail_before_replay() {
    use hft_event::{Note, SegmentRoll};
    use hft_journal::RollMark;
    use hft_sequencer::driver::verify::{VerifyError, VerifyOptions, verify};
    use zerocopy::IntoBytes;

    let t = fixture();
    let mut reader = JournalReader::open(&reader_cfg(&t)).unwrap();
    let log = Log::from_reader(&mut reader, limits()).unwrap();
    let start = log.seg_start[1];
    let find = |kind| start + log.recs[start..].iter().position(|r| r.kind == kind).unwrap();
    let host = find(EventKind::HOST_PROFILE);
    let clock = find(EventKind::CLOCK_STATUS);
    let snap = find(EventKind::STATE_SNAPSHOT);
    let done = start
        + log.recs[start..]
            .iter()
            .position(|r| {
                r.kind == EventKind::SEGMENT_ROLL
                    && r.read::<SegmentRoll>().unwrap().mark == RollMark::BootDone as u8
            })
            .unwrap();
    assert_ne!(
        log.recs[done + 1..]
            .iter()
            .find(|r| r.role() == Role::Input)
            .unwrap()
            .kind,
        EventKind::ENGINE_CLOCK
    );
    for row in 0..8 {
        let mut bad = log.clone();
        match row {
            0 => bad.recs[start].kind = EventKind::NOTE,
            1 => bad.recs[done].kind = EventKind::NOTE,
            2 => bad.recs[host].kind = EventKind::NOTE,
            3 => bad.recs[snap].kind = EventKind::NOTE,
            4 => {
                bad.recs[clock].kind = EventKind::HOST_PROFILE;
                bad.recs[clock].body = log.recs[host].body.clone();
            }
            5 => {
                bad.recs[clock].kind = EventKind::STATE_SNAPSHOT;
                bad.recs[clock].body = log.recs[snap].body.clone();
            }
            6 => bad.recs[snap].body[0] = hft_sequencer::snapshot::PURPOSE_RECOVERY,
            7 => {
                let mut h = bad.recs[host].read::<HostProfile>().unwrap();
                h.build_git[0] ^= 1;
                bad.recs[host].body = h.as_bytes().to_vec();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                verify::<TestSet>(bad, VerifyOptions::default(), hb()),
                Err(VerifyError::RunBoundary { .. })
            ),
            "boundary mutation {row}"
        );
    }
    let mut bad = log.clone();
    let note = Note {
        code: hft_sequencer::codes::seq_note(hft_sequencer::codes::note::RECOVER_MISMATCH),
        level: hft_event::NoteLevel::P0,
        source: hft_event::NoteSource::Sequencer,
        strategy: hft_types::StrategyId(0),
        _pad: 0,
        group: hft_types::GroupId(0),
        inst: InstId(0),
        v: [0; 8],
    };
    bad.recs[clock].kind = EventKind::NOTE;
    bad.recs[clock].body = note.as_bytes().to_vec();
    assert!(matches!(
        verify::<TestSet>(bad, VerifyOptions::default(), hb()),
        Err(VerifyError::RecoveryRange { .. })
    ));
    // An ordinary Output between BootDone and the initial EngineClock would otherwise be
    // skipped by snapshot restoration. This is neither a boot Output nor a recovery range.
    let clock = log
        .recs
        .iter()
        .position(|r| r.kind == EventKind::ENGINE_CLOCK)
        .unwrap();
    let mut extra = log
        .recs
        .iter()
        .find(|r| r.kind == EventKind::CMD_RESULT)
        .unwrap()
        .clone();
    extra.seq = log.recs[clock].seq;
    let mut bad = log;
    for r in &mut bad.recs[clock..] {
        r.seq += 1;
    }
    for start in &mut bad.seg_start {
        if *start >= clock {
            *start += 1;
        }
    }
    bad.recs.insert(clock, extra);
    assert!(matches!(
        verify::<TestSet>(bad, VerifyOptions::default(), hb()),
        Err(VerifyError::RunBoundary { .. })
    ));
}

fn rehash_snapshot(body: &mut [u8]) {
    use hft_sequencer::snapshot::{count_sections, state_hash};
    let n = size_of::<hft_event::SnapshotHdr>();
    let hash = state_hash(&body[n..]);
    let count = count_sections(&body[n..]);
    body[4..8].copy_from_slice(&count.to_le_bytes());
    body[16..32].copy_from_slice(&hash);
}

#[test]
fn seq_387_snapshot_self_check_rejects_corruption_and_loss() {
    use hft_event::{SectionHdr, SnapshotHdr};
    use hft_strategy_sdk::SnapshotReader;
    use zerocopy::IntoBytes;

    let t = fixture();
    let mut r = JournalReader::open(&reader_cfg(&t)).unwrap();
    let log = Log::from_reader(&mut r, limits()).unwrap();
    let indices: Vec<_> = log
        .recs
        .iter()
        .enumerate()
        .filter_map(|(i, r)| (r.kind == EventKind::STATE_SNAPSHOT).then_some(i))
        .collect();
    let ix = indices[0];
    let seq = log.recs[ix].seq;
    // A declared digest is checked, while zero always means compute it independently.
    assert!(
        indices
            .iter()
            .any(|i| log.recs[*i].read::<SnapshotHdr>().unwrap().state_hash == [0; 16])
    );
    assert!(
        indices
            .iter()
            .any(|i| log.recs[*i].read::<SnapshotHdr>().unwrap().state_hash != [0; 16])
    );
    for i in indices {
        let at = log.recs[i].seq;
        let good = check_snapshot::<TestSet>(&log, at).unwrap();
        let mut bad = log.clone();
        let mut wrong = good.state_hash;
        wrong[0] ^= 1;
        bad.recs[i].body[16..32].copy_from_slice(&wrong);
        assert!(matches!(
            check_snapshot::<TestSet>(&bad, at),
            Err(SnapshotCheckError::DeclaredHash { .. })
        ));
        bad.recs[i].body[16..32].fill(0);
        assert_eq!(check_snapshot::<TestSet>(&bad, at).unwrap(), good);
    }
    let mut bad = log.clone();
    bad.recs[ix].body.push(0);
    assert_eq!(
        check_snapshot::<TestSet>(&bad, seq),
        Err(SnapshotCheckError::Sections)
    );
    bad = log.clone();
    bad.recs[ix].body[4..8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        check_snapshot::<TestSet>(&bad, seq),
        Err(SnapshotCheckError::Sections)
    );
    bad = log.clone();
    bad.recs[ix].body[0] = 0;
    assert_eq!(
        check_snapshot::<TestSet>(&bad, seq),
        Err(SnapshotCheckError::Header)
    );
    bad = log.clone();
    bad.recs[ix].body.truncate(size_of::<SnapshotHdr>() - 1);
    assert_eq!(
        check_snapshot::<TestSet>(&bad, seq),
        Err(SnapshotCheckError::Header)
    );

    let n = size_of::<SnapshotHdr>();
    let mut sections = SnapshotReader::new(&log.recs[ix].body[n..]);
    let first = sections.next_section().unwrap().unwrap();
    let first_end = n + sections.pos();
    bad = log.clone();
    bad.recs[ix]
        .body
        .extend_from_slice(&log.recs[ix].body[n..first_end]);
    rehash_snapshot(&mut bad.recs[ix].body);
    assert_eq!(
        check_snapshot::<TestSet>(&bad, seq),
        Err(SnapshotCheckError::Sections)
    );
    assert_ne!(first.section, u16::MAX);

    // The restore path ignores sections it does not own. A validly framed, hashed extra
    // section must still be detected as lost state by the independent serialization check.
    bad = log.clone();
    bad.recs[ix].body.extend_from_slice(
        SectionHdr {
            section: u16::MAX,
            version: 1,
            count: 1,
            len: 8,
            _rsv: 0,
        }
        .as_bytes(),
    );
    bad.recs[ix].body.extend_from_slice(&[0; 8]);
    rehash_snapshot(&mut bad.recs[ix].body);
    assert!(matches!(
        check_snapshot::<TestSet>(&bad, seq),
        Err(SnapshotCheckError::RoundTrip {
            section: u16::MAX,
            ..
        })
    ));

    bad = log.clone();
    let mut sections = SnapshotReader::new(&log.recs[ix].body[n..]);
    loop {
        let pos = n + sections.pos();
        let s = sections.next_section().unwrap().expect("risk section");
        if s.section == 4 {
            bad.recs[ix].body[pos + 2..pos + 4].copy_from_slice(&u16::MAX.to_le_bytes());
            break;
        }
    }
    rehash_snapshot(&mut bad.recs[ix].body);
    assert!(matches!(
        check_snapshot::<TestSet>(&bad, seq),
        Err(SnapshotCheckError::Restore(_))
    ));
    assert_eq!(
        check_snapshot::<TestSet>(&log, u64::MAX),
        Err(SnapshotCheckError::Missing)
    );
    assert!(
        check_snapshot::<TestSet>(&log, seq).is_ok(),
        "source log was modified"
    );
}
