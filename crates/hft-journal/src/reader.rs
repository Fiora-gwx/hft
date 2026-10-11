//! `JournalReader`: tails or replays a journal across hot segments and cold copies, reassembles
//! multi-part records, reports gaps (README "读者", journal-format "读者 API 概要").

#![allow(clippy::disallowed_types, clippy::disallowed_methods)] // allow-list: readers run in other processes and on cold threads

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

use hft_event::{EventKind, RecordFlags, RecordHeader, RecordView};
use zerocopy::FromBytes;

use crate::config::{
    DEFAULT_REASSEMBLY_CAP, DEFAULT_TAIL_POLL_US, GapInfo, GapReason, InstanceName, JournalKind, JournalName,
    OpenErr, Position, ReadErr, RollMark, SegState,
};
use crate::fs::{self, ColdSeg, HotOpen};
use crate::layout::{
    FORMAT_VERSION, HEADER_FIXED_LEN, HEADER_LEN, OFF_COMMITTED, OFF_LAST_SEQ, OFF_RETIRED, OFF_STATE,
    OFF_SYNCED, RECOVERY_GAP, SEGMENT_MAGIC, SegMap, SegmentHeaderFixed,
};
use crate::scan::header_sha;
use crate::scan::{self, KindRules};

/// Reader configuration (README "读者").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReaderConfig {
    pub hot_dir: PathBuf,
    pub cold_root: PathBuf,
    pub instance: InstanceName,
    pub name: JournalName,
    pub start: StartAt,
    pub end: EndAt,
    pub on_gap: OnGap,
    /// 0 = no reassembly: multi-part records are delivered part by part as `Next::Record`.
    pub reassembly_cap: usize,
    pub tail_poll_us: u32,
    /// Same-process tailing reader (advisor, sim-gw, ctl-gateway): reuses the process's existing
    /// hot-segment mappings (fixed slots) instead of `mmap`/`munmap` per segment.
    pub same_process: bool,
}

impl ReaderConfig {
    /// `start = Oldest`, `end = Follow`, `on_gap = Error`, defaults for the rest.
    pub fn new(
        hot_dir: PathBuf,
        cold_root: PathBuf,
        instance: InstanceName,
        name: JournalName,
    ) -> ReaderConfig {
        ReaderConfig {
            hot_dir,
            cold_root,
            instance,
            name,
            start: StartAt::Oldest,
            end: EndAt::Follow,
            on_gap: OnGap::Error,
            reassembly_cap: DEFAULT_REASSEMBLY_CAP,
            tail_poll_us: DEFAULT_TAIL_POLL_US,
            same_process: false,
        }
    }
}

/// Where reading starts.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StartAt {
    Oldest,
    Seq(u64),
    SegmentBoot(u64),
    LatestBoot,
    LatestKeyframe,
    Tail,
}

/// Where reading ends.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EndAt {
    Follow,
    LastCompleteEvent,
}

/// What to do at a gap.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OnGap {
    Error,
    SkipToNextBoot,
}

/// A reassembled multi-part record.
#[derive(Copy, Clone, Debug)]
pub struct LogicalEvent<'a> {
    /// Seq of part 0.
    pub seq: u64,
    pub kind: EventKind,
    pub tsc: u64,
    /// Flags of the last part.
    pub flags: RecordFlags,
    pub parts: u16,
    /// Concatenated logical body (no `PartHdr`).
    pub body: &'a [u8],
}

