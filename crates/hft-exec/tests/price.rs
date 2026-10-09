//! EXE-125–EXE-132: limit price and per-order notional.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{ExecSpec, MarginLevel, TargetState};
use hft_types::{RejectBits, Side};

fn with_cross(tg: &mut hft_strategy_sdk::TargetIntent, mc: &str) {
    if let ExecSpec::TakerSlice(p) = &mut tg.exec {
        p.max_cross = rate(mc);
    }
}

#[test]
fn exe_125_max_cross_eff() {
    for (row, open, mc, want) in [
        ("EXE-125.1", true, "0.003", "100.50"),
        ("EXE-125.2", true, "0.008", "100.70"),
        ("EXE-125.3", false, "0.008", "99.01"),
        ("EXE-125.4", false, "0.02", "99.00"),
    ] {
        let mut fx = Fx::new();
        let mut tg = if open {
            t1("10")
        } else {
            fx.legs(A1, G8, &["10"]);
            t1("0")
        };
        with_cross(&mut tg, mc);
        let o = fx.go(&tg).orders[0];
        assert_eq!(o.intent.px, px(want), "{row}");
    }
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.5", "-1.0"]);
    fx.spot(A1, A, G7, "1.5", "1.5");
    fx.leg_adjust(A1, G7);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!(
        (o.intent.side, o.intent.px),
        (Side::Sell, px("99.41")),
        "EXE-125.5"
    );
}

#[test]
fn exe_126_rr_eff_l3() {
    for (row, l3, want) in [
        ("EXE-126.1", "0", "99.00"),
        ("EXE-126.2", "0.02", "98.01"),
        ("EXE-126.3", "0.005", "99.00"),
    ] {
        let mut fx = Fx::new();
        fx.quote(P, "98.50", "101.50");
        fx.risk_for(A1, P, |r| r.l3_max_cross = rate(l3));
        fx.legs(A1, G8, &["10"]);
        fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
        fx.force(A1, B, "0.2");
        let o = fx.adv().ord(ExecOrderKind::ForceStep)[0];
        assert_eq!((o.intent.side, o.intent.px), (Side::Sell, px(want)), "{row}");
    }
    // .4 repair (not MarginStress/Hard) ignores l3_max_cross.
    let g11 = gspec(G11, &[(P, B), (Q, C)]);
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g11]);
    fx.quote(P, "98.50", "101.50");
    fx.risk_for(A1, P, |r| r.l3_max_cross = rate("0.02"));
    fx.legs(A1, G11, &["1.5", "-1.0"]);
    fx.leg_adjust(A1, G11);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!(
        (o.intent.inst, o.intent.side, o.intent.px),
        (P, Side::Sell, px("99.00")),
        "EXE-126.4"
    );
    // .5 ungrouped flatten.
    let mut fx = Fx::new();
    fx.quote(P, "98.50", "101.50");
    fx.risk_for(A1, P, |r| r.l3_max_cross = rate("0.02"));
    fx.perp(A1, B, P, "3.0");
    fx.flatten(A1);
    let r = fx.adv();
    let o = r.orders.iter().find(|o| o.intent.group == GNONE).unwrap();
    assert_eq!(o.intent.px, px("98.01"), "EXE-126.5");
}

fn c127() -> ExecParams {
    let mut p = c0();
    p.force_slice.max_chunk_notional = usd("2000");
    p
}

fn r127(fx: &mut Fx) {
    fx.risk(A1, |r| {
        r.max_order_notional = usd("2000");
        r.max_reduce_notional = usd("1500");
        r.max_hedge_notional = usd("1600");
    });
}

fn strat(dq: &str, chunk: &str) -> hft_strategy_sdk::TargetIntent {
    let mut tg = t1(dq);
    if let ExecSpec::TakerSlice(p) = &mut tg.exec {
        p.max_chunk_notional = usd(chunk);
        p.depth_frac = rate("1");
    }
    tg
}

