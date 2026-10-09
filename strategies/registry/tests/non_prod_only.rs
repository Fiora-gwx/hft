//! `non_prod_only` of the registered kinds (TESTS.md REG-012).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use hft_strategy_registry::Strategies;
use hft_strategy_sdk::StrategySet;

#[test]
fn reg_012_flags() {
    let np: Vec<u16> = Strategies::kinds()
        .iter()
        .filter(|d| d.non_prod_only)
        .map(|d| d.kind_id)
        .collect();
    assert_eq!(np, vec![5]);
}
