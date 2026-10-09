//! EXE-340–EXE-360, EXE-369: margin-stress default reduction.
mod common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{
    EngineOrigin, ExecUpdateKind, MarginLevel, OrderUpdateKind, StuckReason, TargetOrigin, TargetReason,
    TargetReject, TargetState, Urgency,
};
use hft_types::{Side, Tsc};

fn l3(fx: &mut Fx) {
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
}

fn tq(fx: &Fx, g: hft_types::GroupId, i: usize) -> hft_types::Qty {
    fx.cell(A1, g).target_of(i).expect("target leg")
}

#[test]
fn exe_340_proportional_target() {
    for (row, net, frac, want) in [
        ("EXE-340.1", "-10.0", "0.2", "-8.0"),
        ("EXE-340.2", "-7.3", "0.25", "-5.4"),
        ("EXE-340.3", "3.3", "0.5", "1.6"),
        ("EXE-340.4", "-10.0", "1", "0"),
    ] {
        let mut fx = Fx::new();
        l3(&mut fx);
        fx.legs(A1, G8, &[net]);
        fx.force(A1, B, frac);
        assert!(fx.out.is_empty(), "{row}");
        if row == "EXE-340.1" {
            let a = acct_row(&fx.snap(), A1).unwrap();
            assert_eq!(
                (a.force_active, a.force_kind, a.force_frac),
                (1, 1, rate("0.2")),
                "{row}"
            );
            assert_ne!(a.force_venues & (1 << B.0), 0, "{row}");
        }
        let r = fx.adv();
        assert_eq!(tq(&fx, G8, 0), q(want), "{row}");
        if row == "EXE-340.1" {
            let o = r.ord(ExecOrderKind::ForceStep)[0];
            assert_eq!((o.intent.side, o.intent.qty), (Side::Buy, q("2.0")), "{row}");
            let a = acct_row(&fx.snap(), A1).unwrap();
            assert_eq!((a.force_kind, a.force_active), (0, 1), "EXE-340.5");
        }
    }
}

#[test]
fn exe_341_compounding() {
    for (row, net) in [("EXE-341.1", "-9.5"), ("EXE-341.2", "-8.0")] {
        let mut fx = Fx::new();
        l3(&mut fx);
        fx.legs(A1, G8, &["-10.0"]);
        fx.force(A1, B, "0.2");
        fx.adv();
        fx.legs(A1, G8, &[net]);
        fx.force(A1, B, "0.2");
        fx.adv();
        assert_eq!(tq(&fx, G8, 0), q("-6.4"), "{row}");
    }
}

#[test]
fn exe_342_closer_to_zero() {
    for (row, cur, want) in [
        ("EXE-342.1", Some(("0", TargetReason::Exit)), "0"),
        ("EXE-342.2", Some(("-9.0", TargetReason::Rebalance)), "-8.0"),
        ("EXE-342.3", Some(("-5.0", TargetReason::Rebalance)), "-5.0"),
        ("EXE-342.4", Some(("2.0", TargetReason::Rebalance)), "0"),
        ("EXE-342.5", None, "-8.0"),
    ] {
        let mut fx = Fx::new();
        l3(&mut fx);
        fx.legs(A1, G8, &["-10.0"]);
        if let Some((x, reason)) = cur {
            let mut tg = t1(x);
            tg.reason = reason;
            inject(&mut fx, &tg);
        }
        fx.force(A1, B, "0.2");
        fx.adv();
        assert_eq!(tq(&fx, G8, 0), q(want), "{row}");
    }
}

