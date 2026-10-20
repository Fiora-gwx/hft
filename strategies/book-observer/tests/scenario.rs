//! Scenario tests of `book-observer` on the SDK mock views (TESTS.md). The binary counts
//! allocations, so every `StrategyHarness::feed` also checks that the fast part does not allocate.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code

mod common;

use book_observer::{BookObserver, BookObserverParams};
use common::{A1, CAPACITY, G1, I1, I2, fixture, g1, quote};
use hft_strategy_sdk::params::{RawParams, SchemaView};
use hft_strategy_sdk::testing::ev;
use hft_strategy_sdk::{
    EvMask, GroupTable, ParamError, ParamSet, Part, Strategy, SubsBuilder, SubsLimits, TradeView, VenueSet,
};
use hft_testkit::sdk_mock::StrategyHarness;
use hft_types::{Px, Qty, Side};

#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;

/// Defaults, with `trades` overridden when given.
fn params(n_groups: u16, trades: Option<bool>) -> BookObserverParams {
    let schema = BookObserver::DESC.schema;
    let mut defaults: Vec<i64> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i).map(|f| f.default))
        .collect();
    if let Some(t) = trades {
        defaults[0] = i64::from(t);
    }
    BookObserverParams::from_raw(
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
        n_groups,
    )
}

fn harness() -> StrategyHarness<BookObserver> {
    let mut h = StrategyHarness::<BookObserver>::new(&params(1, None), &g1(), fixture(), A1).expect("params");
    h.views_mut().at_engine_ns(1_000_000_000);
    h
}

fn subs(trades: bool) -> Vec<(hft_types::InstId, u16, EvMask, Part)> {
    let h =
        StrategyHarness::<BookObserver>::new(&params(1, Some(trades)), &g1(), fixture(), A1).expect("params");
    let rd = common::refdata();
    let room = vec![8u16; 16];
    let lim = SubsLimits {
        rd: &rd,
        feeds: VenueSet::EMPTY.with(common::V1).with(common::V2),
        room: &room,
    };
    let mut sb = SubsBuilder::new(lim, 16);
    h.strategy().subscriptions(&g1(), &mut sb);
    sb.entries()
        .iter()
        .map(|e| (e.inst, e.gix, e.ev, e.part))
        .collect()
}

#[test]
fn obs_001_subscribes_books_and_trades_of_every_leg() {
    let both = EvMask(EvMask::BOOK.0 | EvMask::TRADE.0);
    assert_eq!(
        subs(true),
        vec![(I1, 0, both, Part::Fast), (I2, 0, both, Part::Fast)]
    );
    assert_eq!(
        subs(false),
        vec![
            (I1, 0, EvMask::BOOK, Part::Fast),
            (I2, 0, EvMask::BOOK, Part::Fast)
        ]
    );
}

#[test]
fn obs_002_counts_books_and_trades_without_intents() {
    let mut h = harness();
    for (k, (inst, mid)) in [(I1, 6_000_000), (I2, 6_000_100), (I1, 6_000_010)]
        .into_iter()
        .enumerate()
    {
        h.views_mut().at_engine_ns(1_000_000_000 + k as i64 * 1_000_000);
        quote(h.views_mut(), inst, mid);
        let out = h.feed(&ev::book(inst, 0).as_ref());
        assert!(out.items.is_empty(), "no intents: {:?}", out.items);
    }
    let t = TradeView {
        px: Px(6_000_010),
        qty: Qty(1_000),
        side: Side::Buy,
        ex_ts_ns: 1,
    };
    let out = h.feed(&ev::trade(I2, 0, t).as_ref());
    assert!(out.items.is_empty());
    let g = h.strategy().fast.groups[0];
    assert_eq!(g.id, G1);
    assert_eq!(g.books[..2], [2, 1]);
    assert_eq!(g.trades[..2], [0, 1]);
    assert_eq!((g.bid[0], g.ask[0]), (6_000_009, 6_000_011));
    assert_eq!((g.bid[1], g.ask[1]), (6_000_099, 6_000_101));
}

#[test]
fn obs_003_snapshot_roundtrip() {
    let mut h = harness();
    quote(h.views_mut(), I1, 6_000_000);
    h.feed(&ev::book(I1, 0).as_ref());
    h.snapshot_roundtrip().expect("snapshot → restore → snapshot");
}

#[test]
fn obs_004_group_shape() {
    let one_leg = GroupTable::new(&[common::spec(G1, &[(I1, common::V1)])], CAPACITY);
    let none = GroupTable::new(&[common::spec(G1, &[])], CAPACITY);
    let p = params(1, None);
    let rd = fixture();
    assert_eq!(p.validate(&one_leg, &rd.view()), Ok(()));
    assert_eq!(p.validate(&g1(), &rd.view()), Ok(()));
    assert_eq!(
        p.validate(&none, &rd.view()),
        Err(ParamError::GroupShape { gix: 0, code: 1 })
    );
}
