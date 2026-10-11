use hft_event::EventKind;
use hft_rt::Heartbeat;
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::{
    BootSource, EngineSetup, RecoverPlan, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome,
};
use hft_strategy_sdk::SnapshotWriter;
use hft_testkit::dst::SeqSut;
use hft_testkit::dst::obs::Obs;
use hft_testkit::dst::sut::{BootCtx, BootFail, CrashPoint, InputRec, JRec, StepOut, Sut};
use hft_testkit::dst::world::WorldSpec;
use hft_testkit::hash::ComponentHashes;
use hft_testkit::strategies::TestStrategies;
use hft_types::AccountId;

type E = SeqSut<TestStrategies>;

fn boot(world: &WorldSpec) -> E {
    E::boot(&BootCtx {
        seed: 1,
        world,
        planted: &[],
        tsc_hz: 3_000_000_000,
    })
    .unwrap()
}

#[test]
fn tk_112_worlds_do_not_share_risk_replay_state() {
    let a = boot(&WorldSpec::w1());
    let expected = a.hashes();
    let mut other = WorldSpec::w1();
    other.risk.conn_liveness_ms += 17;
    let b = boot(&other);
    assert_eq!(Ok(Some(expected)), a.rebuilt_hashes());
    assert_eq!(Ok(Some(b.hashes())), b.rebuilt_hashes());
    let log = Log::from_segments(&a.sequencer().driver().io.j.segments);
    drop(a);
    drop(b);
    // A new thread has neither a world nor a previous DST run's thread-local parameters.
    std::thread::spawn(move || {
        let at = log
            .recs
            .iter()
            .find(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .unwrap()
            .seq;
        let setup = EngineSetup::from_snapshot(&log, at).unwrap();
        let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, setup.seq.replay_per_round);
        let plan = RecoverPlan {
            start: SnapshotSel::AtSeq(at),
            replay_tail: true,
            epoch: 0,
            flags: RecoveryFlags(0),
            quarantine: Vec::new(),
        };
        let hb = Box::leak(Box::new(Heartbeat::new()));
        let mut seq: Sequencer<ReplayDriver, TestStrategies> =
            Sequencer::boot(BootSource::Journal(plan), &setup, drv, hb).unwrap();
        for _ in 0..10_000 {
            if seq.step() != StepOutcome::Busy {
                break;
            }
        }
        assert!(seq.driver().finished());
        assert!(
            seq.driver().mismatches().is_empty(),
            "{:?}",
            seq.driver().sink.detail
        );
        assert!(seq.driver().leftover().is_empty());
        let mut bytes = SnapshotWriter::new(64 << 20);
        seq.write_sections(&mut bytes);
        assert_eq!(
            expected,
            hft_testkit::hash::component_hashes_of_sections(bytes.as_bytes())
        );
    })
    .join()
    .unwrap();
}

struct MissingRebuild(E, bool);

impl Sut for MissingRebuild {
    type Matcher = <E as Sut>::Matcher;
    fn boot(c: &BootCtx<'_>) -> Result<Self, BootFail> {
        E::boot(c).map(|e| Self(e, c.seed == 2))
    }
    fn process(&mut self, n: u64, t: u64, i: &InputRec, o: &mut StepOut) {
        self.0.process(n, t, i, o);
    }
    fn drain_observations(&mut self, o: &mut StepOut) {
        self.0.drain_observations(o);
    }
    fn next_deadline(&self) -> Option<u64> {
        self.0.next_deadline()
    }
    fn fire_timers(&mut self, n: u64, t: u64, o: &mut StepOut) -> bool {
        self.0.fire_timers(n, t, o)
    }
    fn observe(&self, o: &mut Obs) {
        self.0.observe(o);
    }
    fn hashes(&self) -> ComponentHashes {
        self.0.hashes()
    }
    fn rebuilt_hashes(&self) -> Result<Option<ComponentHashes>, String> {
        if self.1 {
            Err("injected replay failure at seq 17".into())
        } else {
            Ok(None)
        }
    }
    fn seq(&self) -> u64 {
        self.0.seq()
    }
    fn journal(&self) -> &[JRec] {
        self.0.journal()
    }
    fn crash_and_recover(&mut self, p: CrashPoint, i: Option<&InputRec>) -> Result<(), BootFail> {
        self.0.crash_and_recover(p, i)
    }
    fn clean_restart(&mut self) -> Result<(), BootFail> {
        self.0.clean_restart()
    }
    fn is_sim(&self, a: AccountId) -> bool {
        self.0.is_sim(a)
    }
}

#[test]
fn tk_113_missing_rebuild_fails_instead_of_skipping() {
    use hft_testkit::dst::invariants::{InvId, InvariantSet};
    use hft_testkit::dst::{DstOutcome, FailKind, run_record};
    for (every, seed) in [(1, 1), (0, 1), (1, 2), (0, 2)] {
        // During a run, and the mandatory end check with periodic checks off.
        let mut c = crate::support::cfg(seed, 4, &[], 0, &[]);
        c.invariants = InvariantSet::only(InvId::I6);
        c.rebuild_every = every;
        let r = run_record::<MissingRebuild>(&c);
        match r.outcome {
            DstOutcome::Fail(f) => {
                assert_eq!(f.invariant, FailKind::Inv(InvId::I6));
                let message = if seed == 1 {
                    "rebuild unavailable"
                } else {
                    "injected replay failure at seq 17"
                };
                assert!(f.message.contains(message), "{}", f.message);
            }
            DstOutcome::Pass { .. } => panic!("missing rebuild was accepted, interval {every}"),
        }
    }
}
