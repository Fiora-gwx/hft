//! EXE-100–EXE-106, EXE-508, EXE-510: no-trade band and urgency.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{ExecSpec, ExecUpdateKind, TargetReason, Urgency};
use hft_types::Side;

#[test]
fn exe_100_band_boundary() {
    for (row, net, want) in [
        ("EXE-100.1", "9.90", None),
        ("EXE-100.2", "9.89999999", Some(Side::Buy)),
        ("EXE-100.3", "10.10", None),
        ("EXE-100.4", "10.10000001", Some(Side::Sell)),
    ] {
        let mut fx = Fx::new();
        fx.spot(A1, A, G7, "20", "20");
        inject(&mut fx, &taker(A1, G7, &[(0, "10.00")]));
        // P mirrors S so the group carries no naked quantity (no repair).
        let neg = negate(net);
        fx.legs(A1, G7, &[net, &neg]);
        fx.touch(S);
        let r = fx.adv();
        match want {
            None => {
                assert!(r.orders.is_empty(), "{row}");
                assert!(r.states().contains(&(2, 1, 0)), "{row}");
            }
            Some(side) => {
                assert_eq!(r.orders.len(), 1, "{row}");
                assert_eq!(r.orders[0].intent.side, side, "{row}");
            }
        }
    }
}

#[test]
fn exe_101_band_floor() {
    let mut insts = default_insts();
    insts[0].min_notional = 1_000_000;
    for (row, mark, net, want) in [
        ("EXE-101.1", "100", "0.08", None),
        ("EXE-101.2", "100", "0.07", Some("0.03")),
        ("EXE-101.3", "95", "0.079", None),
        ("EXE-101.4", "95", "0.07896", Some("0.02")),
    ] {
        let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts.clone());
        let mut tg = taker(A1, G7, &[(0, "0.10")]);
        tg.band = rate("0.1");
        inject(&mut fx, &tg);
        fx.mark(S, mark);
        let neg = negate(net);
        fx.legs(A1, G7, &[net, &neg]);
        fx.touch(S);
        let r = fx.adv();
        match want {
            None => assert!(r.orders.is_empty(), "{row}"),
            Some(x) => {
                assert_eq!(r.orders.len(), 1, "{row}");
                assert_eq!(
                    (r.orders[0].intent.side, r.orders[0].intent.qty),
                    (Side::Buy, q(x)),
                    "{row}"
                );
            }
        }
    }
}

fn first_slice(fx: &mut Fx, urgency: Urgency, cooldown: u32) {
    let mut tg = t1("-100.0");
    tg.urgency = urgency;
    if let ExecSpec::TakerSlice(p) = &mut tg.exec {
        p.cooldown_ms = cooldown;
    }
    let r = fx.go(&tg);
    let rf = fx.placed(&r.orders[0]);
    fx.at(1);
    fx.legs(A1, G8, &["-1.0"]);
    fx.fill(rf, G8, "1.0", true);
    fx.adv();
}

#[test]
fn exe_102_cooldown_by_urgency() {
    for (row, u, cd, at, order) in [
        ("EXE-102.1", Urgency::Passive, 300, 1199, false),
        ("EXE-102.2", Urgency::Passive, 300, 1200, true),
        ("EXE-102.3", Urgency::Normal, 300, 299, false),
        ("EXE-102.4", Urgency::Normal, 300, 300, true),
        ("EXE-102.5", Urgency::Urgent, 300, 99, false),
        ("EXE-102.6", Urgency::Urgent, 300, 100, true),
        ("EXE-102.7", Urgency::Urgent, 50, 50, true),
    ] {
        let mut fx = Fx::new();
        first_slice(&mut fx, u, cd);
        fx.at(at);
        fx.fire(ExecTimerKind::Wake, A1, G8);
        let r = fx.adv();
        assert_eq!(r.orders.len(), usize::from(order), "{row}");
    }
}

