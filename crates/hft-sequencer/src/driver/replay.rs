//! The replay drivers (README "冷启动恢复", "验证回放", advisor mirror): Inputs are handed in
//! journal order (≤ `replay_per_round` per round); Outputs are compared with the log (verify:
//! failure; recover: mismatch list; mirror: not compared); nothing is emitted.

use hft_event::{BlobHdr, BlobKind, BlobRef, ClockStatus, EventKind, HostProfile, RecordFlags, Role};
use hft_journal::{AppendErr, RollAt, RollErr, RollReason};
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, KillBits, Seq, Tsc, VenueId};
use zerocopy::FromBytes;

use super::{Driver, DriverMode, DriverView, ParseCx, RollInfo};
use crate::journal::{BootSink, JournalSink, MemRecord, MemSegment, NONCMP_FLAGS, mask_measure, role_of};
use crate::parsed::{ParsedConfig, parse_blob};
use crate::types::{ConfigErr, EmitResult, InputSlot, OutRecord};

/// Which replay.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ReplayMode {
    Recover,
    Verify,
    Mirror,
}

/// One Output that differs from the log.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Mismatch {
    /// Seq of the logged record.
    pub seq: u64,
    pub kind: EventKind,
    /// (venue, account) of the record when it carries one.
    pub venue: VenueId,
    pub account: AccountId,
}

/// A differing old-tail Output covered by validated recovery evidence (offline only).
#[cfg(feature = "replay-io")]
#[derive(Debug)]
pub struct RecoveryDiff {
    pub mismatch: Mismatch,
    /// Masked kind followed by body; empty means missing on that side.
    pub logged: Vec<u8>,
    pub regenerated: Vec<u8>,
    pub flags: (Option<u16>, Option<u16>),
}

/// A header-marked recovery boundary retained independently of numerical seq gaps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryBoundary {
    /// Logical record index of the marked segment's Begin.
    pub first: usize,
    pub segment_no: u64,
    pub first_seq: u64,
    pub prev_last_seq: u64,
}

/// A flat view of the logged records with segment boundaries.
#[derive(Clone, Debug, Default)]
pub struct Log {
    pub recs: Vec<MemRecord>,
    /// Index of the first record of every segment.
    pub seg_start: Vec<usize>,
    pub segs: Vec<crate::journal::SegClock>,
    /// Last physical seq of split logical records, sorted by the first part's seq.
    /// MemJournal records and single-part file records need no entry.
    pub part_ends: Vec<(u64, u64)>,
    /// Header flags retained even when seq is contiguous or the supplied input is a suffix.
    /// A marked boundary is evidence of a possibly lost tail, not its size.
    pub recovery_gaps: Vec<RecoveryBoundary>,
}

impl Log {
    pub fn from_segments(segs: &[MemSegment]) -> Log {
        let mut l = Log::default();
        for s in segs {
            l.seg_start.push(l.recs.len());
            l.segs.push(s.clock);
            l.recs.extend(s.records.iter().cloned());
        }
        l
    }
    /// Segment index of a record index.
    pub fn seg_of(&self, ix: usize) -> usize {
        self.seg_start.iter().rposition(|s| *s <= ix).unwrap_or(0)
    }

    /// Inclusive physical end of a retained logical record (including its final part).
    pub fn record_end_seq(&self, ix: usize) -> Option<u64> {
        let seq = self.recs.get(ix)?.seq;
        Some(
            self.part_ends
                .binary_search_by_key(&seq, |p| p.0)
                .ok()
                .and_then(|i| self.part_ends.get(i))
                .map_or(seq, |p| p.1),
        )
    }
}

