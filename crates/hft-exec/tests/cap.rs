//! EXE-200: `hedge_cap_qty` of each order kind.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{MarginLevel, TargetState};

#[test]
fn exe_200_hedge_cap_qty() {
    // .1 HedgeFix P / 0.5.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    assert_eq!(
        fx.adv().ord(ExecOrderKind::HedgeFix)[0].hedge_cap_qty,
        q("0.6"),
        "EXE-200.1"
    );
    // .2 ResidualTrim S / 0.06.
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["10.06", "-10.0"]);
    fx.spot(A1, A, G7, "10.06", "10.06");
    fx.touch(S);
    assert_eq!(
        fx.adv().ord(ExecOrderKind::ResidualTrim)[0].hedge_cap_qty,
        q("0.07"),
        "EXE-200.2"
    );
    // .3 ExternalDefault P / 0.5.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.cm(A1, G7).ext_deadline = t(0);
    fx.at(1);
    fx.touch(P);
    let r = fx.adv();
    assert_eq!(
        r.ord(ExecOrderKind::ExternalDefault)[0].hedge_cap_qty,
        q("0.6"),
        "EXE-200.3"
    );
    // .4 CmdHedgeFix S / 0.30.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.30", "-1.0"]);
    fx.spot(A1, A, G7, "1.30", "1.30");
    let (res, r) = fx.cmd_fix(A1, G7);
    res.unwrap();
    assert_eq!(
        r.ord(ExecOrderKind::CmdHedgeFix)[0].hedge_cap_qty,
        q("0.31"),
        "EXE-200.4"
    );
    // .5 AutoFix P / 0.5.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(0);
    }
    fx.touch(P);
    assert_eq!(
        fx.adv().ord(ExecOrderKind::AutoFix)[0].hedge_cap_qty,
        q("0.6"),
        "EXE-200.5"
    );
    // .6 strategy orders.
    let mut fx = Fx::new();
    let r = fx.go(&t0("10", "-10"));
    assert!(r.orders.iter().all(|o| o.hedge_cap_qty.0 == 0), "EXE-200.6");
    let r = fx.go(&t1("-10"));
    assert!(r.orders.iter().all(|o| o.hedge_cap_qty.0 == 0), "EXE-200.6");
    // .7 ForceStep.
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    fx.perp(A1, B, P4, "-3");
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert!(r.ord(ExecOrderKind::ForceStep).len() >= 2, "EXE-200.7");
    assert!(r.orders.iter().all(|o| o.hedge_cap_qty.0 == 0), "EXE-200.7");
}
