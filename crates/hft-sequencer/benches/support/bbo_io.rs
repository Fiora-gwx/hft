//! B0 initialization over TestIo, followed by production SPSC and RAM JournalWriter hot calls.
use super::common::*;
use hft_event::{Bbo, BlobRef, ClockStatus, EventKind, FeedCmd, GwCmd, HostProfile, RecordFlags};
use hft_journal::{JournalWriter, *};
use hft_rt::{Backing, Consumer, Producer, spsc};
use hft_sequencer::{
    driver::{LiveDriver, LiveIo, RingId, RollInfo},
    *,
};
use hft_types::{InstId, KillBits, Tsc, VenueId};
use std::path::PathBuf;

pub struct Io {
    init: TestIo<TestSet>,
    writer: JournalWriter,
    rx: Consumer<Bbo>,
    ready: bool,
    pub emitted: u64,
}

impl LiveIo<TestSet> for Io {
    type J = JournalWriter;
    fn journal(&mut self) -> &mut Self::J {
        &mut self.writer
    }
    fn tsc(&mut self) -> Tsc {
        Tsc(self.init.tsc)
    }
    fn measure(&mut self) -> Tsc {
        hft_rt::tsc_now()
    }
    fn n_rep(&self) -> u8 {
        2
    }
    fn n_bbo(&self) -> u8 {
        2
    }
    fn n_md(&self) -> u8 {
        2
    }
    fn len(&mut self, r: RingId) -> usize {
        if !self.ready {
            self.init.len(r)
        } else if r == RingId::Bbo(0) {
            self.rx.len_hint()
        } else {
            0
        }
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        if !self.ready {
            self.init.peek_len(r)
        } else {
            (self.len(r) > 0).then_some(core::mem::size_of::<Bbo>())
        }
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        if !self.ready { self.init.peek_inst(r) } else { None } // one queued BBO, never conflated
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        if !self.ready {
            return self.init.pop(r, out);
        }
        if r == RingId::Bbo(0)
            && let Some(b) = self.rx.try_pop()
        {
            out.set(RecordFlags(0), Tsc(self.init.tsc), &b);
            true
        } else {
            false
        }
    }
    fn out_free(&mut self, _: OutRing) -> usize {
        usize::MAX
    }
    fn push_out(&mut self, _: OutRing, c: &GwCmd) -> bool {
        std::hint::black_box(c);
        self.emitted += 1;
        true
    }
    fn push_feed(&mut self, _: VenueId, c: &FeedCmd) -> bool {
        std::hint::black_box(c);
        true
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<TestSet>> {
        self.init.take_parsed(r)
    }
    fn retire(&mut self, p: ParsedConfig<TestSet>) {
        self.init.retire(p);
    }
    fn write_kill(&mut self, _: &[KillBits]) {}
    fn roll_info(&mut self) -> RollInfo {
        self.init.roll_info()
    }
    fn host_profile(&self) -> HostProfile {
        self.init.host_profile()
    }
    fn clock_status(&self) -> ClockStatus {
        self.init.clock_status()
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        self.init.utc_of(t)
    }
}

pub type Seq = Sequencer<LiveDriver<TestSet, Io>, TestSet>;

pub fn fixture(inputs: &[MemRecord], id: usize) -> (Seq, Producer<Bbo>, Root) {
    let root = std::env::current_dir()
        .unwrap()
        .join("target/bbo-profile")
        .join(format!("ram-{}-{id}", std::process::id()));
    let cleanup = Root(root.clone());
    hft_journal::testing::mem_root(&root);
    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: root.join("hot"),
        cold_root: root.join("cold"),
        segment_bytes: 16 << 20,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 0,
    };
    let (ptx, prx) = spsc(4, Backing::Heap).unwrap();
    let (rtx, _rrx) = spsc(4, Backing::Heap).unwrap();
    let mut prep = SegmentPreparer::new(
        &cfg,
        PrepEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            to_writer: ptx,
        },
    );
    std::fs::create_dir_all(&cfg.cold_root).unwrap();
    let (writer, _) = JournalWriter::open(
        &cfg,
        WriterEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            prepared: prx,
            retired: rtx,
        },
    )
    .unwrap();
    assert_ne!(prep.poll(), PrepState::Failed);
    let b = b0();
    let rd = b.rd.load();
    let setup = b.setup(&rd);
    let (tx, rx) = spsc(64, Backing::Heap).unwrap();
    let io = Io {
        init: TestIo::new(setup.clock),
        writer,
        rx,
        ready: false,
        emitted: 0,
    };
    let drv = LiveDriver::new(io, b.live.clone());
    let mut s = Sequencer::boot(
        BootSource::Empty {
            configs: b.configs(&rd),
        },
        &setup,
        drv,
        hb(),
    )
    .unwrap();
    drain(&mut s);
    for r in inputs {
        s.driver_mut().io.init.tsc = r.tsc;
        s.driver_mut().io.init.ring(RingId::Hk).push_back(Elem {
            kind: r.kind,
            flags: r.flags,
            body: r.body.clone(),
        });
        drain(&mut s);
    }
    assert_eq!(
        s.engine().risk.mode.effective(hft_risk::mode::CellKey {
            strategy: S1,
            account: AC1,
            group: G1
        }),
        hft_risk::mode::Effective::Live
    );
    s.driver_mut().io.ready = true;
    s.driver_mut().taken_log.reserve(50_000);
    (s, tx, cleanup)
}

pub fn setup_inputs() -> Vec<MemRecord> {
    reset_script();
    let mut source = b0().boot();
    source.steady();
    source
        .j()
        .records()
        .filter(|r| r.role() == hft_event::Role::Input && r.kind != EventKind::ENGINE_CLOCK)
        .cloned()
        .collect()
}

// Private per-process directory, also removed when the fixture unwinds.
pub struct Root(PathBuf);
impl Drop for Root {
    fn drop(&mut self) {
        hft_journal::testing::mem_purge(&self.0);
        let base = std::env::current_dir().unwrap().join("target/bbo-profile");
        if self.0.parent() == Some(base.as_path()) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
pub fn drain(s: &mut Seq) {
    loop {
        match s.step() {
            StepOutcome::Busy => {}
            StepOutcome::Idle => break,
            x => panic!("benchmark stopped: {x:?}"),
        }
    }
}
