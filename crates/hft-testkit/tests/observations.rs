//! TK-107: real per-Input observations, with counterexamples applied only to diagnostic copies.
use std::cell::RefCell;

use hft_event::{EventKind, OrderAction, OrderDecision, RecordFlags};
use hft_testkit::dst::invariants::{InvId, InvState, InvariantSet, StepView, check_step};
use hft_testkit::dst::obs::{DecClass, Obs};
use hft_testkit::dst::sut::{BootCtx, BootFail, CrashPoint, EventStep, InputRec, JRec, StepOut, Sut};
use hft_testkit::dst::{SeqSut, run_record};
use hft_testkit::hash::ComponentHashes;
use hft_testkit::strategies::TestStrategies;
use hft_types::{AccountId, Side};
use zerocopy::IntoBytes;

type E = SeqSut<TestStrategies>;
thread_local! {
    static SAMPLES: RefCell<Vec<EventStep>> = const { RefCell::new(Vec::new()) };
    static TIMERS: RefCell<u64> = const { RefCell::new(0) };
}
struct Observed(E);

fn capture(events: &[EventStep]) {
    for e in events {
        if e.records.first().is_some_and(|r| r.kind == EventKind::TIMER) {
            TIMERS.with(|n| *n.borrow_mut() += 1);
        }
        if !e.info.decisions.is_empty() || !e.info.force_targets.is_empty() {
            SAMPLES.with(|s| {
                let mut s = s.borrow_mut();
                // Only this test's selected counterexample copies are sampled; the run checks all Inputs.
                if s.len() < 128 {
                    s.push(e.clone());
                }
            });
        }
    }
}

