#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::OrderEventKind;
use hft_oms::*;
use hft_types::{GroupId, Qty, StrategyId, Tif};

fn legs(h: &mut H, n0: &str, n1: &str) {
    h.o.test_set_net(A1, g1(), X, q(n0));
    h.o.test_set_net(A1, g1(), Y, q(n1));
}

#[test]
fn oms_280_not_quiescent() {
    for row in 1..=6 {
        let mut h = H::new();
        legs(&mut h, "0.0004", "-0.0001");
        let mut g = g1();
        match row {
            1 => {
                place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
            }
            2 => {
                let s = h.place_ok(&buy_x(), 0);
                h.what(s, OrderEventKind::OutcomeUnknown, 0);
            }
            3 => {
                h.place_ok(&buy_x(), 0);
            }
            4 => {
                let mut it = buy_x();
                it.intent.tif = Tif::Ioc;
                let s = h.place_ok(&it, 0);
                h.ack(s, 1, 0);
            }
            5 => g = GroupId::compose(StrategyId(1), 9).unwrap(),
            _ => {
                let s = h.place_ok(&buy_x(), 0);
                h.ack(s, 1, 0);
            }
        }
        let before = h.legs(A1, g1()).view;
        let r = h.o.write_off_residual(A1, g);
        if row == 6 {
            assert!(r.is_ok(), "OMS-280.{row}");
        } else {
            assert_eq!(r, Err(OpError::NotQuiescent), "OMS-280.{row}");
            assert_eq!(h.legs(A1, g1()).view, before, "OMS-280.{row}");
        }
    }
}

#[test]
fn oms_281_write_off() {
    for (row, broken) in [("OMS-281.1", true), ("OMS-281.2", false)] {
        let mut h = H::new();
        if broken {
            let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
            h.what(f, OrderEventKind::Expired, 0);
            h.o.test_link_mut(l, |e| e.state = LinkState::Broken);
        }
        legs(&mut h, "0.0004", "-0.0001");
        h.o.test_set_residual(A1, g1(), X, q("0.00002"));
        h.o.test_set_residual(A1, g1(), Y, q("0.0006"));
        h.out.clear();
        let w = h.o.write_off_residual(A1, g1()).unwrap();
        assert_eq!(
            w,
            WriteOff {
                venue: VS,
                inst: X,
                leg_ix: 0,
                qty: q("0.0003")
            },
            "{row}"
        );
        let v = h.legs(A1, g1()).view;
        assert_eq!(
            (v.leg[0].net, v.leg[1].net, v.leg[0].residual, v.leg[1].residual),
            (q("0.0001"), q("-0.0001"), Qty(0), Qty(0)),
            "{row}"
        );
        assert!(h.out.is_empty(), "{row}");
    }
}

#[test]
fn oms_282_zero_naked_sum() {
    let mut h = H::new();
    legs(&mut h, "0.003", "-0.003");
    h.o.test_set_residual(A1, g1(), Y, q("0.0006"));
    let w = h.o.write_off_residual(A1, g1()).unwrap();
    assert_eq!(w.qty, Qty(0));
    let v = h.legs(A1, g1()).view;
    assert_eq!(
        (v.leg[0].net, v.leg[1].net, v.leg[0].residual, v.leg[1].residual),
        (q("0.003"), q("-0.003"), Qty(0), Qty(0))
    );
}

#[test]
fn oms_283_no_stale_residual_hedge() {
    let mut h = H::new();
    legs(&mut h, "0.0004", "-0.0001");
    h.o.test_set_residual(A1, g1(), Y, q("0.0006"));
    h.o.write_off_residual(A1, g1()).unwrap();
    let (f, _) = place_first(&mut h, q("0.0005"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.0005"), p("60000.00"), 1);
    assert!(h.out.hedges.is_empty());
    assert_eq!(h.legs(A1, g1()).view.leg[1].residual, q("0.0005"));
}
