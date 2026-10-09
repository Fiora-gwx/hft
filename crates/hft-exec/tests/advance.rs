//! EXE-080–EXE-091: the per-cell step of `advance`.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{
    ExecSpec, ExecUpdateKind, IntentClass, MarginLevel, OrderUpdateKind, StuckReason, TargetState,
};
use hft_types::{RejectBits, RunMode, Side};

#[test]
fn exe_080_force_req_first() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10.0"));
    fx.cm(A1, G8).view.state = TargetState::Idle;
    fx.legs(A1, G8, &["-10.0"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let fs = r
        .decisions
        .iter()
        .position(|d| d.action == 11 && d.detail == pack(5, 1, 0))
        .unwrap();
    assert_eq!(r.decisions[fs].target_qty, q("-8.0"));
    let sl = r.decisions.iter().position(|d| d.action == 5).unwrap();
    assert!(fs < sl);
    let o = r.orders[0];
    assert_eq!(
        (o.intent.side, o.intent.qty, o.origin),
        (Side::Buy, q("2.0"), ExecOrigin::Engine)
    );
    assert!(o.intent.reduce_only());
    let snap = fx.snap();
    assert_eq!(acct_row(&snap, A1).unwrap().force_kind, 0);
}

fn first_slice_partial(fx: &mut Fx) {
    fx.at(1000);
    let r = fx.go(&t1("-10.0"));
    let rf = fx.placed(&r.orders[0]);
    fx.fill(rf, G8, "4.0", true);
    fx.legs(A1, G8, &["-4.0"]);
}

#[test]
fn exe_081_wake_at_boundary() {
    let mut fx = Fx::new();
    first_slice_partial(&mut fx);
    fx.adv();
    fx.at(1299);
    fx.touch(P);
    assert!(fx.adv().orders.is_empty(), "EXE-081.1");

    let mut fx = Fx::new();
    first_slice_partial(&mut fx);
    fx.adv();
    fx.at(1300);
    fx.fire(ExecTimerKind::Wake, A1, G8);
    assert_eq!(fx.adv().orders.len(), 1, "EXE-081.2");

    let mut fx = Fx::new();
    first_slice_partial(&mut fx);
    fx.adv();
    fx.at(1301);
    fx.touch(P);
    assert_eq!(fx.adv().orders.len(), 1, "EXE-081.3");

    let mut fx = Fx::new();
    fx.at(1000);
    let r = fx.go(&t1("-10.0"));
    let rf = fx.placed(&r.orders[0]);
    fx.at(1100);
    fx.legs(A1, G8, &["-4.0"]);
    fx.fill(rf, G8, "4.0", true);
    assert!(fx.ex.has_pending());
    assert!(fx.adv().orders.is_empty(), "EXE-081.4");
}

#[test]
fn exe_082_zero_target_dust_reach() {
    for (row, net, reached) in [
        ("EXE-082.1", "-0.05", true),
        ("EXE-082.2", "-0.05000001", false),
        ("EXE-082.3", "-0.04999999", true),
    ] {
        let mut fx = Fx::new();
        inject(&mut fx, &t1("0"));
        fx.legs(A1, G8, &[net]);
        fx.touch(P);
        let r = fx.adv();
        assert_eq!(r.states().contains(&(2, 1, 0)), reached, "{row}");
        assert_eq!(
            r.upd().contains(&ExecUpdateKind::Done {
                cleared_by_engine: false
            }),
            reached,
            "{row}"
        );
    }
}

#[test]
fn exe_083_residual_check_after_done() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10.00", "-10.0"));
    fx.legs(A1, G7, &["10.06", "-10.0"]);
    fx.spot(A1, A, G7, "10.06", "10.06");
    fx.touch(S);
    let r = fx.adv();
    let si = r
        .decisions
        .iter()
        .position(|d| d.action == 6 && d.state_to == 1)
        .unwrap();
    let ti = r.decisions.iter().position(|d| d.action == 8).unwrap();
    assert!(si < ti);
    assert!(r.upd().contains(&ExecUpdateKind::Done {
        cleared_by_engine: false
    }));
    let o = r.ord(ExecOrderKind::ResidualTrim);
    assert_eq!(o.len(), 1);
    assert_eq!(
        (
            o[0].intent.side,
            o[0].intent.inst,
            o[0].intent.qty,
            o[0].hedge_cap_qty,
            o[0].origin
        ),
        (Side::Sell, S, q("0.06"), q("0.07"), ExecOrigin::Engine)
    );
    assert!(r.ord(ExecOrderKind::HedgeFix).is_empty());
}