impl Sut for Observed {
    type Matcher = <E as Sut>::Matcher;
    fn boot(c: &BootCtx<'_>) -> Result<Self, BootFail> {
        SAMPLES.with(|s| s.borrow_mut().clear());
        TIMERS.with(|n| *n.borrow_mut() = 0);
        E::boot(c).map(Self)
    }
    fn process(&mut self, n: u64, t: u64, i: &InputRec, o: &mut StepOut) {
        self.0.process(n, t, i, o);
        capture(&o.events);
    }
    fn fire_timers(&mut self, n: u64, t: u64, o: &mut StepOut) -> bool {
        let fired = self.0.fire_timers(n, t, o);
        capture(&o.events);
        fired
    }
    fn drain_observations(&mut self, o: &mut StepOut) {
        let n = o.events.len();
        self.0.drain_observations(o);
        capture(&o.events[n..]);
    }
    fn next_deadline(&self) -> Option<u64> {
        self.0.next_deadline()
    }
    fn observe(&self, o: &mut Obs) {
        self.0.observe(o);
    }
    fn hashes(&self) -> ComponentHashes {
        self.0.hashes()
    }
    fn rebuilt_hashes(&self) -> Option<ComponentHashes> {
        self.0.rebuilt_hashes()
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

fn check(id: InvId, e: &EventStep) -> Result<(), (InvId, String)> {
    check_step(
        InvariantSet::only(id),
        &mut InvState::default(),
        &StepView {
            step: 0,
            now_ns: e.after.engine_ns.unwrap_or(0) as u64,
            records: &e.records,
            delivered: None,
            frames: &e.frames,
            info: &e.info,
            before: &e.before,
            after: &e.after,
            truth: &[],
            rebuild: None,
        },
    )
}

#[test]
fn tk_107_real_observations_and_counterexamples() {
    let c = crate::support::cfg(1, 2_000, &[], 0, &[]);
    let r = run_record::<Observed>(&c);
    assert!(r.outcome.is_pass(), "{:?}", r.outcome);
    assert!(
        TIMERS.with(|n| *n.borrow() > 0),
        "internal Timer was not observed"
    );
    let samples = SAMPLES.with(|s| s.take());
    let e = samples
        .iter()
        .find(|e| !e.info.decisions.is_empty())
        .expect("no decision observation");
    assert!(check(InvId::I11, e).is_ok());
    let mut bad = e.clone();
    bad.after.kill[0].global = !bad.after.kill[0].global;
    assert!(check(InvId::I11, &bad).is_err(), "wrong Kill accepted");
    let mut bad = e.clone();
    let cell = &mut bad.after.cells[0];
    cell.effective = if cell.effective == hft_types::RunMode::Live {
        hft_types::RunMode::Shadow
    } else {
        hft_types::RunMode::Live
    };
    assert!(check(InvId::I11, &bad).is_err(), "wrong effective mode accepted");
    let mut bad = e.clone();
    bad.after.cells.clear();
    assert!(check(InvId::I11, &bad).is_err(), "missing modes accepted");
    let mut bad = e.clone();
    bad.after.margin.clear();
    assert!(check(InvId::I17, &bad).is_err(), "missing margin accepted");
    let mut bad = e.clone();
    bad.after.exec.clear();
    assert!(check(InvId::I18, &bad).is_err(), "missing exec accepted");
    let mut bad = e.clone();
    bad.after.inflight.clear();
    assert!(check(InvId::I19, &bad).is_err(), "missing in-flight accepted");
    let mut bad = e.clone();
    bad.info.decisions.clear();
    assert!(check(InvId::I15, &bad).is_err(), "missing decision accepted");
    let mut bad = e.clone();
    bad.info.decision_inflight.clear();
    assert!(
        check(InvId::I19, &bad).is_err(),
        "missing decision-time in-flight accepted"
    );
    let mut bad = e.clone();
    bad.info.decision_limits.clear();
    assert!(
        check(InvId::I15, &bad).is_err(),
        "missing decision-time limits accepted"
    );
    let mut bad = e.clone();
    bad.info.decision_perp_pending.clear();
    assert!(check(InvId::I17, &bad).is_err(), "missing perp target accepted");
    let mut bad = e.clone();
    bad.after.observation_error = Some("diagnostic overflow".into());
    assert!(check(InvId::I11, &bad).is_err(), "overflow accepted");
    let mut out = StepOut {
        records: e.records.clone(),
        events: vec![e.clone()],
        ..StepOut::default()
    };
    assert!(out.verify_observations().is_ok());
    out.events.clear();
    assert!(out.verify_observations().is_err());
    out.events = vec![e.clone(), e.clone()];
    assert!(out.verify_observations().is_err());

    let (e, seq) = samples
        .iter()
        .find_map(|e| {
            e.info
                .decisions
                .iter()
                .find(|i| i.class == DecClass::Hedge)
                .filter(|i| {
                    e.records.iter().any(|r| {
                        r.seq == i.seq
                            && r.flags.0 & RecordFlags::SHADOW.0 == 0
                            && r.try_read::<OrderDecision>()
                                .is_some_and(|d| d.action == OrderAction::New && d.reject == 0)
                    })
                })
                .map(|i| (e, i.seq))
        })
        .expect("no admitted hedge to test");
    assert!(check(InvId::I15, e).is_ok());
    let mut bad = e.clone();
    let info = *bad.info.decisions.iter().find(|i| i.seq == seq).unwrap();
    let rec = bad.records.iter_mut().find(|r| r.seq == seq).unwrap();
    let mut d = rec.try_read::<OrderDecision>().unwrap();
    let lot = bad.after.inst(d.inst).unwrap().lot;
    d.qty.0 = (info.basis + 2 * lot) as i64;
    rec.body = d.as_bytes().to_vec();
    assert!(check(InvId::I15, &bad).is_err(), "oversized hedge accepted");
    let mut bad = e.clone();
    let i = bad.info.decisions.iter_mut().find(|i| i.seq == seq).unwrap();
    i.class = DecClass::Flatten;
    i.margin_stress = true;
    i.pos = d.qty.0 as i128;
    let rec = bad.records.iter_mut().find(|r| r.seq == seq).unwrap();
    let mut d = rec.try_read::<OrderDecision>().unwrap();
    d.side = Side::Buy;
    rec.body = d.as_bytes().to_vec();
    assert!(
        check(InvId::I17, &bad).is_err(),
        "wrong reduction direction accepted"
    );
}
