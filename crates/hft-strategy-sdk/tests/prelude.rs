//! The prelude re-exports L0 itself (SDK-003).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::any::TypeId;

use hft_strategy_sdk::prelude as p;

macro_rules! same {
    ($($a:ty = $b:ty),* $(,)?) => {{
        let mut n = 0;
        $(
            assert_eq!(TypeId::of::<$a>(), TypeId::of::<$b>(), "{}", stringify!($a));
            n += 1;
        )*
        n
    }};
}

#[test]
fn sdk_003_prelude_is_l0() {
    let n = same!(
        p::Px = hft_types::Px,
        p::Qty = hft_types::Qty,
        p::Rate = hft_types::Rate,
        p::Notional = hft_types::Notional,
        p::Tsc = hft_types::Tsc,
        p::InstId = hft_types::InstId,
        p::VenueId = hft_types::VenueId,
        p::AccountId = hft_types::AccountId,
        p::GroupId = hft_types::GroupId,
        p::StrategyId = hft_types::StrategyId,
        p::Side = hft_types::Side,
        p::Tif = hft_types::Tif,
        p::GroupSpec = hft_event::GroupSpec,
        p::LegDef = hft_event::LegDef,
        p::Level = hft_event::Level,
        p::NoteLevel = hft_event::NoteLevel,
        p::RejectBits = hft_types::RejectBits,
        p::RunMode = hft_types::RunMode,
    );
    assert_eq!(n, 18);
    assert_eq!(p::GROUP_NONE, hft_types::GROUP_NONE);
    assert_eq!(p::GROUP_NONE, hft_types::GroupId(0));
    assert_eq!(p::INST_NONE, hft_types::INST_NONE);
    assert_eq!(p::INST_ALL, hft_types::INST_ALL);
    assert_eq!(p::MAX_BBO_CONN_SLOTS, hft_types::MAX_BBO_CONN_SLOTS);
}
