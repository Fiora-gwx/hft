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
    let setup = b.setup(&rd);
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
    s.driver_mut().io.writer.publish();
    s.driver_mut().io.writer.seal_and_close().unwrap();
    drop(s);
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