#[test]
fn exe_127_cap_n_by_class() {
    // .1 Open buy P @ 100.70.
    let mut fx = Fx::with(c127(), vec![g7(), g8()]);
    r127(&mut fx);
    let o = fx.go(&strat("100", "1800")).orders[0];
    assert_eq!(
        (o.intent.px, o.intent.qty),
        (px("100.70"), q("17.8")),
        "EXE-127.1"
    );
    // .2 Reduce sell P @ 99.31.
    let mut fx = Fx::with(c127(), vec![g7(), g8()]);
    r127(&mut fx);
    fx.legs(A1, G8, &["100"]);
    let o = fx.go(&strat("0", "1800")).orders[0];
    assert_eq!((o.intent.px, o.intent.qty), (px("99.31"), q("15.1")), "EXE-127.2");
    // .3 L3 Flatten buy P.
    let mut fx = Fx::with(c127(), vec![g7(), g8()]);
    r127(&mut fx);
    fx.legs(A1, G8, &["-100"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "1");
    fx.book(P, "99.80", "100.20", "500", 5);
    let o = fx.adv().ord(ExecOrderKind::ForceStep)[0];
    assert_eq!(
        (o.intent.px, o.intent.qty),
        (px("100.70"), q("14.8")),
        "EXE-127.3"
    );
    // .4 repair sell S (naked 30).
    let mut fx = Fx::with(c127(), vec![g7(), g8()]);
    r127(&mut fx);
    fx.legs(A1, G7, &["30.00", "0"]);
    fx.spot(A1, A, G7, "30", "30");
    fx.leg_adjust(A1, G7);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!(
        (o.intent.px, o.intent.qty),
        (px("99.41"), q("16.09")),
        "EXE-127.4"
    );
    // .5 auto-fix sell S.
    let mut fx = Fx::with(c127(), vec![g7(), g8()]);
    r127(&mut fx);
    fx.legs(A1, G7, &["30.00", "0"]);
    fx.spot(A1, A, G7, "30", "30");
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(0);
    }
    fx.touch(S);
    let o = fx.adv().ord(ExecOrderKind::AutoFix)[0];
    assert_eq!(
        (o.intent.px, o.intent.qty),
        (px("99.41"), q("15.08")),
        "EXE-127.5"
    );
    // .6 Open with slice cap 2500 → 2000.
    let mut fx = Fx::with(c127(), vec![g7(), g8()]);
    r127(&mut fx);
    let o = fx.go(&strat("100", "2500")).orders[0];
    assert_eq!(o.intent.qty, q("19.8"), "EXE-127.6");
}

#[test]
fn exe_128_quote_rate() {
    for (row, qr, want) in [
        ("EXE-128.1", "1", "17.8"),
        ("EXE-128.2", "1.001", "17.8"),
        ("EXE-128.3", "1.02", "17.5"),
    ] {
        let g16 = gspec(G16, &[(P2, B)]);
        let mut fx = Fx::with(c0(), vec![g7(), g8(), g16]);
        fx.risk_for(A1, P2, |r| r.quote_rate = rate(qr));
        let mut tg = taker(A1, G16, &[(0, "100")]);
        if let ExecSpec::TakerSlice(p) = &mut tg.exec {
            p.max_chunk_notional = usd("1800");
            p.depth_frac = rate("1");
        }
        let o = fx.go(&tg).orders[0];
        assert_eq!((o.intent.px, o.intent.qty), (px("100.70"), q(want)), "{row}");
    }
}

#[test]
fn exe_130_per_inst_band_class() {
    for (row, inst, want) in [("EXE-130.1", P, "100.70"), ("EXE-130.2", P3, "101.70")] {
        let g16 = gspec(G16, &[(inst, B)]);
        let mut fx = Fx::with(c0(), vec![g7(), g8(), g16]);
        fx.risk_for(A1, P3, |r| {
            r.rr_max_cross = rate("0.03");
            r.ioc_max_cross = rate("0.015");
        });
        let mut tg = taker(A1, G16, &[(0, "1")]);
        with_cross(&mut tg, "0.02");
        assert_eq!(fx.go(&tg).orders[0].intent.px, px(want), "{row}");
    }
}