/// The aligned sink of the replay drivers.
#[derive(Debug)]
pub struct ReplaySink {
    pub mode: ReplayMode,
    pub log: Log,
    /// Next record to align with.
    pub pos: usize,
    pub mismatches: Vec<Mismatch>,
    /// Outputs compared.
    pub compared: u64,
    pub snapshots_compared: u64,
    /// `(seq, logged, regenerated)` masked bodies of the first mismatches (diagnostics).
    pub detail: Vec<(u64, Vec<u8>, Vec<u8>)>,
    pub detail_flags: Vec<(Option<u16>, Option<u16>)>,
    /// Inclusive comparison window. Warm-up still consumes every preceding Input.
    pub compare_from: u64,
    pub compare_to: u64,
    /// Stop accepting Inputs after the first in-window divergence (offline verify only).
    pub stop_on_first: bool,
    pub halted: bool,
    /// Diagnostic bound supplied by the cold caller; ordinary replay retains 16 details.
    pub detail_limit: usize,
    /// Exclusive record boundary of an offline independent run. Other drivers use MAX.
    pub record_end: usize,
    #[cfg(feature = "replay-io")]
    pub recovery_diffs: Vec<RecoveryDiff>,
    #[cfg(feature = "replay-io")]
    pub(super) recoveries: Vec<super::verify_recovery::RecoveryRange>,
    #[cfg(feature = "replay-io")]
    pub(super) recovery_notes: Vec<usize>,
    #[cfg(feature = "replay-io")]
    pub(super) diagnostic_overflow: bool,
    /// Offline Model replay must keep Inputs even when a changed Advice creates extra Outputs.
    #[cfg(feature = "replay-io")]
    pub(super) preserve_inputs: bool,
    scratch_a: Vec<u8>,
    scratch_b: Vec<u8>,
}

pub(super) fn venue_account(kind: EventKind, body: &[u8]) -> (VenueId, AccountId) {
    let b = |o: usize| body.get(o).copied().unwrap_or(0);
    match kind {
        EventKind::ORDER_DECISION => (VenueId(b(57)), AccountId(b(58))),
        EventKind::EXEC_DECISION => (VenueId(0), AccountId(b(25))),
        EventKind::FUNDING_SETTLE => (VenueId(b(0)), AccountId(b(1))),
        _ => (VenueId(0), AccountId(0)),
    }
}

