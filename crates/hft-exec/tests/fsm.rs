//! EXE-060–EXE-076: state machine.
use crate::common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{BrokenReason, ExecUpdateKind, MarginLevel, StuckReason, TargetOrigin, TargetState};
use hft_types::{CapSource, RejectBits, Side, Tsc};

#[test]
fn exe_060_idle_to_working() {
    let mut fx = Fx::new();
    let (res, r) = fx.set(I1, &t1("-10.0"));
    res.unwrap();
    let d = r.dec(1)[0];
    assert_eq!((d.state_from, d.state_to), (1, 2));
    assert_eq!(fx.ex.view(A1, G8).unwrap().state, TargetState::Working);
}

#[test]
fn exe_061_working_to_idle_done() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10.0"));
    fx.cm(A1, G8).view.filled[0] = q("-9.9");
    fx.legs(A1, G8, &["-9.9"]);
    fx.touch(P);
    let r = fx.adv();
    assert_eq!(r.states(), vec![(2, 1, 0)]);
    assert_eq!(
        r.upd(),
        vec![ExecUpdateKind::Done {
            cleared_by_engine: false
        }]
    );
    assert!(r.orders.is_empty());
}

#[test]
fn exe_062_inflight_blocks_done() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10.0"));
    fx.legs(A1, G8, &["-9.9"]);
    fx.infl(A1, G8, 0, "-0.1");
    fx.touch(P);
    let r = fx.adv();
    assert!(r.decisions.is_empty() && r.notify.is_empty() && r.orders.is_empty());
    assert_eq!(fx.cell(A1, G8).view.state, TargetState::Working);
}

#[test]
fn exe_063_clear_target_to_idle() {
    let mut fx = Fx::new();
    let r = fx.go(&t1("-10.0"));
    fx.placed(&r.orders[0]);
    let r = fx.clear(A1, G8);
    let d = r.dec(3)[0];
    assert_eq!((d.state_from, d.state_to), (2, 1));
    assert!(r.cancels.is_empty());
    assert!(!fx.cell(A1, G8).has_target);
}

#[test]
fn exe_064_gated_to_stuck_boundary() {
    for (row, at, stuck) in [
        ("EXE-064.1", 1999u64, false),
        ("EXE-064.2", 2000, true),
        ("EXE-064.3", 2001, true),
    ] {
        let mut fx = Fx::new();
        let r = fx.go(&t1("-10.0"));
        fx.place_res(&r.orders[0], PlaceResult::Rejected(RejectBits::RATE_BACKOFF));
        assert_eq!(fx.cell(A1, G8).gated_since, t(0).max(Tsc(0)));
        fx.at(at);
        let mut r = fx.adv();
        let o = r.orders[0];
        r.extend(fx.place_res(&o, PlaceResult::Rejected(RejectBits::RATE_BACKOFF)));
        if stuck {
            assert!(
                r.decisions.iter().any(|d| d.action == 6
                    && d.state_to == 3
                    && d.detail == pack(2, 1, 0)
                    && d.gate == RejectBits::RATE_BACKOFF.0),
                "{row}"
            );
            assert!(
                r.upd().contains(&ExecUpdateKind::Stuck(StuckReason::Gated(
                    RejectBits::RATE_BACKOFF
                ))),
                "{row}"
            );
        } else {
            assert_eq!(fx.cell(A1, G8).view.state, TargetState::Working, "{row}");
            assert!(r.states().is_empty(), "{row}");
        }
    }
}

