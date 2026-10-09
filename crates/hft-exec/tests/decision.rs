//! EXE-415, EXE-416, EXE-419, EXE-420: decisions and progress notifications.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{
    BrokenReason, EngineOrigin, ExecUpdateKind, ExternalCause, MarginLevel, StuckReason, TargetOrigin,
    TargetState,
};
use hft_types::{RejectBits, Side};

#[test]
fn exe_415_no_decision_on_noop() {
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10")).orders[0];
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["-4"]);
    fx.fill(rf, G8, "4", true);
    fx.adv();
    for k in 0..100u64 {
        fx.at(1 + k);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.decisions.is_empty() && r.notify.is_empty());
    }
}

#[test]
fn exe_416_exec_update() {
    // .1 / .3
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10")).orders[0];
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["-4"]);
    let r = fx.fill(rf, G8, "4.0", true);
    let p: Vec<_> = r
        .notify
        .iter()
        .filter(|n| matches!(n.u.kind, ExecUpdateKind::Progress { .. }))
        .collect();
    assert_eq!(p.len(), 1, "EXE-416.1");
    assert_eq!(
        (p[0].u.kind, p[0].u.origin),
        (
            ExecUpdateKind::Progress {
                leg_ix: 0,
                d: q("-4.0")
            },
            TargetOrigin::Strategy
        ),
        "EXE-416.1"
    );
    fx.legs(A1, G8, &["-10"]);
    fx.touch(P);
    fx.at(300);
    let r = fx.adv();
    assert_eq!(
        r.upd()
            .iter()
            .filter(|u| **u
                == ExecUpdateKind::Done {
                    cleared_by_engine: false
                })
            .count(),
        1,
        "EXE-416.3"
    );
    fx.at(600);
    fx.touch(P);
    assert!(fx.adv().notify.is_empty(), "EXE-416.3");
    // .2
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10")).orders[0];
    let rf = fx.placed(&o);
    let r = fx.nofill(rf, G8);
    assert!(
        r.notify
            .iter()
            .all(|n| !matches!(n.u.kind, ExecUpdateKind::Progress { .. })),
        "EXE-416.2"
    );
    // .4 / .5
    let mut fx = Fx::new();
    fx.stale(P, 3000);
    fx.go(&t1("-10"));
    fx.at(2000);
    fx.touch(P);
    let r = fx.adv();
    assert_eq!(
        r.upd(),
        vec![ExecUpdateKind::Stuck(StuckReason::StaleQuote)],
        "EXE-416.4"
    );
    fx.at(2500);
    fx.touch(P);
    assert!(fx.adv().notify.is_empty(), "EXE-416.4");
    fx.quote_up(P);
    fx.at(3000);
    fx.touch(P);
    let o = fx.adv().orders[0];
    let r = fx.place_res(&o, PlaceResult::Rejected(RejectBits::RATE));
    assert!(
        r.upd()
            .contains(&ExecUpdateKind::Stuck(StuckReason::Gated(RejectBits::RATE))),
        "EXE-416.5"
    );
    // .6
    let mut fx = Fx::new();
    let o = fx.go(&t0("10", "-10")).orders[0];
    let rf = fx.placed(&o);
    let mut l = link(1, A1, G7, LinkPhase::Broken, LinkBrokenCause::Deadline);
    l.first = rf;
    let r = fx.link(&l);
    assert!(
        r.upd()
            .contains(&ExecUpdateKind::Broken(BrokenReason::LinkBroken)),
        "EXE-416.6"
    );
    // .7
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(0);
    }
    fx.touch(P);
    let o = fx.adv().ord(ExecOrderKind::AutoFix)[0];
    let rf = fx.placed(&o);
    fx.legs(A1, G7, &["1.00", "-1.2"]);
    let r = fx.fill(rf, G7, "0.3", true);
    let p = r
        .notify
        .iter()
        .find(|n| matches!(n.u.kind, ExecUpdateKind::Progress { .. }))
        .unwrap();
    assert_eq!(
        (p.u.kind, p.u.origin),
        (
            ExecUpdateKind::Progress {
                leg_ix: 1,
                d: q("0.3")
            },
            TargetOrigin::Engine(EngineOrigin::AutoFix)
        ),
        "EXE-416.7"
    );
}