#[test]
fn exe_343_venue_scope() {
    let groups = vec![
        g7(),
        g8(),
        gspec(G11, &[(P, B), (Q, C)]),
        gspec(G17, &[(Q, C)]),
        gspec(G15, &[(S, A)]),
    ];
    let mut fx = Fx::with(c0(), groups);
    l3(&mut fx);
    fx.legs(A1, G7, &["10.00", "-10.0"]);
    fx.legs(A1, G11, &["-2.0", "2.0"]);
    fx.legs(A1, G17, &["1.0"]);
    fx.legs(A1, G15, &["1.00"]);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert_eq!((tq(&fx, G7, 0), tq(&fx, G7, 1)), (q("8.00"), q("-8.0")));
    assert_eq!((tq(&fx, G11, 0), tq(&fx, G11, 1)), (q("-1.6"), q("1.6")));
    assert!(!fx.cell(A1, G17).has_target && !fx.cell(A1, G15).has_target);
    assert!(r.decisions.iter().all(|d| d.group != G17 && d.group != G15));
    for d in r.dec(11).iter().filter(|d| d.detail == pack(5, 1, 0)) {
        assert_eq!((d.reason, d.urgency, d.origin), (3, 3, 2));
    }
    assert_eq!(fx.cell(A1, G7).view.intent.band, hft_types::Rate(0));
}

#[test]
fn exe_344_override_records() {
    let mut fx = Fx::new();
    l3(&mut fx);
    inject(&mut fx, &t1("-10.0"));
    fx.legs(A1, G8, &["-10.0"]);
    fx.cm(A1, G8).hold = MANUAL_HOLD;
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let d = r.dec(11).into_iter().find(|d| d.detail == pack(5, 1, 0)).unwrap();
    assert_eq!(d.target_qty, q("-8.0"));
    assert!(r.states().contains(&(2, 3, pack(2, 7, 0))));
    assert!(
        r.upd()
            .contains(&ExecUpdateKind::Stuck(StuckReason::EngineOverride))
    );
    assert_eq!(fx.cell(A1, G8).hold, 0);
    let n = r.notes_n(note::EXEC_MANUAL_HOLD);
    assert_eq!(
        (n[0].level, &n[0].v[..3]),
        (NoteLevel::Info, &[0x01_0000_0801u64, 0, 3][..])
    );
}

#[test]
fn exe_345_cancel_resting() {
    let base = || {
        let mut fx = Fx::new();
        l3(&mut fx);
        fx.legs(A1, G7, &["10.00", "-10.0"]);
        fx.spot(A1, A, G7, "10", "10");
        fx
    };
    let want = vec![
        CancelResting { account: A1, inst: S },
        CancelResting { account: A1, inst: P },
    ];
    let mut fx = base();
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert_eq!(r.cancel_resting, want, "EXE-345.1");
    assert!(r.has_dec(11, pack(5, 5, 0b11)), "EXE-345.1");
    fx.at(5000);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert_eq!(r.cancel_resting, want, "EXE-345.2");
    assert_eq!(
        r.dec(11).iter().filter(|d| d.detail == pack(5, 5, 0b11)).count(),
        1,
        "EXE-345.2"
    );
    let mut fx = base();
    fx.flatten(A1);
    let r = fx.adv();
    assert!(
        r.cancel_resting.contains(&want[0]) && r.cancel_resting.contains(&want[1]),
        "EXE-345.3"
    );
    let mut fx = base();
    fx.perp(A1, B, P, "-13.0");
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert_eq!(
        r.cancel_resting.iter().filter(|c| c.inst == P).count(),
        1,
        "EXE-345.4"
    );
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    assert!(fx.adv().cancel_resting.is_empty(), "EXE-345.5");
}

#[test]
fn exe_346_slice_minus_inflight() {
    let mut p = c0();
    p.force_slice.max_chunk_notional = usd("5000");
    p.force_slice.depth_frac = rate("1");
    type Edit = fn(&mut Fx);
    let rows: [(&str, Edit, Option<&str>); 4] = [
        ("EXE-346.1", |_| {}, Some("10.0")),
        (
            "EXE-346.2",
            |fx| fx.lv(A1, G8, |l| l.leg[0].inflight_resting = q("10.0")),
            Some("10.0"),
        ),
        ("EXE-346.3", |fx| fx.infl(A1, G8, 0, "9.0"), Some("1.0")),
        ("EXE-346.4", |fx| fx.infl(A1, G8, 0, "10.0"), None),
    ];
    for (row, edit, want) in rows {
        let mut fx = Fx::with(p, vec![g7(), g8()]);
        fx.risk(A1, |r| r.max_reduce_notional = usd("5000"));
        fx.legs(A1, G8, &["-10.0"]);
        edit(&mut fx);
        fx.flatten(A1);
        let r = fx.adv();
        let o: Vec<_> = r.orders.iter().filter(|o| o.intent.group == G8).collect();
        match want {
            Some(x) => assert_eq!((o[0].intent.side, o[0].intent.qty), (Side::Buy, q(x)), "{row}"),
            None => assert!(o.is_empty(), "{row}"),
        }
    }
}

