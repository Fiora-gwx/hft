//! TK-065: every fault category has its own switch.

use hft_testkit::dst::faults::{FaultCategory, FaultPlan, FaultRates};
use hft_testkit::dst::world::WorldSpec;

#[test]
fn tk_065_category_switches() {
    let w = WorldSpec::w1();
    for (k, c) in FaultCategory::ALL.iter().enumerate() {
        let row = format!("TK-065.{}", k + 1);
        let on = FaultPlan::generate(17, &FaultRates::only(*c, 1_000), &w, 60_000_000_000, 5_000);
        assert!(on.count(*c) >= 1, "{row}: {} on → ≥ 1 item", c.name());
        assert_eq!(on.count(*c), on.len(), "{row}: only {} items", c.name());
        let mut off = FaultRates::only(*c, 1_000);
        off.disable(*c);
        let p = FaultPlan::generate(17, &off, &w, 60_000_000_000, 5_000);
        assert_eq!(p.len(), 0, "{row}: off → 0 items");
    }
    // The category-specific examples of the table.
    use hft_testkit::dst::faults::Fault::*;
    let plan = |c| FaultPlan::generate(17, &FaultRates::only(c, 1_000), &w, 600_000_000_000, 5_000);
    let has =
        |p: &FaultPlan, f: fn(&hft_testkit::dst::faults::Fault) -> bool| p.items.iter().any(|(_, x)| f(x));
    assert!(
        has(&plan(FaultCategory::Conn), |f| matches!(f, ConnDown { .. })),
        "TK-065.1"
    );
    assert!(
        has(&plan(FaultCategory::Report), |f| matches!(
            f,
            ReportReorder { .. } | ReportDup { .. }
        )),
        "TK-065.2"
    );
    assert!(
        has(&plan(FaultCategory::Reject), |f| matches!(
            f,
            RateLimit { .. } | Timeout { .. }
        )),
        "TK-065.3"
    );
    assert!(
        has(&plan(FaultCategory::TwoLeg), |f| matches!(
            f,
            SecondLegSilent { .. } | QuietBook { .. } | PersistentSecondReject { .. }
        )),
        "TK-065.4"
    );
    assert!(
        has(&plan(FaultCategory::Market), |f| matches!(
            f,
            ExTsDisorder { .. } | FundingPeriodChange { .. }
        )),
        "TK-065.5"
    );
    assert!(
        has(&plan(FaultCategory::UserStream), |f| matches!(
            f,
            UnknownEvent { .. }
        )),
        "TK-065.7"
    );
    assert!(
        has(&plan(FaultCategory::Control), |f| matches!(
            f,
            CmdReorder | CmdExpired
        )),
        "TK-065.8"
    );
    assert!(
        has(&plan(FaultCategory::ModelSim), |f| matches!(
            f,
            AdvisorLag(_) | SimReplyFlood { .. }
        )),
        "TK-065.9"
    );
    assert!(
        has(&plan(FaultCategory::Storage), |f| matches!(
            f,
            PersistLag { .. } | Crash(_)
        )),
        "TK-065.10"
    );
}
