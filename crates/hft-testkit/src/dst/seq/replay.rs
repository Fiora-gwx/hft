//! End-of-run INV-6: replay all committed executions, not only the latest snapshot's suffix.

use hft_event::{EventKind, Role, SegmentRoll};
use hft_journal::{RollMark, RollReason};
use hft_rt::Heartbeat;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::verify::{VerifyOptions, verify};
use hft_strategy_sdk::StrategySet;

use crate::dst::sut::JournalVerification;

/// The cold verifier shared with replayer consumes the original records and segment clocks.
/// No live world, cached parameters, regenerated Inputs or new sequence numbers are supplied.
/// Unlike offline debugging of a changed binary, a same-build DST may not accept recovery
/// differences: a RecoverMismatch Note is evidence of an INV-6 failure, not permission to pass.
pub fn verify_all<S: StrategySet>(log: Log, hb: &'static Heartbeat) -> Result<JournalVerification, String> {
    let first = log.recs.first().ok_or("full journal is empty")?;
    if first.seq != 1
        || first.kind != EventKind::SEGMENT_ROLL
        || first
            .read::<SegmentRoll>()
            .is_none_or(|r| r.mark != RollMark::Begin as u8 || r.reason != RollReason::Startup as u8)
    {
        return Err("full journal requires the initial Startup at seq 1".into());
    }
    let inputs = log.recs.iter().filter(|r| r.role() == Role::Input).count() as u64;
    let snapshots = log
        .recs
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .count() as u64;
    let mut checked = JournalVerification {
        through_seq: log.recs.last().map_or(0, |r| r.seq),
        records: log.recs.len() as u64,
        segments: log.seg_start.len() as u64,
        runs: 0,
        inputs,
        outputs: 0,
        snapshots,
    };
    let result = verify::<S>(log, VerifyOptions::default(), hb)
        .map_err(|e| format!("full journal verification failed: {e:?}"))?;
    if let Some(m) = result.sink.mismatches.first() {
        let detail = result.sink.detail.first().map(|(_, a, b)| {
            let offset = a
                .iter()
                .zip(b)
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            (
                offset,
                a.len(),
                b.len(),
                &a[..a.len().min(96)],
                &b[..b.len().min(96)],
            )
        });
        let input = result
            .sink
            .log
            .recs
            .iter()
            .rev()
            .find(|r| r.seq <= m.seq && r.role() == Role::Input)
            .map(|r| (r.seq, r.kind.0, r.tsc, &r.body[..r.body.len().min(160)]));
        let context: Vec<_> = result
            .sink
            .log
            .recs
            .iter()
            .filter(|r| r.seq >= m.seq.saturating_sub(4))
            .take(9)
            .map(|r| (r.seq, r.kind.0, r.flags.0))
            .collect();
        return Err(format!(
            "full journal divergence at seq {} kind {}; detail {detail:?}; flags {:?}; input {input:?}; context {context:?}",
            m.seq,
            m.kind.0,
            result.sink.detail_flags.first()
        ));
    }
    if let Some(d) = result.sink.recovery_diffs.first() {
        return Err(format!(
            "full journal recovery divergence at seq {} kind {}",
            d.mismatch.seq, d.mismatch.kind.0
        ));
    }
    let outputs: u64 = result
        .runs
        .iter()
        .map(|run| {
            result
                .sink
                .log
                .recs
                .iter()
                .filter(|r| {
                    r.seq > run.start_snapshot
                        && run.stop_before.is_none_or(|end| r.seq < end)
                        && r.role() == Role::Output
                })
                .count() as u64
        })
        .sum();
    if inputs == 0
        || outputs == 0
        || result.inputs != inputs
        || result.snapshots != snapshots
        || result.sink.compared != outputs
        || result.runs.is_empty()
    {
        return Err(format!(
            "full journal coverage differs: inputs {}/{inputs}, outputs {}/{outputs}, snapshots {}/{snapshots}, runs {}",
            result.inputs,
            result.sink.compared,
            result.snapshots,
            result.runs.len()
        ));
    }
    checked.runs = result.runs.len() as u64;
    checked.outputs = result.sink.compared;
    Ok(checked)
}