#[test]
fn exe_065_immediate_stuck_reasons() {
    // .1 Shadowed.
    let mut fx = Fx::new();
    let r = fx.go(&t1("-10.0"));
    let r = fx.place_res(&r.orders[0], PlaceResult::Shadowed);
    assert!(r.states().contains(&(2, 3, pack(2, 2, 0))), "EXE-065.1");
    // .2 external Liquidation.
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10.0"));
    let (_, mut r) = fx.ext(
        A1,
        &xfill(
            G8,
            0,
            P,
            B,
            Side::Buy,
            "1",
            hft_strategy_sdk::ExternalCause::Liquidation,
        ),
    );
    r.extend(fx.adv());
    assert!(r.states().contains(&(2, 3, pack(2, 6, 0))), "EXE-065.2");
    // .3 EngineOverride.
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10.0"));
    fx.legs(A1, G8, &["-10.0"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert!(r.states().contains(&(2, 3, pack(2, 7, 0))), "EXE-065.3");
    // .4 three exchange rejects.
    let mut fx = Fx::new();
    let mut all = Rec::default();
    let mut r = fx.go(&t1("-10.0"));
    for k in 0..3 {
        let rf = fx.placed(&r.orders[0]);
        all.extend(fx.exch_reject(rf, G8));
        fx.at(300 * (k + 1));
        r = fx.adv();
        if k < 2 {
            assert_eq!(r.orders.len(), 1);
        }
    }
    assert!(all.states().contains(&(2, 3, pack(2, 5, 0))), "EXE-065.4");
}

#[test]
fn exe_066_stuck_back_to_working() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10.0"));
    fx.cm(A1, G8).view.state = TargetState::Stuck(StuckReason::StaleQuote);
    fx.touch(P);
    let r = fx.adv();
    assert!(r.states().contains(&(3, 2, 0)));
    assert_eq!(r.dec(5).len(), 1);
}

#[test]
fn exe_067_slice_fails_needs_replace() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10.0"));
    {
        let c = fx.cm(A1, G8);
        c.view.state = TargetState::Stuck(StuckReason::SliceFails);
        c.fail_streak = 3;
    }
    let (res, r) = fx.set(I1, &t1("-10.0"));
    assert_eq!(res, Ok(()));
    assert!(r.decisions.is_empty());
    let c = fx.cell(A1, G8);
    assert_eq!(
        (c.view.state, c.fail_streak),
        (TargetState::Stuck(StuckReason::SliceFails), 3)
    );
    let mut tg = t1("-10.0");
    tg.tag = 0x11;
    let (_, r) = fx.set(I1, &tg);
    assert_eq!(r.dec(2).len(), 1);
    let c = fx.cell(A1, G8);
    assert_eq!((c.view.state, c.fail_streak), (TargetState::Working, 0));
}

/// g7 Working with an in-flight pair slice whose link is L1.
fn pair_in_flight(fx: &mut Fx) {
    let r = fx.go(&t0("10", "-10"));
    let o = r.orders[0];
    let rf = fx.placed(&o);
    let mut l = link(1, A1, G7, LinkPhase::Open, LinkBrokenCause::None);
    l.first = rf;
    fx.link(&l);
}

#[test]
fn exe_068_link_broken_causes() {
    let rows = [
        ("EXE-068.1", LinkBrokenCause::RetryExhausted, pack(3, 1, 1)),
        ("EXE-068.2", LinkBrokenCause::Deadline, pack(3, 1, 2)),
        ("EXE-068.3", LinkBrokenCause::Overfill, pack(3, 1, 4)),
        ("EXE-068.4", LinkBrokenCause::ResumeTimeout, pack(3, 1, 6)),
        ("EXE-068.5", LinkBrokenCause::ReverseFailed, pack(3, 2, 0)),
        ("EXE-068.6", LinkBrokenCause::HedgeLimit, pack(3, 4, 0)),
    ];
    for (row, cause, detail) in rows {
        let mut fx = Fx::new();
        pair_in_flight(&mut fx);
        let r = fx.link(&link(1, A1, G7, LinkPhase::Broken, cause));
        assert!(r.states().iter().any(|s| s.1 == 4 && s.2 == detail), "{row}");
        if row == "EXE-068.1" {
            assert_eq!(
                r.broken,
                vec![BrokenReq {
                    account: A1,
                    group: G7
                }]
            );
            let n = r.notes_n(note::EXEC_BROKEN);
            assert_eq!(n.len(), 1);
            assert_eq!(n[0].level, NoteLevel::P0);
            assert!(
                r.upd()
                    .contains(&ExecUpdateKind::Broken(BrokenReason::LinkBroken))
            );
        }
    }
}

#[test]
fn exe_069_capset_broken_from_any_state() {
    for row in ["EXE-069.1", "EXE-069.2", "EXE-069.3", "EXE-069.4"] {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["0.5", "0"]);
        match row {
            "EXE-069.2" => inject(&mut fx, &t0("10", "-10")),
            "EXE-069.3" => {
                inject(&mut fx, &t0("10", "-10"));
                fx.cm(A1, G7).view.state = TargetState::Stuck(StuckReason::Gated(RejectBits::RATE));
            }
            "EXE-069.4" => {
                let c = fx.cm(A1, G7);
                c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
                c.broken_since = t(1000);
                c.auto_fix.phase = AutoFixPhase::Primed as u8;
                c.auto_fix.due_tsc = t(121_000);
            }
            _ => {}
        }
        fx.at(5000);
        fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
        let r = fx.adv();
        let c = fx.cell(A1, G7);
        assert!(matches!(c.view.state, TargetState::Broken(_)), "{row}");
        if row == "EXE-069.4" {
            assert_eq!(
                (c.broken_since, c.auto_fix.due_tsc),
                (t(1000), t(121_000)),
                "{row}"
            );
            assert!(r.states().is_empty(), "{row}");
            assert!(r.timer_set(ExecTimerKind::AutoFix).is_empty(), "{row}");
        } else {
            assert_eq!(c.broken_since, t(5000), "{row}");
            assert_eq!(
                (c.auto_fix.phase, c.auto_fix.due_tsc),
                (AutoFixPhase::Primed as u8, t(125_000)),
                "{row}"
            );
            assert_eq!(r.timer_set(ExecTimerKind::AutoFix), vec![t(125_000)], "{row}");
        }
        if row == "EXE-069.2" {
            assert_eq!(fx.ex.view(A1, G7).unwrap().intent, t0("10", "-10"));
        }
    }
}

