//! `DstDriver` and the in-memory journal (README "DST 的世界模型" 2).
//!
//! `DstDriver` is the sequencer driver of the DST: its time is the world's virtual time
//! (`stamp`), its Inputs are the ones the world hands over (`offer`/`take_input`), its frames go
//! into a collection buffer (`emit`). It holds no random source — the engine gets randomness only
//! through Inputs (TK-048). The `hft_sequencer::Driver` implementation (`begin_round`/`take` from
//! the offered Input with `out.tsc = stamp()`, `run_tsc`/`measure` = `stamp()`, `emit` into the
//! buffer, `journal` = [`MemJournal`]) lands with hft-sequencer.

use hft_event::{EventKind, GwCmd, RecordFlags};

use crate::dst::sut::{FrameRec, InputRec, JRec, OutRing};

/// Virtual-time driver of one engine instance.
#[derive(Debug)]
pub struct DstDriver {
    tsc_hz: u64,
    start_tsc: u64,
    now_ns: u64,
    pending: Option<InputRec>,
    emitted: Vec<FrameRec>,
    journal: MemJournal,
}

impl DstDriver {
    /// `tsc_hz` = fixed virtual TSC frequency; `start_tsc` = TSC at virtual time 0.
    pub fn new(tsc_hz: u64, start_tsc: u64) -> DstDriver {
        DstDriver {
            tsc_hz,
            start_tsc,
            now_ns: 0,
            pending: None,
            emitted: Vec::new(),
            journal: MemJournal::default(),
        }
    }
    /// Moves virtual time (the world calls it before each step).
    pub fn set_now(&mut self, now_ns: u64) {
        self.now_ns = now_ns;
    }
    pub fn now_ns(&self) -> u64 {
        self.now_ns
    }
    /// TSC of the current virtual time: `start + ⌊now_ns × tsc_hz / 10^9⌋`.
    pub fn stamp(&self) -> u64 {
        tsc_at(self.start_tsc, self.tsc_hz, self.now_ns)
    }
    pub fn tsc_hz(&self) -> u64 {
        self.tsc_hz
    }
    /// Hands the next Input over (one per step).
    pub fn offer(&mut self, input: InputRec) {
        self.pending = Some(input);
    }
    /// The Input of this step, once.
    pub fn take_input(&mut self) -> Option<InputRec> {
        self.pending.take()
    }
    /// Collects one out-ring frame.
    pub fn emit(&mut self, ring: OutRing, cmd: GwCmd, trigger_seq: u64) {
        self.emitted.push(FrameRec {
            ring,
            cmd,
            trigger_seq,
        });
    }
    /// Frames emitted since the last call.
    pub fn drain_emitted(&mut self, out: &mut Vec<FrameRec>) {
        out.append(&mut self.emitted);
    }
    pub fn journal(&self) -> &MemJournal {
        &self.journal
    }
    pub fn journal_mut(&mut self) -> &mut MemJournal {
        &mut self.journal
    }
}

/// `start + ⌊ns × hz / 10^9⌋` (u128 intermediate, saturating).
pub fn tsc_at(start: u64, hz: u64, ns: u64) -> u64 {
    let t = u128::from(ns) * u128::from(hz) / 1_000_000_000;
    start.saturating_add(u64::try_from(t).unwrap_or(u64::MAX))
}

/// In-memory journal segment: records with seqs from 1, `EVENT_END` on the last record of each
/// Input event (journal-format "提交游标").
#[derive(Clone, Debug, Default)]
pub struct MemJournal {
    recs: Vec<JRec>,
    /// Records already returned by `take_new`.
    cursor: usize,
}

impl MemJournal {
    /// Appends one record; returns its seq.
    pub fn append(&mut self, kind: EventKind, flags: RecordFlags, tsc: u64, body: &[u8]) -> u64 {
        let seq = self.recs.last().map_or(1, |r| r.seq + 1);
        self.recs.push(JRec {
            seq,
            end_seq: seq,
            tsc,
            kind,
            flags,
            body: body.to_vec(),
        });
        seq
    }
    /// Appends a fixed body.
    pub fn append_body<B: hft_event::Body>(&mut self, flags: RecordFlags, tsc: u64, b: &B) -> u64 {
        self.append(B::KIND, flags, tsc, zerocopy::IntoBytes::as_bytes(b))
    }
    /// Sets `EVENT_END` on the last record.
    pub fn end_event(&mut self) {
        if let Some(r) = self.recs.last_mut() {
            r.flags = r.flags.union(RecordFlags::EVENT_END);
        }
    }
    pub fn records(&self) -> &[JRec] {
        &self.recs
    }
    pub fn last_seq(&self) -> u64 {
        self.recs.last().map_or(0, |r| r.seq)
    }
    /// Records appended since the previous call.
    pub fn take_new(&mut self, out: &mut Vec<JRec>) {
        out.extend_from_slice(&self.recs[self.cursor..]);
        self.cursor = self.recs.len();
    }
    /// Drops every record after the last `EVENT_END` (crash truncation, RC-1).
    pub fn truncate_to_event_end(&mut self) -> usize {
        let keep = self
            .recs
            .iter()
            .rposition(|r| r.flags.contains(RecordFlags::EVENT_END))
            .map_or(0, |i| i + 1);
        let dropped = self.recs.len() - keep;
        self.recs.truncate(keep);
        self.cursor = self.cursor.min(keep);
        dropped
    }
}

/// Records of the last `n` complete events of `recs` (an event = records up to and including one
/// `EVENT_END`); the tail after the last `EVENT_END` is included as a partial event.
pub fn last_events(recs: &[JRec], n: usize) -> &[JRec] {
    if n == 0 {
        return &recs[recs.len()..];
    }
    let mut ends = 0usize;
    let tail_partial = recs
        .last()
        .is_some_and(|r| !r.flags.contains(RecordFlags::EVENT_END));
    let mut need = n;
    if tail_partial {
        need -= 1;
        if need == 0 {
            let start = recs
                .iter()
                .rposition(|r| r.flags.contains(RecordFlags::EVENT_END))
                .map_or(0, |i| i + 1);
            return &recs[start..];
        }
    }
    for (i, r) in recs.iter().enumerate().rev() {
        if r.flags.contains(RecordFlags::EVENT_END) {
            if ends == need {
                return &recs[i + 1..];
            }
            ends += 1;
        }
    }
    recs
}

/// Encodes records in the journal record format (24-byte `RecordHeader`, body, zero padding to
/// 8 B; no CRC) so the fragment can be framed by `hft_event::decode_record`.
pub fn encode_records(recs: &[JRec]) -> Vec<u8> {
    let mut out = Vec::new();
    for r in recs {
        let len = (24 + r.body.len()).div_ceil(8) * 8;
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.extend_from_slice(&r.kind.0.to_le_bytes());
        out.extend_from_slice(&(r.flags.0 & !RecordFlags::CRC.0).to_le_bytes());
        out.extend_from_slice(&r.seq.to_le_bytes());
        out.extend_from_slice(&r.tsc.to_le_bytes());
        out.extend_from_slice(&r.body);
        out.resize(out.len() + (len - 24 - r.body.len()), 0);
    }
    out
}
