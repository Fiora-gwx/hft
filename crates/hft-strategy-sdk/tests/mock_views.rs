//! `MockViews` (SDK-127, SDK-129, SDK-194, SDK-195, SDK-213, SDK-214).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_event::Level;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, ev};
use hft_strategy_sdk::*;
use hft_types::{Notional, Px, Qty, Rate, RunMode, Side, Tsc};

const U0: i64 = 1_759_276_800_000_000_000;

fn at_9e9() -> MockViews {
    let mut v = gt3_views();
    // tsc_per_ms = 1_000_000; engine_ns = 50e9 at tsc 9e9 → base 41e9 at tsc 0.
    let clk = EngineClockAnchor {
        base_engine_ns: 41_000_000_000,
        base_tsc: Tsc(0),
        tsc_hz_milli: 1_000_000_000_000,
        utc_tsc: Tsc(9_000_000_000),
        utc_ns: U0,
    };
    v.at(Now::new(Tsc(9_000_000_000), 100, 100, true, clk));
    v
}

#[test]
fn sdk_127_mock_restart() {
    let mut v = at_9e9();
    {
        let m = &mut v;
        m.bbo(
            I1,
            (Px(6_500_000), Qty(100_000_000)),
            (Px(6_500_100), Qty(100_000_000)),
        );
        m.conn(I1, Tsc(9_000_000_000), true, false);
    }
    assert_eq!(Views::now(&v).engine_ns, 50_000_000_000);
    assert!(v.quote_ok(A1, I1), "healthy before the restart");
    v.restart(Tsc(1_000), 900);
    let n = Views::now(&v);
    assert_eq!(
        (n.tsc, n.engine_ns, n.boot_seq),
        (Tsc(1_000), 50_000_000_000, 900)
    );
    let b = Views::bbo(&v, I1);
    assert_eq!((b.rx_tsc, b.conn_live_tsc, b.conn_up), (Tsc(0), Tsc(0), false));
    assert!(!v.quote_ok(A1, I1));
}

#[test]
fn sdk_129_rx_tsc_zero_unknown() {
    let mut v = gt3_views();
    let m = v.at_tsc(Tsc(5_000));
    m.bbo(
        I1,
        (Px(6_500_000), Qty(100_000_000)),
        (Px(6_500_100), Qty(100_000_000)),
    );
    assert_eq!(Views::bbo(&v, I1).rx_tsc, Tsc(5_000), "step 1");
    v.bbo_rx(I1, Tsc(0), 0, RxTsSrc::None);
    let b = Views::bbo(&v, I1);
    assert_eq!(
        (b.rx_tsc, b.hw_rx_ns, b.rx_ts_src, b.bid_px),
        (Tsc(0), 0, RxTsSrc::None, Px(6_500_000)),
        "step 2"
    );
    v.conn(I1, Tsc(5_000), true, false);
    let c = Views::bbo(&v, I1);
    assert_eq!(
        (c.bid_px, c.bid_qty, c.ask_px, c.ask_qty, c.rx_tsc),
        (b.bid_px, b.bid_qty, b.ask_px, b.ask_qty, b.rx_tsc),
        "step 3: quote unchanged"
    );
    assert_eq!((c.conn_live_tsc, c.conn_up), (Tsc(5_000), true), "step 3");
}

#[test]
fn sdk_194_mock_views_no_matching() {
    let mut v = clocked_views();
    let mut l = LegView::EMPTY;
    l.n_legs = 2;
    l.leg[0].net = Qty(0);
    {
        let m = &mut v;
        m.legs(A1, G1, l);
        m.bbo(
            I1,
            (Px(6_500_000), Qty(100_000_000)),
            (Px(6_500_100), Qty(100_000_000)),
        );
    }
    let mut s = EchoProbe::new(s3(0xFF01), &tp(3, &[]), &gt3(), &v.refview()).unwrap();
    let mut h = CtxHarness::new(&EchoProbe::DESC);
    let r = fast(&mut h, &mut s, &ev::bbo(I1, 0, Views::bbo(&v, I1)), &v);
    assert_eq!(r.places.len(), 1, "EchoProbe placed one Open IOC");
    assert_eq!(*Views::legs(&v, A1, G1), l);
    assert_eq!(Views::legs(&v, A1, G1).leg[0].inflight_immediate, Qty(0));
    assert_eq!(Views::order(&v, OrderRef::new(V1, A1, 0, 1)), None);
}

