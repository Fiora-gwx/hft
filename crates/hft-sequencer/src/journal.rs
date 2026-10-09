//! The journal sink the sequencer writes through (`JournalSink`), the in-memory journal
//! (`MemJournal`, used by tests, the replay drivers and as the reference of the record model),
//! the per-event output buffer and the output hash (README "每条 Input 的处理顺序", "状态哈希").

use hft_event::{BlobRef, Body, EventKind, MEASURE_MASK, RecordFlags, Role, SegmentRoll, classify};
use hft_journal::{AppendErr, RollAt, RollErr, RollReason};
use hft_rt::CalibPair;
use hft_types::{Seq, Tsc};
use zerocopy::IntoBytes;

/// What a roll needs from the environment (segment header values of the new segment).
#[derive(Copy, Clone, Debug)]
pub struct RollMeta<'a> {
    pub utc_ns: i64,
    pub calib: CalibPair,
    pub tsc_hz_milli: u64,
    pub configs: &'a [BlobRef],
}

/// The clock values of the current segment header (`Now.utc_ns`/`tsc_per_ms` come from them).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct SegClock {
    pub tsc_hz_milli: u64,
    pub calib_tsc: u64,
    pub calib_realtime_ns: i64,
}

/// Appends of the boot block of a new segment (after `JournalSink::roll`).
pub trait BootSink {
    /// One boot-block record (logical body from `pieces`).
    fn append_var(&mut self, kind: EventKind, tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr>;
    /// `SegmentRoll{BootDone}` + publish.
    fn finish(self) -> Result<Seq, AppendErr>;
}

/// The writer side of the seq journal as the sequencer uses it. Implemented by hft-journal's
/// `JournalWriter` (feature `live`), by [`MemJournal`] and by the replay drivers' aligned sinks.
pub trait JournalSink {
    type Boot<'a>: BootSink
    where
        Self: 'a;
    /// One record from scattered pieces of its logical body; seq comes from the writer only.
    fn append_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<Seq, AppendErr>;
    /// Fixed-layout convenience.
    fn append<B: Body>(&mut self, flags: RecordFlags, tsc: Tsc, body: &B) -> Result<Seq, AppendErr> {
        self.append_var(B::KIND, flags, tsc, &[body.as_bytes()])
    }
    /// `EVENT_END` on the last non-Meta record appended since the previous `end_event`.
    fn end_event(&mut self);
    /// Makes every complete event visible to readers.
    fn publish(&mut self);
    /// Seals the current segment (`SegmentRoll{End}`) and opens the next (`Begin` written); the
    /// returned sink writes the boot block.
    fn roll(&mut self, reason: RollReason, info: &RollMeta<'_>) -> Result<Self::Boot<'_>, RollErr>;
    /// Size-triggered roll request (two thresholds, hft-journal); `None` when not needed.
    fn needs_roll(&self, now: Tsc) -> Option<(RollReason, RollAt)>;
}

/// Masks the measure fields of an Output logical body in place (hft-event `MEASURE_MASK`).
pub fn mask_measure(kind: EventKind, body: &mut [u8]) {
    for (k, off, len) in MEASURE_MASK.iter() {
        if *k == kind {
            let o = usize::from(*off);
            if let Some(s) = body.get_mut(o..o + usize::from(*len)) {
                s.fill(0);
            }
        }
    }
}

/// Streaming xxh3-128 of Output records (measure fields zeroed): `kind ‖ flags ‖ body`.
pub struct OutHash {
    h: xxhash_rust::xxh3::Xxh3,
    scratch: Vec<u8>,
}

impl core::fmt::Debug for OutHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OutHash").finish_non_exhaustive()
    }
}

impl Default for OutHash {
    fn default() -> OutHash {
        OutHash::new()
    }
}

/// Flag bits that are not part of an Output's comparable identity.
pub const NONCMP_FLAGS: u16 = RecordFlags::EVENT_END.0 | RecordFlags::CRC.0 | RecordFlags::PART.0;

impl OutHash {
    pub fn new() -> OutHash {
        OutHash {
            h: xxhash_rust::xxh3::Xxh3::new(),
            scratch: Vec::with_capacity(4096),
        }
    }
    pub fn reset(&mut self) {
        self.h.reset();
    }
    /// Adds one Output record.
    pub fn add(&mut self, kind: EventKind, flags: RecordFlags, pieces: &[&[u8]]) {
        self.h.update(&kind.0.to_le_bytes());
        self.h.update(&(flags.0 & !NONCMP_FLAGS).to_le_bytes());
        let masked = MEASURE_MASK.iter().any(|(k, ..)| *k == kind);
        if masked {
            self.scratch.clear();
            for p in pieces {
                self.scratch.extend_from_slice(p);
            }
            mask_measure(kind, &mut self.scratch);
            self.h.update(&self.scratch);
        } else {
            for p in pieces {
                self.h.update(p);
            }
        }
    }
    pub fn digest(&self) -> [u8; 16] {
        self.h.digest128().to_le_bytes()
    }
}

