//! EXE-361–EXE-367: `engine_flatten` and `flatten_lock`.
use crate::common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{FirstLeg, MarginLevel, OrderUpdateKind, TargetReason, TargetReject, TargetState};
use hft_types::{CapSource, RejectBits, Side};

#[test]
fn exe_361_engine_flatten_all() {
    let g9 = gspec(G9, &[(P, B)]);
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G11, &[(P, B), (Q, C)]), g9]);
    fx.legs(A1, G7, &["10.00", "-10.0"]);
    fx.spot(A1, A, G7, "10", "10");
    fx.legs(A1, G11, &["-2.0", "2.5"]);
    fx.cm(A1, G11).view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    fx.perp(A1, B, P4, "-1.0");
    fx.flatten(A1);
    let r = fx.adv();
    for g in [G7, G8, G11] {
        let d = r
            .decisions
            .iter()
            .find(|d| d.group == g && d.action == 11 && d.detail == pack(5, 1, 0))
            .unwrap();
        assert_eq!((d.reason, d.origin), (4, 5));
        let c = fx.cell(A1, g);
        if g != G8 {
            assert!(c.view.intent.legs().iter().all(|l| l.qty.0 == 0));
        }
    }
    assert!(
        r.decisions
            .iter()
            .any(|d| d.group == GNONE && d.detail == pack(5, 4, 0))
    );
    let a = acct_row(&fx.snap(), A1).unwrap();
    assert_eq!((a.flatten_lock, a.force_kind), (1, 0));
    assert!(r.decisions.iter().all(|d| d.account != A2));
    let inflight: Vec<_> = r.orders.clone();
    for o in &inflight {
        fx.placed(o);
    }
    fx.flatten(A1);
    let r2 = fx.adv();
    assert!(r2.orders.is_empty());
    let a = acct_row(&fx.snap(), A1).unwrap();
    assert_eq!(a.flatten_lock, 1);
    for g in [G7, G11] {
        assert!(fx.cell(A1, g).view.intent.legs().iter().all(|l| l.qty.0 == 0));
    }
}

#[test]
fn exe_362_flatten_algo() {
    // .1
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["3.00", "-3.0"]);
    fx.spot(A1, A, G7, "3", "3");
    fx.flatten(A1);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G7).copied().unwrap();
    assert_eq!((o.intent.inst, o.intent.side), (P, Side::Buy), "EXE-362.1");
    assert!(o.intent.link.get().is_some(), "EXE-362.1");
    assert_eq!(
        r.dec(5).into_iter().find(|d| d.group == G7).unwrap().first_leg_ix,
        1,
        "EXE-362.1"
    );
    // .2
    let rf = fx.placed(&o);
    let mut l = link(1, A1, G7, LinkPhase::Open, LinkBrokenCause::None);
    l.first = rf;
    fx.link(&l);
    fx.cm(A1, G7).hold |= LINK_HOLD;
    fx.at(1000);
    fx.touch(P);
    assert!(fx.adv().has_dec(11, pack(5, 2, 5)), "EXE-362.2");
    // .3
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["3.00", "-3.0"]);
    fx.spot(A1, A, G7, "3", "3");
    fx.cm(A1, G7).view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    fx.flatten(A1);
    let r = fx.adv();
    let o: Vec<_> = r
        .orders
        .iter()
        .filter(|o| o.intent.group == G7)
        .copied()
        .collect();
    assert_eq!(o.len(), 1, "EXE-362.3");
    assert!(
        o[0].intent.inst == P && o[0].intent.reduce_only() && o[0].intent.link.get().is_none(),
        "EXE-362.3"
    );
    let rf = fx.placed(&o[0]);
    fx.legs(A1, G7, &["3.00", "0"]);
    fx.fill(rf, G7, "3.0", true);
    fx.at(1000);
    fx.touch(S);
    let r = fx.adv();
    assert!(
        r.orders
            .iter()
            .any(|o| o.intent.inst == S && o.intent.side == Side::Sell),
        "EXE-362.3"
    );
    // .4
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["0.30", "-0.3"]);
    fx.spot(A1, A, G7, "0.3", "0.3");
    fx.cm(A1, G7).view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    fx.flatten(A1);
    let r = fx.adv();
    for o in r.orders {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G7));
    fx.at(1000);
    let r = fx.adv();
    assert!(r.states().contains(&(4, 2, 0)), "EXE-362.4");
    assert!(
        fx.cell(A1, G7).view.intent.legs().iter().all(|l| l.qty.0 == 0),
        "EXE-362.4"
    );
    assert!(r.orders.iter().any(|o| o.intent.group == G7), "EXE-362.4");
}

