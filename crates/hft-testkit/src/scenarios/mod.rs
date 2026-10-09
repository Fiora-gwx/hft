//! Scenario suites run with the test strategy set (README "6. DST 框架" 场景套件). Every `dst` and
//! `integ` case whose `实现` points here registers with `Scenario::new("<id>", …)` in a
//! `src/scenarios/<topic>.rs` function; [`suites`] and [`scenarios`] are the single list that
//! `tests/dst/` (one `#[test]` per suite), `cargo xtask dst` and `hft-dst` iterate.

pub mod framework;

use crate::dst::DstConfig;
use crate::dst::faults::{FaultCategory, FaultRates};
use crate::dst::scenario::{DstSuite, Scenario};

fn rates(list: &[(FaultCategory, u32)]) -> FaultRates {
    let mut r = FaultRates::off();
    for (c, pm) in list {
        r.set(*c, true, *pm);
    }
    r
}

fn base(seed: u64) -> DstConfig {
    DstConfig::new(seed, "core")
}

/// Every suite, sorted by name.
pub fn suites() -> Vec<DstSuite> {
    use FaultCategory::*;
    let mut v = vec![
        DstSuite {
            name: "core",
            title: "two venues, two legs, every non-storage fault category",
            faults: rates(&[
                (Conn, 30),
                (Report, 100),
                (Reject, 60),
                (TwoLeg, 30),
                (Market, 30),
                (Account, 20),
                (UserStream, 10),
                (Control, 10),
                (ModelSim, 20),
            ]),
            steps: crate::seed::DST_DEFAULT_STEPS,
            base,
            must_pass: &["QF-1", "MS-2", "MS-3"],
        },
        DstSuite {
            name: "modes",
            title: "control commands, kill, modes and drains under light venue faults",
            faults: rates(&[(Control, 200), (Conn, 20), (Report, 50)]),
            steps: crate::seed::DST_DEFAULT_STEPS,
            base,
            must_pass: &[],
        },
        DstSuite {
            name: "storage",
            title: "crashes, clean restarts, persist lag and disk pressure",
            faults: rates(&[(Storage, 30), (Report, 50), (Reject, 20)]),
            steps: crate::seed::DST_DEFAULT_STEPS,
            base,
            must_pass: &[],
        },
    ];
    v.sort_by_key(|s| s.name);
    v
}

/// Suite names, sorted.
pub fn suite_names() -> Vec<&'static str> {
    suites().iter().map(|s| s.name).collect()
}

/// Every registered scenario (framework cases and the invariants' standing cases).
pub fn scenarios() -> Vec<Scenario> {
    use crate::dst::invariants as i;
    let mut v = framework::all();
    v.extend([
        i::inv_1_position_change_has_fill_evidence(),
        i::inv_2_every_report_matched_or_alarmed(),
        i::inv_3_unhedged_within_risk_limits(),
        i::inv_4_unknown_group_no_open_no_reverse(),
        i::inv_5_kill_blocks_non_risk_reducing(),
        i::inv_6_state_is_function_of_journal(),
        i::inv_7_slots_do_not_leak(),
        i::inv_8_second_leg_equals_net_fill(),
        i::inv_9_same_seed_same_trail(),
        i::inv_11_effective_mode_recomputes(),
        i::inv_12_risk_reducing_only_hard_blocks(),
        i::inv_13_mode_changes_emit_no_open_or_hedge(),
        i::inv_14_gate_rejects_not_counted(),
        i::inv_15_hedge_qty_and_notional_capped(),
        i::inv_16_ledger_matches_sim_truth(),
        i::inv_17_l3_reduces_toward_zero_perp_first(),
        i::inv_18_drain_leaves_no_naked_leg(),
        i::inv_19_no_hedge_beyond_naked(),
        i::inv_20_broken_auto_fix_bounded(),
    ]);
    v
}

/// Scenarios that run in suite `name` (named there, or every-suite cases).
pub fn scenarios_of(name: &str) -> Vec<Scenario> {
    scenarios()
        .into_iter()
        .filter(|s| match s.suite {
            crate::dst::SuiteSel::All => true,
            crate::dst::SuiteSel::Named(n) => n == name,
        })
        .collect()
}
