#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::OrderEventKind;
use hft_oms::*;
use hft_strategy_sdk::{HedgeKind, HedgePlan, LinkRole, OnFail, OrderRef};
use hft_types::{Side, Tif};

fn plan_l2(on_fail: OnFail) -> HedgePlan {
    let mut p = plan_l1();
    p.kind = HedgeKind::Simultaneous;
    p.on_fail = on_fail;
    p
}

/// L2 placed (IOC buy X 0.012); returns (h, first, second request slot, link).
fn l2(on_fail: OnFail, leg1_net: &str, res: &str) -> (H, OrderRef, OrderRef, u32) {
    let mut h = H::new();
    h.o.test_set_net(A1, g1(), Y, q(leg1_net));
    h.o.test_set_residual(A1, g1(), Y, q(res));
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l2(on_fail), 0);
    let req = one_hedge(&h);
    let s = place_hedge(&mut h, &req, 0).unwrap();
    (h, f, s, l)
}

fn reject(h: &mut H, r: OrderRef, t: u64) {
    let e = h.oe(r, OrderEventKind::Reject);
    h.ev(&e, t);
}

#[test]
fn oms_210_simultaneous_place() {
    let mut h = H::new();
    h.o.test_set_net(A1, g1(), Y, q("-0.02"));
    h.o.test_set_residual(A1, g1(), Y, q("0.0004"));
    let (_f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l2(OnFail::Reverse), 0);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.qty, r.basis, r.peer_venue, r.peer_account),
        (LinkRole::Second, q("0.012"), q("0.0124"), VS, A1)
    );
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.0004"));
    assert_eq!(h.link(l).state, LinkState::Hedging);
}

#[test]
fn oms_211_second_short() {
    for (row, on_fail) in [("OMS-211.1", OnFail::Reverse), ("OMS-211.2", OnFail::Hold)] {
        let (mut h, f, s, l) = l2(on_fail, "-0.02", "0.0004");
        h.fill(f, q("0.012"), p("60000.00"), 1);
        h.fill(s, q("0.005"), p("59990.0"), 2);
        h.what(s, OrderEventKind::Expired, 3);
        if on_fail == OnFail::Reverse {
            let r = one_hedge(&h);
            assert_eq!(
                (r.role, r.venue, r.inst, r.side, r.reduce_only, r.qty),
                (LinkRole::Reverse, VS, X, Side::Sell, true, q("0.007")),
                "{row}"
            );
        } else {
            assert!(h.out.hedges.is_empty(), "{row}");
            assert_eq!(h.link(l).state, LinkState::Done, "{row}");
            let v = h.legs(A1, g1()).view;
            assert_eq!((v.leg[0].net, v.leg[1].net), (q("0.012"), q("-0.025")), "{row}");
        }
    }
}

#[test]
fn oms_212_first_short_reverse_second() {
    let (mut h, f, s, l) = l2(OnFail::Reverse, "0", "0");
    reject(&mut h, f, 1);
    h.fill(s, q("0.012"), p("59990.0"), 2);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.venue, r.inst, r.side, r.reduce_only, r.qty),
        (LinkRole::Reverse, VX, Y, Side::Buy, true, q("0.012"))
    );
    let rv = place_hedge(&mut h, &r, 3).unwrap();
    h.fill(rv, q("0.012"), p("60010.0"), 4);
    assert_eq!(h.link(l).hedge_filled, hft_types::Qty(0));
    assert_eq!(h.link(l).state, LinkState::Done);
}

#[test]
fn oms_213_simul_reverse_failed() {
    let mut h = H::with(|c| c.hedge_deadline_ms = 60_000);
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l2(OnFail::Reverse), 0);
    let req = one_hedge(&h);
    let s = place_hedge(&mut h, &req, 0).unwrap();
    reject(&mut h, f, 1);
    h.fill(s, q("0.012"), p("59990.0"), 2);
    let mut n = 0;
    let mut t = 10;
    loop {
        let r = one_hedge(&h);
        assert_eq!(r.role, LinkRole::Reverse);
        n += 1;
        let rv = place_hedge(&mut h, &r, t).unwrap();
        reject(&mut h, rv, t + 1);
        if h.link(l).state == LinkState::Broken {
            break;
        }
        h.retry(l, t + 200);
        t += 300;
    }
    assert_eq!(n, 4);
    assert_eq!(
        h.out
            .links
            .iter()
            .find(|u| u.state == LinkState::Broken)
            .unwrap()
            .broken_reason,
        LinkBroken::ReverseFailed
    );
}

