//! Opt-in, single-writer diagnostics. No counters, reads or branches without `bench-stages`.
//! These are subdivisions of `seq-decide`, not additional latency-budget segments.

/// Exclusive stage indices in the benchmark's per-sample counters.
#[derive(Copy, Clone, Debug)]
pub enum Stage {
    Intake,
    JournalInput,
    DispatchClock,
    Book,
    Ledger,
    FeedsGate,
    Views,
    Strategy,
    Exec,
    FinalChecks,
    Output,
    Publish,
}

pub const N: usize = 12;
pub const NAMES: [&str; N] = [
    "intake_rounds",
    "journal_input",
    "dispatch_clock",
    "book_bbo",
    "ledger_ref_px",
    "feeds_gate",
    "group_views",
    "strategy_delivery_ctx",
    "exec_advance",
    "final_checks",
    "output_publish",
    "round_publish",
];

/// Accumulates counter ticks, never consulted by decision code or persisted.
#[derive(Debug, Default)]
pub struct Stages {
    pub ticks: [u64; N],
    last: u64,
    stage: usize,
    active: bool,
}

impl Stages {
    pub fn start(&mut self) {
        self.ticks.fill(0);
        self.stage = Stage::Intake as usize;
        self.active = true;
        self.last = hft_rt::tsc_ordered().0;
    }

    #[inline]
    pub fn mark(&mut self, next: Stage) {
        if self.active {
            let t = hft_rt::tsc_ordered().0;
            if let Some(v) = self.ticks.get_mut(self.stage) {
                *v += t.saturating_sub(self.last);
            }
            self.last = t;
            self.stage = next as usize;
        }
    }

    pub fn stop(&mut self) {
        self.mark(Stage::Intake);
        self.active = false;
    }
}
