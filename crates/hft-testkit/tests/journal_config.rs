use hft_event::EventKind;
use hft_rt::Heartbeat;
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::{
    BootSource, EngineSetup, RecoverPlan, RecoveryFlags, Sequencer, SnapshotSel, StepOutcome,
};
use hft_strategy_sdk::SnapshotWriter;
use hft_testkit::dst::SeqSut;
use hft_testkit::dst::sut::{BootCtx, Sut};
use hft_testkit::dst::world::WorldSpec;
use hft_testkit::strategies::TestStrategies;

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
    assert_eq!(Some(expected), a.rebuilt_hashes());
    assert_eq!(Some(b.hashes()), b.rebuilt_hashes());
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