/// g7 with S +1.00, P −1.5: one HedgeFix (buy P 0.5) issued.
fn hedge_fix_issued(fx: &mut Fx) -> ExecOrder {
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    let r = fx.adv();
    r.ord(ExecOrderKind::HedgeFix)[0]
}

#[test]
fn exe_070_hedge_limit_to_broken() {
    let mut fx = Fx::new();
    let o = hedge_fix_issued(&mut fx);
    let r = fx.place_res(&o, PlaceResult::Rejected(RejectBits::HEDGE_LIMIT));
    assert!(r.states().contains(&(1, 4, pack(3, 4, 0))));
    let c = fx.cell(A1, G7);
    assert_eq!(c.auto_fix.phase, AutoFixPhase::Primed as u8);
    assert_eq!(c.fix.count, 0);
    assert!(r.notes_n(note::EXEC_ENGINE_ORDER_REJECTED).is_empty());
}

#[test]
fn exe_071_capcleared_to_idle() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
    fx.adv();
    fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G7));
    let r = fx.adv();
    assert!(r.states().contains(&(4, 1, 0)));
    assert!(
        r.dec(3)
            .iter()
            .any(|d| d.eflags == hft_exec::decision::eflags::CLEARED_BY_ENGINE)
    );
    assert_eq!(
        r.upd()
            .iter()
            .filter(|u| **u
                == ExecUpdateKind::Done {
                    cleared_by_engine: true
                })
            .count(),
        1
    );
    assert_eq!(
        fx.cell(A1, G7).auto_fix,
        <AutoFixState as zerocopy::FromZeros>::new_zeroed()
    );
    assert!(
        r.timers
            .iter()
            .any(|x| x.cancel && x.kind() == Some(ExecTimerKind::AutoFix))
    );
}

#[test]
fn exe_072_capcleared_keeps_engine_target() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["0.30", "-0.3"]);
    fx.spot(A1, A, G7, "0.30", "0.30");
    fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
    fx.adv();
    fx.flatten(A1);
    let r = fx.adv();
    for o in r.orders.clone() {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    fx.at(1000);
    fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G7));
    let r = fx.adv();
    assert!(r.states().contains(&(4, 2, 0)));
    let v = fx.ex.view(A1, G7).unwrap();
    assert_eq!(
        v.origin,
        TargetOrigin::Engine(hft_strategy_sdk::EngineOrigin::KillFlatten)
    );
    assert!(v.intent.legs().iter().all(|l| l.qty.0 == 0));
    assert!(
        r.orders
            .iter()
            .any(|o| o.kind == ExecOrderKind::ForceStep && o.intent.group == G7)
    );
}

#[test]
fn exe_073_clear_target_keeps_broken() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(120_000);
    }
    let before = fx.cell(A1, G7).auto_fix;
    let r = fx.clear(A1, G7);
    let d = r.dec(3)[0];
    assert_eq!((d.state_from, d.state_to), (4, 4));
    assert!(matches!(fx.cell(A1, G7).view.state, TargetState::Broken(_)));
    assert_eq!(fx.cell(A1, G7).auto_fix, before);
}

#[test]
fn exe_074_capcleared_non_broken_noop() {
    let mut fx = Fx::new();
    inject(&mut fx, &t1("-10"));
    fx.legs(A1, G8, &["-10"]);
    fx.cm(A1, G8).view.state = TargetState::Working;
    fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G8));
    let r = fx.adv();
    assert!(r.states().iter().all(|s| s.1 != 2 || s.0 != 4));
    assert!(fx.cell(A1, G8).has_target);
    assert!(!matches!(fx.cell(A1, G8).view.state, TargetState::Broken(_)));
}

#[test]
fn exe_075_broken_no_target_orders() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["2.00", "-1.0"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(BrokenReason::LinkBroken);
        c.broken_since = Tsc(1);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(120_000);
    }
    for at in [1000, 60_000] {
        fx.at(at);
        fx.touch(S);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.orders.is_empty() && r.decisions.is_empty());
    }
}

#[test]
fn exe_076_stuck_to_broken() {
    let mut fx = Fx::new();
    pair_in_flight(&mut fx);
    fx.cm(A1, G7).view.state = TargetState::Stuck(StuckReason::Gated(RejectBits::LINK_STALE));
    let r = fx.link(&link(1, A1, G7, LinkPhase::Broken, LinkBrokenCause::Deadline));
    assert!(r.states().contains(&(3, 4, pack(3, 1, 2))));
}
