//! Cold, generic verification assembly. File discovery and JSON belong to the caller;
//! state reconstruction and Output alignment remain in the sequencer crate.

use hft_event::{BlobHdr, BlobKind, EventKind, Role, StrategyParamsHdr};
use hft_rt::Heartbeat;
use hft_strategy_sdk::StrategySet;
use zerocopy::TryFromBytes;

use super::replay::{Log, ReplayDriver, ReplayMode, ReplaySink};
use super::replay_snapshot::{SnapshotCheckError, check_snapshot};
use crate::{
    BootError, BootSource, EngineSetup, RecoverPlan, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct VerifyOptions {
    pub from_seq: Option<u64>,
    pub to_seq: Option<u64>,
    pub keep_going: bool,
}

#[derive(Debug)]
pub enum VerifyError {
    Window,
    NoSnapshot,
    Snapshot(SnapshotCheckError),
    Boot(BootError),
    UnknownStrategy {
        seq: u64,
        kind: u16,
    },
    InvalidParams {
        seq: u64,
    },
    DiagnosticLimit,
    /// A missing/inconsistent boot block cannot establish an independent execution.
    RunBoundary {
        seq: u64,
    },
    /// Invalid recovery evidence, or its referenced old tail is unavailable.
    RecoveryRange {
        seq: u64,
    },
    Unfinished {
        seq: u64,
        outcome: StepOutcome,
    },
}

#[derive(Debug)]
pub struct VerifyResult {
    pub start_snapshot: u64,
    pub inputs: u64,
    pub snapshots: u64,
    pub runs: Vec<RunReport>,
    /// Owns the original log and bounded mismatch details for the cold report formatter.
    pub sink: ReplaySink,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunReport {
    pub start_snapshot: u64,
    /// The next independent run's first record, or None for the final run.
    pub stop_before: Option<u64>,
    pub inputs: u64,
    pub outputs: u64,
    pub snapshots: u64,
    pub mismatches: usize,
    pub recovery_diffs: usize,
}

/// Run from the latest snapshot at/before `from_seq`, or the first boot snapshot. Input
/// delivery extends to the end of the event containing `to_seq`; only Outputs inside the
/// inclusive window count. A stop or fatal outcome is never mistaken for a completed replay.
/// `hb` belongs to the caller, as for every sequencer assembly; it has no decision authority.
pub fn verify<S: StrategySet>(
    log: Log,
    opts: VerifyOptions,
    hb: &'static Heartbeat,
) -> Result<VerifyResult, VerifyError> {
    verify_with_clock::<S>(log, opts, hb, None)
}

/// Optional diagnostic clock, read only around decision computation. Logged TSC and all
/// comparison bytes remain unchanged. One sample is retained per compared OrderDecision.
pub fn verify_with_clock<S: StrategySet>(
    log: Log,
    opts: VerifyOptions,
    hb: &'static Heartbeat,
    clock_ns: Option<fn() -> u64>,
) -> Result<VerifyResult, VerifyError> {
    let from = opts.from_seq.unwrap_or(0);
    let to = opts.to_seq.unwrap_or(u64::MAX);
    if from > to || log.recs.last().is_none_or(|r| from > r.seq) {
        return Err(VerifyError::Window);
    }
    validate_kinds::<S>(&log)?;
    let planned = super::verify_runs::plan(&log)?;
    let starts = planned.runs;
    let selected = if opts.from_seq.is_some() {
        log.recs
            .iter()
            .rposition(|r| r.seq <= from && r.kind == EventKind::STATE_SNAPSHOT)
    } else {
        starts.first().map(|r| r.snapshot)
    }
    .ok_or(VerifyError::NoSnapshot)?;
    let start_snapshot = log.recs[selected].seq;
    if start_snapshot > to {
        return Err(VerifyError::NoSnapshot);
    }
    let window_end = log
        .recs
        .iter()
        .position(|r| r.seq > to && r.role() == Role::Input)
        .unwrap_or(log.recs.len());
    let mut driver = ReplayDriver::new(ReplayMode::Verify, log, 0, 1);
    if let Some(clock) = clock_ns {
        driver.sink.decide_timing = Some(super::replay::DecideTiming::new(clock, driver.end));
    }
    driver.sink.compare_from = from;
    driver.sink.compare_to = to;
    driver.sink.stop_on_first = !opts.keep_going;
    driver.sink.recoveries = planned.recoveries;
    driver.sink.recovery_notes = planned.recovery_notes;
    // At most one retained detail per logged record; overflow is reported, never silent.
    driver.sink.detail_limit = driver.sink.log.recs.len();
    let first_run = starts
        .iter()
        .rposition(|r| r.first <= selected)
        .ok_or(VerifyError::NoSnapshot)?;
    let mut runs = Vec::new();
    let mut snapshots = 0;
    for ri in first_run..starts.len() {
        let start = starts[ri];
        let log = &driver.sink.log;
        if log.recs[start.first].seq > to {
            break;
        }
        let ix = if ri == first_run { selected } else { start.snapshot };
        let at = log.recs[ix].seq;
        if at > to {
            return Err(VerifyError::NoSnapshot);
        }
        check_snapshot::<S>(log, at).map_err(VerifyError::Snapshot)?;
        let setup = EngineSetup::from_snapshot(log, at).map_err(VerifyError::Boot)?;
        let next = starts.get(ri + 1).map(|r| r.first);
        let stop_before = next.map(|i| log.recs[i].seq);
        driver.end = next.unwrap_or(log.recs.len()).min(window_end);
        driver.sink.record_end = driver.end;
        let before = (
            driver.handed,
            driver.sink.compared,
            driver.sink.snapshots_compared,
            driver.sink.mismatches.len(),
            driver.sink.recovery_diffs.len(),
        );
        let plan = RecoverPlan {
            start: SnapshotSel::AtSeq(at),
            replay_tail: true,
            epoch: 0,
            flags: RecoveryFlags(0),
            quarantine: Vec::new(),
        };
        let mut seq = Sequencer::<ReplayDriver, S>::boot(BootSource::Journal(plan), &setup, driver, hb)
            .map_err(VerifyError::Boot)?;
        loop {
            if seq.driver().sink.halted || seq.driver().finished() {
                break;
            }
            let handed = seq.driver().handed;
            let outcome = seq.step();
            if seq.driver().sink.halted {
                break;
            }
            if matches!(outcome, StepOutcome::Fatal(_))
                || (!seq.driver().finished()
                    && (seq.driver().handed == handed || matches!(outcome, StepOutcome::Done { .. })))
            {
                let at = seq
                    .driver()
                    .sink
                    .log
                    .recs
                    .get(seq.driver().cursor)
                    .map_or(0, |r| r.seq);
                return Err(VerifyError::Unfinished { seq: at, outcome });
            }
        }
        driver = seq.into_driver();
        driver.finish_verification();
        let checked = 1 + driver.sink.snapshots_compared - before.2;
        snapshots += checked;
        runs.push(RunReport {
            start_snapshot: at,
            stop_before,
            inputs: driver.handed - before.0,
            outputs: driver.sink.compared - before.1,
            snapshots: checked,
            mismatches: driver.sink.mismatches.len() - before.3,
            recovery_diffs: driver.sink.recovery_diffs.len() - before.4,
        });
        if driver.sink.halted || driver.end == window_end {
            break;
        }
    }
    if driver.sink.detail.len() != driver.sink.mismatches.len() || driver.sink.diagnostic_overflow {
        return Err(VerifyError::DiagnosticLimit);
    }
    Ok(VerifyResult {
        start_snapshot,
        inputs: driver.handed,
        snapshots,
        runs,
        sink: driver.sink,
    })
}

/// Reject unknown kinds before either replay path runs its first Input.
pub(super) fn validate_kinds<S: StrategySet>(log: &Log) -> Result<(), VerifyError> {
    for r in &log.recs {
        if r.kind != EventKind::CONFIG_BLOB {
            continue;
        }
        let b = r
            .read::<BlobHdr>()
            .ok_or(VerifyError::InvalidParams { seq: r.seq })?;
        if b.blob.kind == BlobKind::StrategyParams {
            let bytes = r
                .body
                .get(size_of::<BlobHdr>()..)
                .ok_or(VerifyError::InvalidParams { seq: r.seq })?;
            let (p, _) = StrategyParamsHdr::try_read_from_prefix(bytes)
                .map_err(|_| VerifyError::InvalidParams { seq: r.seq })?;
            if !S::kinds().iter().any(|k| k.kind_id == p.kind_id) {
                return Err(VerifyError::UnknownStrategy {
                    seq: r.seq,
                    kind: p.kind_id,
                });
            }
        }
    }
    Ok(())
}
