use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use hft_event::{BlobRef, ClockStatus, CmdHdr, ControlCmd, EventKind, FeedCmd, GwCmd, HostProfile};
use hft_journal::{
    EndAt, InstanceName, JournalConfig, JournalKind, JournalReader, JournalWriter, PageSize, ReaderConfig,
};
use hft_rt::{CalibPair, Heartbeat};
use hft_sequencer::driver::RollInfo;
use hft_sequencer::driver::live::{LiveCfg, LiveDriver, LiveIo, RingId};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::replay_io::LogReadLimits;
use hft_sequencer::{
    BootSource, InputSlot, MemJournal, OutRing, ParsedConfig, RecoverPlan, RecoveryFlags, Sequencer,
    SnapshotSel, StepOutcome,
};
use hft_strategy_sdk::StrategySet;
use hft_testkit::dst::seq::build;
use hft_testkit::dst::world::WorldSpec;
use hft_testkit::seq_rig::RigIo;
use hft_types::{AccountId, InstId, KillBits, Tsc, VenueId};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Fixture {
    pub root: PathBuf,
    pub file: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Io<S: StrategySet> {
    rings: RigIo<S>,
    writer: JournalWriter,
    old: Option<Log>,
    build: [u8; 20],
}
impl<S: StrategySet> LiveIo<S> for Io<S> {
    type J = JournalWriter;
    fn journal(&mut self) -> &mut Self::J {
        &mut self.writer
    }
    fn tsc(&mut self) -> Tsc {
        self.rings.tsc()
    }
    fn n_rep(&self) -> u8 {
        self.rings.n_rep()
    }
    fn n_bbo(&self) -> u8 {
        self.rings.n_bbo()
    }
    fn n_md(&self) -> u8 {
        self.rings.n_md()
    }
    fn len(&mut self, r: RingId) -> usize {
        self.rings.len(r)
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        self.rings.peek_len(r)
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        self.rings.peek_inst(r)
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        self.rings.pop(r, out)
    }
    fn push_out(&mut self, r: OutRing, c: &GwCmd) -> bool {
        self.rings.push_out(r, c)
    }
    fn push_feed(&mut self, v: VenueId, c: &FeedCmd) -> bool {
        self.rings.push_feed(v, c)
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<S>> {
        self.rings.take_parsed(r)
    }
    fn retire(&mut self, p: ParsedConfig<S>) {
        self.rings.retire(p);
    }
    fn write_kill(&mut self, bits: &[KillBits]) {
        self.rings.write_kill(bits);
    }
    fn roll_info(&mut self) -> RollInfo {
        self.rings.roll_info()
    }
    fn host_profile(&self) -> HostProfile {
        let mut h = self.rings.host_profile();
        h.build_git = self.build;
        h
    }
    fn recovery_log(&mut self) -> Option<Log> {
        self.old.take()
    }
    fn clock_status(&self) -> ClockStatus {
        self.rings.clock_status()
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        self.rings.utc_of(t)
    }
}

fn settle<S: StrategySet>(seq: &mut Sequencer<LiveDriver<S, Io<S>>, S>) {
    for _ in 0..1000 {
        match seq.step() {
            StepOutcome::Idle if !seq.view().snapshot_requested() => return,
            // SnapshotNow is emitted only after a previous idle round. The first Idle
            // after the command is not evidence that the requested snapshot was written.
            StepOutcome::Busy | StepOutcome::Idle => {}
            o => panic!("{o:?}"),
        }
    }
    panic!("fixture did not settle");
}

pub fn create<S: StrategySet>() -> Fixture {
    create_with_restart::<S>(None)
}

#[derive(Clone, Copy, Debug)]
pub enum Restart {
    Recovery,
    FromSnapshot,
    Version,
}

pub fn restarted<S: StrategySet>(restart: Restart) -> Fixture {
    create_with_restart::<S>(Some(restart))
}

pub fn read(f: &Fixture) -> Log {
    let mut c = ReaderConfig::new(
        f.root.clone(),
        f.root.join("absent"),
        InstanceName::Prod,
        "seq".into(),
    );
    c.end = EndAt::LastCompleteEvent;
    c.reassembly_cap = 16 << 20;
    let mut r = JournalReader::open(&c).unwrap();
    Log::from_reader(
        &mut r,
        LogReadLimits {
            records: 20_000,
            body_bytes: 64 << 20,
        },
    )
    .unwrap()
}

fn command<S: StrategySet>(s: &mut Sequencer<LiveDriver<S, Io<S>>, S>, id: u64, c: &ControlCmd<'_>) {
    let mut h = CmdHdr::zeroed();
    h.cmd_id[..8].copy_from_slice(&id.to_le_bytes());
    h.issuer[..7].copy_from_slice(b"cli:rpl");
    h.role = CmdHdr::ROLE_ADMIN;
    h.account = AccountId(0);
    h.cmd_ver = 1;
    h.confirmed = 1;
    h.expect_state_version = s.engine().risk.mode.ctl_state_version();
    let mut body = vec![0; 8192];
    let n = c.encode(&h, &mut body).unwrap();
    body.truncate(n);
    s.driver_mut()
        .io
        .rings
        .rings
        .entry((0, 0))
        .or_default()
        .push_back((EventKind::CONTROL_CMD, body));
}

fn create_with_restart<S: StrategySet>(restart: Option<Restart>) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "replayer-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let f = Fixture {
        file: root.join("prod/seq/0000000001.jrn"),
        root,
    };
    let mut world = WorldSpec::w1();
    for binding in &mut world.strategies {
        binding.kind_id = S::kinds().first().unwrap().kind_id;
    }
    let rd_blob = build::refdata_blob(&world, false).unwrap();
    let rd = build::load_refdata(&rd_blob).unwrap();
    let mut setup = build::engine_setup(&world, 1_000_000_000, 1);
    let configs = build::config_set::<S>(&world, &rd, rd_blob, &setup).unwrap();
    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: f.root.join("unused-hot"),
        cold_root: f.root.join("unused-cold"),
        segment_bytes: 32 << 20,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 1,
    };
    let writer = JournalWriter::open_offline(&cfg, &f.root).unwrap();
    let io = Io {
        writer,
        old: None,
        build: [0; 20],
        rings: RigIo {
            j: MemJournal::new(setup.clock),
            tsc: 0,
            rings: BTreeMap::new(),
            n_venues: world.venues.len() as u8,
            out: Vec::new(),
            mailbox: Vec::new(),
            kills: Vec::new(),
            calib: CalibPair {
                tsc: Tsc(0),
                realtime_ns: setup.clock.calib_realtime_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            hz: setup.clock.tsc_hz_milli,
        },
    };
    let mut s = Sequencer::boot(
        BootSource::Empty { configs },
        &setup,
        LiveDriver::new(io, LiveCfg::default()),
        Box::leak(Box::new(Heartbeat::new())),
    )
    .unwrap();
    settle(&mut s);
    for id in 1..=2u64 {
        command(&mut s, id, &ControlCmd::SnapshotNow);
        settle(&mut s);
    }
    if let Some(restart) = restart {
        if matches!(restart, Restart::Version) {
            command(&mut s, 3, &ControlCmd::Drain { cancel_resting: true });
            let mut done = false;
            for _ in 0..1000 {
                match s.step() {
                    StepOutcome::Busy | StepOutcome::Idle => {}
                    StepOutcome::Done { .. } => {
                        done = true;
                        break;
                    }
                    o => panic!("drain: {o:?}"),
                }
            }
            assert!(done, "drain did not finish");
        }
        s.driver_mut().io.writer.publish();
        let old = read(&f);
        let initial = old
            .recs
            .iter()
            .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .unwrap()
            .seq;
        let mut io = s.into_driver().io;
        io.old = Some(old);
        // A new TSC epoch and frequency must come from the new segment and EngineClock.
        io.rings.tsc = 100;
        io.rings.hz *= 2;
        io.rings.calib.tsc = Tsc(100);
        io.rings.calib.realtime_ns += 20_000_000_000;
        setup.clock.calib_tsc = 100;
        setup.clock.tsc_hz_milli = io.rings.hz;
        setup.clock.calib_realtime_ns = io.rings.calib.realtime_ns;
        if matches!(restart, Restart::Version) {
            io.build = [1; 20];
        }
        let from_snapshot = matches!(restart, Restart::FromSnapshot);
        let plan = RecoverPlan {
            start: if from_snapshot {
                SnapshotSel::AtSeq(initial)
            } else {
                SnapshotSel::Latest
            },
            replay_tail: !from_snapshot,
            epoch: 0,
            flags: RecoveryFlags(if from_snapshot {
                RecoveryFlags::FROM_SNAPSHOT | RecoveryFlags::TAIL_LOST
            } else {
                0
            }),
            quarantine: Vec::new(),
        };
        s = Sequencer::boot(
            BootSource::Journal(plan),
            &setup,
            LiveDriver::new(io, LiveCfg::default()),
            Box::leak(Box::new(Heartbeat::new())),
        )
        .unwrap();
        settle(&mut s);
        for id in 10..=11 {
            command(&mut s, id, &ControlCmd::SnapshotNow);
            settle(&mut s);
        }
    }
    let mut d = s.into_driver();
    d.io.writer.publish();
    d.io.writer.seal_and_close().unwrap();
    f
}
