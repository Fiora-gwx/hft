//! EXE-155–EXE-169, EXE-511: PairOnFill.
use crate::common;
use common::*;
use hft_event::Level;
use hft_exec::*;
use hft_strategy_sdk::{ExecSpec, FirstLeg, MarginView, ResidualPolicy, TargetReason, TargetReject};
use hft_types::{Px, Rate, RejectBits, Side};

fn pair_t(
    legs: &[(u8, &str)],
    f: impl FnOnce(&mut hft_strategy_sdk::PairParams),
) -> hft_strategy_sdk::TargetIntent {
    let mut p = pair0();
    f(&mut p);
    let l: Vec<(u8, hft_types::Qty)> = legs.iter().map(|(i, x)| (*i, q(x))).collect();
    target(A1, G7, &l, ExecSpec::PairOnFill(p))
}

#[test]
fn exe_155_leg_count() {
    let g12 = gspec(G12, &[(S, A), (P, B), (Q, C)]);
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g12]);
    let three = [(0u8, q("1")), (1, q("-1")), (2, q("0"))];
    let (r, rec) = fx.set(I1, &target(A1, G12, &three, ExecSpec::PairOnFill(pair0())));
    assert_eq!(r, Err(TargetReject::TooManyLegs), "EXE-155.1");
    assert!(rec.has_dec(4, pack(1, 8, 0)), "EXE-155.1");
    let simul = hft_strategy_sdk::SimulParams {
        slice: slice0(),
        on_fail: hft_strategy_sdk::SimulOnFail::Hold,
        _pad0: [0; 7],
        fix_tolerance: rate("0.02"),
        residual: ResidualPolicy::Hold,
        _pad1: [0; 7],
    };
    let (r, _) = fx.set(I1, &target(A1, G12, &three, ExecSpec::Simultaneous(simul)));
    assert_eq!(r, Err(TargetReject::TooManyLegs), "EXE-155.2");
    let (r, rec) = fx.set(
        I1,
        &target(A1, G8, &[(0, q("-1"))], ExecSpec::PairOnFill(pair0())),
    );
    assert_eq!(r, Err(TargetReject::SpecInvalid), "EXE-155.3");
    assert!(rec.has_dec(4, pack(1, 3, 0)), "EXE-155.3");
    let (r, _) = fx.set(I1, &target(A1, G12, &three, ExecSpec::TakerSlice(slice0())));
    assert_eq!(r, Ok(()), "EXE-155.4");
}

fn close(fx: &mut Fx, reason: TargetReason, fl: FirstLeg, ix: u8) -> hft_strategy_sdk::TargetIntent {
    fx.legs(A1, G7, &["10", "-10"]);
    fx.spot(A1, A, G7, "10", "10");
    let mut tg = pair_t(&[(0, "0"), (1, "0")], |p| {
        p.first_leg = fl;
        p.first_leg_ix = ix;
    });
    tg.reason = reason;
    tg
}

fn first_ix(r: &Rec) -> u8 {
    r.dec(5)[0].first_leg_ix
}