#[test]
fn exe_084_wait_on_inflight() {
    // .1
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["10", "-10"]);
    fx.infl(A1, G7, 0, "0.05");
    fx.touch(S);
    let r = fx.adv();
    assert!(r.orders.is_empty() && r.decisions.is_empty(), "EXE-084.1");
    // .2
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["5", "-10"]);
    {
        let c = fx.cm(A1, G7);
        c.active.kind = ACT_PAIR;
        c.active.ext[1] = 1;
        c.active.link_id = 1;
    }
    fx.touch(S);
    assert!(fx.adv().orders.is_empty(), "EXE-084.2");
    // .3
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10"));
    {
        let c = fx.cm(A1, G8);
        c.active.kind = ACT_SLICE;
        c.active.ext = [1, 1, 2, 0];
    }
    fx.touch(P);
    assert!(fx.adv().orders.is_empty(), "EXE-084.3");
    // .4
    let g4 = gspec(G12, &[(S, A), (P, B), (Q, C), (S2, D)]);
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g4]);
    inject(
        &mut fx,
        &taker(A1, G12, &[(0, "1"), (1, "-1"), (2, "0"), (3, "0")]),
    );
    fx.infl(A1, G12, 0, "0.5");
    fx.touch(S);
    let r = fx.adv();
    assert_eq!(r.orders.len(), 1, "EXE-084.4");
    assert_eq!(r.orders[0].intent.inst, P);
}

fn flatten_g8(net: &str) -> (Fx, hft_strategy_sdk::OrderRef) {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &[net]);
    fx.flatten(A1);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G8).copied().unwrap();
    assert_eq!((o.intent.side, o.intent.qty), (Side::Buy, q(&net[1..])));
    let rf = fx.placed(&o);
    (fx, rf)
}

#[test]
fn exe_085_unknown_reduce_only_continue() {
    let (mut fx, rf) = flatten_g8("-6.0");
    fx.upd(rf, G8, OrderUpdateKind::Unknown);
    fx.infl(A1, G8, 0, "6.0");
    fx.at(200);
    fx.touch(P);
    let r = fx.adv();
    assert_eq!(fx.cell(A1, G8).active.ext[1], 0);
    assert!(r.orders.is_empty());
    fx.legs(A1, G8, &["-4.0"]);
    fx.infl(A1, G8, 0, "4.0");
    fx.at(400);
    fx.touch(P);
    assert!(fx.adv().orders.is_empty());
}

#[test]
fn exe_086_unknown_wait_cases() {
    // .1 serial spot leg (Broken cell, perp leg already 0).
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["3.00", "0"]);
    fx.spot(A1, A, G7, "3", "3");
    fx.cm(A1, G7).view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    fx.flatten(A1);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G7).copied().unwrap();
    assert_eq!(o.intent.inst, S);
    let rf = fx.placed(&o);
    fx.upd(rf, G7, OrderUpdateKind::Unknown);
    fx.infl(A1, G7, 0, "-1.0");
    fx.at(500);
    fx.touch(S);
    let r = fx.adv();
    assert!(r.orders.iter().all(|o| o.intent.inst != S), "EXE-086.1");
    assert_ne!(fx.cell(A1, G7).active.ext[1], 0, "EXE-086.1");
    // .2 perp leg without reduce-only support.
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-3.0"]);
    fx.cm(A1, G8).legs[0].reduce_only_cap = 0;
    fx.flatten(A1);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G8).copied().unwrap();
    let rf = fx.placed(&o);
    fx.upd(rf, G8, OrderUpdateKind::Unknown);
    fx.infl(A1, G8, 0, "1.0");
    fx.legs(A1, G8, &["-2.0"]);
    fx.at(500);
    fx.touch(P);
    assert!(fx.adv().orders.is_empty(), "EXE-086.2");
    // .3 pair first leg with LegLink.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["3.00", "-3.0"]);
    fx.spot(A1, A, G7, "3", "3");
    fx.flatten(A1);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G7).copied().unwrap();
    assert!(o.intent.link.get().is_some());
    let rf = fx.placed(&o);
    fx.upd(rf, G7, OrderUpdateKind::Unknown);
    fx.at(500);
    fx.touch(P);
    let r = fx.adv();
    assert!(r.has_dec(11, pack(5, 2, 6)), "EXE-086.3");
    // .4 strategy target.
    let mut fx = Fx::new();
    let r = fx.go(&t1("-10"));
    let rf = fx.placed(&r.orders[0]);
    fx.upd(rf, G8, OrderUpdateKind::Unknown);
    fx.infl(A1, G8, 0, "-10");
    fx.at(500);
    fx.touch(P);
    assert!(fx.adv().orders.is_empty(), "EXE-086.4");
}

