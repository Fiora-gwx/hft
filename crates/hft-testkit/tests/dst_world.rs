//! TK-044–TK-048, TK-068, TK-070: the DST world, outcomes and checkers.

use hft_testkit::dst::driver::DstDriver;
use hft_testkit::dst::faults::{FaultCategory, FaultPlan, FaultRates};
use hft_testkit::dst::invariants::{InvId, InvariantSet};
use hft_testkit::dst::sut::Source;
use hft_testkit::dst::world::{World, WorldSpec};
use hft_testkit::dst::{ActorSet, DstOutcome, FailKind, ToyEngine, run, run_record};
use proptest::prelude::*;

use crate::support::{cfg, first_failure};

#[test]
fn tk_044_fail_outcome_fields() {
    let make = |s| cfg(s, 3_000, &[FaultCategory::Reject], 1_000, &["oms-429-fails"]);
    let (c, f) = first_failure(50, make).expect("planted oms-429-fails is found");
    assert_eq!(f.seed, c.seed);
    assert_eq!(f.invariant, FailKind::Inv(InvId::I1), "{}", f.message);
    let last = f.trace.last().expect("trace");
    assert_eq!(last.step, f.step, "trace ends at the violating step");
    assert_eq!(
        f.fault_schedule,
        c.fault_plan(),
        "fault schedule = the seed's plan"
    );
    assert!(f.trace.len() <= 200 && !f.journal_tail.is_empty());
}

fn rates() -> impl Strategy<Value = FaultRates> {
    proptest::collection::vec((any::<bool>(), 0u32..=1_000), 10).prop_map(|v| {
        let mut r = FaultRates::off();
        for (c, (on, pm)) in FaultCategory::ALL.iter().zip(v) {
            r.set(*c, on, pm);
        }
        r
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 500, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn tk_045_fault_plan_upfront(seed in any::<u64>(), r in rates()) {
        let w = WorldSpec::w1();
        let a = FaultPlan::generate(seed, &r, &w, 60_000_000_000, 1_000);
        let b = FaultPlan::generate(seed, &r, &w, 60_000_000_000, 1_000);
        prop_assert_eq!(&a, &b);
        prop_assert!(a.items.windows(2).all(|p| p[0].0 <= p[1].0));
        for c in FaultCategory::ALL {
            if !r.get(c).on {
                prop_assert_eq!(a.count(c), 0);
            }
        }
    }
}

#[test]
fn fault_plan_not_extended_while_running() {
    for s in 1..=5 {
        let c = cfg(s, 300, &FaultCategory::ALL, 300, &[]);
        let rec = run_record::<ToyEngine>(&c);
        assert_eq!(rec.plan, c.fault_plan(), "seed {s}: no fault added while running");
    }
}

#[test]
fn tk_046_heap_total_order() {
    let mut w: World<&str> = World::new(1);
    w.push_at(100, 2, "a");
    w.push_at(100, 1, "b");
    w.push_at(50, 3, "c");
    let order: Vec<(u64, u64)> = std::iter::from_fn(|| w.pop().map(|s| (s.at, s.gen_seq))).collect();
    assert_eq!(order, vec![(50, 3), (100, 1), (100, 2)]);
}

#[test]
fn tk_047_actor_rng_substreams() {
    let market = |advisor: bool| {
        let mut c = cfg(7, 1_000, &[], 0, &[]);
        c.actors = ActorSet {
            advisor,
            ..ActorSet::ALL
        };
        run_record::<ToyEngine>(&c)
            .deliveries
            .into_iter()
            .filter(|(_, i)| i.src == Source::Market)
            .map(|(_, i)| i.body)
            .collect::<Vec<_>>()
    };
    let (a, b) = (market(true), market(false));
    let n = a.len().min(b.len());
    assert!(n > 100, "both runs saw market data ({n})");
    assert_eq!(
        a[..n],
        b[..n],
        "MarketActor output does not depend on AdvisorActor"
    );
}

#[test]
fn tk_048_driver_has_no_rng() {
    let mut d = DstDriver::new(1_000_000_000, 0);
    d.set_now(1_000_000);
    assert_eq!(d.stamp(), 1_000_000);
    let src = include_str!("../src/dst/driver.rs");
    for word in ["Rng", "rand_", "rand::", "ChaCha", "SeedableRng"] {
        assert!(!src.contains(word), "driver.rs mentions {word}");
    }
}

#[test]
fn tk_068_invariant_toggle() {
    let make = |s| cfg(s, 3_000, &[], 0, &["slot-leak"]);
    let (mut c, f) = first_failure(10, make).expect("planted slot-leak is found");
    assert_eq!(f.invariant, FailKind::Inv(InvId::I7), "{}", f.message);
    c.invariants = InvariantSet::all().without(InvId::I7);
    assert!(run::<ToyEngine>(&c).is_pass(), "I7 off → Pass");
}

#[test]
fn tk_070_checkers_read_only() {
    for s in [3, 11] {
        let mut a = cfg(
            s,
            2_000,
            &[FaultCategory::Report, FaultCategory::Reject],
            200,
            &[],
        );
        let mut b = a.clone();
        a.invariants = InvariantSet::all();
        b.invariants = InvariantSet::none();
        let (ra, rb) = (run_record::<ToyEngine>(&a), run_record::<ToyEngine>(&b));
        assert!(
            matches!(ra.outcome, DstOutcome::Pass { .. }),
            "{:?}",
            ra.outcome.failure().map(|f| &f.message)
        );
        assert_eq!(ra.outputs(), rb.outputs(), "seed {s}: intent streams equal");
        assert_eq!(ra.trail, rb.trail, "seed {s}: hash trails equal");
        #[cfg(feature = "seq")]
        {
            type E = hft_testkit::dst::SeqSut<hft_testkit::strategies::TestStrategies>;
            let (ra, rb) = (run_record::<E>(&a), run_record::<E>(&b));
            assert!(ra.outcome.is_pass(), "real engine seed {s}: {:?}", ra.outcome);
            assert_eq!(
                ra.outputs(),
                rb.outputs(),
                "real engine seed {s}: intent streams equal"
            );
            assert_eq!(ra.trail, rb.trail, "real engine seed {s}: hash trails equal");
        }
    }
}
