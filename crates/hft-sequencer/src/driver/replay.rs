//! The replay drivers (README "冷启动恢复", "验证回放", advisor mirror): Inputs are handed in
//! journal order (≤ `replay_per_round` per round); Outputs are compared with the log (verify:
//! failure; recover: mismatch list; mirror: not compared); nothing is emitted.

use hft_event::{BlobHdr, BlobKind, BlobRef, ClockStatus, EventKind, HostProfile, RecordFlags, Role};
use hft_journal::{AppendErr, RollAt, RollErr, RollReason};
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, KillBits, Seq, Tsc, VenueId};

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

/// A flat view of the logged records with segment boundaries.
#[derive(Clone, Debug, Default)]
pub struct Log {
    pub recs: Vec<MemRecord>,
    /// Index of the first record of every segment.
    pub seg_start: Vec<usize>,
    pub segs: Vec<crate::journal::SegClock>,
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
    /// `(seq, logged, regenerated)` masked bodies of the first mismatches (diagnostics).
    pub detail: Vec<(u64, Vec<u8>, Vec<u8>)>,
    scratch_a: Vec<u8>,
    scratch_b: Vec<u8>,
}

fn venue_account(kind: EventKind, body: &[u8]) -> (VenueId, AccountId) {
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
            detail: Vec::new(),
            scratch_a: Vec::with_capacity(1 << 16),
            scratch_b: Vec::with_capacity(1 << 16),
        }
    }
    fn skip_meta(&mut self) {
        while let Some(r) = self.log.recs.get(self.pos) {
            if r.role() == Role::Meta {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
    fn compare(&mut self, kind: EventKind, flags: RecordFlags, pieces: &[&[u8]]) -> u64 {
        self.skip_meta();
        let Some(r) = self.log.recs.get(self.pos) else {
            let m = Mismatch {
                seq: 0,
                kind,
                venue: VenueId(0),
                account: AccountId(0),
            };
            if self.mode != ReplayMode::Mirror {
                self.mismatches.push(m);
            }
            return 0;
        };
        let seq = r.seq;
        if self.mode != ReplayMode::Mirror {
            self.scratch_a.clear();
            for p in pieces {
                self.scratch_a.extend_from_slice(p);
            }
            self.scratch_b.clear();
            self.scratch_b.extend_from_slice(&r.body);
            mask_measure(kind, &mut self.scratch_a);
            mask_measure(r.kind, &mut self.scratch_b);
            let same = r.kind == kind
                && (r.flags.0 & !NONCMP_FLAGS) == (flags.0 & !NONCMP_FLAGS)
                && self.scratch_a == self.scratch_b;
            self.compared += 1;
            if !same {
                if self.detail.len() < 16 {
                    let mut lg = self.scratch_b.clone();
                    lg.splice(0..0, r.kind.0.to_le_bytes());
                    let mut rg = self.scratch_a.clone();
                    rg.splice(0..0, kind.0.to_le_bytes());
                    self.detail.push((seq, lg, rg));
                }
                let (venue, account) = venue_account(kind, &self.scratch_a);
                self.mismatches.push(Mismatch {
                    seq,
                    kind,
                    venue,
                    account,
                });
            }
        }
        self.pos += 1;
        seq
    }
}

/// Boot-block writer of a replay: compared with the logged boot block (Meta skipped).
#[derive(Debug)]
pub struct ReplayBoot<'a> {
    s: &'a mut ReplaySink,
}

impl BootSink for ReplayBoot<'_> {
    fn append_var(&mut self, kind: EventKind, _tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr> {
        let mut probe = Vec::new();
        for p in pieces {
            probe.extend_from_slice(p);
        }
        if role_of(kind, &probe) == Role::Meta {
            return Ok(Seq(0));
        }
        Ok(Seq(self.s.compare(kind, RecordFlags(0), pieces)))
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
            Role::Output => Ok(Seq(self.compare(kind, flags, pieces))),
            _ => Ok(Seq(0)),
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
        while let Some(r) = self.log.recs.get(p) {
            if r.kind == EventKind::SEGMENT_ROLL {
                found = r.body.get(32).copied() == Some(1);
                break;
            }
            if r.role() != Role::Meta {
                break;
            }
            p += 1;
        }
        if !found {
            return Err(RollErr::NextNotReady);
        }
        // Skip End and the new segment's Begin/HostProfile/ClockStatus (all Meta).
        self.pos = p + 1;
        self.skip_meta();
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
        self.next_input(self.cursor).is_none()
    }
    fn next_input(&self, from: usize) -> Option<usize> {
        let mut i = from;
        while i < self.end {
            let r = self.sink.log.recs.get(i)?;
            if r.role() == Role::Input {
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
        if self.taken >= self.per_round {
            return false;
        }
        // Align the sink with the next Input (Outputs between are compared by the sequencer's
        // own appends; anything left unaligned is a mismatch).
        let Some(i) = self.next_input(self.cursor.max(self.sink.pos)) else {
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
        // Unconsumed Outputs of the previous event are mismatches.
        let mut p = self.sink.pos;
        while p < i {
            if let Some(x) = self.sink.log.recs.get(p)
                && x.role() == Role::Output
                && self.sink.mode != ReplayMode::Mirror
                && x.kind != EventKind::SEGMENT_ROLL
            {
                self.sink.mismatches.push(Mismatch {
                    seq: x.seq,
                    kind: x.kind,
                    venue: VenueId(0),
                    account: AccountId(0),
                });
            }
            p += 1;
        }
        self.sink.pos = i;
        self.cursor = i + 1;
        self.taken += 1;
        self.handed += 1;
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