#[test]
fn exe_156_leg_order_table() {
    type Setup = fn(&mut Fx) -> hft_strategy_sdk::TargetIntent;
    let rows: [(&str, Setup, u8); 10] = [
        ("EXE-156.1", |_| t0("10", "-10"), 0),
        (
            "EXE-156.2",
            |fx| {
                fx.book(S, "99.90", "100.10", "80", 5);
                t0("10", "-10")
            },
            1,
        ),
        (
            "EXE-156.3",
            |_| pair_t(&[(0, "10"), (1, "-10")], |p| p.first_leg = FirstLeg::PerpFirst),
            1,
        ),
        (
            "EXE-156.4",
            |_| pair_t(&[(0, "10"), (1, "-10")], |p| p.first_leg = FirstLeg::SpotFirst),
            0,
        ),
        (
            "EXE-156.5",
            |_| {
                pair_t(&[(0, "10"), (1, "-10")], |p| {
                    p.first_leg = FirstLeg::Fixed;
                    p.first_leg_ix = 1
                })
            },
            1,
        ),
        (
            "EXE-156.6",
            |fx| close(fx, TargetReason::Exit, FirstLeg::ByLiquidity, 0),
            0,
        ),
        (
            "EXE-156.7",
            |_| {
                let mut tg = pair_t(&[(0, "10"), (1, "-10")], |p| p.first_leg = FirstLeg::SpotFirst);
                tg.reason = TargetReason::Manual;
                tg
            },
            0,
        ),
        (
            "EXE-156.8",
            |fx| close(fx, TargetReason::MarginStress, FirstLeg::SpotFirst, 0),
            1,
        ),
        (
            "EXE-156.9",
            |fx| close(fx, TargetReason::Hard, FirstLeg::Fixed, 0),
            1,
        ),
        (
            "EXE-156.10",
            |fx| {
                fx.acct(A1).force_active = true;
                close(fx, TargetReason::Exit, FirstLeg::SpotFirst, 0)
            },
            1,
        ),
    ];
    for (row, setup, want) in rows {
        let mut fx = Fx::new();
        let tg = setup(&mut fx);
        let r = fx.go(&tg);
        assert_eq!(first_ix(&r), want, "{row}");
    }
}

#[test]
fn exe_157_both_perp_first_leg() {
    let g11 = || gspec(G11, &[(P, B), (Q, C)]);
    // .1
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g11()]);
    fx.legs(A1, G11, &["-5.0", "5.0"]);
    fx.risk(A1, |r| r.margin_level = hft_strategy_sdk::MarginLevel::L3);
    fx.force(A1, C, "0.2");
    assert_eq!(first_ix(&fx.adv()), 1, "EXE-157.1");
    // .2
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g11()]);
    fx.legs(A1, G11, &["-5.0", "5.0"]);
    let m = |r: &str| MarginView {
        margin_balance: usd("1000"),
        maint_margin: usd("100"),
        avail_margin: usd("900"),
        margin_ratio: rate(r),
        src_ens: 0,
        stale: false,
    };
    fx.v.m.portfolio(A1, |p| {
        p.margin(B, m("0.5"));
        p.margin(C, m("0.7"));
    });
    fx.flatten(A1);
    assert_eq!(first_ix(&fx.adv()), 1, "EXE-157.2");
    // .3
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g11()]);
    fx.legs(A1, G11, &["-8.0", "5.0"]);
    fx.flatten(A1);
    assert_eq!(first_ix(&fx.adv()), 0, "EXE-157.3");
}

#[test]
fn exe_158_fallback_by_liquidity() {
    let g13 = gspec(G13, &[(S, A), (S2, D)]);
    let g11 = gspec(G11, &[(P, B), (Q, C)]);
    // .1 g13 Hard close.
    let mut fx = Fx::with(c0(), vec![g7(), g8(), g13, g11]);
    fx.legs(A1, G13, &["10", "-1"]);
    fx.spot(A1, A, G13, "10", "10");
    fx.v.m.portfolio(A1, |p| {
        p.inventory_of(D, G13, q("10"));
    });
    let mut tg = target(
        A1,
        G13,
        &[(0, q("0")), (1, q("0"))],
        ExecSpec::PairOnFill(pair0()),
    );
    tg.reason = TargetReason::Hard;
    assert_eq!(first_ix(&fx.go(&tg)), 0, "EXE-158.1");
    // .2 g13 PerpFirst.
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G13, &[(S, A), (S2, D)])]);
    let mut p = pair0();
    p.first_leg = FirstLeg::PerpFirst;
    let tg = target(A1, G13, &[(0, q("10")), (1, q("10"))], ExecSpec::PairOnFill(p));
    assert_eq!(first_ix(&fx.go(&tg)), 0, "EXE-158.2");
    // .3 g11 SpotFirst.
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G11, &[(P, B), (Q, C)])]);
    fx.book(Q, "99.80", "100.20", "60", 5);
    let mut p = pair0();
    p.first_leg = FirstLeg::SpotFirst;
    let tg = target(A1, G11, &[(0, q("-10")), (1, q("10"))], ExecSpec::PairOnFill(p));
    assert_eq!(first_ix(&fx.go(&tg)), 0, "EXE-158.3");
}