fn fix_order(fx: &mut Fx) -> ExecOrder {
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.leg_adjust(A1, G7);
    fx.adv().ord(ExecOrderKind::HedgeFix)[0]
}

#[test]
fn exe_131_engine_risk_reject() {
    for (row, bits) in [
        ("EXE-131.1", RejectBits::PX_BAND),
        ("EXE-131.2", RejectBits::NOTIONAL_ORDER),
        ("EXE-131.3", RejectBits::NOTIONAL_ACCOUNT),
    ] {
        let mut fx = Fx::new();
        let o = fix_order(&mut fx);
        assert_eq!((o.intent.qty, o.intent.px), (q("0.5"), px("100.70")));
        let r = fx.place_res(&o, PlaceResult::Rejected(bits));
        assert!(
            r.decisions
                .iter()
                .any(|d| d.action == 7 && d.detail == pack(5, 3, 0) && d.gate == bits.0),
            "{row}"
        );
        let n = r.notes_n(hft_exec::decision::note::EXEC_ENGINE_ORDER_REJECTED);
        assert_eq!(n[0].level, hft_event::NoteLevel::P0, "{row}");
        assert_eq!(
            n[0].v,
            [
                0x01_0000_0701,
                1,
                5,
                u64::from(bits.0),
                q("0.5").0 as u64,
                px("100.70").0 as u64,
                px("100").0 as u64,
                usd("1000").0 as u64
            ],
            "{row}"
        );
        let c = fx.cell(A1, G7);
        assert_eq!((c.fix.count, c.fail_streak), (1, 1), "{row}");
    }
    // .4 L3 ForceStep.
    let mut fx = Fx::new();
    fx.legs(A1, G8, &["-10"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let o = fx.adv().ord(ExecOrderKind::ForceStep)[0];
    let r = fx.place_res(&o, PlaceResult::Rejected(RejectBits::PX_BAND));
    assert!(r.has_dec(11, pack(5, 3, 0)), "EXE-131.4");
    assert_eq!(
        r.notes_n(hft_exec::decision::note::EXEC_ENGINE_ORDER_REJECTED)[0].v[2],
        8,
        "EXE-131.4"
    );
    // .5 RATE_BACKOFF gate.
    let mut fx = Fx::new();
    let o = fix_order(&mut fx);
    let r = fx.place_res(&o, PlaceResult::Rejected(RejectBits::RATE_BACKOFF));
    assert!(
        r.notes_n(hft_exec::decision::note::EXEC_ENGINE_ORDER_REJECTED)
            .is_empty(),
        "EXE-131.5"
    );
    assert_eq!(fx.cell(A1, G7).fix.count, 0, "EXE-131.5");
    assert_eq!(fx.cell(A1, G7).wake_at, t(500), "EXE-131.5");
    // .6 HEDGE_LIMIT.
    let mut fx = Fx::new();
    let o = fix_order(&mut fx);
    let r = fx.place_res(&o, PlaceResult::Rejected(RejectBits::HEDGE_LIMIT));
    assert!(r.states().contains(&(1, 4, pack(3, 4, 0))), "EXE-131.6");
}

#[test]
fn exe_132_strategy_px_band_is_gate() {
    let mut fx = Fx::new();
    let mut all = Rec::default();
    let mut r = fx.go(&t1("-10"));
    let mut ms = 0;
    loop {
        if let Some(o) = r.orders.first().copied() {
            all.extend(fx.place_res(&o, PlaceResult::Rejected(RejectBits::PX_BAND)));
        }
        ms += 500;
        if ms > 2000 {
            break;
        }
        fx.at(ms);
        fx.touch(P);
        r = fx.adv();
        all.extend(r.clone());
    }
    assert!(
        all.notes_n(hft_exec::decision::note::EXEC_ENGINE_ORDER_REJECTED)
            .is_empty()
    );
    assert_eq!(fx.cell(A1, G8).fail_streak, 0);
    assert!(all.decisions.iter().any(|d| d.action == 6
        && d.state_to == 3
        && d.detail == pack(2, 1, 0)
        && d.gate == RejectBits::PX_BAND.0));
}
