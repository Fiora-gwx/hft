//! EXE-390–EXE-398: run modes, Drain and Unknown.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{MarginLevel, OrderUpdateKind, StuckReason, TargetState};
use hft_types::{CapSource, RejectBits, RunMode, Side};

#[test]
fn exe_390_shadow_target_flag() {
    let mut fx = Fx::new();
    fx.mode(A1, G8, RunMode::Shadow);
    let (res, r) = fx.set(I1, &t1("-10"));
    assert_eq!(res, Ok(()));
    assert_eq!(r.dec(1)[0].eflags, hft_exec::decision::eflags::SHADOW);
}

fn shadow_once(fx: &mut Fx) {
    let o = fx.go(&t1("-10")).orders[0];
    let r = fx.place_res(&o, PlaceResult::Shadowed);
    assert!(r.states().contains(&(2, 3, pack(2, 2, 0))));
}

#[test]
fn exe_391_shadow_one_slice() {
    let mut fx = Fx::new();
    fx.mode(A1, G8, RunMode::Shadow);
    shadow_once(&mut fx);
    let mut ms = 100;
    while ms <= 60_000 {
        fx.at(ms);
        fx.touch(P);
        assert!(fx.adv().orders.is_empty(), "EXE-391.2");
        ms += 100;
    }
    let mut fx = Fx::new();
    fx.mode(A1, G8, RunMode::Shadow);
    shadow_once(&mut fx);
    fx.at(1000);
    let r = fx.go(&t1("-12"));
    assert_eq!(r.orders.len(), 1, "EXE-391.3");
    let r = fx.place_res(&r.orders[0], PlaceResult::Shadowed);
    assert!(r.states().contains(&(2, 3, pack(2, 2, 0))), "EXE-391.3");
    let mut fx = Fx::new();
    fx.mode(A1, G8, RunMode::Shadow);
    shadow_once(&mut fx);
    fx.mode(A1, G8, RunMode::Live);
    fx.at(1000);
    let now = fx.now();
    fx.ex
        .on_mode(A1, G8, RunMode::Shadow, RunMode::Live, now, &mut fx.out);
    let r = Rec::take(&mut fx.out);
    assert!(r.states().contains(&(3, 2, 0)), "EXE-391.4");
    assert_eq!(fx.adv().orders.len(), 1, "EXE-391.4");
}

#[test]
fn exe_392_open_not_allowed_once() {
    let mut fx = Fx::new();
    shadow_once(&mut fx);
    let mut ms = 100;
    while ms <= 10_000 {
        fx.at(ms);
        fx.touch(P);
        assert!(fx.adv().orders.is_empty());
        ms += 100;
    }
    assert_eq!(fx.log.orders.len(), 1);
    assert_eq!(
        fx.cell(A1, G8).view.state,
        TargetState::Stuck(StuckReason::Shadow)
    );
}

#[test]
fn exe_393_close_only() {
    let mut fx = Fx::new();
    fx.mode(A1, G8, RunMode::CloseOnly);
    assert!(fx.go(&t1("-10")).orders.is_empty(), "EXE-393.1");
    fx.at(2000);
    fx.touch(P);
    let r = fx.adv();
    let d = r
        .decisions
        .iter()
        .find(|d| d.action == 6 && d.state_to == 3)
        .unwrap();
    assert_eq!(d.detail, pack(2, 1, 0), "EXE-393.1");
    assert_ne!(d.gate & RejectBits::CLOSE_ONLY.0, 0, "EXE-393.1");
    let mut fx = Fx::new();
    fx.mode(A1, G7, RunMode::CloseOnly);
    assert!(fx.go(&t0("10", "-10")).orders.is_empty(), "EXE-393.2");
    let mut fx = Fx::new();
    fx.mode(A1, G8, RunMode::CloseOnly);
    fx.legs(A1, G8, &["-10"]);
    let o = fx.go(&t1("-5")).orders[0];
    assert_eq!((o.intent.side, o.intent.qty), (Side::Buy, q("5.0")), "EXE-393.3");
}

