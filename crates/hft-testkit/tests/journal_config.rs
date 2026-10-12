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
    fn verify_ledger_event(&self, e: &hft_testkit::dst::sut::EventStep) -> Result<(), String> {
        self.0.verify_ledger_event(e)
    }
    fn committed_receipts(&self, fills: &[hft_event::ExternalFill]) -> Result<Vec<bool>, String> {
        self.0.committed_receipts(fills)
    }
    fn reconcile_inputs(
        &self,
        actor: &mut hft_testkit::dst::actors::ReconActor,
        truth: &[hft_testkit::dst::obs::TruthRow],
        history: &[hft_testkit::dst::venue_behavior::ExternalReceipt],
    ) -> Result<Option<Vec<InputRec>>, String> {
        self.0.reconcile_inputs(actor, truth, history)
    }
    fn rebuilt_hashes(&self) -> Result<Option<ComponentHashes>, String> {
        if self.1 {
            Err("injected replay failure at seq 17".into())
        } else {
            Ok(None)
        }
    }
    fn rebuilt_snapshot(&self, p: &hft_testkit::dst::sut::SnapshotPoint) -> Result<ComponentHashes, String> {
        self.0.rebuilt_snapshot(p)
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

#[test]
fn tk_114_round_notes_replay_across_storage_restarts() {
    use hft_event::{Note, OrderEvent, OrderEventKind, Role};
    use hft_testkit::dst::run_record;

    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "storage")
        .unwrap();
    for seed in [
        0x00cc_7b25_194f_1b2c,
        0x00cc_7b25_194f_1b3a,
        0x00cc_7b25_194f_1b4b,
        0x00cc_7b25_194f_1b4d,
        0x00cc_7b25_194f_1bc8,
        0x00cc_7b25_194f_1be0,
    ] {
        let mut c = suite.config(seed);
        // Fault timing depends on the full horizon, including crashes after the first failure.
        c.steps = 10_000;
        let r = run_record::<E>(&c);
        assert!(
            r.outcome.is_pass(),
            "seed={seed:#x}: {}",
            crate::seq_sut::show(&r)
        );
        assert_eq!(r.steps_run, c.steps);
        let mut last_input = None;
        let mut closed = 0;
        for record in &r.journal {
            if hft_event::classify(record.kind, &record.body).is_ok_and(|role| role == Role::Input) {
                last_input = Some(record);
            }
            if let Some(note) = record.try_read::<Note>()
                && note.code == hft_ledger::notes::code::ROUND_CLOSED
            {
                closed += 1;
                let input = last_input.unwrap();
                // These seeds close rounds through OMS; reconciliation must not inherit Notes.
                let event = input
                    .try_read::<OrderEvent>()
                    .expect("RoundClosed belongs to OrderEvent");
                assert!(matches!(
                    event.what,
                    OrderEventKind::Fill | OrderEventKind::PartialFill | OrderEventKind::FeeOnly
                ));
            }
        }
        assert!(closed > 0, "seed={seed:#x}: must actually close a round");
    }
}

#[test]
fn tk_120_drain_restart_conformance_keeps_recovery_gates() {
    use hft_event::{CapRecord, OrderAction, OrderDecision, SnapshotHdr};
    use hft_risk::snapshot::{GateRow, SECTION_GATES, SECTION_RISK_STATE};
    use hft_strategy_sdk::SnapshotReader;
    use hft_testkit::dst::{RerunPolicy, run_record};
    use hft_types::{CapSource, RejectBits};

    let suite = hft_testkit::scenarios::suites()
        .into_iter()
        .find(|s| s.name == "core")
        .unwrap();
    for (seed, rejected_after_restart) in [(7, 0), (28, 2), (34, 0)] {
        let mut c = suite.config(seed);
        c.steps = 2_000;
        c.rerun = RerunPolicy::Always;
        let r = run_record::<E>(&c);
        assert!(r.outcome.is_pass(), "seed={seed}: {}", crate::seq_sut::show(&r));
        assert_eq!(r.steps_run, c.steps);
        assert!(!r.trail.points.is_empty());
        let mut drains = 0;
        let mut recoveries = 0;
        let mut snapshots = 0;
        let mut rejected = 0;
        let mut drained_caps = None;
        for record in &r.journal {
            if record.kind != EventKind::STATE_SNAPSHOT {
                if recoveries > 0
                    && let Some(decision) = record.try_read::<OrderDecision>()
                    && decision.action == OrderAction::New
                {
                    rejected += 1;
                    assert_eq!(decision.reject, RejectBits::CLOSE_ONLY.0);
                    assert_eq!(decision.slot.0, 0, "a rejected order gets no slot");
                }
                continue;
            }
            snapshots += 1;
            let (hdr, _) = hft_testkit::hash::sections(&record.body).unwrap();
            let sections = SnapshotReader::new(&record.body[size_of::<SnapshotHdr>()..]);
            let cap_section = sections.find_section(SECTION_RISK_STATE).unwrap().unwrap();
            let mut reader = cap_section.reader();
            let caps: Vec<CapRecord> = (0..cap_section.count)
                .map(|_| reader.get_row().unwrap())
                .collect();
            assert!(reader.is_empty());
            match hdr.purpose {
                5 => {
                    drains += 1;
                    assert!(caps.iter().any(|cap| cap.source == CapSource::DRAINING));
                    drained_caps = Some(
                        caps.iter()
                            .filter(|cap| !matches!(cap.source, CapSource::DRAINING | CapSource::RECOVERY))
                            .copied()
                            .collect::<Vec<_>>(),
                    );
                }
                4 => {
                    recoveries += 1;
                    let saved = drained_caps.take().expect("recovery follows a real Drain");
                    assert!(!saved.is_empty(), "seed={seed}: other caps must be exercised");
                    assert!(caps.iter().all(|cap| cap.source != CapSource::DRAINING));
                    let kept: Vec<_> = caps
                        .iter()
                        .filter(|cap| cap.source != CapSource::RECOVERY)
                        .copied()
                        .collect();
                    assert_eq!(saved, kept, "seed={seed}: other caps survive unchanged");
                    let gates = sections.find_section(SECTION_GATES).unwrap().unwrap();
                    assert!(gates.count > 0);
                    let mut reader = gates.reader();
                    for _ in 0..gates.count {
                        let gate: GateRow = reader.get_row().unwrap();
                        assert_eq!(gate.released, 0, "seed={seed}: no automatic gate bypass");
                        assert!(caps.iter().any(|cap| {
                            cap.source == CapSource::RECOVERY && cap.scope.account == AccountId(gate.account)
                        }));
                    }
                    assert!(reader.is_empty());
                }
                _ => {}
            }
        }
        assert!(drains > 0, "seed={seed}: actual Drain required");
        assert_eq!(drains, recoveries, "seed={seed}: every Drain restarts");
        assert_eq!(r.snapshot_checks, snapshots);
        assert_eq!(rejected, rejected_after_restart, "seed={seed}");
    }
}