impl ReplaySink {
    pub fn new(mode: ReplayMode, log: Log) -> ReplaySink {
        ReplaySink {
            mode,
            log,
            pos: 0,
            mismatches: Vec::new(),
            compared: 0,
            snapshots_compared: 0,
            detail: Vec::new(),
            detail_flags: Vec::new(),
            compare_from: 0,
            compare_to: u64::MAX,
            stop_on_first: false,
            halted: false,
            detail_limit: 16,
            record_end: usize::MAX,
            #[cfg(feature = "replay-io")]
            recovery_diffs: Vec::new(),
            #[cfg(feature = "replay-io")]
            recoveries: Vec::new(),
            #[cfg(feature = "replay-io")]
            recovery_notes: Vec::new(),
            #[cfg(feature = "replay-io")]
            diagnostic_overflow: false,
            #[cfg(feature = "replay-io")]
            preserve_inputs: false,
            scratch_a: Vec::with_capacity(1 << 16),
            scratch_b: Vec::with_capacity(1 << 16),
        }
    }
    fn comparing(&self, seq: u64) -> bool {
        self.mode != ReplayMode::Mirror && seq >= self.compare_from && seq <= self.compare_to
    }
    fn record_mismatch(
        &mut self,
        m: Mismatch,
        logged: Vec<u8>,
        regenerated: Vec<u8>,
        flags: (Option<u16>, Option<u16>),
    ) {
        let used = self.detail.len();
        #[cfg(feature = "replay-io")]
        let used = used + self.recovery_diffs.len();
        #[cfg(feature = "replay-io")]
        {
            self.diagnostic_overflow |= used >= self.detail_limit;
            if self.mode == ReplayMode::Verify && self.recoveries.iter().any(|r| r.accepts(&self.log, &m)) {
                if used < self.detail_limit {
                    self.recovery_diffs.push(RecoveryDiff {
                        mismatch: m,
                        logged,
                        regenerated,
                        flags,
                    });
                }
                return;
            }
        }
        if used < self.detail_limit {
            self.detail.push((m.seq, logged, regenerated));
            self.detail_flags.push(flags);
        }
        self.mismatches.push(m);
        self.halted = self.stop_on_first;
    }
    fn missing(&mut self, ix: usize) {
        // These pre-EngineClock Notes are evidence already checked by the cold planner.
        // They are emitted by live boot, not by replaying from its completed boot snapshot.
        #[cfg(feature = "replay-io")]
        if self.mode == ReplayMode::Verify && self.recovery_notes.binary_search(&ix).is_ok() {
            return;
        }
        let Some(r) = self.log.recs.get(ix) else { return };
        if r.role() != Role::Output || r.kind == EventKind::SEGMENT_ROLL || !self.comparing(r.seq) {
            return;
        }
        if self.halted {
            return;
        }
        let mut body = r.body.clone();
        mask_measure(r.kind, &mut body);
        body.splice(0..0, r.kind.0.to_le_bytes());
        let flags = (Some(r.flags.0 & !NONCMP_FLAGS), None);
        let (venue, account) = venue_account(r.kind, &r.body);
        self.record_mismatch(
            Mismatch {
                seq: r.seq,
                kind: r.kind,
                venue,
                account,
            },
            body,
            Vec::new(),
            flags,
        );
    }
    fn skip_meta(&mut self) {
        while self.pos < self.record_end {
            let Some(r) = self.log.recs.get(self.pos) else {
                break;
            };
            if r.role() == Role::Meta {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    // Retain a Meta source seq without comparing its body or crossing a business record.
    fn meta_seq(&mut self, kind: EventKind) -> u64 {
        for i in self.pos..self.record_end.min(self.log.recs.len()) {
            let r = &self.log.recs[i];
            if r.role() != Role::Meta {
                break;
            }
            if r.kind == kind {
                self.pos = i + 1;
                return r.seq;
            }
        }
        0
    }
    fn compare(&mut self, kind: EventKind, flags: RecordFlags, pieces: &[&[u8]], boot: bool) -> u64 {
        self.skip_meta();
        let Some(r) = self.log.recs.get(self.pos).filter(|_| self.pos < self.record_end) else {
            let seq = if self.mode == ReplayMode::Verify {
                self.record_end
                    .min(self.log.recs.len())
                    .checked_sub(1)
                    .and_then(|i| self.log.recs.get(i))
                    .map_or(1, |r| r.seq.saturating_add(1))
            } else {
                0
            };
            let m = Mismatch {
                seq,
                kind,
                venue: VenueId(0),
                account: AccountId(0),
            };
            if self.comparing(seq) && !self.halted {
                let mut body: Vec<u8> = pieces.iter().flat_map(|p| p.iter().copied()).collect();
                mask_measure(kind, &mut body);
                body.splice(0..0, kind.0.to_le_bytes());
                self.record_mismatch(m, Vec::new(), body, (None, Some(flags.0 & !NONCMP_FLAGS)));
            }
            return 0;
        };
        let seq = r.seq;
        // An extra generated Output must never eat the next Input. Once divergent, a
        // keep-going replay retains the original Input stream for useful diagnostics.
        let preserve = self.mode == ReplayMode::Verify;
        #[cfg(feature = "replay-io")]
        let preserve = preserve || self.preserve_inputs;
        let advance = boot || !preserve || r.role() != Role::Input;
        if self.comparing(seq) && !self.halted {
            self.scratch_a.clear();
            for p in pieces {
                self.scratch_a.extend_from_slice(p);
            }
            self.scratch_b.clear();
            self.scratch_b.extend_from_slice(&r.body);
            mask_measure(kind, &mut self.scratch_a);
            mask_measure(r.kind, &mut self.scratch_b);
            let mut snapshots_valid = true;
            if kind == EventKind::STATE_SNAPSHOT && r.kind == kind {
                if self.mode == ReplayMode::Verify {
                    snapshots_valid =
                        normalize_snapshot(&mut self.scratch_a) & normalize_snapshot(&mut self.scratch_b);
                }
                snapshots_valid &= mask_clock_cal(&mut self.scratch_a) & mask_clock_cal(&mut self.scratch_b);
            }
            let same = snapshots_valid
                && r.kind == kind
                && (r.flags.0 & !NONCMP_FLAGS) == (flags.0 & !NONCMP_FLAGS)
                && self.scratch_a == self.scratch_b;
            if r.role() == Role::Output {
                self.compared += 1;
            }
            if kind == EventKind::STATE_SNAPSHOT && r.kind == kind {
                self.snapshots_compared += 1;
            }
            if !same {
                let mut lg = self.scratch_b.clone();
                lg.splice(0..0, r.kind.0.to_le_bytes());
                let mut rg = self.scratch_a.clone();
                rg.splice(0..0, kind.0.to_le_bytes());
                let semantic_flags = (Some(r.flags.0 & !NONCMP_FLAGS), Some(flags.0 & !NONCMP_FLAGS));
                let (venue, account) = venue_account(kind, &self.scratch_a);
                self.record_mismatch(
                    Mismatch {
                        seq,
                        kind,
                        venue,
                        account,
                    },
                    lg,
                    rg,
                    semantic_flags,
                );
            }
        }
        if advance {
            self.pos += 1;
        }
        seq
    }
}

// Snapshot hash declarations may be zero. Always calculate both sides, reject an incorrect
// nonzero declaration, then compare the calculated hash together with the state bytes.
fn normalize_snapshot(body: &mut [u8]) -> bool {
    let Some(sections) = body.get(core::mem::size_of::<hft_event::SnapshotHdr>()..) else {
        return false;
    };
    let hash = crate::snapshot::state_hash(sections);
    let Some(declared) = body.get_mut(16..32) else {
        return false;
    };
    let valid = declared == [0; 16] || declared == hash;
    if valid {
        declared.copy_from_slice(&hash);
    }
    valid
}

// Section 16 is the existing cold-calibration exclusion. Keep its framing comparable;
// malformed lengths, duplicate rows and every byte outside that one payload still fail.
fn mask_clock_cal(body: &mut [u8]) -> bool {
    let mut off = core::mem::size_of::<hft_event::SnapshotHdr>();
    let mut seen = false;
    while off < body.len() {
        let Some(bytes) = body.get(off..off + 16) else {
            return false;
        };
        let Ok(h) = hft_event::SectionHdr::read_from_bytes(bytes) else {
            return false;
        };
        let Some(end) = (off + 16).checked_add(h.len as usize) else {
            return false;
        };
        let Some(payload) = body.get_mut(off + 16..end) else {
            return false;
        };
        if h.section == crate::snapshot::SECTION_CLOCK_CAL {
            if seen || h.version != crate::snapshot::CLOCK_CAL_VERSION || h.count != 1 || h.len != 40 {
                return false;
            }
            seen = true;
            payload.fill(0);
        }
        off = end;
    }
    seen
}

/// Boot-block writer of a replay: compared with the logged boot block (Meta skipped).
#[derive(Debug)]
pub struct ReplayBoot<'a> {
    s: &'a mut ReplaySink,
}

impl BootSink for ReplayBoot<'_> {
    fn append_var(&mut self, kind: EventKind, _tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr> {
        if self.s.halted {
            return Err(AppendErr::Full);
        }
        let mut probe = Vec::new();
        for p in pieces {
            probe.extend_from_slice(p);
        }
        if role_of(kind, &probe) == Role::Meta {
            return Ok(Seq(self.s.meta_seq(kind)));
        }
        Ok(Seq(self.s.compare(kind, RecordFlags(0), pieces, true)))
    }
    fn finish(self) -> Result<Seq, AppendErr> {
        // Skip BootDone (Meta).
        self.s.skip_meta();
        Ok(Seq(0))
    }
}

impl JournalSink for ReplaySink {
    type Boot<'a> = ReplayBoot<'a>;
    fn append_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        _tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<Seq, AppendErr> {
        if self.halted {
            return Err(AppendErr::Full);
        }
        let mut probe = [0u8; 80];
        let mut n = 0;
        'o: for p in pieces {
            for b in p.iter() {
                if n >= 80 {
                    break 'o;
                }
                if let Some(x) = probe.get_mut(n) {
                    *x = *b;
                }
                n += 1;
            }
        }
        match role_of(kind, probe.get(..n).unwrap_or(&[])) {
            Role::Input => {
                // The driver positioned `pos` on the handed record.
                let seq = self.log.recs.get(self.pos).map_or(0, |r| r.seq);
                self.pos += 1;
                Ok(Seq(seq))
            }
            Role::Output => Ok(Seq(self.compare(kind, flags, pieces, false))),
            _ => Ok(Seq(self.meta_seq(kind))),
        }
    }
    fn end_event(&mut self) {}
    fn publish(&mut self) {}
    fn roll(
        &mut self,
        _reason: RollReason,
        _info: &crate::journal::RollMeta<'_>,
    ) -> Result<ReplayBoot<'_>, RollErr> {
        // Rolls happen where the log has `SegmentRoll{End}` right after the Timer.
        let mut p = self.pos;
        let mut found = false;
        while p < self.record_end {
            let Some(r) = self.log.recs.get(p) else { break };
            if r.kind == EventKind::SEGMENT_ROLL {
                found = r.body.get(32).copied() == Some(1);
                break;
            }
            if r.role() != Role::Meta {
                break;
            }
            p += 1;
        }
        if !found || p + 1 >= self.record_end {
            return Err(RollErr::NextNotReady);
        }
        // Boot appends consume the new segment's Meta sources without comparing them.
        self.pos = p + 1;
        Ok(ReplayBoot { s: self })
    }
    fn needs_roll(&self, _now: Tsc) -> Option<(RollReason, RollAt)> {
        None
    }
}