#[test]
fn oms_214_simul_equal_done() {
    let (mut h, f, s, l) = l2(OnFail::Reverse, "0", "0");
    h.fill(f, q("0.012"), p("60000.00"), 1);
    h.fill(s, q("0.012"), p("59990.0"), 2);
    assert!(h.out.hedges.is_empty());
    assert_eq!(h.link(l).state, LinkState::Done);
}

#[test]
fn oms_211_closing_pair_completes_only_the_unfilled_leg() {
    for first_short in [false, true] {
        let mut h = H::new();
        h.o.test_set_net(A1, g1(), X, q("-0.012"));
        h.o.test_set_net(A1, g1(), Y, q("0.012"));
        let mut plan = plan_l2(OnFail::Reverse);
        plan.reduce_only = 1;
        plan.tif = Tif::Fok;
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan, 0);
        let req = one_hedge(&h);
        let s = place_hedge(&mut h, &req, 0).unwrap();
        let (filled, unfilled) = if first_short { (s, f) } else { (f, s) };
        h.fill(filled, q("0.012"), p("60000"), 1);
        reject(&mut h, unfilled, 2);
        assert!(h.out.hedges.is_empty());
        assert_eq!(h.link(l).retries, 1);
        h.retry(l, 102);
        let req = one_hedge(&h);
        assert_eq!(req.role, LinkRole::SecondRetry);
        assert_eq!(req.inst, if first_short { X } else { Y });
        assert_eq!(req.side, if first_short { Side::Buy } else { Side::Sell });
        assert!(req.reduce_only);
        assert_eq!(req.qty, q("0.012"));
        let retry = place_hedge(&mut h, &req, 102).unwrap();
        assert_eq!(h.slot(retry).tif, Tif::Ioc);
        // Duplicate timer delivery cannot create a second order while this close is in flight.
        h.retry(l, 103);
        assert!(h.out.hedges.is_empty());
        h.fill(retry, q("0.012"), p("60000"), 104);
        assert_eq!(h.link(l).state, LinkState::Done);
        assert_eq!(h.link(l).first_net, h.link(l).hedge_filled);
        assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0"));
        assert_eq!(h.legs(A1, g1()).view.leg[1].net, q("0"));
    }
}

#[test]
fn oms_213_closing_pair_retry_budget_and_limiter_backoff() {
    let mut h = H::new();
    let mut plan = plan_l2(OnFail::Reverse);
    plan.reduce_only = 1;
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan, 0);
    let req = one_hedge(&h);
    let s = place_hedge(&mut h, &req, 0).unwrap();
    h.fill(f, q("0.012"), p("60000"), 1);
    reject(&mut h, s, 2);
    h.retry(l, 102);
    h.out.clear();
    h.o.hedge_gated(
        l,
        HedgeGate::Risk(hft_types::RejectBits::RATE),
        ms(500),
        ms(102),
        50,
        &mut h.out,
    );
    assert!(h.out.hedges.is_empty());
    assert_eq!(h.link(l).retries, 1);
    assert!(h.out.timers.iter().any(|t| t.deadline_tsc == ms(500)));
    for attempt in 0..3 {
        h.retry(l, 500 + attempt * 200);
        let req = one_hedge(&h);
        assert_eq!(req.role, LinkRole::SecondRetry);
        let retry = place_hedge(&mut h, &req, 500 + attempt * 200).unwrap();
        reject(&mut h, retry, 501 + attempt * 200);
    }
    assert_eq!(h.link(l).state, LinkState::Broken);
    assert!(
        h.out
            .links
            .iter()
            .any(|u| u.broken_reason == LinkBroken::RetryExhausted)
    );
    h.retry(l, 1200);
    assert!(h.out.hedges.is_empty());
}