#[test]
fn exe_159_by_liquidity_tie() {
    let mut fx = Fx::new();
    // P bids mirror S asks: equal depth notional on the traded sides.
    let lv = |p: &str, k: i64| Level {
        px: Px(px(p).0 - k * px("0.10").0),
        qty: q("40"),
    };
    let bids: Vec<Level> = (0..5).map(|k| lv("100.50", k)).collect();
    let asks: Vec<Level> = (0..5)
        .map(|k| Level {
            px: Px(px("100.90").0 + k * px("0.10").0),
            qty: q("40"),
        })
        .collect();
    hft_strategy_sdk::testing::MockViews::bbo(
        &mut fx.v.m,
        P,
        (px("100.50"), q("40")),
        (px("100.90"), q("40")),
    );
    hft_strategy_sdk::testing::MockViews::book(&mut fx.v.m, P, &bids, &asks);
    fx.mark(P, "100.70");
    let r = fx.go(&t0("10", "-10"));
    assert_eq!(first_ix(&r), 0);
}

#[test]
fn exe_160_first_qty_weak_side() {
    for (row, num, s, p, want, depth) in [
        ("EXE-160.1", 1u32, "100", "-100", "100.0", "200.0"),
        ("EXE-160.2", 2, "100", "-200", "62.50", "125.0"),
    ] {
        let mut fx = Fx::new();
        fx.risk(A1, |r| {
            r.max_order_notional = usd("100000");
            r.max_reduce_notional = usd("100000");
            r.max_hedge_notional = usd("100000");
        });
        let tg = pair_t(&[(0, s), (1, p)], |x| {
            x.ratio_num = num;
            x.slice.max_chunk_notional = usd("100000");
            x.slice.depth_frac = rate("0.5");
        });
        let r = fx.go(&tg);
        assert_eq!(r.orders[0].intent.inst, S, "{row}");
        assert_eq!(r.orders[0].intent.qty, q(want), "{row}");
        assert_eq!(r.dec(5)[0].depth_qty, q(depth), "{row}");
    }
}

#[test]
fn exe_162_second_leg_side_and_degenerate() {
    // .1
    let mut fx = Fx::new();
    let o = fx
        .go(&pair_t(&[(0, "10"), (1, "-10")], |p| {
            p.first_leg = FirstLeg::SpotFirst
        }))
        .orders[0];
    assert_eq!(
        (o.intent.link.get().unwrap().hedge.side, o.kind),
        (Side::Sell, ExecOrderKind::PairFirst),
        "EXE-162.1"
    );
    // .2
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["10", "-10"]);
    fx.spot(A1, A, G7, "10", "10");
    let o = fx
        .go(&pair_t(&[(0, "0"), (1, "0")], |p| {
            p.first_leg = FirstLeg::SpotFirst
        }))
        .orders[0];
    assert_eq!(o.intent.link.get().unwrap().hedge.side, Side::Buy, "EXE-162.2");
    // .3
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["0", "-10.0"]);
    let o = fx
        .go(&pair_t(&[(0, "10"), (1, "-10")], |p| {
            p.first_leg = FirstLeg::SpotFirst
        }))
        .orders[0];
    assert_eq!(
        (o.intent.side, o.intent.link.get().is_none(), o.kind),
        (Side::Buy, true, ExecOrderKind::Slice),
        "EXE-162.3"
    );
}

#[test]
fn exe_163_link_done_cooldown() {
    let mut fx = Fx::new();
    let o = fx.go(&t0("10", "-10")).orders[0];
    let rf = fx.placed(&o);
    let mut l = link(1, A1, G7, LinkPhase::Open, LinkBrokenCause::None);
    l.first = rf;
    fx.link(&l);
    fx.at(100);
    l.state = LinkPhase::Done;
    fx.link(&l);
    assert_eq!(fx.cell(A1, G7).active.kind, ACT_NONE, "EXE-163.1");
    assert!(fx.adv().orders.is_empty(), "EXE-163.1");
    fx.at(299);
    fx.touch(S);
    assert!(fx.adv().orders.is_empty(), "EXE-163.2");
    fx.at(300);
    fx.touch(S);
    let r = fx.adv();
    assert_eq!(r.orders.len(), 1, "EXE-163.3");
    assert!(r.orders[0].intent.link.get().is_some());
}

