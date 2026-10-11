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
    BootSource, EngineSetup, InputSlot, OutRing, ParsedConfig, Sequencer, SnapshotSel, StepOutcome,
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
    writer: JournalWriter,
}
impl LiveIo<TestSet> for FileIo {
    type J = JournalWriter;
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
    let mut configs = b.configs(&rd);
    configs.engine_blob = setup.encode_blob(1, HostClass::Dev2, UTC0);
    configs.risk_blob = configs.risk.encode_blob().unwrap();
    configs.risk_sha256 = sha(&configs.risk_blob);
    let io = FileIo {
        io: TestIo::new(setup.clock),
        writer,
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
    driver.io.writer.seal_and_close().unwrap();
    t
}

#[test]
fn seq_385_file_journal_independent_verify() {
    let t = fixture();
    let cfg = reader_cfg(&t);
    let mut reader = JournalReader::open(&cfg).unwrap();
    let log = Log::from_reader(&mut reader, limits()).unwrap();
    let snapshots: Vec<_> = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .map(|r| r.seq)
        .collect();
    assert!(snapshots.len() >= 4);
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
    for at in snapshots {
        let checked = check_snapshot::<TestSet>(&log, at).unwrap_or_else(|e| panic!("snapshot {at}: {e:?}"));
        assert_ne!(checked.state_hash, [0; 16]);
        let setup = EngineSetup::from_snapshot(&log, at).unwrap();
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
