#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{ConnRole, ConnStateKind, OrderEventKind};
use hft_strategy_sdk::IntentClass;
use hft_types::{Px, Qty, Side, Tif};

#[test]
fn oms_300_inflight_lifecycle() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    let check = |h: &H, live: bool| {
        let ib = h.o.inflight();
        assert_eq!((ib.open_sell(A1, X), ib.imm_sell(A1, X)), (Qty(0), Qty(0)));
        let v = if live { usdt("600.00") } else { 0 };
        assert_eq!(ib.buy_notional(A1, VS).0, v);
        assert_eq!(ib.open_notional_group(A1, g1()).0, v);
        assert_eq!(ib.open_notional_venue(A1, VS).0, v);
    };
    check(&h, true);
    h.ack(s, 1, 1);
    check(&h, true);
    h.cancel(s, 2).unwrap();
    check(&h, true);
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Down, 3);
    check(&h, true);
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 4);
    h.query(s, 1, Qty(0), Px(0), 4);
    check(&h, true);
    h.what(s, OrderEventKind::CancelAck, 5);
    check(&h, false);
}

#[test]
fn oms_301_buy_notional() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    assert_eq!(h.o.inflight().buy_notional(A1, VS).0, usdt("600.00"), "OMS-301.1");
    h.ack(s, 1, 0);
    h.fill(s, q("0.004"), p("60000.00"), 1);
    assert_eq!(h.o.inflight().buy_notional(A1, VS).0, usdt("360.00"), "OMS-301.2");
    let mut h = H::new();
    h.place_ok(
        &intent(VS, A1, X, Side::Sell, Tif::Gtc, p("60000.00"), q("0.01"), g1()),
        0,
    );
    assert_eq!(h.o.inflight().buy_notional(A1, VS).0, 0, "OMS-301.3");
    let mut h = H::new();
    h.place_ok(
        &intent(VX, A1, Y, Side::Buy, Tif::Gtc, p("60000.0"), q("0.01"), g1()),
        0,
    );
    assert_eq!(h.o.inflight().buy_notional(A1, VX).0, 0, "OMS-301.4");
}

#[test]
fn oms_302_open_margin() {
    for (row, lev, class, partial, want) in [
        ("OMS-302.1", 2000u16, IntentClass::Open, false, "30.00"),
        ("OMS-302.2", 0, IntentClass::Open, false, "600.00"),
        ("OMS-302.3", 2000, IntentClass::Open, false, "30.00"),
        ("OMS-302.4", 2000, IntentClass::Open, true, "18.00"),
        ("OMS-302.5", 0, IntentClass::Reduce, false, "0"),
    ] {
        let mut h = H::new();
        let mut it = intent(VX, A1, Y, Side::Buy, Tif::Gtc, p("60000.0"), q("0.01"), g1());
        it.lev_x100 = lev;
        it.class = class;
        let s = h.place_ok(&it, 0);
        if partial {
            h.ack(s, 1, 0);
            h.fill(s, q("0.004"), p("60000.0"), 1);
        }
        assert_eq!(h.o.inflight().open_margin(A1, VX).0, usdt(want), "{row}");
    }
}

#[test]
fn oms_303_open_notional() {
    for (row, side, class, want) in [
        ("OMS-303.1", Side::Buy, IntentClass::Open, "600.00"),
        ("OMS-303.2", Side::Sell, IntentClass::Reduce, "0"),
        ("OMS-303.3", Side::Sell, IntentClass::Hedge, "0"),
    ] {
        let mut h = H::new();
        let mut it = intent(VS, A1, X, side, Tif::Gtc, p("60000.00"), q("0.01"), g1());
        it.class = class;
        h.place_ok(&it, 0);
        let ib = h.o.inflight();
        assert_eq!(
            (
                ib.open_notional_group(A1, g1()).0,
                ib.open_notional_venue(A1, VS).0
            ),
            (usdt(want), usdt(want)),
            "{row}"
        );
    }
}
