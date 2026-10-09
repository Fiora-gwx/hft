//! `MockViews` (SDK-127, SDK-129, SDK-194, SDK-195).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_event::Level;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, ev};
use hft_strategy_sdk::*;
use hft_types::{Notional, Side, Px, Qty, Rate, RunMode, Tsc};

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
        m.bbo(I1, (Px(6_500_000), Qty(100_000_000)), (Px(6_500_100), Qty(100_000_000)));
        m.conn(I1, Tsc(9_000_000_000), true, false);
    }
    assert_eq!(Views::now(&v).engine_ns, 50_000_000_000);
    assert!(v.quote_ok(A1, I1), "healthy before the restart");
    v.restart(Tsc(1_000), 900);
    let n = Views::now(&v);
    assert_eq!((n.tsc, n.engine_ns, n.boot_seq), (Tsc(1_000), 50_000_000_000, 900));
    let b = Views::bbo(&v, I1);
    assert_eq!((b.rx_tsc, b.conn_live_tsc, b.conn_up), (Tsc(0), Tsc(0), false));
    assert!(!v.quote_ok(A1, I1));
}

#[test]
fn sdk_129_rx_tsc_zero_unknown() {
    let mut v = gt3_views();
    let m = v.at_tsc(Tsc(5_000));
    m.bbo(I1, (Px(6_500_000), Qty(100_000_000)), (Px(6_500_100), Qty(100_000_000)));
    assert_eq!(Views::bbo(&v, I1).rx_tsc, Tsc(5_000), "step 1");
    v.bbo_rx(I1, Tsc(0), 0, RxTsSrc::None);
    let b = Views::bbo(&v, I1);
    assert_eq!((b.rx_tsc, b.hw_rx_ns, b.rx_ts_src, b.bid_px), (Tsc(0), 0, RxTsSrc::None, Px(6_500_000)), "step 2");
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
        m.bbo(I1, (Px(6_500_000), Qty(100_000_000)), (Px(6_500_100), Qty(100_000_000)));
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
    let r = RiskStateView { conn_liveness_ms: 2_000, ..RiskStateView::EMPTY };
    let x = InstRiskView { max_px_dev: Rate(123), ..InstRiskView::EMPTY };
    let f = FundingView { rate: Rate(10_000), interval_s: 28_800, ..FundingView::EMPTY };
    let p = GroupPosView { held_notional: Notional(5_000_000), ..GroupPosView::EMPTY };
    let lv = |px: i64| Level { px: Px(px), qty: Qty(1_000) };
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
    assert_eq!((v.venue_ready(A1, V2), v.venue_ready(A1, V1)), (false, true), "SDK-195.4");
    assert_eq!(v.leverage(A1, I2), Some(300), "SDK-195.5");
    assert!(v.target(A1, G1).is_none(), "SDK-195.6");
    assert_eq!(*v.funding(I2), f, "SDK-195.7");
    let b = v.book(I2);
    let mut out = [Level { px: Px(0), qty: Qty(0) }; 8];
    assert_eq!((b.status(), b.levels(Side::Buy, &mut out)), (BookStatus::Live, 3), "SDK-195.8");
    assert_eq!(*v.group_pos(A1, G1), p, "SDK-195.9");
    assert_eq!(v.portfolio(A1).cash(V1, USDT).free, Qty(1_000_000_000), "SDK-195.10");
}
