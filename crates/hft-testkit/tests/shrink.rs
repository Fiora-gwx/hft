//! TK-051, TK-052: shrinking a failing fault plan.

use hft_testkit::dst::faults::FaultCategory;
use hft_testkit::dst::invariants::InvId;
use hft_testkit::dst::world::VS;
use hft_testkit::dst::{DstConfig, DstOutcome, FailKind, ToyEngine, run, shrink};

use crate::support::cfg;

/// First failing seed (with a plan of ≥ 30 items) of a planted defect.
fn failing(
    planted: &'static str,
    cats: &[FaultCategory],
    tune: fn(&mut DstConfig),
) -> (DstConfig, Box<hft_testkit::dst::DstFailure>) {
    for s in 1..=200 {
        let mut c = cfg(s, 2_000, cats, 1_000, &[planted]);
        c.faults.slot_ms = 1_000;
        tune(&mut c);
        if c.fault_plan().len() < 30 {
            continue;
        }
        if let DstOutcome::Fail(f) = run::<ToyEngine>(&c) {
            return (c, f);
        }
    }
    panic!("{planted}: no failing seed with ≥ 30 faults");
}

fn none(_: &mut DstConfig) {}

fn base_fee(c: &mut DstConfig) {
    for v in &mut c.world.venues {
        if v.id == VS {
            v.fee_in_base = true;
        }
    }
}

type Row = (
    &'static str,
    &'static str,
    &'static [FaultCategory],
    fn(&mut DstConfig),
    InvId,
);

#[test]
fn tk_051_shrink_to_five() {
    let rows: [Row; 3] = [
        (
            "TK-051.1",
            "oms-429-fails",
            &[FaultCategory::Reject, FaultCategory::Report],
            none,
            InvId::I1,
        ),
        (
            "TK-051.2",
            "hedge-gross-qty",
            &[FaultCategory::Report, FaultCategory::Market],
            base_fee,
            InvId::I8,
        ),
        (
            "TK-051.3",
            "double-hedge-external",
            &[FaultCategory::Account, FaultCategory::Report],
            none,
            InvId::I19,
        ),
    ];
    for (row, planted, cats, tune, inv) in rows {
        let (c, f) = failing(planted, cats, tune);
        assert_eq!(f.invariant, FailKind::Inv(inv), "{row}: {}", f.message);
        let s = shrink::<ToyEngine>(&c, &f, 120);
        assert!(
            s.fault_schedule.len() <= 5,
            "{row}: shrunk to {} items",
            s.fault_schedule.len()
        );
        let mut again = c.clone();
        again.plan = Some(s.fault_schedule.clone());
        match run::<ToyEngine>(&again) {
            DstOutcome::Fail(g) => assert_eq!(g.invariant, FailKind::Inv(inv), "{row}"),
            DstOutcome::Pass { .. } => panic!("{row}: shrunk plan passes"),
        }
    }
}

#[test]
fn tk_052_zero_budget_identity() {
    let (c, f) = failing(
        "oms-429-fails",
        &[FaultCategory::Reject, FaultCategory::Report],
        none,
    );
    let s = shrink::<ToyEngine>(&c, &f, 0);
    assert_eq!(s.fault_schedule, f.fault_schedule);
    assert_eq!(s, *f);
}