/// The role of a record (Input, Output, Meta) from its kind and logical body.
pub fn role_of(kind: EventKind, body: &[u8]) -> Role {
    classify(kind, body).unwrap_or(Role::Meta)
}

/// One record of a [`MemJournal`] (logical body: Multi records are not split).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemRecord {
    pub seq: u64,
    pub kind: EventKind,
    pub flags: RecordFlags,
    pub tsc: u64,
    pub body: Vec<u8>,
}

impl MemRecord {
    pub fn role(&self) -> Role {
        role_of(self.kind, &self.body)
    }
    /// The body as `B` (zero-extended / truncated).
    pub fn read<B: Body>(&self) -> Option<B> {
        crate::types::read_body::<B>(&self.body)
    }
}

/// One segment of a [`MemJournal`].
#[derive(Clone, Debug, Default)]
pub struct MemSegment {
    pub no: u64,
    pub reason: u8,
    pub clock: SegClock,
    pub records: Vec<MemRecord>,
}

/// Fault hooks of a [`MemJournal`] (tests).
#[derive(Clone, Debug, Default)]
pub struct MemFaults {
    /// The n-th (0-based, counted from installation) Output append returns `Full`.
    pub full_at_output: Option<u64>,
    pub outputs_seen: u64,
    /// The next `roll` calls return `NextNotReady` this many times.
    pub roll_not_ready: u32,
    /// `needs_roll` answer.
    pub needs_roll: Option<(RollReason, RollAt)>,
    /// Override of the seq returned by the next Input append (SEQ-121).
    pub force_seq: Option<u64>,
}

/// In-memory seq journal: records in seq order grouped by segment; `committed` is the count of
/// published records. Used by tests and replay; behaves like hft-journal's writer for EVENT_END
/// placement and publication.
#[derive(Clone, Debug, Default)]
pub struct MemJournal {
    pub segments: Vec<MemSegment>,
    pub next_seq: u64,
    /// Records visible to readers (count over all segments).
    pub committed: usize,
    /// Count of all records (published or not).
    total: usize,
    /// Index (segment, record) of the last non-Meta record since the last `end_event`.
    last_non_meta: Option<(usize, usize)>,
    /// Records before this index are complete events (event boundary).
    boundary: usize,
    pub faults: MemFaults,
    /// `publish` calls (tests).
    pub publishes: u64,
    /// Byte length of all published records (a stand-in for `committed_len`).
    pub committed_len: u64,
    pending_len: u64,
    in_boot: bool,
}

impl MemJournal {
    /// A journal whose first segment carries `clock`.
    pub fn new(clock: SegClock) -> MemJournal {
        MemJournal {
            segments: vec![MemSegment {
                no: 1,
                reason: RollReason::Startup as u8,
                clock,
                records: Vec::new(),
            }],
            next_seq: 1,
            ..MemJournal::default()
        }
    }
    /// Every record in seq order.
    pub fn records(&self) -> impl Iterator<Item = &MemRecord> + '_ {
        self.segments.iter().flat_map(|s| s.records.iter())
    }
    /// Every published record in seq order.
    pub fn published(&self) -> impl Iterator<Item = &MemRecord> + '_ {
        self.records().take(self.committed)
    }
    pub fn last_segment(&self) -> Option<&MemSegment> {
        self.segments.last()
    }
    pub fn len(&self) -> usize {
        self.total
    }
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
    /// Drops every record after the last one carrying `EVENT_END` (the half event of a crash);
    /// returns the number of records dropped.
    pub fn truncate_to_event_end(&mut self) -> usize {
        let mut dropped = 0;
        loop {
            let Some(seg) = self.segments.last_mut() else {
                break;
            };
            match seg.records.last() {
                Some(r) if r.flags.0 & RecordFlags::EVENT_END.0 == 0 => {
                    seg.records.pop();
                    dropped += 1;
                }
                Some(_) => break,
                None => {
                    if self.segments.len() > 1 {
                        self.segments.pop();
                    } else {
                        break;
                    }
                }
            }
        }
        self.total -= dropped;
        self.next_seq = self.records().last().map_or(1, |r| r.seq + 1);
        self.boundary = self.total;
        self.committed = self.committed.min(self.total);
        self.last_non_meta = None;
        dropped
    }
    fn push(&mut self, kind: EventKind, flags: RecordFlags, tsc: u64, pieces: &[&[u8]]) -> u64 {
        let mut body = Vec::with_capacity(pieces.iter().map(|p| p.len()).sum());
        for p in pieces {
            body.extend_from_slice(p);
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let role = role_of(kind, &body);
        let len = 24 + body.len().div_ceil(8) as u64 * 8;
        if self.segments.is_empty() {
            self.segments.push(MemSegment::default());
        }
        let si = self.segments.len() - 1;
        let Some(seg) = self.segments.last_mut() else {
            return seq;
        };
        seg.records.push(MemRecord {
            seq,
            kind,
            flags: RecordFlags(flags.0 & !RecordFlags::EVENT_END.0),
            tsc,
            body,
        });
        if role != Role::Meta {
            self.last_non_meta = Some((si, seg.records.len() - 1));
        }
        self.total += 1;
        self.pending_len += len;
        seq
    }
}

