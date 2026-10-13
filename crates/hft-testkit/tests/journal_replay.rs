//! Full-history INV-6 uses the same verifier as the file replayer, including prior executions.
use hft_event::{EventKind, RecordFlags, Role, SegmentRoll, Timer};
use hft_rt::Heartbeat;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::driver::verify::{VerifyOptions, verify};
use hft_testkit::dst::obs::Obs;
use hft_testkit::dst::seq::replay::verify_all;
use hft_testkit::dst::sut::{
    BootCtx, BootFail, CrashPoint, InputRec, JRec, JournalVerification, SnapshotPoint, Source, StepOut, Sut,
};
use hft_testkit::dst::world::WorldSpec;
use hft_testkit::dst::{DstOutcome, FailKind, InvId, InvariantSet, SeqSut, run_record};
use hft_testkit::hash::ComponentHashes;
use hft_testkit::strategies::TestStrategies;
use hft_types::{AccountId, GroupId};
use zerocopy::FromZeros;

type E = SeqSut<TestStrategies>;
static HB: Heartbeat = Heartbeat::new();

fn boot() -> E {
    boot_world(&WorldSpec::w1())
}
fn boot_world(world: &WorldSpec) -> E {
    E::boot(&BootCtx {
        seed: 0,
        world,
        planted: &[],
        tsc_hz: 3_000_000_000,
    })
    .unwrap()
}

fn timer(e: &mut E, id: u32) {
    let input = InputRec::of(
        &Timer {
            owner: 1,
            timer_id: id,
            group: GroupId(0),
            ..Timer::new_zeroed()
        },
        Source::Engine,
    );
    e.process(0, 0, &input, &mut StepOut::default());
}

fn history() -> Log {
    history_of(boot())
}
fn history_of(mut e: E) -> Log {
    // Both snapshot purposes, an ordinary roll, a drained restart and two crash recoveries.
    for id in [1, 7, 2] {
        timer(&mut e, id);
    }
    e.clean_restart().unwrap();
    for _ in 0..2 {
        e.crash_and_recover(CrashPoint::SeqInput, None).unwrap();
    }
    timer(&mut e, 7);
    Log::from_segments(&e.sequencer().driver().io.j.segments)
}

#[test]
fn tk_150_physical_journal_replays_each_run_and_snapshot_boundary() {
    let mut world = WorldSpec::w1();
    world.physical_journal = true;
    let log = history_of(boot_world(&world));
    let checked = verify_all::<TestStrategies>(log.clone(), &HB).unwrap();
    assert_eq!((checked.segments, checked.runs), (5, 4));
    for (ix, r) in log.recs.iter().enumerate() {
        let parts = match hft_event::kind_info(r.kind).unwrap().layout {
            hft_event::Layout::Multi { .. } => r.body.len().div_ceil(hft_event::PART_MAX_CHUNK).max(1) as u64,
            _ => 1,
        };
        assert_eq!(log.record_end_seq(ix), Some(r.seq + parts - 1));
        if let Some(after) = log.recs.get(ix + 1) {
            assert_eq!(after.seq, r.seq + parts);
        }
    }
    assert_eq!(
        checked.through_seq,
        log.record_end_seq(log.recs.len() - 1).unwrap()
    );
    // The world loop checks all snapshot observations and ledger-event boundaries too.
    let mut cfg = crate::support::cfg(0, 128, &[], 0, &[]);
    cfg.world = world;
    cfg.hash_every = 1;
    let checked = run_record::<E>(&cfg);
    assert!(checked.outcome.is_pass(), "{}", crate::seq_sut::show(&checked));
    assert!(checked.snapshot_checks > 0);
    assert!(!checked.ledger_checks.is_empty());
    assert!(checked.journal_verification.is_some());
    cfg.invariants = InvariantSet::none();
    let unchecked = run_record::<E>(&cfg);
    assert!(
        unchecked.outcome.is_pass(),
        "{}",
        crate::seq_sut::show(&unchecked)
    );
    assert_eq!(checked.journal, unchecked.journal);
    assert_eq!(checked.frames, unchecked.frames);
    assert_eq!(checked.deliveries, unchecked.deliveries);
    assert_eq!(checked.trail.to_bytes(), unchecked.trail.to_bytes());
}

