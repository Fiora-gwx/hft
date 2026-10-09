//! The registered kinds (TESTS.md REG-001).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use hft_strategy_registry::{Strategies, TEST_KIND_IDS};
use hft_strategy_sdk::StrategySet;

#[test]
fn reg_001_default_kinds() {
    let k = Strategies::kinds();
    let lp: Vec<_> = k.iter().filter(|d| d.name == "latency-probe").collect();
    assert_eq!(lp.len(), 1);
    assert_eq!(
        (lp[0].kind_id, lp[0].non_prod_only, lp[0].schema.id),
        (5, true, 5)
    );
    for d in k {
        assert_eq!(d.schema.id, u32::from(d.kind_id), "{}", d.name);
        assert!(!TEST_KIND_IDS.contains(&d.kind_id), "{}", d.name);
    }
}