/// One `poll` result.
#[derive(Copy, Clone, Debug)]
pub enum Next<'a> {
    Record(RecordView<'a>),
    Event(LogicalEvent<'a>),
    Pending,
    Gap(GapInfo),
    End,
    Err(ReadErr),
}

#[derive(Clone, Debug, Default)]
struct Loc {
    hot: Option<PathBuf>,
    cold: Option<ColdSeg>,
}

#[derive(Debug)]
#[allow(dead_code)] // paths kept for Debug output
enum Src {
    /// Own mapping (released when leaving).
    Hot(PathBuf, HotOpen),
    /// Borrowed in-process mapping (never released by the reader).
    Slot(PathBuf, SegMap),
    Cold(ColdSeg, HotOpen),
}

impl Src {
    fn map(&self) -> SegMap {
        *self.map_ref()
    }

    fn map_ref(&self) -> &SegMap {
        match self {
            Src::Hot(_, h) | Src::Cold(_, h) => &h.map,
            Src::Slot(_, m) => m,
        }
    }
}

#[derive(Debug)]
struct Cur {
    no: u64,
    src: Src,
    fixed: SegmentHeaderFixed,
    bound: usize,
    last_fstat_ns: u64,
    /// `bad()` computed once per opened segment.
    bad_cache: Option<bool>,
}

impl Cur {
    /// Header unusable: short mapping, unknown magic/version, CRC mismatch.
    fn bad(&self) -> bool {
        let f = &self.fixed;
        self.src.map().len() < HEADER_LEN
            || f.magic != SEGMENT_MAGIC
            || f.format_version != FORMAT_VERSION
            || f.header_crc32c != f.compute_crc()
    }
}

#[derive(Copy, Clone, Debug)]
struct Group {
    first_seq: u64,
    kind: u16,
    tsc: u64,
    parts: u16,
    next: u16,
    total: usize,
}

/// Read-only reader of one journal, in any process.
#[derive(Debug)]
pub struct JournalReader {
    cfg: ReaderConfig,
    hot_jdir: PathBuf,
    cold_jdir: PathBuf,
    kind: JournalKind,
    cur: Option<Cur>,
    /// Header of the previous segment (chain check of the next one).
    prev_fixed: Option<SegmentHeaderFixed>,
    off: usize,
    next_seq: u64,
    pos: Position,
    in_boot: bool,
    boot_closing: bool,
    group: Option<Group>,
    buf: Vec<u8>,
    stuck: Option<GapInfo>,
    next_open: Option<u64>,
    skip_checks: bool,
    end_limit: Option<(u64, usize)>,
    ended: bool,
    pending_at: Option<u64>,
    clock: Option<u64>,
    t0: Instant,
    row_reads: u64,
    known: BTreeSet<u64>,
    production: bool,
}

const FSTAT_EVERY_NS: u64 = 1_000_000_000;

impl JournalReader {
    /// Opens a reader at `cfg.start`.
    pub fn open(cfg: &ReaderConfig) -> Result<JournalReader, OpenErr> {
        let hot_jdir = cfg.hot_dir.join(cfg.instance.as_str()).join(cfg.name.as_str());
        let cold_jdir = cfg.cold_root.join(cfg.instance.as_str()).join(cfg.name.as_str());
        let mut r = JournalReader {
            cfg: cfg.clone(),
            hot_jdir,
            cold_jdir,
            kind: JournalKind::Seq,
            cur: None,
            prev_fixed: None,
            off: HEADER_LEN,
            next_seq: 0,
            pos: Position::default(),
            in_boot: false,
            boot_closing: false,
            group: None,
            buf: Vec::new(),
            stuck: None,
            next_open: None,
            skip_checks: false,
            end_limit: None,
            ended: false,
            pending_at: None,
            clock: None,
            t0: Instant::now(),
            row_reads: 0,
            known: BTreeSet::new(),
            production: !scan::dev_kinds_known(),
        };
        let segs = r.list();
        if segs.is_empty() {
            return Err(OpenErr::NotFound);
        }
        r.known = segs.keys().copied().collect();
        // Journal kind from the first readable header.
        for no in segs.keys() {
            if let Some(f) = r.header_of(*no, &segs) {
                r.kind = JournalKind::from_u8(f.journal_kind).unwrap_or(JournalKind::Seq);
                break;
            }
        }
        if r.cfg.end == EndAt::LastCompleteEvent {
            r.end_limit = r
                .compute_last_complete(&segs)
                .map(|p| (p.segment_no, p.offset as usize));
        }
        r.position_start(&segs)?;
        Ok(r)
    }

    fn now(&self) -> u64 {
        self.clock.unwrap_or_else(|| self.t0.elapsed().as_nanos() as u64)
    }

    /// Hot (non-Preparing) and cold segments by number.
    fn list(&self) -> BTreeMap<u64, Loc> {
        let mut m: BTreeMap<u64, Loc> = BTreeMap::new();
        for (no, p) in fs::hot_list(&self.hot_jdir) {
            if scan::read_header(&p).is_some_and(|(_, st)| st == SegState::Preparing as u32) {
                continue;
            }
            // A named segment with an unreadable/corrupt header still exists. Keep it in the
            // ordered set so open/poll reports the error instead of silently moving Oldest
            // to a later segment (or moving LastCompleteEvent to an earlier one).
            m.entry(no).or_default().hot = Some(p);
        }
        for (no, c) in fs::cold_list(&self.cold_jdir) {
            m.entry(no).or_default().cold = Some(c);
        }
        m
    }

    fn header_of(&self, no: u64, segs: &BTreeMap<u64, Loc>) -> Option<SegmentHeaderFixed> {
        let l = segs.get(&no)?;
        let p = l.hot.clone().or_else(|| l.cold.as_ref().map(ColdSeg::jrn))?;
        let b = fs::read_prefix(&p, HEADER_FIXED_LEN).ok()?;
        SegmentHeaderFixed::read_from_bytes(b.get(..HEADER_FIXED_LEN)?).ok()
    }

    fn rules(&self) -> KindRules {
        KindRules {
            journal: self.kind,
            require_registered: false,
            production: self.production,
        }
    }

    /// Opens segment `no` (hot preferred) without positioning.
    fn open_seg(&mut self, no: u64, segs: &BTreeMap<u64, Loc>) -> Option<Cur> {
        let l = segs.get(&no)?;
        let now = self.now();
        if let Some(p) = &l.hot {
            if self.cfg.same_process
                && let Some(m) = fs::hot_inproc(p)
            {
                return Some(Cur {
                    no,
                    fixed: m.fixed(),
                    src: Src::Slot(p.clone(), m),
                    bound: HEADER_LEN,
                    last_fstat_ns: now,
                    bad_cache: None,
                });
            }
            if let Ok(h) = fs::hot_open(p, false) {
                return Some(Cur {
                    no,
                    fixed: h.map.fixed(),
                    src: Src::Hot(p.clone(), h),
                    bound: HEADER_LEN,
                    last_fstat_ns: now,
                    bad_cache: None,
                });
            }
        }
        let c = l.cold.clone()?;
        let h = fs::cold_open(&c.jrn()).ok()?;
        Some(Cur {
            no,
            fixed: h.map.fixed(),
            src: Src::Cold(c, h),
            bound: HEADER_LEN,
            last_fstat_ns: now,
            bad_cache: None,
        })
    }

    fn drop_cur(&mut self) {
        if let Some(c) = self.cur.take() {
            match c.src {
                Src::Hot(_, h) => fs::release(None, h.map),
                Src::Cold(_, h) => fs::release(None, h.map),
                Src::Slot(..) => {}
            }
        }
    }

    /// Readable end of the current segment (re-reads the writer row / cold length).
    fn refresh_bound(&mut self) -> Result<(), GapInfo> {
        let now = self.now();
        let limit = self.end_limit;
        let Some(c) = self.cur.as_mut() else { return Ok(()) };
        let mut switch = false;
        match &c.src {
            Src::Hot(_, h) => {
                self.row_reads += 1;
                let m = h.map;
                if m.load32(OFF_RETIRED) == 1 {
                    switch = true;
                } else if now.saturating_sub(c.last_fstat_ns) >= FSTAT_EVERY_NS {
                    c.last_fstat_ns = now;
                    switch = !fs::still_linked(h);
                }
                if !switch {
                    c.bound = (m.load64(OFF_COMMITTED) as usize).min(m.len());
                }
            }
            Src::Slot(_, m) => {
                self.row_reads += 1;
                if m.load32(OFF_RETIRED) == 1 {
                    switch = true;
                } else {
                    c.bound = (m.load64(OFF_COMMITTED) as usize).min(m.len());
                }
            }
            Src::Cold(cs, h) => {
                let linked = if now.saturating_sub(c.last_fstat_ns) >= FSTAT_EVERY_NS {
                    c.last_fstat_ns = now;
                    fs::still_linked(h) && cs.jrn().exists()
                } else {
                    true
                };
                if !linked {
                    let at = self.pos;
                    let no = c.no;
                    self.drop_cur();
                    return Err(self.gap(at, no, GapReason::Deleted));
                }
                let cs = cs.clone();
                let sealed = cs.seal().exists();
                let len = fs::file_len(&cs.jrn()).unwrap_or(0) as usize;
                let mut synced = [0u8; 8];
                if let Ok(f) = std::fs::File::open(cs.jrn()) {
                    let _ = fs::pread(&f, OFF_SYNCED as u64, &mut synced);
                }
                let synced = u64::from_le_bytes(synced) as usize;
                let want = if sealed { len } else { len.min(synced) };
                if !cfg!(target_os = "linux") || len != h.map.len() {
                    // Re-map to see new bytes (heap copies) or a new length.
                    if let Ok(nh) = fs::cold_open(&cs.jrn()) {
                        let old = std::mem::replace(&mut c.src, Src::Cold(cs, nh));
                        if let Src::Cold(_, oh) = old {
                            fs::release(None, oh.map);
                        }
                    }
                }
                c.bound = want.min(c.src.map().len());
            }
        }
        if switch {
            self.switch_to_cold()?;
        }
        if let (Some((no, lim)), Some(c)) = (limit, self.cur.as_mut())
            && c.no == no
        {
            c.bound = lim;
        }
        Ok(())
    }

    /// Per poll: a retired hot segment, or (every second) a mapped file whose link is gone, is
    /// released at once — hot continues on the cold copy, a deleted cold copy is a `Deleted` gap.
    fn check_alive(&mut self) -> Result<(), GapInfo> {
        let now = self.now();
        let Some(c) = self.cur.as_mut() else { return Ok(()) };
        let due = now.saturating_sub(c.last_fstat_ns) >= FSTAT_EVERY_NS;
        if due {
            c.last_fstat_ns = now;
        }
        let gone = match &c.src {
            Src::Hot(_, h) => h.map.load32(OFF_RETIRED) == 1 || (due && !fs::still_linked(h)),
            Src::Slot(_, m) => m.load32(OFF_RETIRED) == 1,
            Src::Cold(cs, h) => {
                if due && !(fs::still_linked(h) && cs.jrn().exists()) {
                    let (at, no) = (self.pos, c.no);
                    self.drop_cur();
                    return Err(self.gap(at, no, GapReason::Deleted));
                }
                false
            }
        };
        if gone {
            self.switch_to_cold()?;
        }
        Ok(())
    }

    /// Hot segment retired or deleted: continue on its cold copy at the same offset.
    fn switch_to_cold(&mut self) -> Result<(), GapInfo> {
        let Some(c) = self.cur.take() else { return Ok(()) };
        let no = c.no;
        let fixed = c.fixed;
        match c.src {
            Src::Hot(_, h) => fs::release(None, h.map),
            Src::Cold(_, h) => fs::release(None, h.map),
            Src::Slot(..) => {}
        }
        let stem = fs::cold_stem(&self.cold_jdir, fixed.created_utc_ns, no, fixed.first_seq);
        let cs = ColdSeg {
            first_seq: fixed.first_seq,
            stem,
        };
        let Ok(h) = fs::cold_open(&cs.jrn()) else {
            let at = self.pos;
            return Err(self.gap(at, no, GapReason::Deleted));
        };
        let now = self.now();
        self.cur = Some(Cur {
            no,
            fixed,
            src: Src::Cold(cs, h),
            bound: HEADER_LEN,
            last_fstat_ns: now,
            bad_cache: None,
        });
        self.refresh_bound()
    }

    /// A gap after segment `no` (resume = next existing segment).
    fn gap(&mut self, at: Position, no: u64, reason: GapReason) -> GapInfo {
        let segs = self.list();
        let resume = segs.range(no + 1..).next().map_or(0, |(n, _)| *n);
        GapInfo {
            at,
            expected_seq: self.next_seq,
            resume_segment_no: resume,
            reason,
        }
    }

    fn deliver_gap(&mut self, g: GapInfo) -> Next<'_> {
        self.group = None;
        if self.cfg.on_gap == OnGap::Error || g.resume_segment_no == 0 {
            self.stuck = Some(g);
        } else {
            self.next_open = Some(g.resume_segment_no);
            self.skip_checks = true;
        }
        Next::Gap(g)
    }

    /// Positions at `cfg.start`.
    fn position_start(&mut self, segs: &BTreeMap<u64, Loc>) -> Result<(), OpenErr> {
        let first_no = *segs.keys().next().ok_or(OpenErr::NotFound)?;
        let newest = *segs.keys().next_back().ok_or(OpenErr::NotFound)?;
        match self.cfg.start {
            StartAt::Oldest => self.start_at(first_no, segs, None),
            StartAt::SegmentBoot(k) => {
                if !segs.contains_key(&k) {
                    return Err(OpenErr::SeqUnavailable);
                }
                self.start_at(k, segs, None)
            }
            StartAt::LatestBoot | StartAt::LatestKeyframe => {
                let mut chosen = None;
                for no in segs.keys().rev() {
                    let Some(c) = self.open_seg(*no, segs) else {
                        continue;
                    };
                    let m = c.src.map();
                    let bound = seg_bound(&c);
                    let complete = !self.kind.has_boot() || bound > HEADER_LEN;
                    let kf = if self.cfg.start == StartAt::LatestKeyframe && complete {
                        scan::rebuild_point(m.bytes(0, bound), *no, c.fixed.first_seq, self.kind)
                            .filter(|p| p.offset as usize != HEADER_LEN)
                    } else {
                        None
                    };
                    self.cur = Some(c);
                    self.drop_cur();
                    if complete {
                        chosen = Some((*no, kf));
                        break;
                    }
                }
                let (no, kf) = chosen.ok_or(OpenErr::NotFound)?;
                self.start_at(no, segs, kf.map(|p| (p.offset as usize, p.seq)))
            }
            StartAt::Tail => {
                let c = self.open_seg(newest, segs).ok_or(OpenErr::Io)?;
                let m = c.src.map();
                let (off, last) = match &c.src {
                    Src::Cold(..) => {
                        let b = seg_bound(&c);
                        (b, last_seq_in(m.bytes(0, b), c.fixed.first_seq, self.kind))
                    }
                    _ => (m.load64(OFF_COMMITTED) as usize, m.load64(OFF_LAST_SEQ)),
                };
                self.prev_fixed = None;
                self.cur = Some(c);
                self.off = off;
                self.next_seq = last + 1;
                self.pos = Position {
                    segment_no: newest,
                    offset: off as u64,
                    seq: last,
                };
                Ok(())
            }
            StartAt::Seq(n) => {
                // Last seq of the journal.
                let c = self.open_seg(newest, segs).ok_or(OpenErr::Io)?;
                let b = seg_bound(&c);
                let last = last_seq_in(c.src.map().bytes(0, b), c.fixed.first_seq, self.kind);
                self.cur = Some(c);
                self.drop_cur();
                if n == last + 1 {
                    self.cfg.start = StartAt::Tail;
                    return self.position_start(segs);
                }
                if n > last + 1 {
                    return Err(OpenErr::SeqUnavailable);
                }
                let mut target = None;
                for no in segs.keys().rev() {
                    if let Some(f) = self.header_of(*no, segs)
                        && f.first_seq <= n
                    {
                        target = Some(*no);
                        break;
                    }
                }
                let no = target.ok_or(OpenErr::SeqUnavailable)?;
                let c = self.open_seg(no, segs).ok_or(OpenErr::SeqUnavailable)?;
                let b = seg_bound(&c);
                let found = find_seq(c.src.map().bytes(0, b), c.fixed.first_seq, n, self.rules());
                self.cur = Some(c);
                self.drop_cur();
                let (off, seq) = found.ok_or(OpenErr::SeqUnavailable)?;
                self.start_at(no, segs, Some((off, seq)))
            }
        }
    }

    fn start_at(
        &mut self,
        no: u64,
        segs: &BTreeMap<u64, Loc>,
        at: Option<(usize, u64)>,
    ) -> Result<(), OpenErr> {
        let c = self.open_seg(no, segs).ok_or(OpenErr::Io)?;
        let (off, seq) = at.unwrap_or((HEADER_LEN, c.fixed.first_seq));
        self.next_seq = seq;
        self.off = off;
        self.pos = Position {
            segment_no: no,
            offset: off as u64,
            seq: seq.saturating_sub(1),
        };
        self.cur = Some(c);
        Ok(())
    }

    /// Opens segment `no` after an `End` record or a skipped gap.
    fn advance_to(&mut self, no: u64) -> Option<Next<'static>> {
        let segs = self.list();
        let Some(c) = self.open_seg(no, &segs) else {
            let later = segs.range(no + 1..).next().map(|(n, _)| *n);
            let at = self.pos;
            return match later {
                None if !self.known.contains(&no) => Some(Next::Pending),
                _ => {
                    let reason = if self.known.contains(&no) {
                        GapReason::Deleted
                    } else {
                        GapReason::MissingSegment
                    };
                    let g = GapInfo {
                        at,
                        expected_seq: self.next_seq,
                        resume_segment_no: later.unwrap_or(0),
                        reason,
                    };
                    Some(Next::Gap(g))
                }
            };
        };
        self.known.insert(no);
        let f = c.fixed;
        if c.bad() {
            self.cur = Some(c);
            return Some(Next::Err(ReadErr::Corrupt));
        }
        if !self.skip_checks {
            let gapf = f.flags & RECOVERY_GAP != 0;
            let seq_ok = f.first_seq == self.next_seq;
            let sha_ok = self
                .prev_fixed
                .is_none_or(|p| f.prev_header_sha256 == header_sha(zerocopy::IntoBytes::as_bytes(&p)));
            if !(seq_ok && (sha_ok || gapf)) {
                let reason = if gapf {
                    GapReason::RecoveryGap
                } else {
                    GapReason::ChainBroken
                };
                let g = GapInfo {
                    at: self.pos,
                    expected_seq: self.next_seq,
                    resume_segment_no: no,
                    reason,
                };
                self.cur = Some(c);
                self.drop_cur();
                return Some(Next::Gap(g));
            }
        }
        self.skip_checks = false;
        self.next_seq = f.first_seq;
        self.off = HEADER_LEN;
        self.cur = Some(c);
        None
    }

    /// Next record, reassembled event, or status.
    pub fn poll(&mut self) -> Next<'_> {
        if let Some(g) = self.stuck {
            return Next::Gap(g);
        }
        if self.ended {
            return Next::End;
        }
        loop {
            if let (Some((n, o)), Some(c)) = (self.end_limit, self.cur.as_ref())
                && c.no == n
                && self.off >= o
            {
                self.ended = true;
                return Next::End;
            }
            if let Some(no) = self.next_open {
                self.drop_cur();
                match self.advance_to(no) {
                    None => self.next_open = None,
                    Some(Next::Pending) => return Next::Pending,
                    Some(Next::Gap(g)) => {
                        self.next_open = None;
                        return self.deliver_gap(g);
                    }
                    Some(Next::Err(e)) => {
                        self.next_open = None;
                        return Next::Err(e);
                    }
                    Some(_) => return Next::Pending,
                }
            }
            if self.cur.is_none() {
                return Next::Pending;
            }
            if let Some(c) = self.cur.as_mut() {
                let bad = match c.bad_cache {
                    Some(b) => b,
                    None => *c.bad_cache.insert(c.bad()),
                };
                if bad {
                    return Next::Err(ReadErr::Corrupt);
                }
            }
            if let Err(g) = self.check_alive() {
                return self.deliver_gap(g);
            }
            let Some(bound) = self.cur.as_ref().map(|c| c.bound) else {
                return Next::Pending;
            };
            if self.off >= bound {
                if let Some(t) = self.pending_at
                    && self.now().saturating_sub(t) < u64::from(self.cfg.tail_poll_us) * 1000
                {
                    return Next::Pending;
                }
                if let Err(g) = self.refresh_bound() {
                    return self.deliver_gap(g);
                }
                let (no, bound) = match self.cur.as_ref() {
                    Some(c) => (c.no, c.bound),
                    None => return Next::Pending,
                };
                if self.off >= bound {
                    if self.end_limit.is_some_and(|(n, o)| n == no && self.off >= o) {
                        self.ended = true;
                        return Next::End;
                    }
                    self.pending_at = Some(self.now());
                    return Next::Pending;
                }
                self.pending_at = None;
            }
            match self.step() {
                Step::Again => continue,
                Step::Out(o) => return self.emit(o),
            }
        }
    }

    fn emit(&mut self, o: Out) -> Next<'_> {
        let Some(c) = self.cur.as_ref() else {
            return Next::Pending;
        };
        let m = c.src.map_ref();
        match o {
            Out::Err(e) => Next::Err(e),
            Out::Record { off, len } => {
                // Already checked by `step` (framing, CRC, seq, kind): re-slice without a second CRC.
                let b = m.bytes(off, len);
                let Ok((hdr, _)) = RecordHeader::read_from_prefix(b) else {
                    return Next::Err(ReadErr::Corrupt);
                };
                let end = if RecordFlags(hdr.flags).contains(RecordFlags::CRC) {
                    len - 4
                } else {
                    len
                };
                Next::Record(RecordView {
                    hdr,
                    body: b.get(24..end).unwrap_or(&[]),
                })
            }
            Out::Event(e) => Next::Event(LogicalEvent {
                seq: e.seq,
                kind: EventKind(e.kind),
                tsc: e.tsc,
                flags: RecordFlags(e.flags),
                parts: e.parts,
                body: self.buf.get(..e.total).unwrap_or(&[]),
            }),
        }
    }

    /// Consumes one record at `off`.
    fn step(&mut self) -> Step {
        let rules = self.rules();
        let cap = self.cfg.reassembly_cap;
        let Some(c) = self.cur.as_ref() else {
            return Step::Out(Out::Err(ReadErr::Corrupt));
        };
        let m = c.src.map();
        let no = c.no;
        let buf = m.bytes(self.off, c.bound.saturating_sub(self.off));
        let v = match scan::check(buf, self.next_seq, rules) {
            Ok(Some(v)) => v,
            Ok(None) => return Step::Out(Out::Err(ReadErr::Corrupt)),
            Err(e) => return Step::Out(Out::Err(e)),
        };
        let off = self.off;
        let len = v.hdr.len as usize;
        let seq = v.hdr.seq;
        let flags = RecordFlags(v.hdr.flags);
        let multi = cap > 0 && scan::is_multi(v.hdr.kind);
        if self.boot_closing {
            self.in_boot = false;
            self.boot_closing = false;
        }
        if !multi && self.group.is_some() {
            return Step::Out(Out::Err(ReadErr::Corrupt));
        }
        let mut out = Out::Record { off, len };
        if multi {
            let Some(ph) = scan::part_hdr(v.body) else {
                return Step::Out(Out::Err(ReadErr::Corrupt));
            };
            let last = ph.part.wrapping_add(1) == ph.parts;
            if flags.contains(RecordFlags::PART) == last || ph.parts == 0 {
                self.group = None;
                return Step::Out(Out::Err(ReadErr::Corrupt));
            }
            let g = match self.group {
                None => {
                    if ph.part != 0 {
                        return Step::Out(Out::Err(ReadErr::Corrupt));
                    }
                    if ph.total_len as usize > cap {
                        return Step::Out(Out::Err(ReadErr::ReassemblyCap));
                    }
                    self.buf.clear();
                    Group {
                        first_seq: seq,
                        kind: v.hdr.kind,
                        tsc: v.hdr.tsc,
                        parts: ph.parts,
                        next: 0,
                        total: ph.total_len as usize,
                    }
                }
                Some(g) => {
                    if ph.part != g.next
                        || ph.parts != g.parts
                        || v.hdr.kind != g.kind
                        || v.hdr.tsc != g.tsc
                        || ph.total_len as usize != g.total
                    {
                        self.group = None;
                        return Step::Out(Out::Err(ReadErr::Corrupt));
                    }
                    g
                }
            };
            let chunk = v.body.get(16..16 + ph.chunk_len as usize);
            let Some(chunk) = chunk.filter(|ch| self.buf.len() + ch.len() <= g.total) else {
                self.group = None;
                return Step::Out(Out::Err(ReadErr::Corrupt));
            };
            self.buf.extend_from_slice(chunk);
            if last {
                if self.buf.len() != g.total {
                    self.group = None;
                    return Step::Out(Out::Err(ReadErr::Corrupt));
                }
                self.group = None;
                out = Out::Event(Ev {
                    seq: g.first_seq,
                    kind: g.kind,
                    tsc: g.tsc,
                    flags: v.hdr.flags,
                    parts: g.parts,
                    total: g.total,
                });
            } else {
                self.group = Some(Group {
                    next: g.next + 1,
                    ..g
                });
            }
        }
        // Boot-block tracking and segment hand-over.
        let kind = EventKind(v.hdr.kind);
        let mut end_other = None;
        if kind == EventKind::SEGMENT_ROLL {
            let mark = v.body.get(32).copied().unwrap_or(0);
            let other = v
                .body
                .get(8..16)
                .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap_or([0; 8])));
            if mark == RollMark::Begin as u8 {
                self.in_boot = true;
                self.boot_closing = !self.kind.has_boot();
            } else if mark == RollMark::BootDone as u8 {
                self.boot_closing = true;
            } else if mark == RollMark::End as u8 {
                end_other = Some(if other == 0 { no + 1 } else { other });
            }
        }
        self.off = off + len;
        self.next_seq = seq + 1;
        self.pos = Position {
            segment_no: no,
            offset: self.off as u64,
            seq,
        };
        if let Some(n) = end_other {
            self.prev_fixed = self.cur.as_ref().map(|c| c.fixed);
            self.next_open = Some(n);
        }
        match out {
            Out::Record { .. } if multi => Step::Again,
            o => Step::Out(o),
        }
    }

    /// Position after the last delivered record.
    pub fn position(&self) -> Position {
        self.pos
    }

    /// Immutable header of the segment currently being read. After a delivered Record/Event
    /// this is its segment, including when `poll` crossed a segment boundary. No file is opened
    /// or mapped by this accessor; the reference lasts until the next mutable reader operation.
    pub fn segment_header(&self) -> Option<&SegmentHeaderFixed> {
        self.cur.as_ref().map(|c| &c.fixed)
    }

    /// Whether the last delivered record belongs to a boot block.
    pub fn in_boot_block(&self) -> bool {
        self.in_boot
    }

    /// Position after the last `EVENT_END` record of the newest segment (the recovery truncation
    /// point; README "恢复" step 2).
    pub fn last_complete_event(&self) -> Option<Position> {
        let segs = self.list();
        self.compute_last_complete(&segs)
    }

    fn compute_last_complete(&self, segs: &BTreeMap<u64, Loc>) -> Option<Position> {
        let (no, l) = segs.iter().next_back()?;
        let h = match &l.hot {
            Some(p) => fs::hot_open(p, false).ok()?,
            None => fs::cold_open(&l.cold.as_ref()?.jrn()).ok()?,
        };
        let r = self.last_complete_in(*no, l, h.map);
        fs::release(None, h.map);
        r
    }

    fn last_complete_in(&self, no: u64, l: &Loc, m: SegMap) -> Option<Position> {
        let b = m.bytes(0, m.len());
        let f = SegmentHeaderFixed::read_from_bytes(b.get(..HEADER_FIXED_LEN)?).ok()?;
        let kind = JournalKind::from_u8(f.journal_kind)?;
        let rd = |o: usize| {
            b.get(o..o + 8)
                .map_or(0, |x| u64::from_le_bytes(x.try_into().unwrap_or([0; 8])))
        };
        let (from, seq) = if l.hot.is_some() {
            (rd(OFF_COMMITTED) as usize, rd(OFF_LAST_SEQ))
        } else {
            (HEADER_LEN, f.first_seq - 1)
        };
        let st = b
            .get(OFF_STATE..OFF_STATE + 4)
            .map_or(0, |x| u32::from_le_bytes(x.try_into().unwrap_or([0; 4])));
        if l.hot.is_some() && st == SegState::Sealed as u32 {
            return Some(Position {
                segment_no: no,
                offset: from as u64,
                seq,
            });
        }
        let t = scan::scan_tail(b, from.max(HEADER_LEN), seq, kind);
        Some(Position {
            segment_no: no,
            offset: t.end as u64,
            seq: t.end_seq,
        })
    }

    /// Test hook: manual reader clock in ns (tail-poll interval and `fstat` cadence).
    #[doc(hidden)]
    pub fn set_clock_ns(&mut self, ns: u64) {
        if self.clock.is_none()
            && let Some(c) = self.cur.as_mut()
        {
            c.last_fstat_ns = ns;
        }
        self.clock = Some(ns);
    }

    /// Test hook: number of writer-row (`committed_len`) reads so far.
    #[doc(hidden)]
    pub fn writer_row_reads(&self) -> u64 {
        self.row_reads
    }

    /// Test hook: base address of the current segment mapping (0 = none).
    #[doc(hidden)]
    pub fn debug_map_addr(&self) -> usize {
        self.cur.as_ref().map_or(0, |c| c.src.map().base_addr())
    }

    /// Test hook: behave like a production build (`0x7Fxx` → `DevKind`).
    #[doc(hidden)]
    pub fn set_production(&mut self, on: bool) {
        self.production = on;
    }
}