fn latest_only(log: Log) {
    let latest = log
        .recs
        .iter()
        .rfind(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .unwrap()
        .seq;
    let r = verify::<TestStrategies>(
        log,
        VerifyOptions {
            from_seq: Some(latest),
            ..VerifyOptions::default()
        },
        &HB,
    )
    .unwrap();
    assert!(r.sink.mismatches.is_empty());
    assert!(r.sink.recovery_diffs.is_empty());
}

#[test]
fn tk_148_full_journal_checks_every_run_and_earlier_divergences() {
    let log = history();
    let ok = verify_all::<TestStrategies>(log.clone(), &HB).unwrap();
    assert_eq!(ok.records, log.recs.len() as u64);
    assert_eq!(ok.through_seq, log.recs.last().unwrap().seq);
    assert_eq!(ok.segments, 5);
    assert_eq!(ok.runs, 4, "ordinary rolling is not a restart");
    assert_eq!(
        ok.inputs,
        log.recs.iter().filter(|r| r.role() == Role::Input).count() as u64
    );
    assert_eq!(
        ok.snapshots,
        log.recs
            .iter()
            .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
            .count() as u64
    );
    assert!(ok.inputs > 0 && ok.outputs > 0 && ok.snapshots >= 8);
    // Count Outputs independently of the verifier: only each process's initial boot block is
    // restored, whereas the ordinary roll's boot Outputs are regenerated and compared.
    let mut restoring = true;
    let mut outputs = 0;
    for r in &log.recs {
        if let Some(b) = r.read::<SegmentRoll>()
            && b.mark == hft_journal::RollMark::Begin as u8
        {
            restoring = matches!(
                hft_journal::RollReason::from_u8(b.reason),
                Some(hft_journal::RollReason::Startup | hft_journal::RollReason::Recovery)
            );
        }
        if !restoring && r.role() == Role::Output {
            outputs += 1;
        }
        if restoring && r.kind == EventKind::STATE_SNAPSHOT {
            restoring = false;
        }
    }
    assert_eq!(ok.outputs, outputs);

    let result_ix = log
        .recs
        .iter()
        .position(|r| r.kind == EventKind::CMD_RESULT)
        .unwrap();
    let target = log.recs[result_ix].seq;
    for flags in [false, true] {
        let mut bad = log.clone();
        if flags {
            bad.recs[result_ix].flags.0 ^= RecordFlags::SHADOW.0;
        } else {
            bad.recs[result_ix].body[0] ^= 1;
        }
        latest_only(bad.clone());
        let err = verify_all::<TestStrategies>(bad, &HB).unwrap_err();
        assert!(err.contains(&format!("divergence at seq {target}")), "{err}");
        for field in ["detail Some(", "flags Some(", "input Some(", "context ["] {
            assert!(err.contains(field), "{err}");
        }
        assert!(err.len() < 4_096, "bounded first-divergence context");
    }
    let keyframe = log
        .recs
        .iter()
        .position(|r| r.kind == EventKind::STATE_SNAPSHOT && r.body.first() == Some(&2))
        .unwrap();
    let mut bad = log.clone();
    // Header business field, not snap_tsc or the excluded calibration section.
    let offset = std::mem::offset_of!(hft_event::SnapshotHdr, engine_ns);
    bad.recs[keyframe].body[offset] ^= 1;
    latest_only(bad.clone());
    let err = verify_all::<TestStrategies>(bad, &HB).unwrap_err();
    assert!(
        err.contains(&format!("divergence at seq {}", log.recs[keyframe].seq)),
        "{err}"
    );

    // An unreached Output after the last Input is still a divergence at EOF.
    let mut bad = log.clone();
    let mut extra = bad.recs[result_ix].clone();
    extra.seq = bad.recs.last().unwrap().seq + 1;
    let extra_seq = extra.seq;
    bad.recs.push(extra);
    let err = verify_all::<TestStrategies>(bad, &HB).unwrap_err();
    assert!(err.contains(&format!("divergence at seq {extra_seq}")), "{err}");

    // No accepting a newer complete suffix as evidence of a full run.
    let mut suffix = log.clone();
    let first = suffix.seg_start[1];
    suffix.recs.drain(..first);
    suffix.seg_start = suffix.seg_start[1..].iter().map(|x| x - first).collect();
    suffix.segs.remove(0);
    assert!(
        verify_all::<TestStrategies>(suffix, &HB)
            .unwrap_err()
            .contains("initial Startup")
    );
    assert!(
        verify_all::<TestStrategies>(Log::default(), &HB)
            .unwrap_err()
            .contains("empty")
    );
    let mut bad = log.clone();
    bad.seg_start[1] += 1;
    assert!(
        verify_all::<TestStrategies>(bad, &HB)
            .unwrap_err()
            .contains("RunBoundary")
    );
    let again = std::thread::spawn(move || verify_all::<TestStrategies>(log, &HB).unwrap())
        .join()
        .unwrap();
    assert_eq!(
        ok, again,
        "replay must not read thread-local state from live execution"
    );
}

struct Guard(E, u64);
impl Sut for Guard {
    type Matcher = <E as Sut>::Matcher;
    fn boot(c: &BootCtx<'_>) -> Result<Self, BootFail> {
        E::boot(c).map(|e| Self(e, c.seed))
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
        self.0.rebuilt_hashes()
    }
    fn rebuilt_snapshot(&self, p: &SnapshotPoint) -> Result<ComponentHashes, String> {
        self.0.rebuilt_snapshot(p)
    }
    fn verify_journal(&self) -> Result<Option<JournalVerification>, String> {
        match self.1 {
            1 => Ok(None),
            2 => Err("injected full journal verifier failure".into()),
            3 => {
                let mut log = Log::from_segments(&self.0.sequencer().driver().io.j.segments);
                let r = log
                    .recs
                    .iter_mut()
                    .find(|r| r.kind == EventKind::CMD_RESULT)
                    .unwrap();
                r.body[0] ^= 1;
                verify_all::<TestStrategies>(log, &HB).map(Some)
            }
            _ => self.0.verify_journal(),
        }
    }
    fn verify_ledger_event(&self, e: &hft_testkit::dst::sut::EventStep) -> Result<(), String> {
        self.0.verify_ledger_event(e)
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
    fn committed_receipts(&self, f: &[hft_event::ExternalFill]) -> Result<Vec<bool>, String> {
        self.0.committed_receipts(f)
    }
    fn reconcile_inputs(
        &self,
        a: &mut hft_testkit::dst::actors::ReconActor,
        t: &[hft_testkit::dst::obs::TruthRow],
        h: &[hft_testkit::dst::venue_behavior::ExternalReceipt],
    ) -> Result<Option<Vec<InputRec>>, String> {
        self.0.reconcile_inputs(a, t, h)
    }
}

#[test]
fn tk_149_end_guard_requires_full_replay_and_leaves_the_run_unchanged() {
    let mut c = crate::support::cfg(0, 64, &[], 0, &[]);
    c.rebuild_every = 0;
    c.hash_every = 1;
    let on = run_record::<Guard>(&c);
    assert!(on.outcome.is_pass(), "{}", crate::seq_sut::show(&on));
    let evidence = on.journal_verification.unwrap();
    assert!(evidence.outputs > 0 && evidence.inputs > 0);
    assert_eq!(evidence.records, on.journal.len() as u64);
    assert_eq!(evidence.through_seq, on.journal.last().unwrap().seq);
    c.invariants = InvariantSet::none();
    let off = run_record::<Guard>(&c);
    assert!(off.outcome.is_pass(), "{}", crate::seq_sut::show(&off));
    assert_eq!(off.journal_verification, None);
    assert_eq!(on.journal, off.journal);
    assert_eq!(on.frames, off.frames);
    assert_eq!(on.deliveries, off.deliveries);
    assert_eq!(on.trail.to_bytes(), off.trail.to_bytes());
    assert_eq!(on.trail.points.len(), c.steps as usize);
    for (seed, text) in [
        (1, "unavailable"),
        (2, "injected full journal"),
        (3, "full journal divergence"),
    ] {
        c.seed = seed;
        c.invariants = InvariantSet::only(InvId::I6);
        let r = run_record::<Guard>(&c);
        assert_eq!(r.journal_verification, None);
        assert_eq!(r.steps_run, c.steps);
        match r.outcome {
            DstOutcome::Fail(f) => {
                assert_eq!(f.invariant, FailKind::Inv(InvId::I6));
                assert_eq!(f.step, c.steps - 1);
                assert!(f.message.contains(text), "{}", f.message);
            }
            _ => panic!("accepted invalid full verification: {seed}"),
        }
    }
}
