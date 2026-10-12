//! TK-049, TK-050: the repro line and the failure artifacts.

use hft_event::{RecordFlags, decode_record};
use hft_testkit::dst::faults::FaultCategory;
use hft_testkit::dst::obs::Obs;
use hft_testkit::dst::report::{repro_line, write_artifacts};
use hft_testkit::dst::sut::{BootCtx, BootFail, CrashPoint, InputRec, JRec, StepOut, Sut};
use hft_testkit::dst::{DstOutcome, FailKind, ToyEngine, run};
use hft_testkit::hash::ComponentHashes;
use hft_types::AccountId;

use crate::support::{cfg, first_failure};

#[test]
fn tk_049_single_repro_line() {
    let make = |s| cfg(s, 3_000, &[FaultCategory::Reject], 1_000, &["oms-429-fails"]);
    let (c, f) = first_failure(50, make).expect("failure");
    let line = repro_line(&f);
    let want = format!(
        "DST FAILED seed={:#018x} step={} invariant=I1(fill_evidence) -> cargo xtask dst --seeds 1 --seed {:#018x} --suite core --steps {} --trace",
        c.seed,
        f.step,
        c.seed,
        f.step + 1
    );
    assert_eq!(line, want);
    assert_eq!(line.lines().count(), 1);
    assert!(line.contains(&format!("seed=0x{:016x}", c.seed)), "16 hex digits");
    // Rerunning with the line's parameters reproduces the same line.
    let mut again = c.clone();
    again.steps = f.step + 1;
    match run::<ToyEngine>(&again) {
        DstOutcome::Fail(g) => assert_eq!(repro_line(&g), line),
        DstOutcome::Pass { .. } => panic!("repro did not fail"),
    }
}

/// The toy engine with a planted panic at step `N` (a "planted defect" test double).
struct PanicAt<const N: u64> {
    inner: ToyEngine,
    steps: u64,
}

impl<const N: u64> Sut for PanicAt<N> {
    type Matcher = <ToyEngine as Sut>::Matcher;
    fn boot(ctx: &BootCtx<'_>) -> Result<Self, BootFail> {
        Ok(PanicAt {
            inner: ToyEngine::boot(ctx)?,
            steps: 0,
        })
    }
    fn process(&mut self, now_ns: u64, tsc: u64, input: &InputRec, out: &mut StepOut) {
        self.inner.process(now_ns, tsc, input, out);
        self.steps += 1;
        assert!(self.steps <= N, "planted panic at step {N}");
    }
    fn next_deadline(&self) -> Option<u64> {
        self.inner.next_deadline()
    }
    fn fire_timers(&mut self, now_ns: u64, tsc: u64, out: &mut StepOut) -> bool {
        let fired = self.inner.fire_timers(now_ns, tsc, out);
        if fired {
            self.steps += 1;
            assert!(self.steps <= N, "planted panic at step {N}");
        }
        fired
    }
    fn observe(&self, o: &mut Obs) {
        self.inner.observe(o)
    }
    fn hashes(&self) -> ComponentHashes {
        self.inner.hashes()
    }
    fn reconcile_inputs(
        &self,
        actor: &mut hft_testkit::dst::actors::ReconActor,
        truth: &[hft_testkit::dst::obs::TruthRow],
        history: &[hft_testkit::dst::venue_behavior::ExternalReceipt],
    ) -> Result<Option<Vec<InputRec>>, String> {
        self.inner.reconcile_inputs(actor, truth, history)
    }
    fn rebuilt_hashes(&self) -> Result<Option<ComponentHashes>, String> {
        Ok(None)
    }
    fn seq(&self) -> u64 {
        self.inner.seq()
    }
    fn journal(&self) -> &[JRec] {
        self.inner.journal()
    }
    fn crash_and_recover(&mut self, p: CrashPoint, i: Option<&InputRec>) -> Result<(), BootFail> {
        self.inner.crash_and_recover(p, i)
    }
    fn clean_restart(&mut self) -> Result<(), BootFail> {
        self.inner.clean_restart()
    }
    fn is_sim(&self, a: AccountId) -> bool {
        self.inner.is_sim(a)
    }
}

fn events_in(bytes: &[u8]) -> usize {
    let mut off = 0;
    let mut n = 0;
    while off < bytes.len() {
        let r = decode_record(&bytes[off..]).expect("fragment record decodes");
        if RecordFlags(r.hdr.flags).contains(RecordFlags::EVENT_END) {
            n += 1;
        }
        off += r.hdr.len as usize;
    }
    n
}

fn artifacts<E: Sut>(want_step: u64, want_events: usize) {
    let c = cfg(1, 10_000, &[], 0, &[]);
    let f = match run::<E>(&c) {
        DstOutcome::Fail(f) => f,
        DstOutcome::Pass { .. } => panic!("planted panic not hit"),
    };
    assert_eq!(f.invariant, FailKind::Panic);
    assert_eq!(f.step, want_step);
    let dir = std::env::temp_dir().join(format!("hft-tk-050-{want_step}-{}", std::process::id()));
    let a = write_artifacts(&dir, &f).expect("artifacts written");
    let j = std::fs::read(&a.journal).expect("journal");
    assert_eq!(events_in(&j), want_events, "events in the fragment");
    let faults = std::fs::read_to_string(&a.faults).expect("faults");
    assert_eq!(faults.lines().count(), f.fault_schedule.len());
    assert!(
        std::fs::read_to_string(&a.repro)
            .expect("repro")
            .starts_with("DST FAILED")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tk_050_failure_artifacts() {
    // Step numbers in the case are 1-based; `DstFailure.step` is 0-based.
    // TK-050.1: violation at the 5 000th step → exactly 200 events (the violating one included).
    artifacts::<PanicAt<4_999>>(4_999, 200);
    // TK-050.2: violation at the 50th step → all 50 events.
    artifacts::<PanicAt<49>>(49, 50);
}