fn g7_l3() -> Fx {
    let mut fx = Fx::new();
    l3(&mut fx);
    fx.legs(A1, G7, &["10.00", "-10.0"]);
    fx.spot(A1, A, G7, "10", "10");
    fx
}

/// First L3 step on g7 with both legs usable: a linked first leg (P) placed with link 1.
fn g7_linked(fx: &mut Fx) -> hft_strategy_sdk::OrderRef {
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G7).copied().unwrap();
    assert!(o.intent.link.get().is_some());
    let rf = fx.placed(&o);
    let mut l = link(1, A1, G7, LinkPhase::Open, LinkBrokenCause::None);
    l.first = rf;
    fx.link(&l);
    rf
}

#[test]
fn exe_347_serial_fallback_reasons() {
    // .1 / .2
    for (row, reason) in [("EXE-347.1", 1u16), ("EXE-347.2", 4)] {
        let mut fx = g7_l3();
        if reason == 1 {
            fx.quote_down(S);
        } else {
            fx.ready(A1, A, false);
        }
        fx.force(A1, B, "0.2");
        let r = fx.adv();
        assert_eq!(fx.cell(A1, G7).serial, 1, "{row}");
        assert!(r.has_dec(11, pack(5, 2, reason)), "{row}");
        let n = r.notes_n(note::EXEC_SERIAL_FALLBACK);
        assert_eq!(n.len(), 1, "{row}");
        assert_eq!(n[0].level, NoteLevel::P0, "{row}");
        assert_eq!(
            &n[0].v[..6],
            &[
                1,
                0,
                u64::from(reason),
                q("-10.0").0 as u64,
                q("10.00").0 as u64,
                3
            ],
            "{row}"
        );
        let o = r.orders.iter().find(|o| o.intent.group == G7).unwrap();
        assert_eq!(
            (o.intent.side, o.intent.inst, o.intent.qty),
            (Side::Buy, P, q("2.0")),
            "{row}"
        );
        assert!(o.intent.link.get().is_none() && o.intent.reduce_only(), "{row}");
    }
    // .3 LINK_HOLD, .4 first Unknown.
    for (row, reason) in [("EXE-347.3", 5u16), ("EXE-347.4", 6)] {
        let mut fx = g7_l3();
        let rf = g7_linked(&mut fx);
        if reason == 5 {
            fx.cm(A1, G7).hold |= LINK_HOLD;
        } else {
            fx.upd(rf, G7, OrderUpdateKind::Unknown);
        }
        fx.at(1000);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.has_dec(11, pack(5, 2, reason)), "{row}");
        assert_eq!(fx.cell(A1, G7).serial, 1, "{row}");
        if reason == 5 {
            assert_eq!(fx.cell(A1, G7).active.link_id, 0, "{row}");
        }
    }
    // .5
    let mut fx = g7_l3();
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G7).unwrap();
    let l = o.intent.link.get().unwrap();
    assert_eq!(
        (o.intent.side, o.intent.inst, l.hedge.residual),
        (Side::Buy, P, hft_strategy_sdk::ResidualPolicy::TrimExcess),
        "EXE-347.5"
    );
    assert_eq!(r.dec(5)[0].first_leg_ix, 1, "EXE-347.5");
    // .6
    let mut fx = g7_l3();
    g7_linked(&mut fx);
    fx.quote_down(S);
    fx.ready(A1, A, false);
    fx.cm(A1, G7).hold |= LINK_HOLD;
    fx.at(1000);
    fx.touch(P);
    let r = fx.adv();
    let n = r.notes_n(note::EXEC_SERIAL_FALLBACK);
    assert_eq!((n.len(), n[0].v[2]), (1, 1), "EXE-347.6");
    assert!(r.has_dec(11, pack(5, 2, 1)), "EXE-347.6");
    // .7
    let mut fx = g7_l3();
    let rf = g7_linked(&mut fx);
    fx.ready(A1, A, false);
    fx.upd(rf, G7, OrderUpdateKind::Unknown);
    fx.at(1000);
    fx.touch(P);
    let r = fx.adv();
    assert_eq!(r.notes_n(note::EXEC_SERIAL_FALLBACK)[0].v[2], 4, "EXE-347.7");
    assert!(r.has_dec(11, pack(5, 2, 4)), "EXE-347.7");
}

