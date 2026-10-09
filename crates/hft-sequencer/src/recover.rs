//! Cold-start recovery (README "冷启动恢复"): placeholder until the recovery module lands.

use crate::snapshot::ClockCalRow;

/// Recovery bookkeeping kept in the engine.
#[derive(Clone, Debug)]
pub struct RecoverState {
    pub clock_cal: ClockCalRow,
    /// `now` of `recovery_end` (Recovery snapshot `snap_tsc`).
    pub recovery_now: hft_types::Tsc,
}

pub(crate) fn mirror_restore_all(_e: &mut crate::engine::Engine) {}

pub(crate) fn boot_journal<D: crate::Driver<S>, S: hft_strategy_sdk::StrategySet>(
    _plan: crate::types::RecoverPlan,
    _setup: &crate::engine::EngineSetup,
    _drv: D,
    _hb: &'static hft_rt::Heartbeat,
) -> Result<crate::Sequencer<D, S>, crate::types::BootError> {
    Err(crate::types::BootError::Journal(crate::types::JournalErr::NoSnapshot))
}

impl Default for RecoverState {
    fn default() -> RecoverState {
        RecoverState {
            clock_cal: ClockCalRow::default(),
            recovery_now: hft_types::Tsc(0),
        }
    }
}
