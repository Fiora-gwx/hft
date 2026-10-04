//! `H(cfg)` of TESTS.md: one thread holding the writer, `SegmentPreparer`, `Persister`,
//! `HotRetirer` and manually advanced `mono_ns`/`utc_ns`. Preparation, copies and retirement happen
//! only when a test polls them. Hot directory: the heap backend (any platform) or, for tests marked
//! Linux, a `/dev/shm` temporary directory when running on Linux.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use hft_event::dev::{TCrc, TIn, TMeta, TOut, TVarHdr};
use hft_event::{BlobRef, EventKind, RecordFlags, RecordHeader};
use hft_journal::testing;
use hft_journal::*;
use hft_rt::{Backing, CalibPair, Consumer, Producer, spsc};
use hft_types::{Seq, Tsc, VenueId};
use zerocopy::{FromBytes, IntoBytes};

/// 2026-10-01T00:00:00Z.
pub const UTC0: i64 = 1_790_812_800_000_000_000;

static N: AtomicU32 = AtomicU32::new(0);

/// A unique scratch path (not created).
pub fn scratch(tag: &str) -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("hftj-{tag}-{}-{n}", std::process::id()))
}

/// A fresh directory on disk.
pub fn disk_dir(tag: &str) -> PathBuf {
    let d = scratch(tag);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A fresh cold root on disk.
pub fn cold_dir() -> PathBuf {
    disk_dir("cold")
}

/// A heap-backend hot root.
pub fn mem_hot() -> PathBuf {
    let d = scratch("memhot");
    testing::mem_root(&d);
    d
}

/// A tmpfs hot root (`/dev/shm`), Linux only.
#[cfg(target_os = "linux")]
pub fn shm_hot() -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = PathBuf::from(format!("/dev/shm/hftj-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Hot root for tests marked Linux: tmpfs on Linux, heap backend elsewhere.
pub fn linux_hot() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        shm_hot()
    }
    #[cfg(not(target_os = "linux"))]
    {
        mem_hot()
    }
}

/// `C0` with the given hot/cold roots.
pub fn c0(hot: &Path, cold: &Path) -> JournalConfig {
    JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: hot.to_path_buf(),
        cold_root: cold.to_path_buf(),
        segment_bytes: 1_048_576,
        page: PageSize::Normal,
        size_idle_pct: 75,
        size_valve_pct: 90,
        hot_keep: 1,
    }
}

/// `R0`.
pub fn r0(hot: &Path, cold: &Path) -> JournalConfig {
    JournalConfig {
        name: "raw-feed-binance-usdm".into(),
        kind: JournalKind::Raw,
        venue: VenueId::BINANCE_USDM,
        ..c0(hot, cold)
    }
}

/// C0 on a heap hot root and a disk cold root.
pub fn mem_c0() -> JournalConfig {
    c0(&mem_hot(), &cold_dir())
}

/// C0 for tests marked Linux.
pub fn linux_c0() -> JournalConfig {
    c0(&linux_hot(), &cold_dir())
}

pub fn calib0() -> CalibPair {
    CalibPair {
        tsc: Tsc(0),
        realtime_ns: 0,
        phc_ns: 0,
        err_ns: 0,
    }
}

pub fn tin(b: u8) -> TIn {
    TIn { bytes: [b; 80] }
}
pub fn tout(b: u8) -> TOut {
    TOut { bytes: [b; 56] }
}
pub fn tmeta(b: u8) -> TMeta {
    TMeta { bytes: [b; 32] }
}
pub fn tcrc(b: u8) -> TCrc {
    TCrc { bytes: [b; 40] }
}

/// `TVar` logical body with a tail of `l` bytes (prefix 16 B).
pub fn tvar(l: usize, fill: u8) -> Vec<u8> {
    let h = TVarHdr {
        len: l as u32,
        bytes: [0; 12],
    };
    let mut v = h.as_bytes().to_vec();
    v.resize(16 + l, fill);
    v
}

/// Record length of a `TVar` with tail `l`.
pub fn tvar_len(l: usize) -> usize {
    (24 + 16 + l).div_ceil(8) * 8
}

