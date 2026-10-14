//! `hft-sequencer`: hft-core's single-writer main loop. It owns every business-state instance
//! (books, `Oms`, `Risk`, `Ledger`, `Exec`, the strategy instances, the timer heap, the command
//! de-duplication table), takes Inputs one by one in priority order, writes the seq journal
//! first, drives the components, and writes decisions back. Generic over the assembled
//! `StrategySet` (static dispatch). Specification: `crates/hft-sequencer/README.md`.
//!
//! No threads, no system clock, no I/O except journal appends: every environment decision is an
//! Input written by the live driver, so state = f(journal Inputs).
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[cfg(target_endian = "big")]
compile_error!("hft-sequencer: snapshot rows are little-endian only");

pub mod advisor;
#[cfg(feature = "testkit")]
pub mod audit;
pub mod clock;
pub mod codes;
mod commands;
pub mod config_blob;
pub mod driver;
mod engine;
mod exec_glue;
mod intents;
pub mod journal;
pub mod market;
mod mode_glue;
pub mod notify;
pub mod params;
pub mod parsed;
#[cfg(feature = "planted")]
pub mod planted;
mod process;
mod recover;
pub mod snapshot;
mod strategies;
pub mod subs;
pub mod timers;
pub mod trace;
pub mod types;
pub mod views;

pub use commands::{
    ActiveConfigRow, CmdDedupHdr, CmdDedupRow, FillSeqRow, RECON_VIEW_DIFFS, RECON_VIEW_REPORTS, ReconDiff,
    ReconDiffRow, ReconPendingHdr, ReconView, ReconViewRow, ReconViews, VenueNoticeRow, ViewerPreviewRow,
};
pub use driver::{Driver, DriverMode, DriverView, ParseCx};
pub use engine::{Engine, EngineSetup};
pub use journal::{BootSink, JournalSink, MemJournal, MemRecord, MemSegment, RollMeta, SegClock};
pub use params::{CapacitiesRow, RingClass, RingLimit, SeqParamsRow};
pub use parsed::{ParsedConfig, ParsedParams, RiskDecode};
pub use recover::event_end as recover_event_end;
pub use strategies::{InstanceSlot, Instances};
pub use types::*;

use hft_rt::Heartbeat;
use hft_strategy_sdk::StrategySet;

/// The configuration of an empty start (no journal): refdata, engine setup, risk parameters and
/// the initial strategy parameters, each with the blob bytes the boot block records.
pub struct ConfigSet<S: StrategySet> {
    pub refdata_blob: Vec<u8>,
    pub engine_blob: Vec<u8>,
    pub risk_blob: Vec<u8>,
    /// SHA-256 of `risk_blob` (computed by the assembler).
    pub risk_sha256: [u8; 32],
    pub risk: hft_risk::RiskParams,
    /// Initial `StrategyParams` (blob bytes + parsed object).
    pub params: Vec<(Vec<u8>, ParsedParams<S>)>,
}

impl<S: StrategySet> core::fmt::Debug for ConfigSet<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConfigSet")
            .field("n_params", &self.params.len())
            .finish_non_exhaustive()
    }
}

/// Where `boot` starts from (built once per boot; the size difference does not matter).
#[allow(clippy::large_enum_variant)]
pub enum BootSource<S: StrategySet> {
    /// Cold-start recovery from the journal the driver reads.
    Journal(RecoverPlan),
    /// Empty start.
    Empty { configs: ConfigSet<S> },
}

impl<S: StrategySet> core::fmt::Debug for BootSource<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BootSource::Journal(p) => f.debug_tuple("Journal").field(p).finish(),
            BootSource::Empty { .. } => f.write_str("Empty"),
        }
    }
}

/// The sequencer: all business state + the driver + the strategy instances.
pub struct Sequencer<D: Driver<S>, S: StrategySet> {
    pub(crate) e: Engine,
    pub(crate) inst: Instances<S>,
    pub(crate) drv: D,
    pub(crate) hb: &'static Heartbeat,
    pub(crate) slot: InputSlot,
}

impl<D: Driver<S>, S: StrategySet> core::fmt::Debug for Sequencer<D, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Sequencer").finish_non_exhaustive()
    }
}

impl<D: Driver<S>, S: StrategySet> Sequencer<D, S> {
    /// Builds every component from `boot` and `setup` (README "装配与运行"); preallocates.
    pub fn boot(
        boot: BootSource<S>,
        setup: &EngineSetup,
        drv: D,
        hb: &'static Heartbeat,
    ) -> Result<Self, BootError> {
        let seq = match boot {
            BootSource::Empty { configs } => engine::boot_empty(configs, setup, drv, hb),
            BootSource::Journal(plan) => recover::boot_journal(plan, setup, drv, hb),
        }?;
        #[cfg(feature = "testkit")]
        let seq = {
            let mut seq = seq;
            seq.drv.observe_boundary(audit::StateView {
                e: &seq.e,
                inst: &seq.inst,
            });
            seq
        };
        Ok(seq)
    }

    /// One main-loop round (README "主循环"): non-blocking; Inputs taken and processed one by one.
    pub fn step(&mut self) -> StepOutcome {
        if let Some(o) = self.e.finished {
            return o;
        }
        self.hb.beat();
        self.drv.begin_round(&DriverView::new(&self.e));
        let mut busy = false;
        loop {
            let more = self.drv.take(&DriverView::new(&self.e), &mut self.slot);
            if !more {
                break;
            }
            busy = true;
            if let Err(f) = process::process(self) {
                self.e.finished = Some(StepOutcome::Fatal(f));
                return StepOutcome::Fatal(f);
            }
            if let Some(o) = self.e.finished {
                // The event that ended the run (Drain snapshot included) is complete: publish it
                // before returning, like the end of any round.
                self.drv.journal().publish();
                return o;
            }
        }
        self.drv.journal().publish();
        if busy {
            StepOutcome::Busy
        } else {
            StepOutcome::Idle
        }
    }

    /// The driver's read-only view.
    pub fn view(&self) -> DriverView<'_> {
        DriverView::new(&self.e)
    }
    /// The engine state (tests, tools).
    pub fn engine(&self) -> &Engine {
        &self.e
    }
    /// Mutable engine state (test hooks only).
    #[cfg(feature = "testing")]
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.e
    }
    pub fn instances(&self) -> &Instances<S> {
        &self.inst
    }
    #[cfg(feature = "testing")]
    pub fn instances_mut(&mut self) -> &mut Instances<S> {
        &mut self.inst
    }
    pub fn driver(&self) -> &D {
        &self.drv
    }
    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.drv
    }
    /// Writes every component's snapshot sections of the current state (readers, tools).
    pub fn write_sections(&self, w: &mut hft_strategy_sdk::SnapshotWriter) {
        snapshot::write_sections(&self.e, &self.inst, w);
    }
    /// Hands the driver back (end of a replay).
    pub fn into_driver(self) -> D {
        self.drv
    }
}