/// The replay driver (recover, verify, mirror).
#[derive(Debug)]
pub struct ReplayDriver {
    pub sink: ReplaySink,
    /// Next record to hand over.
    pub cursor: usize,
    /// Stop before this record index (recover: the last EVENT_END boundary).
    pub end: usize,
    per_round: u16,
    taken: u16,
    run_tsc: Option<Tsc>,
    /// Inputs handed (tests).
    pub handed: u64,
    /// Hand Meta inputs too (mirror skips them).
    pub hand_meta: bool,
}

impl ReplayDriver {
    /// A replay of `log` starting at record `start` (the first record after the start snapshot's
    /// event).
    pub fn new(mode: ReplayMode, log: Log, start: usize, per_round: u16) -> ReplayDriver {
        let end = log.recs.len();
        let mut sink = ReplaySink::new(mode, log);
        sink.pos = start;
        ReplayDriver {
            sink,
            cursor: start,
            end,
            per_round: per_round.max(1),
            taken: 0,
            run_tsc: None,
            handed: 0,
            hand_meta: mode != ReplayMode::Mirror,
        }
    }
    /// Appends newly committed records (advisor mirror tailing the journal).
    pub fn extend(&mut self, recs: &[MemRecord], segs: &[crate::journal::SegClock], seg_start: &[usize]) {
        let base = self.sink.log.recs.len();
        self.sink.log.recs.extend_from_slice(recs);
        // Segment boundaries of the appended part.
        for (k, st) in seg_start.iter().enumerate() {
            if *st >= base && *st < self.sink.log.recs.len() && !self.sink.log.seg_start.contains(st) {
                self.sink.log.seg_start.push(*st);
                if let Some(c) = segs.get(k) {
                    self.sink.log.segs.push(*c);
                }
            }
        }
        self.end = self.sink.log.recs.len();
    }
    pub fn set_run_tsc(&mut self, t: Option<Tsc>) {
        self.run_tsc = t;
    }
    pub fn finished(&self) -> bool {
        // Drain may finish before the segment's trailing Meta. Only an unread Input
        // means the run stopped early; finish_verification checks unreached Outputs.
        !self
            .sink
            .log
            .recs
            .iter()
            .take(self.end)
            .skip(self.cursor.max(self.sink.pos))
            .any(|r| r.role() == Role::Input)
    }
    fn next_record(&self, from: usize) -> Option<usize> {
        let mut i = from;
        while i < self.end {
            let r = self.sink.log.recs.get(i)?;
            if r.role() == Role::Input || (self.hand_meta && r.role() == Role::Meta) {
                return Some(i);
            }
            i += 1;
        }
        None
    }
    pub fn blob(&self, sha: &[u8; 32]) -> Option<&[u8]> {
        let upto = self.sink.pos.min(self.end);
        let wanted = self.sink.log.recs.iter().take(upto).rev().find_map(|r| {
            (r.kind == EventKind::CONFIG_BLOB)
                .then(|| r.read::<BlobHdr>())
                .flatten()
                .filter(|h| h.blob.sha256 == *sha)
                .map(|h| h.blob)
        })?;
        crate::config_blob::content_before(&self.sink.log, upto, &wanted).ok()
    }
    pub fn mismatches(&self) -> &[Mismatch] {
        &self.sink.mismatches
    }
    /// Collect unreached Outputs at the selected boundary, including a final missing Output.
    pub fn finish_verification(&mut self) {
        while self.sink.pos < self.end && !self.sink.halted {
            self.sink.missing(self.sink.pos);
            self.sink.pos += 1;
        }
    }
    /// Logged Outputs never reached by the replay (verify: a failure each).
    pub fn leftover(&self) -> Vec<Mismatch> {
        let mut v = Vec::new();
        for r in self.sink.log.recs.iter().take(self.end).skip(self.sink.pos) {
            if r.role() == Role::Output && r.kind != EventKind::SEGMENT_ROLL {
                v.push(Mismatch {
                    seq: r.seq,
                    kind: r.kind,
                    venue: VenueId(0),
                    account: AccountId(0),
                });
            }
        }
        v
    }
}

