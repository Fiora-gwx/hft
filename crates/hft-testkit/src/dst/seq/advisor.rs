//! Journal-only Model producer for file-replay DST. No checkpoint or synthetic payload source.

use hft_event::{EventKind, RecordFlags};
use hft_rt::Heartbeat;
use hft_sequencer::advisor::{AdviceRing, Advisor, AdvisorCfg, CheckpointStore, CkptHdr, CkptIoErr};
use hft_sequencer::driver::replay::Log;
use hft_sequencer::{EngineSetup, MemJournal, StepOutcome};
use hft_strategy_sdk::StrategySet;
use hft_types::StrategyId;

use crate::dst::sut::{InputRec, Source};

const RING_BYTES: usize = 8 << 20;
const RING_RECORDS: usize = 4_096;

struct NoCheckpoints;
impl CheckpointStore for NoCheckpoints {
    fn load(&mut self, _: StrategyId, _: &mut Vec<u8>) -> Option<CkptHdr> {
        None
    }
    fn begin(&mut self, _: StrategyId, _: &CkptHdr) -> Result<(), CkptIoErr> {
        Err(CkptIoErr)
    }
    fn write_chunk(&mut self, _: &[u8]) -> Result<(), CkptIoErr> {
        Err(CkptIoErr)
    }
    fn commit(&mut self) -> Result<(), CkptIoErr> {
        Err(CkptIoErr)
    }
}

#[derive(Default)]
struct Ring {
    inputs: Vec<InputRec>,
    bytes: usize,
}
impl AdviceRing for Ring {
    fn free(&self) -> usize {
        if self.inputs.len() >= RING_RECORDS {
            0
        } else {
            RING_BYTES - self.bytes
        }
    }
    fn push(&mut self, kind: EventKind, body: &[u8]) -> bool {
        if body.len() > self.free() || self.inputs.len() >= RING_RECORDS {
            return false;
        }
        self.bytes += body.len();
        self.inputs.push(InputRec {
            kind,
            body: body.to_vec(),
            src: Source::Advisor,
        });
        true
    }
}

/// Trailing measurement Meta can be removed by recovery. Never expose that replaceable tail
/// to an incremental consumer; the last complete EVENT_END and all earlier bytes are retained.
pub(super) fn committed_log(j: &MemJournal) -> Result<Log, String> {
    let end = j
        .records()
        .take(j.committed)
        .filter(|r| r.flags.contains(RecordFlags::EVENT_END))
        .last()
        .ok_or("journal has no committed event boundary")?;
    Ok(super::snapshots::prefix(j, j.record_end_seq(end.seq)))
}

pub(super) struct ModelDriver<S: StrategySet> {
    a: Advisor<S, NoCheckpoints, Ring>,
}

impl<S: StrategySet> ModelDriver<S> {
    pub fn start(log: &Log, setup: &EngineSetup, hb: &'static Heartbeat) -> Result<Self, String> {
        if log
            .recs
            .iter()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .count()
            != 1
        {
            return Err("real Model advisor must start at the first boot snapshot".into());
        }
        let a = Advisor::start(
            log,
            setup,
            AdvisorCfg {
                batch: 1,
                checkpoint_ms: 0,
                ..AdvisorCfg::default()
            },
            NoCheckpoints,
            Ring::default(),
            hb,
        )
        .map_err(|e| format!("real Model advisor boot: {e:?}"))?;
        Ok(Self { a })
    }

    pub fn poll(&mut self, log: &Log) -> Result<Vec<InputRec>, String> {
        let prior = &self.a.mirror.driver().sink.log;
        if log.recs.len() < prior.recs.len()
            || prior.recs.iter().zip(&log.recs).any(|(a, b)| {
                a.seq != b.seq || a.tsc != b.tsc || a.kind != b.kind || a.flags != b.flags || a.body != b.body
            })
        {
            return Err("real Model advisor committed prefix changed".into());
        }
        // One Input per poll, plus recorded-run changes and a final idle poll. This is a
        // deterministic runaway bound, independent of wall time or checker settings.
        for _ in 0..log.recs.len().saturating_mul(2).saturating_add(64) {
            let more = self.a.poll_once(log);
            if self.a.failure.is_some()
                || matches!(self.a.mirror.engine().finished, Some(StepOutcome::Fatal(_)))
            {
                return Err(format!(
                    "real Model advisor failed: {:?} {:?}",
                    self.a.failure,
                    self.a.mirror.engine().finished
                ));
            }
            if !more {
                if !self.a.mirror.driver().finished()
                    || self.a.checkpointing()
                    || !self.a.outbox.is_empty()
                    || !self.a.notes.is_empty()
                {
                    return Err("real Model advisor unfinished or output buffer full".into());
                }
                self.a.ring.bytes = 0;
                return Ok(core::mem::take(&mut self.a.ring.inputs));
            }
        }
        Err("real Model advisor poll limit exceeded".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dst::{BootCtx, SeqSut, Sut, WorldSpec};
    use crate::strategies::TestStrategies;
    use hft_sequencer::advisor::OutboxItem;
    use hft_types::{AccountId, GroupId};

    #[test]
    fn tk_154_model_buffers_and_prefix_changes_fail_explicitly() {
        let mut ring = Ring::default();
        for _ in 0..RING_RECORDS {
            assert!(ring.push(EventKind::ADVICE, &[0]));
        }
        assert_eq!(ring.free(), 0);
        assert!(!ring.push(EventKind::ADVICE, &[0]));
        assert_eq!(ring.inputs.len(), RING_RECORDS);
        let mut bytes = Ring::default();
        assert!(bytes.push(EventKind::ADVICE, &vec![0; RING_BYTES]));
        assert!(!bytes.push(EventKind::ADVICE, &[0]));
        assert_eq!(bytes.inputs.len(), 1);
        let mut world = WorldSpec::w1();
        world.model_advice = true;
        let mut e = SeqSut::<TestStrategies>::boot(&BootCtx {
            seed: 0,
            world: &world,
            planted: &[],
            tsc_hz: 1_000_000_000,
        })
        .unwrap();
        let log = committed_log(&e.sequencer().driver().io.j).unwrap();
        let driver = e.advisor.as_mut().unwrap();
        let mut bad = log.clone();
        bad.recs[0].tsc += 1;
        assert!(driver.poll(&bad).unwrap_err().contains("prefix changed"));
        driver.a.ring = ring;
        driver.a.outbox.push(OutboxItem {
            key: (StrategyId(1), AccountId(1), GroupId(0), 1),
            rec: vec![0; 64],
        });
        assert!(driver.poll(&log).unwrap_err().contains("output buffer full"));
    }
}
