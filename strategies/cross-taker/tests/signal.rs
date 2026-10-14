//! Core signal/sizing scenarios from README and TESTS.md.
use cross_taker::{XtFields, fast::signal::*};
use hft_strategy_sdk::prelude::*;

fn params() -> XtFields {
    XtFields {
        enabled: true,
        min_edge: Rate(1_000_000),
        slip: Rate(0),
        depth_frac: Rate(500_000_000),
        max_chunk_notional: Notional(100_000_000),
        max_position_notional: Notional(500_000_000),
        max_quote_age_ms: 5,
        cooldown_ms: 100,
        max_hedge_failures: 3,
    }
}
fn leg(bid: i64, ask: i64) -> LegInput {
    LegInput {
        book: BboView {
            bid_px: Px(bid),
            ask_px: Px(ask),
            bid_qty: Qty(10_000),
            ask_qty: Qty(10_000),
            ..BboView::EMPTY
        },
        hot: InstHot {
            px_exp: -2,
            qty_exp: -2,
            tick: 1,
            lot: 2,
            min_qty: Qty(2),
            max_qty: Qty(1_000_000),
            min_notional: Notional(1_000_000),
            ..InstHot::NONE
        },
        fee: Rate(1_000_000),
        position: Qty(0),
        spot: false,
        spot_free: Qty(0),
    }
}

#[test]
fn signal_deducts_both_fee_notionals_and_requires_edge() {
    let (a, b) = (leg(999, 1000), leg(1003, 1004));
    assert_eq!(net_edge(Px(1000), Px(1003), a.fee, b.fee), Some(Rate(997_000)));
    assert!(signal(a, b, &params()).is_none());
    let b = leg(1004, 1005);
    assert_eq!(signal(a, b, &params()).unwrap().edge, Rate(1_996_000));
    assert!(signal(b, a, &params()).is_none());
    let mut f = params();
    f.slip = Rate(2_000_000);
    assert!(signal(a, b, &f).is_none());
}
#[test]
fn signal_sizes_both_depths_caps_and_common_lot() {
    let (mut a, mut b) = (leg(999, 1000), leg(1010, 1011));
    a.hot.lot = 6;
    b.hot.lot = 10;
    assert_eq!(pair_lot(6, 10), Some(30));
    let f = params();
    assert_eq!(signal(a, b, &f).unwrap().qty, Qty(960)); // 100 quote on either venue
    b.book.bid_qty = Qty(101);
    assert_eq!(signal(a, b, &f).unwrap().qty, Qty(30)); // 50% of the smaller touch
    b.book.bid_qty = Qty(59);
    assert!(signal(a, b, &f).is_none());
}
#[test]
fn signal_caps_each_position_and_never_shorts_spot() {
    let (mut a, mut b) = (leg(999, 1000), leg(1010, 1011));
    b.spot = true;
    assert!(signal(a, b, &params()).is_none());
    b.position = Qty(1000);
    b.spot_free = Qty(40);
    assert_eq!(signal(a, b, &params()).unwrap().qty, Qty(40));
    b.spot = false;
    b.position = Qty(-4900);
    let s = signal(a, b, &params()).unwrap();
    assert!(s.qty.0 <= 44, "sell leg is almost at its own position cap: {s:?}");
    a.position = Qty(5000);
    assert!(signal(a, b, &params()).is_none());
}
#[test]
fn signal_reduction_stops_at_zero_even_above_position_cap() {
    let (mut a, mut b) = (leg(999, 1000), leg(1010, 1011));
    a.position = Qty(-6000);
    b.position = Qty(6000);
    assert!(signal(a, b, &params()).unwrap().reducing);
    a.position = Qty(-20);
    b.position = Qty(20);
    assert_eq!(signal(a, b, &params()).unwrap().qty, Qty(20));
}
#[test]
fn signal_rejects_bad_books_and_arithmetic_overflow() {
    let (a, mut b) = (leg(999, 1000), leg(1010, 1011));
    b.book.ask_px = Px(1000);
    assert!(signal(a, b, &params()).is_none());
    assert_eq!(pair_lot(i64::MAX, 2), None);
    assert_eq!(pair_lot(0, 2), None);
    assert_eq!(net_edge(Px(0), Px(100), Rate(0), Rate(0)), None);
    b = leg(1010, 1011);
    b.hot.qty_exp = -18;
    b.hot.px_exp = -18;
    assert!(signal(a, b, &params()).is_none());
}
#[test]
fn signal_accepts_exact_minimum_notional_and_all_asset_exponents() {
    let (mut a, mut b) = (leg(999, 1000), leg(1010, 1011));
    a.hot.min_notional = Notional(2_000_000);
    a.book.ask_qty = Qty(40);
    assert_eq!(signal(a, b, &params()).unwrap().qty, Qty(20));
    b.book.bid_px = Px(1100);
    b.book.ask_px = Px(1101);
    b.hot.min_notional = Notional(2_200_000);
    assert_eq!(signal(a, b, &params()).unwrap().qty, Qty(20));
    let mut f = params();
    f.slip = Rate(10_000_000);
    assert!(
        signal(a, b, &f).is_none(),
        "sell limit must meet the minimum after slip"
    );
    for (p, q, expected) in [(-2, -8, 10_000_000), (-4, -8, 1_000_000_000), (0, 0, 0)] {
        b.hot.px_exp = p;
        b.hot.qty_exp = q;
        assert_eq!(
            qty_for_notional(Notional(100_000_000), Px(100_000), &b.hot),
            Some(expected)
        );
    }
}
