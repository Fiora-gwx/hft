//! EXE-110–EXE-112: class hints, exchange reduce-only flag, origin; spot sell caps; second-leg
//! reduce-only.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{
    EngineOrigin, ExecSpec, FirstLeg, IntentClass, LinkRole, MarginLevel, TargetOrigin, TargetReason,
    TargetState,
};

fn one(r: &Rec, g: hft_types::GroupId) -> ExecOrder {
    *r.orders.iter().find(|o| o.intent.group == g).expect("order")
}

#[test]
fn exe_110_class_hint_table() {
    // .1
    let mut fx = Fx::new();
    let o = one(&fx.go(&t1("-10")), G8);
    assert_eq!(
        (o.intent.class, o.intent.reduce_only(), o.origin),
        (IntentClass::Open, false, ExecOrigin::Target),
        "EXE-110.1"
    );
    // .2 / .4
    for (row, reason) in [
        ("EXE-110.2", TargetReason::Rebalance),
        ("EXE-110.4", TargetReason::Manual),
    ] {
        let mut fx = Fx::new();
        fx.legs(A1, G8, &["-10"]);
        let mut tg = t1("-5");
        tg.reason = reason;
        let o = one(&fx.go(&tg), G8);
        assert_eq!(
            (o.intent.class, o.intent.reduce_only(), o.origin),
            (IntentClass::Reduce, true, ExecOrigin::Target),
            "{row}"
        );
    }
    // .3
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["5.00", "-5.00"]);
    fx.spot(A1, A, G7, "5", "5");
    let mut tg = taker(A1, G7, &[(0, "0")]);
    tg.reason = TargetReason::Exit;
    let o = one(&fx.go(&tg), G7);
    assert_eq!(
        (o.intent.class, o.intent.reduce_only(), o.origin),
        (IntentClass::Reduce, false, ExecOrigin::Target),
        "EXE-110.3"
    );
    // .5
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let o = one(&fx.adv(), G8);
    assert_eq!(
        (o.intent.class, o.intent.reduce_only(), o.origin, o.reason),
        (
            IntentClass::Flatten,
            true,
            ExecOrigin::Engine,
            TargetReason::MarginStress
        ),
        "EXE-110.5"
    );
    // .6
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    fx.flatten(A1);
    let o = one(&fx.adv(), G8);
    assert_eq!(
        (o.intent.class, o.reason, o.origin),
        (IntentClass::Flatten, TargetReason::Hard, ExecOrigin::Engine),
        "EXE-110.6"
    );
    // .7
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    let o = one(&fx.adv(), G7);
    assert_eq!(o.kind, ExecOrderKind::HedgeFix);
    assert_eq!(
        (o.intent.class, o.intent.reduce_only(), o.hedge_cap_qty),
        (IntentClass::Hedge, true, q("0.6")),
        "EXE-110.7"
    );
    assert_eq!(
        o.intent.link.get().map(|l| l.role),
        Some(LinkRole::HedgeFix),
        "EXE-110.7"
    );
    // .8
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["10.06", "-10.0"]);
    fx.spot(A1, A, G7, "10.06", "10.06");
    fx.touch(S);
    let o = one(&fx.adv(), G7);
    assert_eq!(o.kind, ExecOrderKind::ResidualTrim);
    assert_eq!(
        (o.intent.class, o.intent.reduce_only(), o.hedge_cap_qty),
        (IntentClass::Hedge, false, q("0.07")),
        "EXE-110.8"
    );
    // .9 / .10
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-8"]);
    inject(&mut fx, &t1("-8"));
    {
        let c = fx.cm(A1, G8);
        c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
        c.view.intent.reason = TargetReason::MarginStress;
        c.view.state = TargetState::Idle;
    }
    fx.acct(A1).force_active = true;
    let mut tg = t1("-4");
    tg.reason = TargetReason::Exit;
    let o = one(&fx.go(&tg), G8);
    assert_eq!(
        (o.intent.class, o.origin),
        (IntentClass::Flatten, ExecOrigin::Engine),
        "EXE-110.9"
    );
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-4"]);
    let o = one(&fx.go(&t1("-2")), G8);
    assert_eq!(
        (o.intent.class, o.origin),
        (IntentClass::Reduce, ExecOrigin::Target),
        "EXE-110.10"
    );
}

#[test]
fn exe_111_spot_sell_capped() {
    for (row, inv, free, want) in [
        ("EXE-111.1", "10.00", "6.00", Some("6.00")),
        ("EXE-111.2", "4.00", "6.00", Some("4.00")),
        ("EXE-111.3", "10.00", "0.005", None),
    ] {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["10.00", "-10.00"]);
        fx.spot(A1, A, G7, inv, free);
        let mut s = slice0();
        s.depth_frac = rate("1");
        let mut tg = target(A1, G7, &[(0, q("0"))], ExecSpec::TakerSlice(s));
        tg.reason = TargetReason::Exit;
        let r = fx.go(&tg);
        let o: Vec<_> = r.orders.iter().filter(|o| o.intent.inst == S).collect();
        match want {
            Some(x) => assert_eq!(o[0].intent.qty, q(x), "{row}"),
            None => assert!(o.is_empty(), "{row}"),
        }
    }
}

#[test]
fn exe_112_second_leg_reduce_only() {
    for (row, open, first, want) in [
        ("EXE-112.1", true, 0u8, 0u8),
        ("EXE-112.2", false, 0, 1),
        ("EXE-112.3", false, 1, 0),
    ] {
        let mut fx = Fx::new();
        let mut p = pair0();
        p.first_leg = FirstLeg::Fixed;
        p.first_leg_ix = first;
        let tg = if open {
            target(A1, G7, &[(0, q("10")), (1, q("-10"))], ExecSpec::PairOnFill(p))
        } else {
            fx.legs(A1, G7, &["10", "-10"]);
            fx.spot(A1, A, G7, "10", "10");
            target(A1, G7, &[(0, q("0")), (1, q("0"))], ExecSpec::PairOnFill(p))
        };
        let o = one(&fx.go(&tg), G7);
        assert_eq!(o.intent.link.get().unwrap().hedge.reduce_only, want, "{row}");
    }
}