/// `TMulti` logical body of `l` increasing bytes.
pub fn multi_body(l: usize) -> Vec<u8> {
    (0..l).map(|i| (i % 251) as u8).collect()
}

pub const F0: RecordFlags = RecordFlags(0);
pub const PART: u16 = 1 << 4;
pub const CRC: u16 = 1 << 5;
pub const EVENT_END: u16 = 1 << 6;

pub struct Rings {
    pub ptx: Producer<PreparedSegment>,
    pub prx: Consumer<PreparedSegment>,
    pub rtx: Producer<RetiredSegment>,
    pub rrx: Consumer<RetiredSegment>,
}

pub fn rings() -> Rings {
    let (ptx, prx) = spsc(4, Backing::Heap).unwrap();
    let (rtx, rrx) = spsc(256, Backing::Heap).unwrap();
    Rings { ptx, prx, rtx, rrx }
}

pub fn env(prx: Consumer<PreparedSegment>, rtx: Producer<RetiredSegment>) -> WriterEnv {
    WriterEnv {
        writer_epoch: 7,
        build_git: [0xAB; 20],
        host_profile_sha256: [0xCD; 32],
        prepared: prx,
        retired: rtx,
    }
}

pub fn prep_env(ptx: Producer<PreparedSegment>) -> PrepEnv {
    PrepEnv {
        writer_epoch: 7,
        build_git: [0xAB; 20],
        host_profile_sha256: [0xCD; 32],
        to_writer: ptx,
    }
}

/// Decoded record header at `off` of `b`.
pub fn rh(b: &[u8], off: usize) -> RecordHeader {
    RecordHeader::read_from_bytes(&b[off..off + 24]).unwrap()
}

pub fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

pub fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

/// Header fields of a segment image.
pub fn fixed(b: &[u8]) -> SegmentHeaderFixed {
    SegmentHeaderFixed::read_from_bytes(&b[..3072]).unwrap()
}

/// `(offset, header)` of every record in `[4096, end)` of a segment image.
pub fn records(b: &[u8], end: usize) -> Vec<(usize, RecordHeader)> {
    let mut v = Vec::new();
    let mut off = 4096;
    while off + 24 <= end {
        let h = rh(b, off);
        if h.len == 0 {
            break;
        }
        v.push((off, h));
        off += h.len as usize;
    }
    v
}

/// `SegmentRoll` body of a record at `off`.
pub fn roll_body(b: &[u8], off: usize) -> hft_event::SegmentRoll {
    hft_event::SegmentRoll::read_from_bytes(&b[off + 24..off + 72]).unwrap()
}

/// The test harness.
pub struct H {
    pub cfg: JournalConfig,
    pub w: Option<JournalWriter>,
    pub rep: Option<RecoveryReport>,
    pub prep: SegmentPreparer,
    pub pers: Persister,
    pub ret: HotRetirer,
    pub mono: u64,
    pub utc: i64,
    pub calib: CalibPair,
    pub tsc_hz_milli: u64,
    pub configs: Vec<BlobRef>,
    /// The retired ring's consumer when the test holds it instead of the retirer (`tap`).
    pub tap: Option<Consumer<RetiredSegment>>,
    pending: Option<(Consumer<PreparedSegment>, Producer<RetiredSegment>)>,
}

impl H {
    /// Harness + `open` (no roll yet).
    pub fn new(cfg: JournalConfig) -> H {
        let mut h = H::bare(cfg, false);
        h.open().unwrap();
        h
    }

    /// Harness + `open0`.
    pub fn open0(cfg: JournalConfig) -> H {
        let mut h = H::new(cfg);
        h.roll(RollReason::Startup);
        h
    }