#[test]
fn exe_087_precheck_open() {
    type Setup = fn(&mut Fx);
    let rows: [(&str, Setup, bool); 7] = [
        ("EXE-087.1", |fx| fx.mode(A1, G8, RunMode::Shadow), true),
        ("EXE-087.2", |fx| fx.mode(A1, G8, RunMode::CloseOnly), false),
        (
            "EXE-087.3",
            |fx| fx.cm(A1, G8).view.state = TargetState::Stuck(StuckReason::Shadow),
            false,
        ),
        ("EXE-087.4", |fx| fx.risk(A1, |r| r.hard = true), false),
        ("EXE-087.5", |fx| fx.risk(A1, |r| r.open_blocked = true), false),
        (
            "EXE-087.6",
            |fx| fx.risk(A1, |r| r.margin_level = MarginLevel::L1),
            false,
        ),
        ("EXE-087.7", |_| {}, true),
    ];
    for (row, setup, order) in rows {
        let mut fx = Fx::new();
        fx.at(1000);
        inject(&mut fx, &t1("-10.0"));
        setup(&mut fx);
        fx.touch(P);
        let r = fx.adv();
        assert_eq!(r.orders.len(), usize::from(order), "{row}");
        let c = fx.cell(A1, G8);
        if order {
            assert_eq!(c.gated_since.0, 0, "{row}");
        } else if row != "EXE-087.3" {
            assert_eq!(c.gated_since, t(1000), "{row}");
        }
        if row == "EXE-087.2" {
            assert_ne!(c.gated_bits.0 & RejectBits::CLOSE_ONLY.0, 0);
        }
    }
}

#[test]
fn exe_088_precheck_reduce() {
    type Setup = fn(&mut Fx);
    let rows: [(&str, Setup, bool); 4] = [
        ("EXE-088.1", |fx| fx.mode(A1, G8, RunMode::CloseOnly), true),
        ("EXE-088.2", |fx| fx.risk(A1, |r| r.open_blocked = true), true),
        (
            "EXE-088.3",
            |fx| fx.risk(A1, |r| r.margin_level = MarginLevel::L2),
            true,
        ),
        ("EXE-088.4", |fx| fx.risk(A1, |r| r.hard = true), false),
    ];
    for (row, setup, order) in rows {
        let mut fx = Fx::new();
        inject(&mut fx, &t1("-5.0"));
        fx.legs(A1, G8, &["-10.0"]);
        setup(&mut fx);
        fx.touch(P);
        let r = fx.adv();
        assert_eq!(r.orders.len(), usize::from(order), "{row}");
        if order {
            assert_eq!(
                (r.orders[0].intent.side, r.orders[0].intent.class),
                (Side::Buy, IntentClass::Reduce),
                "{row}"
            );
        } else {
            assert_ne!(fx.cell(A1, G8).gated_since.0, 0, "{row}");
        }
    }
}

#[test]
fn exe_089_second_leg_quote_blocks_pair() {
    let mut fx = Fx::new();
    fx.at(5000);
    fx.stale(P, 2001);
    let r = fx.go(&t0("10", "-10"));
    assert!(r.orders.is_empty());
    assert_eq!(fx.cell(A1, G7).gated_since, t(5000));
    fx.at(7000);
    fx.touch(S);
    let r = fx.adv();
    assert!(r.states().contains(&(2, 3, pack(2, 4, 0))));
}

#[test]
fn exe_090_cross_zero_split() {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["3.0"]);
    let r = fx.go(&t1("-2.0"));
    let o = r.orders[0];
    assert_eq!(
        (o.intent.side, o.intent.qty, o.intent.class),
        (Side::Sell, q("3.0"), IntentClass::Reduce)
    );
    assert!(o.intent.reduce_only());
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["0"]);
    fx.fill(rf, G8, "3.0", true);
    fx.adv();
    fx.at(300);
    fx.touch(P);
    let r = fx.adv();
    let o = r.orders[0];
    assert_eq!(
        (o.intent.side, o.intent.qty, o.intent.class),
        (Side::Sell, q("2.0"), IntentClass::Open)
    );
}

#[test]
fn exe_091_slice_records() {
    let mut fx = Fx::new();
    fx.at(1000);
    fx.seq = 76;
    let r = fx.go(&t1("-10.0"));
    let d = r.dec(5)[0];
    assert_eq!((d.algo, d.origin, d.reason, d.urgency), (1, 1, 1, 2));
    assert_eq!((d.leg_ix, d.first_leg_ix, d.side), (0, 0xFF, Side::Sell as u8));
    assert_eq!(
        (d.qty, d.px, d.target_qty, d.pos_qty, d.inflight_qty),
        (q("10.0"), px("99.31"), q("-10.0"), q("0"), q("0"))
    );
    assert_eq!(
        (d.depth_qty, d.set_seq, d.exec_order_id),
        (q("250.0"), 77, 1 | 1 << 16)
    );
    let o = r.orders[0];
    assert_eq!(o.id, ExecOrderId { cix: 1, n: 1 });
    assert_eq!(
        (o.kind, o.origin, o.reason, o.hedge_cap_qty),
        (
            ExecOrderKind::Slice,
            ExecOrigin::Target,
            hft_strategy_sdk::TargetReason::Rebalance,
            q("0")
        )
    );
    assert!(r.timer_set(ExecTimerKind::Wake).contains(&t(1300)));
    let _ = ExecSpec::TakerSlice(slice0());
}
