//! Cold, generic verification assembly. File discovery and JSON belong to the caller;
//! state reconstruction and Output alignment remain in the sequencer crate.

use hft_event::{
    BlobHdr, BlobKind, EventKind, HostProfile, Role, SegmentRoll, SnapshotHdr, StrategyParamsHdr,
};
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
    /// Until independent recovery/version runs are assembled, never claim to verify them.
    UnsupportedRun {
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
    /// Owns the original log and bounded mismatch details for the cold report formatter.
    pub sink: ReplaySink,
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
    let from = opts.from_seq.unwrap_or(0);
    let to = opts.to_seq.unwrap_or(u64::MAX);
    if from > to || log.recs.last().is_none_or(|r| from > r.seq) {
        return Err(VerifyError::Window);
    }
    // Reject unsupported strategy kinds before replaying even the first Input. A syntactically
    // valid blob is checked by the normal parser when its command becomes visible.
    let mut build = None;
    for r in &log.recs {
        if r.kind == EventKind::SEGMENT_ROLL
            && r.read::<SegmentRoll>().is_some_and(|s| matches!(s.reason, 4 | 6))
        {
            return Err(VerifyError::UnsupportedRun { seq: r.seq });
        }
        if r.kind == EventKind::CONFIG_BLOB {
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
        if r.kind == EventKind::HOST_PROFILE {
            let p = r
                .read::<HostProfile>()
                .ok_or(VerifyError::UnsupportedRun { seq: r.seq })?;
            if build.is_some_and(|b| b != p.build_git) {
                return Err(VerifyError::UnsupportedRun { seq: r.seq });
            }
            build = Some(p.build_git);
        }
        if r.kind == EventKind::STATE_SNAPSHOT
            && r.read::<SnapshotHdr>()
                .is_some_and(|h| h.purpose == crate::snapshot::PURPOSE_RECOVERY)
        {
            return Err(VerifyError::UnsupportedRun { seq: r.seq });
        }
    }
    let selected = if opts.from_seq.is_some() {
        log.recs
            .iter()
            .rev()
            .find(|r| r.seq <= from && r.kind == EventKind::STATE_SNAPSHOT)
    } else {
        log.recs.iter().find(|r| {
            r.kind == EventKind::STATE_SNAPSHOT
                && r.read::<SnapshotHdr>()
                    .is_some_and(|h| h.purpose == crate::snapshot::PURPOSE_BOOT)
        })
    }
    .ok_or(VerifyError::NoSnapshot)?
    .seq;
    if selected > to {
        return Err(VerifyError::NoSnapshot);
    }
    check_snapshot::<S>(&log, selected).map_err(VerifyError::Snapshot)?;
    let setup = EngineSetup::from_snapshot(&log, selected).map_err(VerifyError::Boot)?;
    let end = log
        .recs
        .iter()
        .position(|r| r.seq > to && r.role() == Role::Input)
        .unwrap_or(log.recs.len());
    let mut driver = ReplayDriver::new(ReplayMode::Verify, log, 0, 1);
    driver.end = end;
    driver.sink.compare_from = from;
    driver.sink.compare_to = to;
    driver.sink.stop_on_first = !opts.keep_going;
    // At most one retained detail per logged record; overflow is reported, never silent.
    driver.sink.detail_limit = driver.sink.log.recs.len();
    let plan = RecoverPlan {
        start: SnapshotSel::AtSeq(selected),
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
        let before = seq.driver().handed;
        let outcome = seq.step();
        if seq.driver().sink.halted {
            break;
        }
        if matches!(outcome, StepOutcome::Fatal(_))
            || (!seq.driver().finished()
                && (seq.driver().handed == before || matches!(outcome, StepOutcome::Done { .. })))
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
    let mut driver = seq.into_driver();
    driver.finish_verification();
    if driver.sink.detail.len() != driver.sink.mismatches.len() {
        return Err(VerifyError::DiagnosticLimit);
    }
    let snapshots = 1 + driver.sink.snapshots_compared;
    Ok(VerifyResult {
        start_snapshot: selected,
        inputs: driver.handed,
        snapshots,
        sink: driver.sink,
    })
}
