//! raw journals on the hft-journal heap backend, and a reader of their `RawRx`/`RawTx` records.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use hft_event::{EventKind, RawHdr, RecordFlags};
use hft_journal::testing;
use hft_journal::{
    EndAt, InstanceName, JournalConfig, JournalKind, JournalReader, JournalWriter, Next, PageSize, PrepEnv,
    PreparedSegment, ReaderConfig, RetiredSegment, RollInfo, RollReason, SegmentPreparer, WriterEnv,
};
use hft_rt::{Backing, CalibPair, Consumer, Producer, spsc};
use hft_types::Tsc;
use zerocopy::FromBytes;

use super::rd::V7;

static N: AtomicUsize = AtomicUsize::new(0);

/// A raw journal: the writer goes to the runtime, this keeps what is needed to read it back.
pub struct RawJ {
    pub cfg: JournalConfig,
    prep: SegmentPreparer,
    _retired: Consumer<RetiredSegment>,
}

/// One raw record read back.
#[derive(Clone, Debug)]
pub struct RawRec {
    pub kind: EventKind,
    pub tsc: u64,
    pub flags: RecordFlags,
    pub hdr: RawHdr,
    pub payload: Vec<u8>,
}

/// A fresh raw journal named `name` (segment `mib` MiB), rolled once so appends can start.
pub fn raw_journal(name: &str, mib: u64) -> (JournalWriter, RawJ) {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("hftv-{}-{n}", std::process::id()));
    let hot = root.join("hot");
    testing::mem_root(&hot);
    let cold = root.join("cold");
    std::fs::create_dir_all(&cold).unwrap();
    let cfg = JournalConfig {
        name: name.into(),
        kind: JournalKind::Raw,
        venue: V7,
        instance: InstanceName::Prod,
        hot_dir: hot,
        cold_root: cold,
        segment_bytes: mib * 1_048_576,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 8,
    };
    let (ptx, prx): (Producer<PreparedSegment>, Consumer<PreparedSegment>) = spsc(4, Backing::Heap).unwrap();
    let (rtx, rrx): (Producer<RetiredSegment>, Consumer<RetiredSegment>) = spsc(256, Backing::Heap).unwrap();
    let mut prep = SegmentPreparer::new(
        &cfg,
        PrepEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            to_writer: ptx,
        },
    );
    let (mut w, _) = JournalWriter::open(
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
    prep.poll();
    let info = RollInfo {
        utc_ns: 1_790_812_800_000_000_000,
        calib: CalibPair {
            tsc: Tsc(1),
            realtime_ns: 0,
            phc_ns: 0,
            err_ns: 0,
        },
        tsc_hz_milli: 1_000_000_000_000,
        configs: &[],
    };
    let b = w.roll(RollReason::Startup, &info).unwrap();
    b.finish().unwrap();
    w.publish();
    prep.poll();
    (
        w,
        RawJ {
            cfg,
            prep,
            _retired: rrx,
        },
    )
}

impl Drop for RawJ {
    fn drop(&mut self) {
        // The writer (inside the runtime) is dropped before this (field order of the harnesses).
        if let Some(root) = self.cfg.hot_dir.parent() {
            testing::mem_purge(&self.cfg.hot_dir);
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

impl RawJ {
    pub fn prepare(&mut self) {
        self.prep.poll();
    }
    /// Every `RawRx`/`RawTx` record so far, in order.
    pub fn records(&self) -> Vec<RawRec> {
        let mut c = ReaderConfig::new(
            self.cfg.hot_dir.clone(),
            self.cfg.cold_root.clone(),
            self.cfg.instance,
            self.cfg.name.clone(),
        );
        c.end = EndAt::LastCompleteEvent;
        c.tail_poll_us = 0;
        let mut r = JournalReader::open(&c).unwrap();
        let mut out = Vec::new();
        loop {
            match r.poll() {
                Next::Event(e) => {
                    if e.kind == EventKind::RAW_RX || e.kind == EventKind::RAW_TX {
                        let (h, rest) = RawHdr::read_from_prefix(e.body).unwrap();
                        out.push(RawRec {
                            kind: e.kind,
                            tsc: e.tsc,
                            flags: e.flags,
                            hdr: h,
                            payload: rest.get(..h.payload_len as usize).unwrap_or(&[]).to_vec(),
                        });
                    }
                }
                Next::Record(v) => {
                    if v.hdr.kind == EventKind::RAW_RX.0 || v.hdr.kind == EventKind::RAW_TX.0 {
                        panic!("raw record outside a logical event");
                    }
                }
                Next::Pending | Next::End => break,
                Next::Gap(_) => continue,
                Next::Err(e) => panic!("{e:?}"),
            }
        }
        out
    }

    pub fn dir(&self) -> PathBuf {
        self.cfg.hot_journal_dir()
    }
}