#[test]
fn exe_363_hard_stops_flatten() {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10.0"]);
    fx.flatten(A1);
    fx.risk(A1, |r| r.hard = true);
    let mut stuck = Vec::new();
    let mut ms = 0;
    while ms <= 5000 {
        fx.at(ms);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.orders.iter().all(|o| o.intent.group != G8));
        for d in r
            .decisions
            .iter()
            .filter(|d| d.group == G8 && d.action == 6 && d.state_to == 3)
        {
            stuck.push((ms, d.detail, d.gate));
        }
        for n in r.notes_n(note::EXEC_STUCK) {
            assert_eq!((ms, n.level), (1000, NoteLevel::P1));
        }
        ms += 500;
    }
    assert_eq!(stuck, vec![(1000, pack(2, 1, 0), RejectBits::HARD.0)]);
    assert!(fx.cell(A1, G8).view.intent.legs().iter().all(|l| l.qty.0 == 0));
}

#[test]
fn exe_364_lock_accept() {
    let flatten_g8 = |net: &str| {
        let mut fx = Fx::new();
        fx.legs(A1, G8, &[net]);
        fx.flatten(A1);
        fx.adv();
        fx
    };
    let mut fx = flatten_g8("-5.0");
    let (res, r) = fx.set(I1, &t1("-3.0"));
    assert_eq!(res, Err(TargetReject::FlattenLocked), "EXE-364.1");
    assert!(r.has_dec(4, pack(1, 9, 0)), "EXE-364.1");
    let mut fx = flatten_g8("-5.0");
    for o in fx.log.orders.clone() {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    let mut tg = t1("0");
    tg.reason = TargetReason::Exit;
    assert_eq!(fx.set(I1, &tg).0, Ok(()), "EXE-364.2");
    fx.at(1000);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G8).unwrap();
    assert_eq!(o.origin, ExecOrigin::Engine, "EXE-364.2");
    assert_eq!(
        r.dec(5).into_iter().find(|d| d.group == G8).unwrap().urgency,
        3,
        "EXE-364.2"
    );
    let mut fx = flatten_g8("0");
    assert_eq!(
        fx.set(I1, &t1("-1.0")).0,
        Err(TargetReject::FlattenLocked),
        "EXE-364.3"
    );
    let mut fx = flatten_g8("0");
    fx.legs(A1, G8, &["-2.0"]);
    assert_eq!(fx.set(I1, &t1("-1.5")).0, Ok(()), "EXE-364.4");
    let mut fx = flatten_g8("0");
    fx.legs(A1, G8, &["-2.0"]);
    assert_eq!(
        fx.set(I1, &t1("1.0")).0,
        Err(TargetReject::FlattenLocked),
        "EXE-364.5"
    );
    assert_eq!(
        fx.set(I1, &t1("-2.5")).0,
        Err(TargetReject::FlattenLocked),
        "EXE-364.6"
    );
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["3.00", "-3.0"]);
    fx.spot(A1, A, G7, "3", "3");
    fx.flatten(A1);
    let r = fx.adv();
    for o in r.orders {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    let mut pp = pair0();
    pp.first_leg = FirstLeg::SpotFirst;
    let mut tg = target(
        A1,
        G7,
        &[(0, q("0")), (1, q("0"))],
        hft_strategy_sdk::ExecSpec::PairOnFill(pp),
    );
    tg.reason = TargetReason::Exit;
    assert_eq!(fx.set(I1, &tg).0, Ok(()), "EXE-364.7");
    fx.at(1000);
    let r = fx.adv();
    assert_eq!(
        r.dec(5).into_iter().find(|d| d.group == G7).unwrap().first_leg_ix,
        1,
        "EXE-364.7"
    );
}

