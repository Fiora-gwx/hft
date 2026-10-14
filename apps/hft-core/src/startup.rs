//! Boot measurements are Meta only. They cannot release an account or change its mode.
use hft_event::{Note, NoteLevel, NoteSource, note_code};
use hft_types::{GroupId, InstId, StrategyId, Tsc};
use serde::Deserialize;
use std::time::Instant;

pub const NOTE_CODES: &[(NoteSource, u32, &str)] = &[
    (NoteSource::Boot, 1, "StartupStep"),
    (NoteSource::Boot, 4, "RefdataWarning"),
];

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub budget_s: u32,
    pub gate_timeout_s: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            budget_s: 30,
            gate_timeout_s: 15,
        }
    }
}

pub fn note(n: u32, level: NoteLevel, values: [u64; 4]) -> Note {
    Note {
        code: note_code(NoteSource::Boot, n),
        level,
        source: NoteSource::Boot,
        strategy: StrategyId(0),
        _pad: 0,
        group: GroupId(0),
        inst: InstId(0),
        v: [values[0], values[1], values[2], values[3], 0, 0, 0, 0],
    }
}

fn step(step: u8, end: bool, us: u64, budget_us: u64, flags: u64) -> Note {
    let flags = if end { flags | u64::from(us > budget_us) } else { 0 };
    note(
        1,
        if flags != 0 {
            NoteLevel::P1
        } else {
            NoteLevel::Info
        },
        [u64::from(step), u64::from(end), us, flags],
    )
}

#[derive(Debug)]
pub struct Serial {
    start: Instant,
    records: Vec<(u8, bool, u64)>,
    status: bool,
}
impl Default for Serial {
    fn default() -> Self {
        let mut records = Vec::with_capacity(10);
        records.push((1, false, 0));
        Self {
            start: Instant::now(),
            records,
            status: false,
        }
    }
}
impl Serial {
    /// CLI startup enables status; read-only config validation has no notify side effects.
    pub fn enable_status(&mut self) -> Result<(), crate::Error> {
        crate::runtime::notify("STATUS=Startup step 1")?;
        self.status = true;
        Ok(())
    }
    pub fn mark(&mut self, number: u8, end: bool) {
        self.records.push((number, end, self.elapsed()));
        if self.status && !end {
            // Progress is best effort; journal records are the durable measurement authority.
            let _ = crate::runtime::notify(&format!("STATUS=Startup step {number}"));
        }
    }
    pub fn elapsed(&self) -> u64 {
        self.start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
    }
    pub fn notes(&self, cfg: Config) -> impl Iterator<Item = Note> + '_ {
        self.records
            .iter()
            .map(move |&(n, end, us)| step(n, end, us, u64::from(cfg.budget_s) * 1_000_000, 0))
    }
}

/// Fixed-capacity pending Meta, consumed on the sequencer thread. Never writes the hk producer.
#[derive(Debug, Default)]
pub struct Live {
    pending: [Option<Note>; 6],
    len: usize,
    read: usize,
    phase: u8,
    gate_start_us: u64,
    budget_us: u64,
    gate_timeout_us: u64,
    base_us: u64,
    base_tsc: u64,
    hz_milli: u64,
}
impl Live {
    pub fn start(cfg: Config, elapsed_us: u64, now: Tsc, hz_milli: u64) -> Self {
        let mut result = Self {
            phase: 6,
            budget_us: u64::from(cfg.budget_s) * 1_000_000,
            gate_timeout_us: u64::from(cfg.gate_timeout_s) * 1_000_000,
            base_us: elapsed_us,
            base_tsc: now.0,
            hz_milli,
            ..Self::default()
        };
        result.push(6, false, elapsed_us, 0);
        result
    }
    fn push(&mut self, number: u8, end: bool, us: u64, flags: u64) {
        // Each of steps 6, 7, 8 is emitted at most once, giving exactly six slots.
        if let Some(slot) = self.pending.get_mut(self.len) {
            *slot = Some(step(number, end, us, self.budget_us, flags));
            self.len += 1;
        }
    }
    pub fn observe(&mut self, now: Tsc, connected: bool, released: bool) {
        if self.phase == 0 || self.phase >= 9 || self.hz_milli == 0 {
            return;
        }
        let us = self.base_us.saturating_add(
            (u128::from(now.0.saturating_sub(self.base_tsc)) * 1_000_000_000 / u128::from(self.hz_milli))
                .min(u128::from(u64::MAX)) as u64,
        );
        // Step 6 uses the frozen 8-second connection budget, measured from its start.
        if self.phase == 6 && (connected || us.saturating_sub(self.base_us) >= 8_000_000) {
            self.push(6, true, us, 0);
            self.push(7, false, us, 0);
            self.gate_start_us = us;
            self.phase = 7;
        }
        if self.phase == 7 && (released || us.saturating_sub(self.gate_start_us) >= self.gate_timeout_us) {
            self.push(7, true, us, if released { 0 } else { 2 });
            self.phase = 8;
        }
        if self.phase == 8 && released {
            self.push(8, false, us, 0);
            self.push(8, true, us, 0);
            self.phase = 9;
        }
    }
    pub fn len(&self) -> usize {
        self.len - self.read
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn pop(&mut self) -> Option<Note> {
        if self.read == self.len {
            return None;
        }
        let value = self.pending[self.read].take();
        self.read += 1;
        value
    }
    pub fn phase(&self) -> u8 {
        self.phase
    }
}
