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
fn reg_002_default_kinds_m4() {
    let kinds: Vec<_> = Strategies::kinds()
        .iter()
        .map(|d| (d.name, d.kind_id, d.non_prod_only))
        .collect();
    let mut expected = vec![];
    if cfg!(feature = "cross-taker") {
        expected.push(("cross-taker", 3, false));
    }
    expected.push(("latency-probe", 5, true));
    expected.push(("portfolio-carry", 1, false));
    expected.push(("book-observer", 6, false));
    expected.push(("binary-maker", 7, false));
    assert_eq!(kinds, expected);
}

#[test]
fn reg_001_default_kinds() {
    let k = Strategies::kinds();
    let lp: Vec<_> = k.iter().filter(|d| d.name == "latency-probe").collect();
    assert_eq!(lp.len(), 1);
    assert_eq!(
        k.iter()
            .any(|d| d.name == "cross-taker" && d.kind_id == 3 && !d.non_prod_only),
        cfg!(feature = "cross-taker")
    );
    assert_eq!(
        (lp[0].kind_id, lp[0].non_prod_only, lp[0].schema.id),
        (5, true, 5)
    );
    for d in k {
        assert_eq!(d.schema.id, u32::from(d.kind_id), "{}", d.name);
        assert!(!TEST_KIND_IDS.contains(&d.kind_id), "{}", d.name);
    }
}
