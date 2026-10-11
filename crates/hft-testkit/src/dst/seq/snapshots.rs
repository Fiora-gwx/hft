//! Snapshot boundary observations and independent journal-only rebuilds (INV-6).

use hft_event::{EventKind, RecordFlags};
use hft_rt::Heartbeat;
use hft_sequencer::audit::StateView;
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::{
    BootSource, EngineSetup, MemJournal, RecoverPlan, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome,
};
use hft_strategy_sdk::{SnapshotWriter, StrategySet};

use crate::dst::sut::{SnapshotPoint, StepOut};
use crate::hash::ComponentHashes;

pub const CAPACITY: usize = 4_096;

#[derive(Debug, Default)]
pub struct Observations {
    points: Vec<SnapshotPoint>,
    error: Option<String>,
    seen_through: u64,
}

impl Observations {
    pub fn capture<S: StrategySet>(&mut self, j: &MemJournal, state: StateView<'_, S>) {
        let Some(end) = j
            .segments
            .iter()
            .rev()
            .flat_map(|s| s.records.iter().rev())
            .find(|r| r.flags.0 & RecordFlags::EVENT_END.0 != 0)
        else {
            return;
        };
        let through_seq = end.seq;
        let snaps: Vec<_> = j
            .segments
            .iter()
            .flat_map(|s| &s.records[s.records.partition_point(|r| r.seq <= self.seen_through)..])
            .take_while(|r| r.seq <= through_seq)
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .map(|r| r.seq)
            .take(CAPACITY + 1)
            .collect();
        self.seen_through = through_seq;
        if snaps.is_empty() {
            return;
        }
        if self.points.len() + snaps.len() > CAPACITY {
            self.error = Some("snapshot observation buffer overflow".into());
            return;
        }
        let mut w = SnapshotWriter::new(64 << 20);
        state.write_sections(&mut w);
        let live = crate::hash::component_hashes_of_sections(w.as_bytes());
        self.points
            .extend(snaps.into_iter().map(|snapshot_seq| SnapshotPoint {
                snapshot_seq,
                through_seq,
                live,
            }));
    }

    pub fn drain(&mut self, out: &mut StepOut) {
        if out.snapshots.len() + self.points.len() > CAPACITY {
            out.snapshot_error = Some("snapshot observation buffer overflow".into());
            self.points.clear();
        } else {
            out.snapshots.append(&mut self.points);
        }
        if let Some(e) = self.error.take() {
            out.snapshot_error = Some(e);
        }
    }

    pub fn restart(&mut self, committed_seq: u64) {
        self.points.retain(|p| p.through_seq <= committed_seq);
        self.seen_through = committed_seq;
    }
}

/// Only the selected prefix is copied. Later Inputs and future configuration are unavailable.
pub fn prefix(j: &MemJournal, through_seq: u64) -> Log {
    let mut log = Log::default();
    for s in &j.segments {
        let n = s.records.partition_point(|r| r.seq <= through_seq);
        if n == 0 {
            break;
        }
        log.seg_start.push(log.recs.len());
        log.segs.push(s.clock);
        log.recs.extend_from_slice(&s.records[..n]);
        if n < s.records.len() {
            break;
        }
    }
    log
}

pub fn rebuild<S: StrategySet>(
    log: Log,
    snap: u64,
    hb: &'static Heartbeat,
) -> Result<ComponentHashes, String> {
    let setup =
        EngineSetup::from_snapshot(&log, snap).map_err(|e| format!("snapshot {snap} config: {e:?}"))?;
    let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, setup.seq.replay_per_round);
    let plan = RecoverPlan {
        start: SnapshotSel::AtSeq(snap),
        replay_tail: true,
        epoch: 0,
        flags: RecoveryFlags(0),
        quarantine: Vec::new(),
    };
    let mut s: Sequencer<ReplayDriver, S> = Sequencer::boot(BootSource::Journal(plan), &setup, drv, hb)
        .map_err(|e| format!("snapshot {snap} restore: {e:?}"))?;
    for _ in 0..10_000_000 {
        if s.step() != StepOutcome::Busy {
            break;
        }
    }
    let d = s.driver();
    if !d.finished() {
        return Err(format!(
            "snapshot {snap}: replay unfinished at cursor {}",
            d.cursor
        ));
    }
    if !d.mismatches().is_empty() || !d.leftover().is_empty() {
        let detail = d.sink.detail.first().map(|(seq, a, b)| {
            let at = a
                .iter()
                .zip(b)
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            (
                *seq,
                a.len(),
                b.len(),
                at,
                &a[at.min(a.len())..(at + 16).min(a.len())],
                &b[at.min(b.len())..(at + 16).min(b.len())],
            )
        });
        return Err(format!(
            "snapshot {snap}: replay outputs differ: mismatches {:?}, leftover {:?}, detail {:?}",
            d.mismatches().first(),
            d.leftover().first(),
            detail
        ));
    }
    Ok(super::SeqSut::<S>::hashes_of(&s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tk_116_snapshot_observations_follow_commit_boundary_and_capacity() {
        let point = |seq| SnapshotPoint {
            snapshot_seq: seq - 1,
            through_seq: seq,
            live: ComponentHashes::default(),
        };
        let mut observations = Observations {
            points: vec![point(10), point(20)],
            error: None,
            seen_through: 20,
        };
        observations.restart(10);
        observations.restart(10);
        assert_eq!(observations.points, vec![point(10)]);
        assert_eq!(observations.seen_through, 10);
        let mut out = StepOut::default();
        observations.drain(&mut out);
        observations.drain(&mut out);
        assert_eq!(out.snapshots, vec![point(10)]);
        out.snapshots.resize(CAPACITY, point(10));
        observations.points.push(point(30));
        observations.drain(&mut out);
        assert_eq!(
            out.snapshot_error.as_deref(),
            Some("snapshot observation buffer overflow")
        );
        assert_eq!(out.snapshots.len(), CAPACITY);
        assert!(observations.points.is_empty());
    }
}