#[test]
fn exe_365_clear_ignored() {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-5.0"]);
    fx.flatten(A1);
    let r = fx.adv();
    for o in r.orders {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    let r = fx.clear(A1, G8);
    assert!(r.has_dec(4, pack(1, 9, 0)), "EXE-365.1");
    fx.at(1000);
    fx.touch(P);
    assert!(fx.adv().orders.iter().any(|o| o.intent.group == G8), "EXE-365.1");
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-5.0"]);
    fx.flatten(A1);
    fx.adv();
    let mut tg = t1("0");
    tg.reason = TargetReason::Exit;
    assert_eq!(fx.set(I1, &tg).0, Ok(()));
    let r = fx.clear(A1, G8);
    assert!(r.has_dec(4, pack(1, 9, 0)), "EXE-365.2");
    assert!(fx.cell(A1, G8).has_target, "EXE-365.2");
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-5.0"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    fx.adv();
    let r = fx.clear(A1, G8);
    assert!(r.has_dec(4, pack(1, 7, 0)), "EXE-365.3");
    assert!(fx.cell(A1, G8).has_target, "EXE-365.3");
}

#[test]
fn exe_366_lock_release() {
    type Act = fn(&mut Fx);
    let rows: [(&str, Act, u8); 6] = [
        (
            "EXE-366.1",
            |fx| fx.risk_ev(A1, cap_clr(CapSource::KILL, A1, GNONE)),
            0,
        ),
        (
            "EXE-366.2",
            |fx| fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G7)),
            1,
        ),
        (
            "EXE-366.3",
            |fx| fx.risk_ev(A1, cap_clr(CapSource::HEDGE_FAILURE, A1, GNONE)),
            1,
        ),
        (
            "EXE-366.4",
            |fx| fx.risk_ev(A2, cap_clr(CapSource::KILL, A2, GNONE)),
            1,
        ),
        (
            "EXE-366.5",
            |fx| {
                let now = fx.now();
                fx.ex.clear_account(A1, now, &mut fx.out);
            },
            1,
        ),
        (
            "EXE-366.6",
            |fx| {
                let s = fx.snap();
                let mut ex = fx.restore(&s);
                ex.recovery_end(t(10), t(5), 1_000_000_000_000, 1_000_000_000_000);
                fx.ex = ex;
            },
            1,
        ),
    ];
    for (row, act, want) in rows {
        let mut fx = Fx::new();
        fx.flatten(A1);
        fx.adv();
        act(&mut fx);
        assert_eq!(
            u8::from(fx.ex.acct_state(A1).unwrap().flatten_lock),
            want,
            "{row}"
        );
        if row == "EXE-366.1" {
            assert_eq!(fx.set(I1, &t1("-1")).0, Ok(()), "{row}");
        }
    }
}

#[test]
fn exe_367_l3_under_lock_zero() {
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-5.0"]);
    fx.flatten(A1);
    let r = fx.adv();
    for o in r.orders {
        let rf = fx.placed(&o);
        fx.upd(rf, o.intent.group, OrderUpdateKind::Cancelled);
    }
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert_eq!(fx.cell(A1, G8).target_of(0), Some(q("0")));
    let d = r
        .dec(11)
        .into_iter()
        .find(|d| d.group == G8 && d.detail == pack(5, 1, 0))
        .unwrap();
    assert_eq!(d.target_qty, q("0"));
}