#[test]
fn sdk_195_mock_setters() {
    let mut v = gt3_views();
    let r = RiskStateView {
        conn_liveness_ms: 2_000,
        ..RiskStateView::EMPTY
    };
    let x = InstRiskView {
        max_px_dev: Rate(123),
        ..InstRiskView::EMPTY
    };
    let f = FundingView {
        rate: Rate(10_000),
        interval_s: 28_800,
        ..FundingView::EMPTY
    };
    let p = GroupPosView {
        held_notional: Notional(5_000_000),
        ..GroupPosView::EMPTY
    };
    let lv = |px: i64| Level {
        px: Px(px),
        qty: Qty(1_000),
    };
    {
        let m = &mut v;
        m.mode(A1, G1, RunMode::CloseOnly);
        m.risk(A1, r);
        m.risk_for(A1, I2, x);
        m.venue_ready(A1, V2, false);
        m.leverage(A1, I2, Some(300));
        m.target(A1, G1, None);
        m.funding(I2, f);
        m.book(I2, &[lv(100), lv(99), lv(98)], &[lv(101), lv(102)]);
        m.group_pos(A1, G1, p);
        m.portfolio(A1, |pf| {
            pf.cash(V1, USDT, 1_000_000_000, 0);
        });
    }
    let v = &v;
    assert_eq!(v.mode(A1, G1), RunMode::CloseOnly, "SDK-195.1");
    assert_eq!(v.risk(A1), r, "SDK-195.2");
    assert_eq!(v.risk_for(A1, I2), x, "SDK-195.3");
    assert_eq!(
        (v.venue_ready(A1, V2), v.venue_ready(A1, V1)),
        (false, true),
        "SDK-195.4"
    );
    assert_eq!(v.leverage(A1, I2), Some(300), "SDK-195.5");
    assert!(v.target(A1, G1).is_none(), "SDK-195.6");
    assert_eq!(*v.funding(I2), f, "SDK-195.7");
    let b = v.book(I2);
    let mut out = [Level {
        px: Px(0),
        qty: Qty(0),
    }; 8];
    assert_eq!(
        (b.status(), b.levels(Side::Buy, &mut out)),
        (BookStatus::Live, 3),
        "SDK-195.8"
    );
    assert_eq!(*v.group_pos(A1, G1), p, "SDK-195.9");
    assert_eq!(
        v.portfolio(A1).cash(V1, USDT).free,
        Qty(1_000_000_000),
        "SDK-195.10"
    );
}

#[test]
fn sdk_213_mock_orders_and_md_stamps() {
    let mut v = gt3_views();
    let r = OrderRef::new(V1, A1, 7, 2);
    let o = OrderView {
        r,
        group: G1,
        inst: I1,
        side: Side::Buy,
        tif: hft_types::Tif::PostOnly,
        class: IntentClass::Open,
        px: Px(450),
        qty: Qty(5_000_000),
        filled: Qty(1_000_000),
        state: OrderStateView::Live,
        tag: 1,
    };
    v.set_order(o);
    assert_eq!(Views::order(&v, r), Some(o), "SDK-213.1");
    assert_eq!(
        Views::order(&v, OrderRef::new(V1, A1, 7, 3)),
        None,
        "SDK-213.1 other gen"
    );
    v.clear_order(r);
    assert_eq!(Views::order(&v, r), None, "SDK-213.2");
    assert_eq!(
        (Views::md_count(&v, I1), Views::md_stamp(&v, I1, 1)),
        (0, None),
        "SDK-213.3"
    );
    for k in 1..=20i64 {
        v.md_push(
            I1,
            MdStampView {
                kind: MdStampView::MD_BATCH,
                ex_ts_ns: k,
                sw_rx_ns: 100 + k,
                rx_tsc: Tsc(0),
            },
        );
    }
    assert_eq!(Views::md_count(&v, I1), 20, "SDK-213.4");
    assert_eq!(
        Views::md_stamp(&v, I1, 4),
        None,
        "SDK-213.4 older than the last 16"
    );
    assert_eq!(
        Views::md_stamp(&v, I1, 5).map(|m| m.ex_ts_ns),
        Some(5),
        "SDK-213.4"
    );
    assert_eq!(
        Views::md_stamp(&v, I1, 20).map(|m| m.sw_rx_ns),
        Some(120),
        "SDK-213.4"
    );
    assert_eq!(Views::md_stamp(&v, I1, 21), None, "SDK-213.4 not yet");
    assert_eq!(Views::md_count(&v, I2), 0, "SDK-213.4 other inst");
}

#[test]
fn sdk_214_provided_stamps_and_open_orders() {
    let v = at_9e9();
    // .1 no Bbo stamp from the provided default.
    assert_eq!(Views::bbo_stamp(&v, I1), None, "SDK-214.1");
    // .2 stamps: only the engine clock.
    let n = Views::now(&v);
    let s = Views::stamps(&v);
    assert_eq!((s.engine_ns, s.utc_ns), (n.engine_ns, n.utc_ns), "SDK-214.2");
    assert_eq!(
        (s.kind, s.venue_ns, s.sw_rx_ns, s.hw_rx_ns, s.rx_tsc),
        (0, 0, 0, 0, Tsc(0)),
        "SDK-214.2"
    );
    // .3 no open orders.
    let blank = OrderView {
        r: OrderRef::new(V1, A1, 0, 0),
        group: G1,
        inst: I1,
        side: Side::Buy,
        tif: hft_types::Tif::FAK,
        class: IntentClass::Open,
        px: Px(0),
        qty: Qty(0),
        filled: Qty(0),
        state: OrderStateView::Live,
        tag: 0,
    };
    let mut out = [blank; 2];
    assert_eq!(Views::open_orders(&v, A1, I1, &mut out), 0, "SDK-214.3");
    assert_eq!(hft_types::Tif::FAK, hft_types::Tif::Ioc, "SDK-214.3 FAK is Ioc");
}