impl Drop for JournalReader {
    fn drop(&mut self) {
        self.drop_cur();
    }
}

#[derive(Copy, Clone, Debug)]
struct Ev {
    seq: u64,
    kind: u16,
    tsc: u64,
    flags: u16,
    parts: u16,
    total: usize,
}

#[derive(Copy, Clone, Debug)]
enum Out {
    Record { off: usize, len: usize },
    Event(Ev),
    Err(ReadErr),
}

enum Step {
    Again,
    Out(Out),
}

/// Readable end of a freshly opened segment.
fn seg_bound(c: &Cur) -> usize {
    let m = c.src.map();
    match &c.src {
        Src::Cold(cs, _) => {
            if cs.seal().exists() {
                m.len()
            } else {
                (m.load64(OFF_SYNCED) as usize).min(m.len())
            }
        }
        _ => (m.load64(OFF_COMMITTED) as usize).min(m.len()),
    }
}

/// Seq of the last record in `[4096, len)`.
fn last_seq_in(b: &[u8], first_seq: u64, kind: JournalKind) -> u64 {
    let rules = KindRules {
        journal: kind,
        require_registered: false,
        production: false,
    };
    let mut off = HEADER_LEN;
    let mut seq = first_seq;
    while let Some(buf) = b.get(off..) {
        match scan::check(buf, seq, rules) {
            Ok(Some(v)) => {
                off += v.hdr.len as usize;
                seq += 1;
            }
            _ => break,
        }
    }
    seq - 1
}

/// Offset and seq of the record holding seq `n` (group start when inside a multi-part group).
fn find_seq(b: &[u8], first_seq: u64, n: u64, rules: KindRules) -> Option<(usize, u64)> {
    let mut off = HEADER_LEN;
    let mut seq = first_seq;
    let mut group_start: Option<(usize, u64)> = None;
    while let Some(buf) = b.get(off..) {
        let Ok(Some(v)) = scan::check(buf, seq, rules) else {
            return None;
        };
        let multi = scan::is_multi(v.hdr.kind);
        let ph = if multi { scan::part_hdr(v.body) } else { None };
        if let Some(ph) = ph
            && ph.part == 0
        {
            group_start = Some((off, seq));
        }
        if seq == n {
            return match ph {
                Some(ph) if ph.part != 0 => group_start,
                _ => Some((off, seq)),
            };
        }
        if !RecordFlags(v.hdr.flags).contains(RecordFlags::PART) {
            group_start = None;
        }
        off += v.hdr.len as usize;
        seq += 1;
    }
    None
}
