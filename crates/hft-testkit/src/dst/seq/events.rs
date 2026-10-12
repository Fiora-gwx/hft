//! Per-Input snapshots, retained until the world checks this step. No sampler may drop an Input.

use hft_event::{EventKind, RecordFlags};
use hft_sequencer::{Engine, MemJournal, types::InputSlot};

use super::{
    audit::History,
    observe::{self, Before, Carry},
};
use crate::dst::obs::Obs;
use crate::dst::sut::{EventStep, FrameRec, JRec, LedgerEvent, StepOut};
use crate::dst::world::WorldSpec;

/// Explicit diagnostic limits; exceeding one is a failed run.
pub const MAX_EVENTS: usize = 4_096;
const MAX_RECORDS: usize = 1_024;
const MAX_FRAMES: usize = 512;

#[derive(Debug)]
pub struct Pending {
    seq: u64,
    kind: EventKind,
    pos: Before,
    before: Obs,
    frames: Vec<FrameRec>,
    overflow: bool,
}

impl Pending {
    pub fn begin(e: &Engine, w: &WorldSpec, h: &History, c: &Carry, input: &InputSlot) -> Self {
        let mut before = Obs::default();
        observe::fill_engine(e, w, h, c, &mut before);
        before.normalize();
        Self {
            seq: e.cur_seq,
            kind: input.kind,
            pos: Before::input(e, w, input),
            before,
            frames: Vec::with_capacity(MAX_FRAMES),
            overflow: false,
        }
    }
    pub fn frame(&mut self, f: FrameRec) {
        if self.frames.len() == MAX_FRAMES {
            self.overflow = true;
        } else {
            self.frames.push(f);
        }
    }
    pub fn finish(self, e: &Engine, w: &WorldSpec, h: &History, c: &Carry, j: &MemJournal) -> EventStep {
        let mut after = Obs::default();
        observe::fill_engine(e, w, h, c, &mut after);
        let mut out = StepOut::default();
        for r in j
            .segments
            .iter()
            .flat_map(|s| &s.records[s.records.partition_point(|r| r.seq < self.seq)..])
        {
            if out.records.len() == MAX_RECORDS {
                after.observation_error = Some("per-Input diagnostic record buffer overflow".into());
                break;
            }
            out.records.push(JRec {
                seq: r.seq,
                tsc: r.tsc,
                kind: r.kind,
                flags: r.flags,
                body: r.body.clone(),
            });
        }
        if self.overflow {
            after.observation_error = Some("per-Input diagnostic frame buffer overflow".into());
        }
        observe::step_info_engine(e, w, &self.pos, &mut out);
        if let Err(err) = super::decisions::fill(e, h, &self.before, &out.records, &mut out.info) {
            after.observation_error = Some(err);
        }
        after.normalize();
        let ledger = if crate::dst::sut::ledger_input(self.kind) {
            let through_seq = out
                .records
                .iter()
                .rev()
                .find(|r| r.flags.contains(RecordFlags::EVENT_END))
                .map(|r| r.seq);
            match (through_seq, super::ledger::event_sections(&e.ledger)) {
                (Some(through_seq), Ok(sections)) => Some(LedgerEvent {
                    through_seq,
                    sections,
                }),
                (_, Err(err)) => {
                    after.observation_error = Some(err);
                    None
                }
                (None, _) => {
                    after.observation_error = Some("ledger observation lacks an event boundary".into());
                    None
                }
            }
        } else {
            None
        };
        EventStep {
            records: out.records,
            frames: self.frames,
            info: out.info,
            before: self.before,
            after,
            ledger,
        }
    }
}