#[test]
fn exe_103_stuck_after_by_urgency() {
    for (row, u, at, stuck) in [
        ("EXE-103.1", Urgency::Passive, 7999, false),
        ("EXE-103.2", Urgency::Passive, 8000, true),
        ("EXE-103.3", Urgency::Normal, 1999, false),
        ("EXE-103.4", Urgency::Normal, 2000, true),
        ("EXE-103.5", Urgency::Urgent, 999, false),
        ("EXE-103.6", Urgency::Urgent, 1000, true),
    ] {
        let mut fx = Fx::new();
        fx.stale(P, 3000);
        let mut tg = t1("-10.0");
        tg.urgency = u;
        let r = fx.go(&tg);
        assert!(r.orders.is_empty(), "{row}");
        fx.at(at);
        fx.touch(P);
        let r = fx.adv();
        assert_eq!(r.states().contains(&(2, 3, pack(2, 4, 0))), stuck, "{row}");
    }
}

#[test]
fn exe_104_passive_top_level_only() {
    for (row, u, qty, depth) in [
        ("EXE-104.1", Urgency::Normal, "50.0", "250.0"),
        ("EXE-104.2", Urgency::Passive, "10.0", "50.0"),
    ] {
        let mut fx = Fx::new();
        fx.risk(A1, |r| r.max_order_notional = usd("100000"));
        let mut tg = t1("-100.0");
        tg.urgency = u;
        if let ExecSpec::TakerSlice(p) = &mut tg.exec {
            p.depth_frac = rate("0.2");
            p.max_chunk_notional = usd("100000");
        }
        let r = fx.go(&tg);
        assert_eq!(r.orders[0].intent.qty, q(qty), "{row}");
        assert_eq!(r.dec(5)[0].depth_qty, q(depth), "{row}");
    }
}

#[test]
fn exe_106_risk_reason_urgent() {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10.0"]);
    let mut tg = t1("0");
    tg.reason = TargetReason::Hard;
    tg.urgency = Urgency::Passive;
    let r = fx.go(&tg);
    let rf = fx.placed(&r.orders[0]);
    fx.at(1);
    fx.legs(A1, G8, &["-9.0"]);
    fx.fill(rf, G8, "1.0", true);
    fx.adv();
    fx.at(100);
    fx.fire(ExecTimerKind::Wake, A1, G8);
    assert_eq!(fx.adv().orders.len(), 1);
}

#[test]
fn exe_508_zero_target_dust_boundary() {
    for (row, mark, idle) in [
        ("EXE-508.1", "49.99", true),
        ("EXE-508.2", "50.00", true),
        ("EXE-508.3", "50.01", false),
    ] {
        let mut fx = Fx::new();
        let mut tg = t1("0");
        tg.band = rate("0.5");
        inject(&mut fx, &tg);
        fx.mark(P, mark);
        fx.legs(A1, G8, &["-0.1"]);
        fx.touch(P);
        let r = fx.adv();
        assert_eq!(r.states().contains(&(2, 1, 0)), idle, "{row}");
        if idle {
            assert!(r.orders.is_empty(), "{row}");
        } else {
            assert_eq!(
                (r.orders[0].intent.side, r.orders[0].intent.qty),
                (Side::Buy, q("0.1")),
                "{row}"
            );
        }
    }
}

#[test]
fn exe_510_urgent_zero_target_ignores_band() {
    for (row, u) in [
        ("EXE-510.1", Urgency::Urgent),
        ("EXE-510.2", Urgency::Normal),
        ("EXE-510.3", Urgency::Passive),
    ] {
        let mut fx = Fx::new();
        let mut tg = t1("0");
        tg.band = rate("0.5");
        tg.urgency = u;
        inject(&mut fx, &tg);
        fx.legs(A1, G8, &["-0.1"]);
        fx.touch(P);
        let r = fx.adv();
        assert_eq!(r.orders.len(), 1, "{row}");
        assert_eq!(
            (r.orders[0].intent.side, r.orders[0].intent.qty),
            (Side::Buy, q("0.1")),
            "{row}"
        );
        let _ = ExecUpdateKind::Done {
            cleared_by_engine: false,
        };
    }
}