    /// Harness without a writer; `tap` = the test holds the retired ring instead of the retirer.
    pub fn bare(cfg: JournalConfig, tap: bool) -> H {
        let r = rings();
        let prep = SegmentPreparer::new(&cfg, prep_env(r.ptx));
        let mut pers = Persister::new(PersistConfig::default());
        pers.add(&cfg);
        let (ret, tapc) = if tap {
            let (_, dummy) = spsc::<RetiredSegment>(2, Backing::Heap).unwrap();
            (HotRetirer::new(&cfg, dummy), Some(r.rrx))
        } else {
            (HotRetirer::new(&cfg, r.rrx), None)
        };
        H {
            cfg,
            w: None,
            rep: None,
            prep,
            pers,
            ret,
            mono: 0,
            utc: UTC0,
            calib: calib0(),
            tsc_hz_milli: 0,
            configs: Vec::new(),
            tap: tapc,
            pending: Some((r.prx, r.rtx)),
        }
    }

    pub fn open(&mut self) -> Result<RecoveryReport, OpenErr> {
        let (prx, rtx) = self
            .pending
            .take()
            .expect("rings already used; call crash() first");
        let (w, rep) = JournalWriter::open(&self.cfg, env(prx, rtx))?;
        self.w = Some(w);
        self.rep = Some(rep);
        Ok(rep)
    }

    pub fn w(&mut self) -> &mut JournalWriter {
        self.w.as_mut().unwrap()
    }

    pub fn persister(&mut self, pc: PersistConfig) {
        let mut p = Persister::new(pc);
        p.add(&self.cfg);
        self.pers = p;
    }

