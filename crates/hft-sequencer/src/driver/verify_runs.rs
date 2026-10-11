//! Cold planning of independent executions. A segment rollover keeps the existing engine;
//! a process restart or build change restores the new boot snapshot instead.

use hft_event::{EventKind, HostProfile, Note, SegmentRoll, SnapshotHdr};
use hft_journal::{RollMark, RollReason};

use super::replay::Log;
use super::verify::VerifyError;
use crate::snapshot::{PURPOSE_BOOT, PURPOSE_RECOVERY};

#[derive(Clone, Copy, Debug)]
pub(super) struct RunStart {
    pub first: usize,
    pub snapshot: usize,
}

pub(super) fn plan(log: &Log) -> Result<Vec<RunStart>, VerifyError> {
    let bad = |ix: usize| VerifyError::RunBoundary {
        seq: log.recs.get(ix).map_or(0, |r| r.seq),
    };
    if log.seg_start.first() != Some(&0)
        || log.seg_start.len() != log.segs.len()
        || log.seg_start.windows(2).any(|w| w[0] >= w[1])
    {
        return Err(bad(0));
    }
    let mut runs = Vec::new();
    let mut previous_build = None;
    for (si, &first) in log.seg_start.iter().enumerate() {
        let end = log.seg_start.get(si + 1).copied().unwrap_or(log.recs.len());
        let records = log.recs.get(first..end).ok_or_else(|| bad(first))?;
        let begin = records
            .first()
            .filter(|r| r.kind == EventKind::SEGMENT_ROLL)
            .and_then(|r| r.read::<SegmentRoll>())
            .filter(|r| r.mark == RollMark::Begin as u8)
            .ok_or_else(|| bad(first))?;
        let reason = RollReason::from_u8(begin.reason).ok_or_else(|| bad(first))?;
        let done = records
            .iter()
            .position(|r| {
                r.kind == EventKind::SEGMENT_ROLL
                    && r.read::<SegmentRoll>()
                        .is_some_and(|s| s.mark == RollMark::BootDone as u8)
            })
            .ok_or_else(|| bad(first))?;
        let mut boot_snapshot = None;
        let mut build = None;
        for (j, r) in records.iter().enumerate() {
            if r.kind == EventKind::HOST_PROFILE && j < done {
                let h = r.read::<HostProfile>().ok_or_else(|| bad(first + j))?;
                if build.replace(h.build_git).is_some() {
                    return Err(bad(first + j));
                }
            }
            if r.kind == EventKind::STATE_SNAPSHOT {
                let h = r.read::<SnapshotHdr>().ok_or_else(|| bad(first + j))?;
                if j < done {
                    let recovery = matches!(reason, RollReason::Recovery | RollReason::RecoverFromSnapshot);
                    let want = if recovery { PURPOSE_RECOVERY } else { PURPOSE_BOOT };
                    if h.purpose != want || boot_snapshot.replace(first + j).is_some() {
                        return Err(bad(first + j));
                    }
                } else if h.purpose == PURPOSE_RECOVERY || h.purpose == PURPOSE_BOOT {
                    return Err(bad(first + j));
                }
            }
            // Recovery-mismatch acceptance needs a separately verified recovery range. Until
            // that is available, do not silently skip these pre-EngineClock output Notes.
            if r.kind == EventKind::NOTE
                && r.read::<Note>()
                    .is_some_and(|n| n.code == crate::codes::seq_note(crate::codes::note::RECOVER_MISMATCH))
            {
                return Err(VerifyError::UnsupportedRun { seq: r.seq });
            }
        }
        let build = build.ok_or_else(|| bad(first))?;
        let snapshot = boot_snapshot.ok_or_else(|| bad(first))?;
        let changed = previous_build.is_some_and(|b| b != build);
        if si == 0
            || changed
            || matches!(
                reason,
                RollReason::Startup | RollReason::Recovery | RollReason::RecoverFromSnapshot
            )
        {
            // The first ordinary Input of a process run must establish its EngineClock.
            // Initial partial inputs beginning at an ordinary segment boot need not do so.
            if si > 0
                || matches!(
                    reason,
                    RollReason::Startup | RollReason::Recovery | RollReason::RecoverFromSnapshot
                )
            {
                let next = records[done + 1..]
                    .iter()
                    .find(|r| r.role() == hft_event::Role::Input);
                if next.is_some_and(|r| r.kind != EventKind::ENGINE_CLOCK) {
                    return Err(bad(first + done + 1));
                }
            }
            runs.push(RunStart { first, snapshot });
        }
        previous_build = Some(build);
    }
    Ok(runs)
}