impl<S: StrategySet> Driver<S> for ReplayDriver {
    type J = ReplaySink;
    fn begin_round(&mut self, _v: &DriverView<'_>) {
        self.taken = 0;
    }
    fn take(&mut self, _v: &DriverView<'_>, out: &mut InputSlot) -> bool {
        if self.taken >= self.per_round || self.sink.halted {
            return false;
        }
        // Align the sink with the next Input (Outputs between are compared by the sequencer's
        // own appends; anything left unaligned is a mismatch).
        let Some(i) = self.next_record(self.cursor.max(self.sink.pos)) else {
            return false;
        };
        let Some(r) = self.sink.log.recs.get(i) else {
            return false;
        };
        out.set_bytes(
            r.kind,
            RecordFlags(r.flags.0 & !(RecordFlags::EVENT_END.0 | RecordFlags::CRC.0 | RecordFlags::PART.0)),
            Tsc(r.tsc),
            &r.body,
        );
        out.seq = r.seq;
        let is_input = r.role() == Role::Input;
        // Unconsumed Outputs of the previous event are mismatches.
        let mut p = self.sink.pos;
        while p < i {
            self.sink.missing(p);
            if self.sink.halted {
                return false;
            }
            p += 1;
        }
        self.sink.pos = i;
        self.cursor = i + 1;
        // Calibration Meta does not spend the Input budget or change replayed-input counts.
        // Consume it in journal order before this round's next Input (including at EOF).
        if is_input {
            self.taken += 1;
            self.handed += 1;
        }
        true
    }
    fn run_tsc(&mut self) -> Option<Tsc> {
        self.run_tsc
    }
    fn measure(&mut self) -> Tsc {
        Tsc(0)
    }
    fn emit(&mut self, _o: &OutRecord<'_>) -> EmitResult {
        EmitResult::Skipped
    }
    fn take_parsed(&mut self, r: &BlobRef, cx: &ParseCx<'_, S>) -> Result<ParsedConfig<S>, ConfigErr> {
        // Only configuration already reached in journal order can satisfy this command.
        // Prefetching the file or extending an advisor mirror does not make future blobs
        // visible. Match the complete reference and verify the content before parsing it.
        let content = crate::config_blob::content_before(&self.sink.log, self.sink.pos.min(self.end), r)?;
        parse_blob::<S>(
            r,
            content,
            cx.rd,
            cx.max_groups,
            cx.base,
            cx.risk_account,
            cx.risk_decode,
        )
        .map_err(ConfigErr::Params)
    }
    fn retire(&mut self, _old: ParsedConfig<S>) {}
    fn journal(&mut self) -> &mut ReplaySink {
        &mut self.sink
    }
    fn mode(&self) -> DriverMode {
        match self.sink.mode {
            ReplayMode::Recover => DriverMode::Recover,
            ReplayMode::Verify => DriverMode::Verify,
            ReplayMode::Mirror => DriverMode::Mirror,
        }
    }
    fn write_kill(&mut self, _bits: &[KillBits]) {}
    fn roll_info(&mut self) -> RollInfo {
        // The next segment's header values.
        let si = (self.sink.log.seg_of(self.sink.pos) + 1).min(self.sink.log.segs.len().saturating_sub(1));
        let c = self.sink.log.segs.get(si).copied().unwrap_or_default();
        RollInfo {
            utc_ns: c.calib_realtime_ns,
            calib: hft_rt::CalibPair {
                tsc: Tsc(c.calib_tsc),
                realtime_ns: c.calib_realtime_ns,
                phc_ns: 0,
                err_ns: 0,
            },
            tsc_hz_milli: c.tsc_hz_milli,
        }
    }
    fn host_profile(&self) -> HostProfile {
        crate::snapshot::host_profile(0)
    }
    fn clock_status(&self) -> ClockStatus {
        let si = (self.sink.log.seg_of(self.sink.pos) + 1).min(self.sink.log.segs.len().saturating_sub(1));
        if let Some(&start) = self.sink.log.seg_start.get(si) {
            for r in self.sink.log.recs.iter().take(self.end).skip(start) {
                if r.role() != Role::Meta {
                    break;
                }
                if r.kind == EventKind::CLOCK_STATUS
                    && let Some(c) = r.read::<ClockStatus>()
                {
                    return c;
                }
            }
        }
        crate::snapshot::clock_status(hft_rt::CalibPair::default(), 0)
    }
    fn start_log(&mut self) -> Option<Log> {
        Some(self.sink.log.clone())
    }
    fn start_at(&mut self, ix: usize) {
        self.end = self.end.min(crate::recover::event_end(&self.sink.log));
        self.cursor = ix;
        self.sink.pos = ix;
    }
}

#[allow(dead_code)]
fn _unused(_: BlobKind) {}