#[test]
fn exe_164_link_hold_clear() {
    for (row, state, flags, want) in [
        ("EXE-164.1", LinkPhase::Hedging, LinkNotice::RESUME_HOLD, true),
        ("EXE-164.2", LinkPhase::Hedging, 0, false),
        ("EXE-164.3", LinkPhase::Done, LinkNotice::RESUME_HOLD, false),
        ("EXE-164.4", LinkPhase::Broken, LinkNotice::RESUME_HOLD, false),
    ] {
        let mut fx = Fx::new();
        inject(&mut fx, &t0("10", "-10"));
        {
            let c = fx.cm(A1, G7);
            c.active.kind = ACT_PAIR;
            c.active.link_id = 9;
            c.active.ext = [2, 1, 0, 0];
        }
        let now = fx.now();
        fx.ex
            .recovery_end(now.tsc, now.tsc, 1_000_000_000_000, 1_000_000_000_000);
        assert_ne!(cell_row(&fx.snap(), A1, G7).unwrap().flags & 8, 0);
        let mut l = link(9, A1, G7, state, LinkBrokenCause::Deadline);
        l.flags = flags;
        fx.link(&l);
        assert_eq!(
            cell_row(&fx.snap(), A1, G7).unwrap().flags & 8 != 0,
            want,
            "{row}"
        );
        if row == "EXE-164.4" {
            assert!(matches!(
                fx.cell(A1, G7).view.state,
                hft_strategy_sdk::TargetState::Broken(_)
            ));
        }
    }
}

#[test]
fn exe_165_second_leg_tail() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10.00", "-10.0"));
    fx.legs(A1, G7, &["10.00", "-9.0"]);
    fx.touch(S);
    let o = fx.adv().orders[0];
    assert_eq!(
        (o.intent.side, o.intent.qty, o.kind, o.intent.class),
        (
            Side::Sell,
            q("1.0"),
            ExecOrderKind::Slice,
            hft_strategy_sdk::IntentClass::Open
        ),
        "EXE-165.1"
    );
    assert!(o.intent.link.get().is_none());
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10.00", "-10.0"));
    fx.legs(A1, G7, &["10.00", "-10.5"]);
    fx.touch(S);
    let o = fx.adv().orders[0];
    assert_eq!(
        (
            o.intent.side,
            o.intent.qty,
            o.kind,
            o.intent.class,
            o.hedge_cap_qty
        ),
        (
            Side::Buy,
            q("0.5"),
            ExecOrderKind::ResidualTrim,
            hft_strategy_sdk::IntentClass::Hedge,
            q("0.6")
        ),
        "EXE-165.2"
    );
}

#[test]
fn exe_166_link_stale_backoff() {
    let mut fx = Fx::new();
    let o = fx.go(&t0("10", "-10")).orders[0];
    fx.place_res(&o, PlaceResult::Rejected(RejectBits::LINK_STALE));
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.fail_streak, c.gated_bits),
        (0, RejectBits::LINK_STALE),
        "EXE-166.3"
    );
    fx.at(499);
    fx.touch(S);
    assert!(fx.adv().orders.is_empty(), "EXE-166.1");
    fx.at(500);
    fx.touch(S);
    let r = fx.adv();
    assert_eq!(r.orders[0].intent.qty, o.intent.qty, "EXE-166.2");
    assert_eq!(fx.cell(A1, G7).fund_shrink, 0);
}