#[test]
fn exe_394_soft_kill() {
    let base = || {
        let mut fx = Fx::new();
        fx.risk(A1, |r| r.open_blocked = true);
        fx
    };
    let mut fx = base();
    assert!(fx.go(&t1("-10")).orders.is_empty(), "EXE-394.1");
    fx.at(2000);
    fx.touch(P);
    assert!(
        fx.adv()
            .states()
            .iter()
            .any(|s| s.1 == 3 && (s.2 & 0xFFFF) == pack(2, 1, 0)),
        "EXE-394.1"
    );
    let mut fx = base();
    fx.legs(A1, G8, &["-10"]);
    assert_eq!(fx.go(&t1("-5")).orders.len(), 1, "EXE-394.2");
    let mut fx = base();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    assert_eq!(fx.adv().ord(ExecOrderKind::HedgeFix).len(), 1, "EXE-394.3");
    let mut fx = base();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(0);
    }
    fx.touch(P);
    assert!(fx.adv().has_dec(14, pack(4, 1, 0)), "EXE-394.4");
    let mut fx = base();
    fx.legs(A1, G8, &["-10"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    assert_eq!(fx.adv().ord(ExecOrderKind::ForceStep).len(), 1, "EXE-394.5");
}

#[test]
fn exe_395_draining() {
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10")).orders[0];
    fx.place_res(&o, PlaceResult::Rejected(RejectBits::DRAINING));
    assert_eq!(fx.cell(A1, G8).fail_streak, 0, "EXE-395.1");
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    assert_eq!(fx.go(&t1("-5")).orders.len(), 1, "EXE-395.2");
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    assert_eq!(fx.adv().ord(ExecOrderKind::HedgeFix).len(), 1, "EXE-395.3");
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.cm(A1, G7).ext_deadline = t(0);
    fx.at(1);
    fx.touch(P);
    assert_eq!(fx.adv().ord(ExecOrderKind::ExternalDefault).len(), 1, "EXE-395.4");
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    fx.flatten(A1);
    assert_eq!(fx.adv().ord(ExecOrderKind::ForceStep).len(), 1, "EXE-395.5");
    assert!(fx.ex.acct_state(A1).unwrap().flatten_lock, "EXE-395.5");
}

#[test]
fn exe_396_drain_ready_acct() {
    type Setup = fn(&mut Fx);
    let rows: [(&str, Setup, bool); 12] = [
        ("EXE-396.1", |_| {}, true),
        (
            "EXE-396.2",
            |fx| {
                fx.go(&t1("-10"));
            },
            false,
        ),
        (
            "EXE-396.3",
            |fx| {
                fx.legs(A1, G7, &["1.00", "-1.5"]);
                fx.leg_adjust(A1, G7);
                let r = fx.adv();
                let o = r.ord(ExecOrderKind::HedgeFix)[0];
                let rf = fx.placed(&o);
                fx.nofill(rf, G7);
            },
            false,
        ),
        ("EXE-396.4", |fx| fx.cm(A1, G7).ext_deadline = t(5000), false),
        (
            "EXE-396.5",
            |fx| fx.cm(A1, G7).auto_fix.phase = AutoFixPhase::Running as u8,
            false,
        ),
        (
            "EXE-396.6",
            |fx| {
                fx.legs(A1, G8, &["-10"]);
                fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
                fx.force(A1, B, "0.2");
                let r = fx.adv();
                let rf = fx.placed(&r.orders[0]);
                fx.nofill(rf, G8);
            },
            false,
        ),
        (
            "EXE-396.7",
            |fx| {
                fx.legs(A1, G8, &["-10"]);
                fx.quote_down(P);
                fx.flatten(A1);
                fx.adv();
            },
            false,
        ),
        (
            "EXE-396.8",
            |fx| {
                fx.perp(A1, B, P4, "-3");
                fx.quote_down(P4);
                fx.flatten(A1);
                fx.adv();
            },
            false,
        ),
        (
            "EXE-396.9",
            |fx| {
                inject(fx, &t0("6", "-6"));
                let c = fx.cm(A1, G7);
                c.view.origin =
                    hft_strategy_sdk::TargetOrigin::Engine(hft_strategy_sdk::EngineOrigin::MarginStress);
                c.view.intent.reason = hft_strategy_sdk::TargetReason::Hard;
            },
            false,
        ),
        (
            "EXE-396.10",
            |fx| {
                inject(fx, &t1("-10"));
            },
            true,
        ),
        (
            "EXE-396.11",
            |fx| {
                let c = fx.cm(A1, G7);
                c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
                c.auto_fix.phase = AutoFixPhase::Primed as u8;
                c.auto_fix.due_tsc = t(100_000);
            },
            true,
        ),
        (
            "EXE-396.12",
            |fx| {
                fx.legs(A1, G8, &["-10"]);
                fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
                fx.force(A1, B, "0.2");
                let r = fx.adv();
                let rf = fx.placed(&r.orders[0]);
                fx.legs(A1, G8, &["-8"]);
                fx.fill(rf, G8, "2", true);
                fx.adv();
            },
            true,
        ),
    ];
    for (row, setup, want) in rows {
        let mut fx = Fx::new();
        setup(&mut fx);
        assert_eq!(fx.ex.drain_ready_acct(A1), want, "{row}");
        assert_eq!(fx.ex.drain_ready(), want, "{row}");
    }
    let mut fx = Fx::new();
    fx.set(I2, &taker(A2, hft_types::GroupId(0x0200_0009), &[(0, "1")]))
        .0
        .ok();
    let g9 = gspec(G9, &[(P, B)]);
    let mut fx2 = Fx::with(c0(), vec![g7(), g8(), g9]);
    fx2.set(I2, &taker(A2, G9, &[(0, "-10")])).0.unwrap();
    fx2.adv();
    assert!(fx2.ex.drain_ready_acct(A1), "EXE-396.13");
    assert!(!fx2.ex.drain_ready(), "EXE-396.13");
    let _ = fx.ex.drain_ready();
}

#[test]
fn exe_397_drain_with_flatten() {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-20.0"]);
    fx.flatten(A1);
    let o = fx.adv().ord(ExecOrderKind::ForceStep)[0];
    assert_eq!(o.intent.qty, q("9.9"));
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["-10.1"]);
    fx.fill(rf, G8, "9.9", true);
    fx.at(100);
    assert!(!fx.ex.drain_ready());
    fx.at(200);
    fx.touch(P);
    let o = fx.adv().ord(ExecOrderKind::ForceStep)[0];
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["-0.2"]);
    fx.fill(rf, G8, "9.9", true);
    fx.at(400);
    fx.touch(P);
    let o = fx.adv().ord(ExecOrderKind::ForceStep)[0];
    assert_eq!(o.intent.qty, q("0.2"));
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["0"]);
    fx.fill(rf, G8, "0.2", true);
    fx.adv();
    assert!(fx.ex.drain_ready());
}

#[test]
fn exe_398_unknown_wait() {
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10")).orders[0];
    let rf = fx.placed(&o);
    fx.upd(rf, G8, OrderUpdateKind::Unknown);
    fx.infl(A1, G8, 0, "-10");
    let mut ms = 1000;
    while ms <= 60_000 {
        fx.at(ms);
        fx.touch(P);
        assert!(fx.adv().orders.is_empty(), "EXE-398.1");
        ms += 1000;
    }
    fx.infl(A1, G8, 0, "0");
    fx.legs(A1, G8, &["-4.0"]);
    fx.fill(rf, G8, "4.0", true);
    let r = fx.adv();
    assert_eq!(r.orders.len(), 1, "EXE-398.2");
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.lv(A1, G7, |l| l.has_unknown = true);
    fx.leg_adjust(A1, G7);
    assert!(fx.adv().ord(ExecOrderKind::HedgeFix).is_empty(), "EXE-398.3");
    let _ = CapSource::KILL;
}