#[test]
fn exe_348_serial_sticky() {
    let mut fx = g7_l3();
    fx.quote_down(S);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == G7).copied().unwrap();
    let rf = fx.placed(&o);
    fx.legs(A1, G7, &["10.00", "-8.0"]);
    fx.fill(rf, G7, "2.0", true);
    for ms in [1000, 2000, 3000, 4000] {
        fx.at(ms);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.orders.iter().all(|o| o.intent.inst != S));
        assert!(r.ord(ExecOrderKind::HedgeFix).is_empty());
    }
    fx.at(5000);
    fx.quote_up(S);
    fx.touch(S);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.inst == S).copied().unwrap();
    assert_eq!((o.intent.side, o.intent.qty), (Side::Sell, q("2.00")));
    assert!(o.intent.link.get().is_none());
    let rf = fx.placed(&o);
    fx.legs(A1, G7, &["8.00", "-8.0"]);
    fx.fill(rf, G7, "2.00", true);
    let r = fx.adv();
    assert!(r.states().iter().any(|s| s.1 == 1));
    assert_eq!(fx.cell(A1, G7).serial, 0);
}

#[test]
fn exe_349_perp_quote_gates() {
    let mut fx = g7_l3();
    fx.quote_down(P);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert!(r.orders.iter().all(|o| o.intent.group != G7));
    assert_eq!(fx.cell(A1, G7).serial, 0);
    assert!(r.notes_n(note::EXEC_SERIAL_FALLBACK).is_empty());
    assert_eq!(fx.cell(A1, G7).gated_since, t(0));
}

#[test]
fn exe_350_algo() {
    let mut fx = g7_l3();
    fx.legs(A1, G8, &["-10.0"]);
    fx.force(A1, B, "0.5");
    let r = fx.adv();
    let o7 = r.orders.iter().find(|o| o.intent.group == G7).unwrap();
    let l = o7.intent.link.get().unwrap();
    assert_eq!(
        (o7.intent.side, o7.intent.qty, l.hedge.side, l.hedge.residual),
        (
            Side::Buy,
            q("5.0"),
            Side::Sell,
            hft_strategy_sdk::ResidualPolicy::TrimExcess
        ),
        "EXE-350.1"
    );
    let d7 = r.dec(5).into_iter().find(|d| d.group == G7).unwrap();
    assert_eq!((d7.algo, d7.first_leg_ix), (2, 1), "EXE-350.1");
    let o8 = r.orders.iter().find(|o| o.intent.group == G8).unwrap();
    assert_eq!(
        (o8.intent.side, o8.intent.qty),
        (Side::Buy, q("5.0")),
        "EXE-350.2"
    );
    assert_eq!(
        r.dec(5).into_iter().find(|d| d.group == G8).unwrap().algo,
        1,
        "EXE-350.2"
    );
}