#[test]
fn exe_167_link_funds_shrink() {
    let mut fx = Fx::new();
    let mut qs = Vec::new();
    let mut o = fx.go(&t0("10", "-10")).orders[0];
    for k in 0..6u64 {
        qs.push(o.intent.qty);
        fx.place_res(&o, PlaceResult::Rejected(RejectBits::LINK_FUNDS));
        fx.at(500 * (k + 1));
        fx.touch(S);
        o = fx.adv().orders[0];
    }
    qs.push(o.intent.qty);
    let want = ["9.9", "4.9", "2.4", "1.2", "0.6", "0.6"];
    for (k, w) in want.iter().enumerate() {
        assert_eq!(qs[k], q(w), "EXE-167.{}", k + 1);
    }
    // .7: the 5th first leg accepted → shrink cleared.
    let rf = fx.placed(&o);
    assert_eq!(fx.cell(A1, G7).fund_shrink, 0);
    let mut l = link(1, A1, G7, LinkPhase::Done, LinkBrokenCause::None);
    l.first = rf;
    fx.link(&l);
    fx.at(10_000);
    fx.touch(S);
    assert_eq!(fx.adv().orders[0].intent.qty, q("9.9"), "EXE-167.7");
    // .8: unshrunk 0.3 limited by depth, shrink 2 → pair_min 0.1.
    let mut fx = Fx::new();
    let bids: Vec<Level> = vec![Level {
        px: px("99.90"),
        qty: q("0.6"),
    }];
    let asks: Vec<Level> = vec![Level {
        px: px("100.10"),
        qty: q("0.6"),
    }];
    hft_strategy_sdk::testing::MockViews::book(&mut fx.v.m, S, &bids, &asks);
    fx.cm(A1, G7).fund_shrink = 0;
    let tg = pair_t(&[(0, "10"), (1, "-10")], |p| {
        p.first_leg = FirstLeg::SpotFirst;
        p.slice.min_chunk_notional = usd("0");
    });
    inject(&mut fx, &tg);
    fx.cm(A1, G7).fund_shrink = 2;
    fx.touch(S);
    assert_eq!(fx.adv().orders[0].intent.qty, q("0.1"), "EXE-167.8");
}

#[test]
fn exe_168_first_leg_quote_blocks() {
    let mut fx = Fx::new();
    let b = fx.v.m.bbo_of(S);
    hft_strategy_sdk::testing::MockViews::conn(&mut fx.v.m, S, b.conn_live_tsc, true, true);
    fx.live.retain(|x| *x != S);
    let r = fx.go(&t0("10", "-10"));
    assert!(r.orders.is_empty());
    assert_eq!(fx.cell(A1, G7).gated_since, t(0));
}

#[test]
fn exe_169_link_tolerance_updates() {
    for (row, tol, fix) in [("EXE-169.1", "0.10", false), ("EXE-169.2", "0.05", true)] {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["1.08", "-1.0"]);
        fx.spot(A1, A, G7, "1.08", "1.08");
        let mut l = link(1, A1, G7, LinkPhase::Done, LinkBrokenCause::None);
        l.tolerance = rate(tol);
        l.residual = ResidualPolicy::Hold;
        fx.link(&l);
        let r = fx.adv();
        let f = r.ord(ExecOrderKind::HedgeFix);
        assert_eq!(!f.is_empty(), fix, "{row}");
        if fix {
            assert_eq!(
                (f[0].intent.side, f[0].intent.qty),
                (Side::Sell, q("0.08")),
                "{row}"
            );
        }
    }
}

#[test]
fn exe_511_second_leg_worst_notional() {
    for (row, perp_first, hedge, want) in [
        ("EXE-511.1", false, "500", "4.9"),
        ("EXE-511.2", true, "500", "4.9"),
        ("EXE-511.3", false, "2000", "9.9"),
    ] {
        let mut fx = Fx::new();
        fx.risk(A1, |r| r.max_hedge_notional = usd(hedge));
        let tg = pair_t(&[(0, "10"), (1, "-10")], |p| {
            p.first_leg = if perp_first {
                FirstLeg::PerpFirst
            } else {
                FirstLeg::SpotFirst
            };
        });
        let o = fx.go(&tg).orders[0];
        assert_eq!(o.intent.qty, q(want), "{row}");
        let _ = Rate(0);
    }
}