fn timed(fx: &mut Fx) {
    inject(fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["1.00", "-0.5"]);
    fx.spot(A1, A, G7, "1", "1");
    fx.at(1000);
    let (_, r) = fx.ext(
        A1,
        &xfill(G7, 1, P, B, Side::Buy, "0.5", ExternalCause::Liquidation),
    );
    assert!(r.has_dec(9, pack(6, 1, 1)));
    fx.adv();
}

#[test]
fn exe_419_ext_timing_end_records() {
    let cleared = pack(6, 0, 5);
    let mut fx = Fx::new();
    timed(&mut fx);
    fx.at(6000);
    fx.touch(S);
    assert_eq!(fx.adv().dec(10).len(), 1, "EXE-419.1");
    let mut fx = Fx::new();
    timed(&mut fx);
    let mut tg = t0("10", "-10");
    tg.tag = 9;
    assert!(fx.set(I1, &tg).1.has_dec(9, cleared), "EXE-419.2");
    let mut fx = Fx::new();
    timed(&mut fx);
    assert!(fx.clear(A1, G7).has_dec(9, cleared), "EXE-419.3");
    let mut fx = Fx::new();
    timed(&mut fx);
    let now = fx.now();
    fx.ex.note_strategy_action(A1, G7, now, &mut fx.out);
    assert!(Rec::take(&mut fx.out).has_dec(9, cleared), "EXE-419.4");
    let mut fx = Fx::new();
    timed(&mut fx);
    let (_, r) = fx.ext(A1, &xfill(G7, 1, P, B, Side::Buy, "0.1", ExternalCause::Manual));
    assert!(r.has_dec(9, cleared), "EXE-419.5");
    let mut fx = Fx::new();
    timed(&mut fx);
    let now = fx.now();
    fx.ex.clear_account(A1, now, &mut fx.out);
    assert!(fx.adv().has_dec(9, cleared), "EXE-419.6");
}

#[test]
fn exe_420_engine_target_records() {
    let step = pack(5, 1, 0);
    // .1 L3 set → reached.
    let mut fx = Fx::new();
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.legs(A1, G8, &["-10"]);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert!(r.has_dec(11, step), "EXE-420.1");
    let rf = fx.placed(&r.orders[0]);
    fx.legs(A1, G8, &["-8"]);
    fx.fill(rf, G8, "2", true);
    assert!(fx.adv().states().contains(&(2, 1, 0)), "EXE-420.1");
    // .2 L3 set → L3 release.
    let mut fx = Fx::new();
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.legs(A1, G8, &["-10"]);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let rf = fx.placed(&r.orders[0]);
    fx.nofill(rf, G8);
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    assert_eq!(fx.adv().dec(3).len(), 1, "EXE-420.2");
    // .3 flatten → Exit replaces it.
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    fx.flatten(A1);
    assert!(fx.adv().has_dec(11, step), "EXE-420.3");
    let mut tg = t1("0");
    tg.reason = hft_strategy_sdk::TargetReason::Exit;
    assert_eq!(fx.set(I1, &tg).1.dec(2).len(), 1, "EXE-420.3");
    // .4 ungrouped → reached.
    let mut fx = Fx::new();
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.perp(A1, B, P4, "-3");
    fx.force(A1, B, "1");
    let r = fx.adv();
    assert!(
        r.decisions
            .iter()
            .any(|d| d.group == GNONE && d.detail == pack(5, 4, 0)),
        "EXE-420.4"
    );
    let o = r
        .orders
        .iter()
        .find(|o| o.intent.group == GNONE)
        .copied()
        .unwrap();
    let rf = fx.placed(&o);
    fx.perp(A1, B, P4, "0");
    fx.fill(rf, GNONE, "3", true);
    let r = fx.adv();
    assert!(
        r.decisions
            .iter()
            .any(|d| d.group == GNONE && d.action == 6 && d.state_to == 1),
        "EXE-420.4"
    );
    // .5 HardAlign → reached.
    let mut fx = Fx::new();
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.legs(A1, G7, &["10", "-6"]);
    fx.spot(A1, A, G7, "10", "10");
    fx.quote_down(S);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    for o in r.orders {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    fx.at(500);
    assert!(fx.adv().has_dec(11, pack(5, 6, 0)), "EXE-420.5");
    fx.legs(A1, G7, &["6", "-6"]);
    fx.at(1000);
    fx.touch(S);
    assert!(fx.adv().states().iter().any(|s| s.1 == 1), "EXE-420.5");
}