#[test]
fn exe_351_broken_serial() {
    let mut fx = Fx::new();
    l3(&mut fx);
    fx.legs(A1, G7, &["10.00", "-6.0"]);
    fx.spot(A1, A, G7, "10.00", "3.00");
    fx.cm(A1, G7).view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    fx.force(A1, B, "0.5");
    let r = fx.adv();
    assert_eq!((tq(&fx, G7, 0), tq(&fx, G7, 1)), (q("5.00"), q("-3.0")));
    let o: Vec<_> = r
        .orders
        .iter()
        .filter(|o| o.intent.group == G7)
        .copied()
        .collect();
    assert_eq!(o.len(), 1);
    assert_eq!(
        (o[0].intent.inst, o[0].intent.side, o[0].kind),
        (P, Side::Buy, ExecOrderKind::ForceStep)
    );
    assert!(o[0].intent.reduce_only() && o[0].intent.link.get().is_none());
    assert_eq!(r.dec(5).into_iter().find(|d| d.group == G7).unwrap().algo, 5);
    assert!(matches!(fx.cell(A1, G7).view.state, TargetState::Broken(_)));
    let rf = fx.placed(&o[0]);
    fx.legs(A1, G7, &["10.00", "-3.0"]);
    fx.fill(rf, G7, "3.0", true);
    fx.at(1000);
    fx.touch(S);
    let r = fx.adv();
    let s = r.orders.iter().find(|o| o.intent.inst == S).unwrap();
    assert_eq!((s.intent.side, s.intent.qty), (Side::Sell, q("3.00")));
    assert!(
        fx.log
            .orders
            .iter()
            .all(|o| o.origin == ExecOrigin::Engine || o.intent.group != G7)
    );
}

#[test]
fn exe_352_ungrouped_u() {
    let g16 = gspec(G16, &[(P, B)]);
    for (row, inst, acct, grp, want) in [
        ("EXE-352.1", P, "-5.0", Some((G8, "-3.0")), Some("-1.0")),
        ("EXE-352.2", P4, "-4.0", None, Some("-2.0")),
        ("EXE-352.3", P, "-5.0", Some((G16, "0")), Some("-2.5")),
        ("EXE-352.4", P, "-1.0", Some((G8, "-3.0")), None),
        ("EXE-352.5", P, "-3.05", Some((G8, "-3.0")), None),
        ("EXE-352.6", P, "-1.0", Some((G8, "1.0")), Some("-0.5")),
    ] {
        let mut fx = Fx::with(c0(), vec![g7(), g8(), g16]);
        if row == "EXE-352.3" {
            fx.mode(A1, G16, hft_types::RunMode::Shadow);
        }
        l3(&mut fx);
        fx.perp(A1, B, inst, acct);
        if let Some((g, x)) = grp {
            fx.legs(A1, g, &[x]);
        }
        fx.force(A1, B, "0.5");
        fx.adv();
        let u = fx.cell(A1, GNONE);
        let leg = (0..usize::from(u.n_legs)).find(|&i| u.legs[i].inst == inst);
        match want {
            Some(x) => {
                let i = leg.unwrap_or_else(|| panic!("{row}: not loaded"));
                assert_eq!(u.target_of(i), Some(q(x)), "{row}");
            }
            None => assert!(leg.is_none(), "{row}"),
        }
    }
}