    /// Prepares the next segment, rolls and returns the boot writer.
    pub fn roll_boot(&mut self, reason: RollReason) -> BootWriter<'_> {
        assert_ne!(self.prep.poll(), PrepState::Failed, "prepare");
        self.try_roll(reason).unwrap()
    }

    /// Rolls without preparing first.
    pub fn try_roll(&mut self, reason: RollReason) -> Result<BootWriter<'_>, RollErr> {
        let info = RollInfo {
            utc_ns: self.utc,
            calib: self.calib,
            tsc_hz_milli: self.tsc_hz_milli,
            configs: &self.configs,
        };
        self.w.as_mut().unwrap().roll(reason, &info)
    }

    /// Prepares, rolls and finishes the boot block `BB0`; returns `BootDone`'s seq.
    pub fn roll(&mut self, reason: RollReason) -> Seq {
        self.roll_boot(reason).finish().unwrap()
    }

    pub fn hot_jdir(&self) -> PathBuf {
        self.cfg.hot_journal_dir()
    }

    pub fn cold_jdir(&self) -> PathBuf {
        self.cfg.cold_journal_dir()
    }

    pub fn seg_path(&self, no: u64) -> PathBuf {
        self.hot_jdir().join(format!("{no:010}.jrn"))
    }

    /// Image of hot segment `no`.
    pub fn seg(&self, no: u64) -> Vec<u8> {
        testing::read(&self.seg_path(no)).unwrap()
    }

    pub fn hdr(&self, no: u64) -> SegmentHeaderFixed {
        fixed(&self.seg(no))
    }

    pub fn committed(&self, no: u64) -> u64 {
        u64_at(&self.seg(no), 3072)
    }

    pub fn last_seq(&self, no: u64) -> u64 {
        u64_at(&self.seg(no), 3080)
    }

    pub fn state(&self, no: u64) -> u32 {
        u32_at(&self.seg(no), 3088)
    }

    pub fn synced(&self, no: u64) -> u64 {
        u64_at(&self.seg(no), 3144)
    }

    pub fn persisted(&self, no: u64) -> u64 {
        u64_at(&self.seg(no), 3136)
    }

    pub fn retired(&self, no: u64) -> u32 {
        u32_at(&self.seg(no), 3152)
    }

    /// Writes `n` events of `[TIn, TOut]`, each `end_event` + `publish`.
    pub fn events(&mut self, n: usize) {
        let w = self.w();
        for i in 0..n {
            w.append(F0, Tsc(i as u64), &tin(i as u8)).unwrap();
            w.append(F0, Tsc(i as u64), &tout(i as u8)).unwrap();
            w.end_event();
            w.publish();
        }
    }

    /// Single-`TIn` events (end_event + publish) until `next_seq() == seq + 1`.
    pub fn write_to_seq(&mut self, seq: u64) {
        let w = self.w();
        while w.next_seq().0 <= seq {
            w.append(F0, Tsc(1), &tin(1)).unwrap();
            w.end_event();
        }
        w.publish();
    }

    /// Pushes `off` to exactly `target` with `TVar` events, then publishes.
    pub fn push_to(&mut self, target: usize) {
        let crc = if self.cfg.kind == JournalKind::Raw { 4 } else { 0 };
        let w = self.w();
        let mut off = w.offset() as usize;
        assert!(
            target >= off && (target - off).is_multiple_of(8),
            "push_to {off} → {target}"
        );
        while off < target {
            let d = target - off;
            // Largest record ≤ 65 040 leaving either 0 or ≥ 40 bytes.
            let mut rec = d.min(65_040);
            let min = 40 + 2 * crc;
            if d - rec != 0 && d - rec < min {
                rec -= min;
            }
            let l = rec - 40 - crc;
            let body = tvar(l, 0x33);
            w.append_var(EventKind::T_VAR, F0, Tsc(1), &[&body]).unwrap();
            w.end_event();
            off += rec;
        }
        assert_eq!(w.offset() as usize, target);
        w.publish();
    }

    /// Simulated crash: the writer, preparer, retirer and persister vanish without cleanup.
    pub fn crash(&mut self) {
        if let Some(w) = self.w.take() {
            std::mem::forget(w);
        }
        let r = rings();
        std::mem::forget(std::mem::replace(
            &mut self.prep,
            SegmentPreparer::new(&self.cfg, prep_env(r.ptx)),
        ));
        std::mem::forget(std::mem::replace(
            &mut self.ret,
            HotRetirer::new(&self.cfg, r.rrx),
        ));
        let mut p = Persister::new(PersistConfig::default());
        p.add(&self.cfg);
        std::mem::forget(std::mem::replace(&mut self.pers, p));
        self.pending = Some((r.prx, r.rtx));
    }

    /// Polls the persister until a round copies nothing and seals nothing.
    pub fn persist_all(&mut self) {
        for _ in 0..10_000 {
            self.mono += 25_000_000;
            let p = self.pers.poll(self.mono, self.utc);
            if p.bytes_copied == 0 && p.sealed == 0 && p.syncs == 0 {
                return;
            }
        }
        panic!("persist did not settle");
    }

    pub fn reader_cfg(&self, start: StartAt) -> ReaderConfig {
        let mut c = ReaderConfig::new(
            self.cfg.hot_dir.clone(),
            self.cfg.cold_root.clone(),
            self.cfg.instance,
            self.cfg.name.clone(),
        );
        c.start = start;
        c.tail_poll_us = 0;
        c
    }

    pub fn reader(&self, start: StartAt) -> JournalReader {
        JournalReader::open(&self.reader_cfg(start)).unwrap()
    }

    /// Cold copy stem of segment `no` (from its hot header).
    pub fn cold_stem(&self, no: u64) -> PathBuf {
        let h = self.hdr(no);
        testing::cold_stem(&self.cold_jdir(), h.created_utc_ns, no, h.first_seq)
    }
}

/// Reads everything currently deliverable: `(seq, kind, is_event, flags)` until `Pending`/`End`.
pub fn drain(r: &mut JournalReader) -> Vec<(u64, u16, bool, u16)> {
    let mut v = Vec::new();
    loop {
        match r.poll() {
            Next::Record(x) => v.push((x.hdr.seq, x.hdr.kind, false, x.hdr.flags)),
            Next::Event(e) => v.push((e.seq, e.kind.0, true, e.flags.0)),
            Next::Pending | Next::End => return v,
            other => panic!("unexpected {other:?} after {} items", v.len()),
        }
    }
}

/// A writer for a raw journal at `off` writes a `RawRx` with `payload` bytes.
pub fn raw_body(payload: usize, rflags: u8, fill: u8) -> Vec<u8> {
    let h = hft_event::RawHdr {
        venue: VenueId::BINANCE_USDM,
        conn: hft_types::ConnId(1),
        what: 1,
        rflags,
        stream: 0,
        hw_rx_ns: 0,
        payload_len: payload as u32,
        _pad: 0,
    };
    let mut v = h.as_bytes().to_vec();
    v.resize(24 + payload, fill);
    v
}