/// Boot-block writer of a [`MemJournal`].
#[derive(Debug)]
pub struct MemBoot<'a> {
    j: &'a mut MemJournal,
    n: u32,
}

impl BootSink for MemBoot<'_> {
    fn append_var(&mut self, kind: EventKind, tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr> {
        self.n += 1;
        Ok(Seq(self.j.push(kind, RecordFlags(0), tsc.0, pieces)))
    }
    fn finish(self) -> Result<Seq, AppendErr> {
        let mut sr = SegmentRoll::new_zeroed_body();
        sr.mark = 3;
        sr.journal_kind = 2;
        sr.boot_records = self.n + 1;
        sr.segment_no = self.j.segments.last().map_or(0, |s| s.no);
        let seq = self.j.push(
            EventKind::SEGMENT_ROLL,
            RecordFlags::EVENT_END,
            0,
            &[sr.as_bytes()],
        );
        // The BootDone record ends the boot block (Meta, but it carries EVENT_END).
        if let Some(r) = self.j.segments.last_mut().and_then(|s| s.records.last_mut()) {
            r.flags = RecordFlags(r.flags.0 | RecordFlags::EVENT_END.0);
        }
        self.j.in_boot = false;
        self.j.last_non_meta = None;
        self.j.boundary = self.j.total;
        self.j.publish();
        Ok(Seq(seq))
    }
}

trait ZeroedBody {
    fn new_zeroed_body() -> Self;
}
impl ZeroedBody for SegmentRoll {
    fn new_zeroed_body() -> SegmentRoll {
        <SegmentRoll as zerocopy::FromZeros>::new_zeroed()
    }
}