#[test]
fn exe_353_ungrouped_top4() {
    let mut fx = Fx::new();
    l3(&mut fx);
    let insts = [P, P4, PX[0], PX[1], PX[2], PX[3]];
    let qs = ["-6", "-5", "-4", "-3", "-2", "-1"];
    for (i, x) in insts.iter().zip(qs.iter()) {
        fx.perp(A1, B, *i, x);
    }
    fx.force(A1, B, "1");
    let r = fx.adv();
    let loaded = |fx: &Fx| {
        let u = fx.cell(A1, GNONE);
        let mut v: Vec<_> = (0..usize::from(u.n_legs)).map(|i| u.legs[i].inst).collect();
        v.sort();
        v
    };
    let mut want = vec![P, P4, PX[0], PX[1]];
    want.sort();
    assert_eq!(loaded(&fx), want);
    assert_eq!(r.notes_n(note::EXEC_UNGROUPED_REDUCE)[0].v[4], 4);
    for o in r.orders.clone() {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    fx.perp(A1, B, P, "0");
    fx.at(5000);
    fx.force(A1, B, "1");
    fx.adv();
    let mut want = vec![P4, PX[0], PX[1], PX[2]];
    want.sort();
    assert_eq!(loaded(&fx), want);
}

#[test]
fn exe_354_ungrouped_records() {
    let setup = || {
        let mut fx = Fx::new();
        l3(&mut fx);
        fx.perp(A1, B, P, "-3.0");
        fx.perp(A1, B, P4, "-2.0");
        fx.v.m.portfolio(A1, |p| {
            p.inventory(
                A,
                BASE,
                hft_strategy_sdk::InventoryView {
                    total: q("5"),
                    assigned: q("0"),
                    unassigned: q("5"),
                    baseline: q("0"),
                },
            );
        });
        fx
    };
    let mut fx = setup();
    fx.force(A1, B, "0.5");
    let r = fx.adv();
    assert!(
        r.decisions
            .iter()
            .any(|d| d.group == GNONE && d.action == 11 && d.detail == pack(5, 4, 0)),
        "EXE-354.1"
    );
    let n = r.notes_n(note::EXEC_UNGROUPED_REDUCE);
    assert_eq!(
        (n[0].level, &n[0].v[..6]),
        (NoteLevel::P0, &[1u64, u64::from(B.0), 2, 500, 2, 3][..]),
        "EXE-354.1"
    );
    assert!(r.notify.iter().all(|x| x.u.group != GNONE), "EXE-354.1");
    assert!(r.orders.iter().all(|o| o.intent.inst != S), "EXE-354.1");
    let mut fx = setup();
    fx.flatten(A1);
    let r = fx.adv();
    assert_eq!(
        &r.notes_n(note::EXEC_UNGROUPED_REDUCE)[0].v[..6],
        &[1, 0xFF, 2, 500, 2, 4],
        "EXE-354.2"
    );
    for d in fx.log.decisions.iter().filter(|d| d.group == GNONE) {
        assert!([11u8, 5, 6, 3].contains(&d.action), "EXE-354.3");
        assert_eq!(d.owner, I1, "EXE-354.3");
    }
}

#[test]
fn exe_355_l3_fall_clears() {
    let mut fx = Fx::new();
    l3(&mut fx);
    inject(&mut fx, &t1("-10.0"));
    fx.legs(A1, G8, &["-10.0"]);
    fx.perp(A1, B, P4, "-2.0");
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    let o8 = r.orders.iter().find(|o| o.intent.group == G8).copied().unwrap();
    let rf = fx.placed(&o8);
    for o in r.orders.iter().filter(|o| o.intent.group == GNONE) {
        let rf2 = fx.placed(o);
        fx.nofill(rf2, GNONE);
    }
    fx.risk(A1, |r| r.margin_level = MarginLevel::L2);
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    fx.adv();
    assert_eq!(acct_row(&fx.snap(), A1).map_or(0, |a| a.force_active), 0);
    fx.at(100);
    fx.nofill(rf, G8);
    let r = fx.adv();
    assert!(r.dec(3).iter().any(|d| d.group == G8 && d.eflags == 2));
    assert!(r.notify.iter().any(|n| n.u.group == G8
        && n.u.kind
            == ExecUpdateKind::Done {
                cleared_by_engine: true
            }));
    assert!(!fx.cell(A1, G8).has_target);
    assert!(fx.log.dec(3).iter().any(|d| d.group == GNONE));
    fx.at(1000);
    fx.touch(P);
    assert!(fx.adv().orders.is_empty());
}

fn serial_g7(fx: &mut Fx, s: &str, p: &str) {
    l3(fx);
    fx.legs(A1, G7, &[s, p]);
    fx.spot(A1, A, G7, s, s);
    fx.quote_down(S);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    for o in r.orders {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    assert_eq!(fx.cell(A1, G7).serial, 1);
}

#[test]
fn exe_356_hard_align() {
    let mut fx = Fx::new();
    serial_g7(&mut fx, "10.00", "-6.0");
    fx.risk(A1, |r| r.margin_level = MarginLevel::L2);
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    fx.at(1000);
    let r = fx.adv();
    assert!(r.has_dec(11, pack(5, 6, 0)));
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.target_of(1), c.target_of(0)),
        (Some(q("-6.0")), Some(q("6.00")))
    );
    assert_eq!(
        (c.view.intent.reason, c.view.intent.urgency, c.view.origin),
        (
            TargetReason::Hard,
            Urgency::Urgent,
            TargetOrigin::Engine(EngineOrigin::MarginStress)
        )
    );
    assert_eq!(c.serial, 1);
    for ms in [2000, 3000] {
        fx.at(ms);
        fx.touch(S);
        let r = fx.adv();
        assert!(r.orders.is_empty() && r.states().iter().all(|s| s.1 != 4));
    }
    fx.at(4000);
    fx.quote_up(S);
    fx.touch(S);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.inst == S).copied().unwrap();
    assert_eq!((o.intent.side, o.intent.qty), (Side::Sell, q("4.00")));
    assert!(o.intent.link.get().is_none());
    let rf = fx.placed(&o);
    fx.legs(A1, G7, &["6.00", "-6.0"]);
    fx.fill(rf, G7, "4.00", true);
    let r = fx.adv();
    assert_eq!(r.states().iter().filter(|s| s.1 == 1).count(), 1);
    assert!(r.upd().contains(&ExecUpdateKind::Done {
        cleared_by_engine: true
    }));
    let c = fx.cell(A1, G7);
    assert_eq!((c.serial, c.has_target), (0, false));
    assert!(r.dec(3).is_empty());
}

