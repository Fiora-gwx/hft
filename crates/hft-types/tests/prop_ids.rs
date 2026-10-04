//! Property test for `GroupId::compose` (TYP-084).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_types::*;
use proptest::prelude::*;

fn pair() -> impl Strategy<Value = (u8, u32)> {
    prop_oneof![
        (1u8..=254, 1u32..=GroupId::N_MAX),
        (
            prop::sample::select(vec![1u8, 254]),
            prop::sample::select(vec![1u32, GroupId::N_MAX])
        ),
    ]
}

const C: Option<GroupId> = GroupId::compose(StrategyId(254), GroupId::N_MAX);

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn typ_084_compose_unique_and_never_sentinel((s, n) in pair(), (s2, n2) in pair()) {
        let g = GroupId::compose(StrategyId(s), n).unwrap();
        prop_assert_eq!(g.0, ((s as u32) << 24) | n);
        prop_assert!((0x0100_0001..=0xFEFF_FFFE).contains(&g.0));
        prop_assert!(g != GROUP_NONE && g != GROUP_ALL);
        prop_assert_eq!(g.strategy(), StrategyId(s));
        prop_assert_eq!(g.counter(), n);
        let g2 = GroupId::compose(StrategyId(s2), n2).unwrap();
        prop_assert_eq!(g == g2, (s, n) == (s2, n2));
        prop_assert_eq!(C, Some(GroupId(0xFEFF_FFFE)));
    }
}
