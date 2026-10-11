//! Each snapshot, including several snapshots and restarts inside one world step, needs INV-6.

use hft_event::{EventKind, Timer};
use hft_testkit::dst::invariants::{InvId, InvariantSet};
use hft_testkit::dst::obs::Obs;
use hft_testkit::dst::sut::{
    BootCtx, BootFail, CrashPoint, InputRec, JRec, SnapshotPoint, Source, StepOut, Sut,
};
use hft_testkit::dst::{DstOutcome, FailKind, SeqSut, run_record};
use hft_testkit::hash::ComponentHashes;
use hft_testkit::strategies::TestStrategies;
use hft_types::{AccountId, GroupId};
use zerocopy::FromZeros;

type E = SeqSut<TestStrategies>;

struct Snapshots(E, u64, bool);

impl Sut for Snapshots {
    type Matcher = <E as Sut>::Matcher;
    fn boot(c: &BootCtx<'_>) -> Result<Self, BootFail> {
        E::boot(c).map(|e| Self(e, c.seed, false))
    }
    fn process(&mut self, n: u64, t: u64, i: &InputRec, o: &mut StepOut) {
        if self.2 {
            self.0.process(n, t, i, o);
            return;
        }
        self.2 = true;
        // Separate real Inputs in one world step. Each needs its own live-state boundary.
        let mut records = Vec::new();
        for timer_id in [1, 7, 2] {
            let input = InputRec::of(
                &Timer {
                    owner: 1,
                    timer_id,
                    group: GroupId(0),
                    ..Timer::new_zeroed()
                },
                Source::Engine,
            );
            self.0.process(n, t, &input, o);
            records.append(&mut o.records);
        }
        self.0.clean_restart().unwrap();
        for _ in 0..2 {
            self.0.crash_and_recover(CrashPoint::SeqInput, None).unwrap();
        }
        self.0.process(n, t, i, o);
        records.append(&mut o.records);
        o.records = records;
    }
    fn drain_observations(&mut self, o: &mut StepOut) {
        self.0.drain_observations(o);
        if !self.2 || o.snapshots.len() < 2 {
            return;
        }
        match self.1 {
            1 => {
                o.snapshots.remove(0);
            }
            2 => {
                o.snapshots.insert(1, o.snapshots[0]);
            }
            3 => {
                o.snapshots[0].through_seq = o.snapshots[0].snapshot_seq - 1;
            }
            4 => {
                o.snapshots[0].live.risk ^= 1;
            }
            5 => {
                o.snapshot_error = Some("snapshot observation buffer overflow".into());
            }
            7 => {
                o.snapshots.clear();
            }
            8 => {
                o.snapshots[0].through_seq = o.snapshots[1].through_seq;
            }
            _ => {}
        }
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
        if self.1 == 6 && self.2 {
            Err(format!("snapshot {} injected rebuild failure", p.snapshot_seq))
        } else {
            self.0.rebuilt_snapshot(p)
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
fn tk_115_every_snapshot_checked_with_periodic_rebuild_off() {
    let mut c = crate::support::cfg(0, 4, &[], 0, &[]);
    c.invariants = InvariantSet::only(InvId::I6);
    c.rebuild_every = 0;
    c.hash_every = 1;
    let r = run_record::<Snapshots>(&c);
    assert!(r.outcome.is_pass(), "{}", crate::seq_sut::show(&r));
    let snaps: Vec<_> = r
        .journal
        .iter()
        .filter(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .collect();
    assert!(
        snaps.len() >= 8,
        "must cover boot, keyframe, demand, roll, Drain and three recoveries"
    );
    assert_eq!(r.snapshot_checks, snaps.len() as u64);
    for purpose in [1, 2, 3, 4, 5] {
        assert!(
            snaps.iter().any(|s| s.body.first() == Some(&purpose)),
            "missing purpose {purpose}"
        );
    }
    // Observation and verification leave both journal decisions and hash sampling unchanged.
    c.invariants = InvariantSet::none();
    let off = run_record::<Snapshots>(&c);
    assert!(off.outcome.is_pass());
    assert_eq!(r.journal, off.journal);
    assert_eq!(r.trail.points.len(), c.steps as usize);
    assert_eq!(r.trail.to_bytes(), off.trail.to_bytes());
    assert_eq!(off.snapshot_checks, 0);

    // The same case also requires every corrupt observation to fail at its own boundary.
    for (seed, message) in [
        (1, "missing or reordered"),
        (2, "missing or reordered"),
        (3, "invalid boundary"),
        (4, "rebuilt state differs in risk"),
        (5, "buffer overflow"),
        (6, "injected rebuild failure"),
        (7, "missing snapshot observation"),
        (8, "crosses a later Input"),
    ] {
        let mut c = crate::support::cfg(seed, 4, &[], 0, &[]);
        c.invariants = InvariantSet::only(InvId::I6);
        c.rebuild_every = 0;
        match run_record::<Snapshots>(&c).outcome {
            DstOutcome::Fail(f) => {
                assert_eq!(f.invariant, FailKind::Inv(InvId::I6), "{}", f.message);
                assert_eq!(f.step, 0, "must fail at the snapshot step, before the end check");
                assert!(f.message.contains(message), "seed {seed}: {}", f.message);
            }
            DstOutcome::Pass { .. } => panic!("accepted corrupt snapshot observation {seed}"),
        }
    }
}