#[test]
fn exe_357_hardalign_replaced() {
    let setup = || {
        let mut fx = Fx::new();
        serial_g7(&mut fx, "10.00", "-6.0");
        fx.risk(A1, |r| r.margin_level = MarginLevel::L2);
        fx.risk_ev(A1, ms_margin(MarginLevel::L2));
        fx.at(1000);
        fx.adv();
        assert!(!fx.ex.acct_state(A1).unwrap().force_active);
        fx
    };
    let mut fx = setup();
    l3(&mut fx);
    fx.at(2000);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert_eq!(fx.cell(A1, G7).serial, 1, "EXE-357.1");
    assert!(
        r.orders
            .iter()
            .filter(|o| o.intent.group == G7)
            .all(|o| o.intent.link.get().is_none()),
        "EXE-357.1"
    );
    let mut fx = setup();
    let (res, _) = fx.set(I1, &t0("8.00", "-5.0"));
    assert_eq!(res, Ok(()), "EXE-357.2");
    assert_eq!(fx.cell(A1, G7).serial, 0, "EXE-357.2");
    let r = fx.adv();
    assert!(r.orders.iter().all(|o| o.intent.inst != S), "EXE-357.2");
}

#[test]
fn exe_358_l3_fall_broken_autofix() {
    let mut fx = Fx::new();
    l3(&mut fx);
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.spot(A1, A, G7, "1", "1");
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.broken_since = t(0);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(120_000);
    }
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    for o in r.orders {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    fx.at(200_000);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L2);
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    let r = fx.adv();
    assert!(matches!(fx.cell(A1, G7).view.state, TargetState::Broken(_)));
    assert!(r.has_dec(14, pack(4, 1, 0)));
}