impl JournalSink for MemJournal {
    type Boot<'a> = MemBoot<'a>;
    fn append_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<Seq, AppendErr> {
        if self.in_boot {
            return Err(AppendErr::InBoot);
        }
        let mut probe = [0u8; 80];
        let mut n = 0;
        for p in pieces {
            for b in p.iter() {
                if let Some(s) = probe.get_mut(n) {
                    *s = *b;
                }
                n += 1;
                if n >= probe.len() {
                    break;
                }
            }
        }
        let role = role_of(kind, probe.get(..n.min(80)).unwrap_or(&[]));
        if role == Role::Output {
            let k = self.faults.outputs_seen;
            self.faults.outputs_seen += 1;
            if self.faults.full_at_output == Some(k) {
                return Err(AppendErr::Full);
            }
        }
        let seq = self.push(kind, flags, tsc.0, pieces);
        if role == Role::Input
            && let Some(s) = self.faults.force_seq.take()
        {
            if let Some(r) = self.segments.last_mut().and_then(|g| g.records.last_mut()) {
                r.seq = s;
            }
            return Ok(Seq(s));
        }
        Ok(Seq(seq))
    }
    fn end_event(&mut self) {
        if let Some((si, ri)) = self.last_non_meta.take()
            && let Some(r) = self.segments.get_mut(si).and_then(|s| s.records.get_mut(ri))
        {
            r.flags = RecordFlags(r.flags.0 | RecordFlags::EVENT_END.0);
        }
        self.boundary = self.total;
    }
    fn publish(&mut self) {
        self.publishes += 1;
        if self.boundary > self.committed {
            // Recompute the published byte length (records are 8-aligned; header 24 B).
            let mut len = 0u64;
            for r in self.records().take(self.boundary) {
                len += 24 + r.body.len().div_ceil(8) as u64 * 8;
            }
            self.committed_len = len;
            self.committed = self.boundary;
        }
    }
    fn roll(&mut self, reason: RollReason, info: &RollMeta<'_>) -> Result<MemBoot<'_>, RollErr> {
        if self.faults.roll_not_ready > 0 {
            self.faults.roll_not_ready -= 1;
            return Err(RollErr::NextNotReady);
        }
        // A fresh journal: the first segment is opened in place (no `End`).
        if self.total == 0 {
            let clock = SegClock {
                tsc_hz_milli: info.tsc_hz_milli,
                calib_tsc: info.calib.tsc.0,
                calib_realtime_ns: info.calib.realtime_ns,
            };
            if self.segments.is_empty() {
                self.segments.push(MemSegment::default());
            }
            if let Some(s) = self.segments.last_mut() {
                s.no = s.no.max(1);
                s.reason = reason as u8;
                s.clock = clock;
            }
            if self.next_seq == 0 {
                self.next_seq = 1;
            }
            let no = self.segments.last().map_or(1, |s| s.no);
            let mut begin = SegmentRoll::new_zeroed_body();
            begin.mark = 2;
            begin.journal_kind = 2;
            begin.segment_no = no;
            begin.seq_mark = self.next_seq;
            begin.utc_ns = info.utc_ns;
            begin.reason = reason as u8;
            self.push(
                EventKind::SEGMENT_ROLL,
                RecordFlags(0),
                info.calib.tsc.0,
                &[begin.as_bytes()],
            );
            return Ok(MemBoot { j: self, n: 1 });
        }
        let old_no = self.segments.last().map_or(0, |s| s.no);
        let mut end = SegmentRoll::new_zeroed_body();
        end.mark = 1;
        end.journal_kind = 2;
        end.segment_no = old_no;
        end.other_segment_no = old_no + 1;
        end.seq_mark = self.next_seq.saturating_sub(1);
        end.utc_ns = info.utc_ns;
        end.reason = reason as u8;
        self.push(
            EventKind::SEGMENT_ROLL,
            RecordFlags::EVENT_END,
            0,
            &[end.as_bytes()],
        );
        if let Some(r) = self.segments.last_mut().and_then(|s| s.records.last_mut()) {
            r.flags = RecordFlags(r.flags.0 | RecordFlags::EVENT_END.0);
        }
        self.boundary = self.total;
        self.last_non_meta = None;
        self.publish();
        self.segments.push(MemSegment {
            no: old_no + 1,
            reason: reason as u8,
            clock: SegClock {
                tsc_hz_milli: info.tsc_hz_milli,
                calib_tsc: info.calib.tsc.0,
                calib_realtime_ns: info.calib.realtime_ns,
            },
            records: Vec::new(),
        });
        let mut begin = SegmentRoll::new_zeroed_body();
        begin.mark = 2;
        begin.journal_kind = 2;
        begin.segment_no = old_no + 1;
        begin.other_segment_no = old_no;
        begin.seq_mark = self.next_seq;
        begin.utc_ns = info.utc_ns;
        begin.reason = reason as u8;
        self.push(
            EventKind::SEGMENT_ROLL,
            RecordFlags(0),
            info.calib.tsc.0,
            &[begin.as_bytes()],
        );
        self.in_boot = false;
        Ok(MemBoot { j: self, n: 1 })
    }
    fn needs_roll(&self, _now: Tsc) -> Option<(RollReason, RollAt)> {
        self.faults.needs_roll
    }
}

#[cfg(feature = "live")]
mod live_writer {
    use super::{BootSink, JournalSink, RollMeta};
    use hft_event::{EventKind, RecordFlags};
    use hft_journal::{AppendErr, BootWriter, JournalWriter, RollAt, RollErr, RollInfo, RollReason};
    use hft_types::{Seq, Tsc};

    impl BootSink for BootWriter<'_> {
        fn append_var(&mut self, kind: EventKind, tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr> {
            BootWriter::append_var(self, kind, tsc, pieces)
        }
        fn finish(self) -> Result<Seq, AppendErr> {
            BootWriter::finish(self)
        }
    }

    impl JournalSink for JournalWriter {
        type Boot<'a> = BootWriter<'a>;
        fn append_var(
            &mut self,
            kind: EventKind,
            flags: RecordFlags,
            tsc: Tsc,
            pieces: &[&[u8]],
        ) -> Result<Seq, AppendErr> {
            JournalWriter::append_var(self, kind, flags, tsc, pieces)
        }
        fn end_event(&mut self) {
            JournalWriter::end_event(self);
        }
        fn publish(&mut self) {
            JournalWriter::publish(self);
        }
        fn roll(&mut self, reason: RollReason, info: &RollMeta<'_>) -> Result<BootWriter<'_>, RollErr> {
            JournalWriter::roll(
                self,
                reason,
                &RollInfo {
                    utc_ns: info.utc_ns,
                    calib: info.calib,
                    tsc_hz_milli: info.tsc_hz_milli,
                    configs: info.configs,
                },
            )
        }
        fn needs_roll(&self, now: Tsc) -> Option<(RollReason, RollAt)> {
            JournalWriter::needs_roll(self, now)
        }
    }
}