#[test]
fn exe_359_set_target_during_force() {
    let setup = || {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["10.00", "-10.0"]);
        fx.spot(A1, A, G7, "10", "10");
        inject(&mut fx, &t0("8.00", "-8.0"));
        let c = fx.cm(A1, G7);
        c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
        c.view.intent.reason = TargetReason::MarginStress;
        c.serial = 1;
        fx.acct(A1).force_active = true;
        fx
    };
    let mut fx = setup();
    let (res, _) = fx.set(I1, &t0("6.00", "-6.0"));
    assert_eq!(res, Ok(()), "EXE-359.1");
    let r = fx.adv();
    assert_eq!(fx.cell(A1, G7).serial, 1, "EXE-359.1");
    let o = r.orders.iter().find(|o| o.intent.group == G7).unwrap();
    assert_eq!((o.intent.inst, o.origin), (P, ExecOrigin::Engine), "EXE-359.1");
    let d = r.dec(5).into_iter().find(|d| d.group == G7).unwrap();
    assert_eq!(d.urgency, 3, "EXE-359.1");
    let mut tg = t0("0", "0");
    tg.reason = TargetReason::Exit;
    assert_eq!(setup().set(I1, &tg).0, Ok(()), "EXE-359.2");
    assert_eq!(setup().set(I1, &t0("8.00", "-8.0")).0, Ok(()), "EXE-359.3");
    let (res, r) = setup().set(I1, &t0("9.00", "-8.0"));
    assert_eq!(res, Err(TargetReject::ForcedReduceActive), "EXE-359.4");
    assert!(r.has_dec(4, pack(1, 7, 0)), "EXE-359.4");
    assert_eq!(
        setup().set(I1, &t0("6.00", "1.0")).0,
        Err(TargetReject::ForcedReduceActive),
        "EXE-359.5"
    );
    let mut fx = setup();
    fx.cm(A1, G7).view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    let (res, r) = fx.set(I1, &t0("6.00", "-6.0"));
    assert_eq!(res, Err(TargetReject::CellBroken), "EXE-359.6");
    assert!(r.has_dec(4, pack(1, 6, 0)), "EXE-359.6");
}

#[test]
fn exe_360_step_records() {
    let mut fx = Fx::new();
    l3(&mut fx);
    fx.legs(A1, G8, &["-10.0"]);
    for k in 0..3u64 {
        fx.at(k * 5000);
        fx.force(A1, B, "0.2");
        let r = fx.adv();
        assert_eq!(r.notes_n(note::EXEC_FORCE_STEP).len(), 1);
        assert_eq!(r.notes_n(note::EXEC_FORCE_STEP)[0].level, NoteLevel::P1);
        assert_eq!(r.dec(11).iter().filter(|d| d.detail == pack(5, 1, 0)).count(), 1);
        for o in r.orders {
            let rf = fx.placed(&o);
            fx.nofill(rf, o.intent.group);
        }
    }
    assert_eq!(cell_row(&fx.snap(), A1, G8).unwrap().force_steps, 3);
}

#[test]
fn exe_369_engine_target_state() {
    let mut fx = Fx::new();
    l3(&mut fx);
    inject(&mut fx, &t1("-10.0"));
    fx.legs(A1, G8, &["-10.0"]);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert!(r.states().contains(&(2, 3, pack(2, 7, 0))), "EXE-369.1");
    let o = r.orders[0];
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["-8.0"]);
    fx.fill(rf, G8, "2.0", true);
    let r = fx.adv();
    assert!(r.states().contains(&(3, 1, 0)), "EXE-369.1");
    let mut fx = Fx::new();
    l3(&mut fx);
    fx.legs(A1, G8, &["-10.0"]);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert!(r.states().contains(&(1, 2, 0)), "EXE-369.2");
    let o = r.orders[0];
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["-8.0"]);
    fx.fill(rf, G8, "2.0", true);
    assert!(fx.adv().states().contains(&(2, 1, 0)), "EXE-369.2");
    let mut fx = Fx::new();
    fx.flatten(A1);
    let r = fx.adv();
    let s: Vec<_> = r
        .decisions
        .iter()
        .filter(|d| d.group == G8 && d.action == 6)
        .map(|d| (d.state_from, d.state_to))
        .collect();
    assert!(s.contains(&(1, 2)) && s.contains(&(2, 1)), "EXE-369.3");
    assert!(r.orders.iter().all(|o| o.intent.group != G8), "EXE-369.3");
    let _ = Tsc(0);
}
