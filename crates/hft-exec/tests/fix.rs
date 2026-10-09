//! EXE-205–EXE-216, EXE-220–EXE-222: repair of the excess leg and the HedgeFix command.
mod common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{ExecSpec, LinkRole, TargetState};
use hft_types::{CapSource, RejectBits, Side, Tif, Tsc};

fn naked_g7(fx: &mut Fx, s: &str, p: &str) {
    fx.legs(A1, G7, &[s, p]);
    fx.spot(A1, A, G7, s, s);
}

fn fix_now(fx: &mut Fx) -> Rec {
    fx.leg_adjust(A1, G7);
    fx.adv()
}

#[test]
fn exe_205_hedge_fix_after_link() {
    let mut fx = Fx::new();
    fx.at(1000);
    let tg = t0("1.00", "-1.5");
    inject(&mut fx, &tg);
    fx.cm(A1, G7).view.state = TargetState::Idle;
    naked_g7(&mut fx, "1.00", "-1.5");
    fx.link(&link(1, A1, G7, LinkPhase::Done, LinkBrokenCause::None));
    let r = fx.adv();
    let o = r.ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!(
        (
            o.kind.code(),
            o.origin,
            o.intent.side,
            o.intent.inst,
            o.intent.qty
        ),
        (5, ExecOrigin::Engine, Side::Buy, P, q("0.5"))
    );
    assert_eq!(
        (o.intent.tif, o.intent.px, o.hedge_cap_qty),
        (Tif::Ioc, px("100.70"), q("0.6"))
    );
    assert_eq!(o.intent.link.get().unwrap().role, LinkRole::HedgeFix);
    assert_eq!(o.reason, hft_strategy_sdk::TargetReason::Rebalance);
    let d = r.dec(7)[0];
    assert_eq!((d.leg_ix, d.qty), (1, q("0.5")));
    assert_eq!(fx.cell(A1, G7).fix.first_tsc, t(1000));
    assert_eq!(r.timer_set(ExecTimerKind::Fix), vec![t(2000)]);
}

#[test]
fn exe_206_trigger_thresholds() {
    let mut p = c0();
    p.fix_tolerance_default = rate("0.05");
    for (row, s, pp, tol, want) in [
        ("EXE-206.1", "1.05", "-1.0", "0.02", None),
        ("EXE-206.2", "1.05000001", "-1.0", "0.02", Some("0.05")),
        ("EXE-206.3", "10.00", "-9.8", "0.02", None),
        ("EXE-206.4", "10.00", "-9.79999999", "0.02", Some("0.20")),
        ("EXE-206.5", "10.00", "-9.6", "0", None),
        ("EXE-206.6", "10.00", "-9.4", "0", Some("0.60")),
        ("EXE-206.7", "10.00", "-9.5", "0", None),
    ] {
        // S min notional 4 USDT: 0.05 S at the capped sell limit (99.41) is still orderable.
        let mut insts = default_insts();
        insts[0].min_notional = 4_000_000;
        let mut fx = Fx::with_cfg(cfg(p), vec![g7(), g8()], insts);
        naked_g7(&mut fx, s, pp);
        fx.cm(A1, G7).tolerance = rate(tol);
        let r = fix_now(&mut fx);
        let f = r.ord(ExecOrderKind::HedgeFix);
        match want {
            None => assert!(f.is_empty(), "{row}"),
            Some(x) => assert_eq!((f[0].intent.side, f[0].intent.qty), (Side::Sell, q(x)), "{row}"),
        }
    }
}

#[test]
fn exe_207_suppressions() {
    type Edit = fn(&mut Fx);
    let rows: [(&str, Edit, bool); 13] = [
        ("EXE-207.1", |fx| fx.lv(A1, G7, |l| l.open_links = 1), false),
        ("EXE-207.2", |fx| fx.lv(A1, G7, |l| l.has_unknown = true), false),
        ("EXE-207.3", |fx| fx.infl(A1, G7, 0, "0.1"), false),
        (
            "EXE-207.4",
            |fx| fx.lv(A1, G7, |l| l.leg[0].inflight_resting = q("0.5")),
            true,
        ),
        ("EXE-207.5", |fx| fx.cm(A1, G7).cap_block = CAP_BROKEN, false),
        ("EXE-207.6", |fx| fx.cm(A1, G7).cap_block = CAP_REORG, false),
        ("EXE-207.7", |fx| fx.risk(A1, |r| r.hard = true), false),
        (
            "EXE-207.8",
            |fx| {
                let c = fx.cm(A1, G7);
                c.active.kind = ACT_SLICE;
                c.active.ext = [1, 2, 0, 0];
            },
            false,
        ),
        ("EXE-207.9", |fx| fx.cm(A1, G7).serial = SERIAL_FALLBACK, false),
        ("EXE-207.10", |fx| fx.cm(A1, G7).ext_deadline = t(60_000), false),
        ("EXE-207.11", |fx| fx.cm(A1, G7).hold = MANUAL_HOLD, false),
        ("EXE-207.12", |fx| fx.acct(A1).venue_open &= !(1u64 << B.0), false),
        ("EXE-207.13", |fx| fx.acct(A1).venue_open = 1u64 << B.0, true),
    ];
    for (row, edit, order) in rows {
        let mut fx = Fx::new();
        naked_g7(&mut fx, "1.00", "-1.5");
        edit(&mut fx);
        let r = fix_now(&mut fx);
        let f = r.ord(ExecOrderKind::HedgeFix);
        assert_eq!(!f.is_empty(), order, "{row}");
        let c = fx.cell(A1, G7);
        assert_eq!(c.fix.count, 0, "{row}");
        if order {
            assert_eq!(
                (f[0].intent.side, f[0].intent.qty),
                (Side::Buy, q("0.5")),
                "{row}"
            );
        } else if row == "EXE-207.12" {
            assert_eq!(c.fix.first_tsc, t(0), "{row}");
        } else if row != "EXE-207.5" {
            assert_eq!(c.fix.first_tsc, Tsc(0), "{row}");
        }
        if row == "EXE-207.7" {
            assert_eq!(c.fix, FixState::ZERO, "{row}");
        }
    }
}

#[test]
fn exe_208_hard_clears_fixstate() {
    let mut fx = Fx::new();
    naked_g7(&mut fx, "1.00", "-1.5");
    {
        let c = fx.cm(A1, G7);
        c.fix.count = 2;
        c.fix.first_tsc = t(0);
    }
    fx.at(3000);
    fx.risk(A1, |r| r.hard = true);
    fx.risk_ev(A1, cap_set(CapSource::KILL, A1, GNONE));
    let r = fx.adv();
    assert!(r.orders.is_empty());
    let c = fx.cell(A1, G7);
    assert_eq!((c.fix.count, c.fix.first_tsc), (0, Tsc(0)));
    fx.at(9000);
    fx.risk(A1, |r| r.hard = false);
    fx.risk_ev(A1, cap_clr(CapSource::KILL, A1, GNONE));
    fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
    let r = fx.adv();
    assert!(r.ord(ExecOrderKind::HedgeFix).is_empty());
    let c = fx.cell(A1, G7);
    assert!(matches!(c.view.state, TargetState::Broken(_)));
    assert_eq!((c.broken_since, c.auto_fix.due_tsc), (t(9000), t(129_000)));
}

#[test]
fn exe_209_excess_leg_choice() {
    // .1 / .2
    for (row, s, p, side, inst, x) in [
        ("EXE-209.1", "1.50", "-1.0", Side::Sell, S, "0.50"),
        ("EXE-209.2", "1.00", "-1.5", Side::Buy, P, "0.5"),
    ] {
        let mut fx = Fx::new();
        naked_g7(&mut fx, s, p);
        let o = fix_now(&mut fx).ord(ExecOrderKind::HedgeFix)[0];
        assert_eq!(
            (o.intent.side, o.intent.inst, o.intent.qty),
            (side, inst, q(x)),
            "{row}"
        );
    }
    // .3
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G11, &[(P, B), (Q, C)])]);
    fx.legs(A1, G11, &["-2.0", "2.5"]);
    fx.leg_adjust(A1, G11);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!(
        (o.intent.side, o.intent.inst, o.intent.qty),
        (Side::Sell, Q, q("0.5")),
        "EXE-209.3"
    );
    assert!(o.intent.reduce_only());
    // .4
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G12, &[(P, B), (S, A), (Q, C)])]);
    fx.legs(A1, G12, &["1.0", "1.00", "-1.5"]);
    fx.spot(A1, A, G12, "1", "1");
    fx.leg_adjust(A1, G12);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!(
        (o.intent.side, o.intent.inst, o.intent.qty),
        (Side::Sell, P, q("0.5")),
        "EXE-209.4"
    );
}

#[test]
fn exe_210_fix_qty_caps() {
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G12, &[(S, A), (S2, D), (P, B)])]);
    fx.legs(A1, G12, &["0.30", "0.29", "-0.1"]);
    fx.spot(A1, A, G12, "1", "1");
    fx.leg_adjust(A1, G12);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix)[0];
    assert_eq!((o.intent.inst, o.intent.qty), (S, q("0.30")), "EXE-210.1");
    for (row, inv, free, want) in [
        ("EXE-210.2", "1.50", "0.20", "0.20"),
        ("EXE-210.3", "0.10", "0.20", "0.10"),
    ] {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["1.50", "-1.0"]);
        fx.spot(A1, A, G7, inv, free);
        let o = fix_now(&mut fx).ord(ExecOrderKind::HedgeFix)[0];
        assert_eq!((o.intent.side, o.intent.qty), (Side::Sell, q(want)), "{row}");
    }
}

#[test]
fn exe_211_below_min_order_no_fix() {
    let mut insts = default_insts();
    insts[0].min_qty = dec("0.1", 8);
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    let mut pp = pair0();
    pp.residual = hft_strategy_sdk::ResidualPolicy::Hold;
    inject(
        &mut fx,
        &target(
            A1,
            G7,
            &[(0, q("1.08")), (1, q("-1.0"))],
            ExecSpec::PairOnFill(pp),
        ),
    );
    fx.cm(A1, G7).view.state = TargetState::Idle;
    naked_g7(&mut fx, "1.08", "-1.0");
    let r = fix_now(&mut fx);
    assert!(r.orders.is_empty());
    assert_eq!(fx.cell(A1, G7).fix.count, 0);
    assert!(r.notes_n(note::EXEC_BROKEN).is_empty());
}

fn issued(fx: &mut Fx) -> (ExecOrder, hft_strategy_sdk::OrderRef) {
    naked_g7(fx, "1.00", "-1.5");
    let o = fix_now(fx).ord(ExecOrderKind::HedgeFix)[0];
    let rf = fx.next_ref(&o);
    (o, rf)
}

#[test]
fn exe_212_fix_count_by_outcome() {
    type Act = fn(&mut Fx, &ExecOrder, hft_strategy_sdk::OrderRef);
    let placed = |fx: &mut Fx, o: &ExecOrder, rf: hft_strategy_sdk::OrderRef| {
        fx.place_res(o, PlaceResult::Placed(rf));
    };
    let _ = placed;
    let rows: [(&str, Act, u8, u8, u8); 6] = [
        (
            "EXE-212.1",
            |fx, o, rf| {
                fx.place_res(o, PlaceResult::Placed(rf));
                fx.exch_reject(rf, G7);
            },
            1,
            1,
            0,
        ),
        (
            "EXE-212.2",
            |fx, o, rf| {
                fx.place_res(o, PlaceResult::Placed(rf));
                fx.nofill(rf, G7);
            },
            1,
            0,
            1,
        ),
        (
            "EXE-212.3",
            |fx, o, _| {
                fx.place_res(o, PlaceResult::Rejected(RejectBits::PX_BAND));
            },
            1,
            1,
            0,
        ),
        (
            "EXE-212.4",
            |fx, o, rf| {
                fx.place_res(o, PlaceResult::Placed(rf));
                fx.local_fail(rf, G7, 6);
            },
            1,
            1,
            0,
        ),
        (
            "EXE-212.6",
            |fx, o, _| {
                fx.place_res(o, PlaceResult::Rejected(RejectBits::RATE_BACKOFF));
            },
            0,
            0,
            0,
        ),
        (
            "EXE-212.7",
            |fx, o, rf| {
                fx.place_res(o, PlaceResult::Placed(rf));
                fx.fill(rf, G7, "0.2", true);
            },
            0,
            0,
            0,
        ),
    ];
    for (row, act, count, fail, nofill) in rows {
        let mut fx = Fx::new();
        let (o, rf) = issued(&mut fx);
        act(&mut fx, &o, rf);
        let c = fx.cell(A1, G7);
        assert_eq!(
            (c.fix.count, c.fail_streak, c.nofill_streak),
            (count, fail, nofill),
            "{row}"
        );
    }
    for status in [2u32, 3, 5, 7] {
        let mut fx = Fx::new();
        let (o, rf) = issued(&mut fx);
        fx.place_res(&o, PlaceResult::Placed(rf));
        fx.local_fail(rf, G7, status);
        let c = fx.cell(A1, G7);
        assert_eq!((c.fix.count, c.wake_at), (0, t(500)), "EXE-212.5 status {status}");
    }
}

#[test]
fn exe_213_fix_cooldown() {
    let mut fx = Fx::new();
    let (o, rf) = issued(&mut fx);
    fx.place_res(&o, PlaceResult::Placed(rf));
    fx.at(5);
    fx.nofill(rf, G7);
    assert_eq!(fx.cell(A1, G7).fix.count, 1);
    fx.adv();
    fx.at(999);
    fx.touch(P);
    assert!(fx.adv().ord(ExecOrderKind::HedgeFix).is_empty(), "EXE-213.1");
    fx.at(1000);
    fx.fire(ExecTimerKind::Fix, A1, G7);
    assert_eq!(fx.adv().ord(ExecOrderKind::HedgeFix).len(), 1, "EXE-213.2");
}

#[test]
fn exe_214_fix_deadline() {
    let mut fx = Fx::new();
    naked_g7(&mut fx, "1.00", "-1.5");
    fx.leg_adjust(A1, G7);
    let mut r = fx.adv();
    let mut ms = 0;
    let mut broken_at = None;
    while ms <= 10_001 {
        for o in r.ord(ExecOrderKind::HedgeFix) {
            fx.place_res(&o, PlaceResult::Rejected(RejectBits::RATE_BACKOFF));
        }
        ms = match ms {
            9500 => 9999,
            9999 => 10_000,
            10_000 => 10_001,
            x => x + 500,
        };
        if ms > 10_001 {
            break;
        }
        fx.at(ms);
        fx.touch(P);
        r = fx.adv();
        if r.states().contains(&(1, 4, pack(3, 3, 0))) {
            broken_at = Some(ms);
            assert_eq!(r.notes_n(note::EXEC_BROKEN)[0].level, NoteLevel::P0);
            assert_eq!(r.broken.len(), 1);
        }
    }
    assert_eq!(broken_at, Some(10_001));
}

#[test]
fn exe_215_fix_max() {
    let mut fx = Fx::new();
    naked_g7(&mut fx, "1.00", "-1.5");
    fx.leg_adjust(A1, G7);
    let mut r = fx.adv();
    for k in 1..=3u64 {
        let o = r.ord(ExecOrderKind::HedgeFix)[0];
        let rf = fx.placed(&o);
        fx.exch_reject(rf, G7);
        assert_eq!(fx.cell(A1, G7).fix.count, k as u8);
        fx.at(1000 * k);
        fx.fire(ExecTimerKind::Fix, A1, G7);
        r = fx.adv();
        if k == 2 {
            assert!(
                !matches!(fx.cell(A1, G7).view.state, TargetState::Broken(_)),
                "EXE-215.1"
            );
            assert_eq!(r.ord(ExecOrderKind::HedgeFix).len(), 1, "EXE-215.1");
        }
    }
    assert!(r.states().contains(&(1, 4, pack(3, 3, 0))), "EXE-215.2");
    assert_eq!(r.notes_n(note::EXEC_BROKEN).len(), 1);
    assert!(r.ord(ExecOrderKind::HedgeFix).is_empty());
}

#[test]
fn exe_216_fixstate_reset() {
    let mut fx = Fx::new();
    naked_g7(&mut fx, "1.00", "-1.5");
    {
        let c = fx.cm(A1, G7);
        c.fix.count = 2;
        c.fix.first_tsc = t(0);
    }
    fx.legs(A1, G7, &["1.00", "-1.0"]);
    let r = fix_now(&mut fx);
    assert_eq!(fx.cell(A1, G7).fix, FixState::ZERO);
    assert!(r.orders.is_empty());
}

#[test]
fn exe_220_cmd_bypasses_suppression() {
    type Edit = fn(&mut Fx);
    let rows: [(&str, Edit); 5] = [
        ("EXE-220.1", |fx| {
            let c = fx.cm(A1, G7);
            c.fix.count = 2;
            c.fix.next_tsc = t(900);
        }),
        ("EXE-220.2", |fx| {
            let c = fx.cm(A1, G7);
            c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
            c.auto_fix.phase = AutoFixPhase::Primed as u8;
        }),
        ("EXE-220.3", |fx| fx.cm(A1, G7).hold = MANUAL_HOLD),
        ("EXE-220.4", |fx| fx.acct(A1).venue_open = 0),
        ("EXE-220.5", |fx| fx.cm(A1, G7).cap_block = CAP_REORG),
    ];
    for (row, edit) in rows {
        let mut fx = Fx::new();
        naked_g7(&mut fx, "1.00", "-1.5");
        edit(&mut fx);
        let (res, r) = fx.cmd_fix(A1, G7);
        assert_eq!(res, Ok(()), "{row}");
        let o = r.ord(ExecOrderKind::CmdHedgeFix)[0];
        assert_eq!(
            (
                o.kind.code(),
                o.intent.side,
                o.intent.inst,
                o.intent.qty,
                o.hedge_cap_qty
            ),
            (9, Side::Buy, P, q("0.5"), q("0.6")),
            "{row}"
        );
        assert_eq!(
            r.dec(12)[0].origin,
            hft_event::TargetOriginWire::EngineCommand as u8,
            "{row}"
        );
        if row == "EXE-220.2" {
            assert_eq!(fx.cell(A1, G7).auto_fix.phase, AutoFixPhase::Cancelled as u8);
        }
        if row == "EXE-220.3" {
            assert_eq!(fx.cell(A1, G7).hold & MANUAL_HOLD, 0);
            let n = r.notes_n(note::EXEC_MANUAL_HOLD);
            assert_eq!(n[0].level, NoteLevel::Info);
            assert_eq!(&n[0].v[..3], &[0x01_0000_0701, 0, 2]);
        }
    }
    let mut fx = Fx::new();
    naked_g7(&mut fx, "10.00", "-9.9");
    let (res, r) = fx.cmd_fix(A1, G7);
    assert_eq!(res, Ok(()), "EXE-220.6");
    let o = r.ord(ExecOrderKind::CmdHedgeFix)[0];
    assert_eq!(
        (o.intent.side, o.intent.qty),
        (Side::Sell, q("0.10")),
        "EXE-220.6"
    );
}

#[test]
fn exe_221_cmd_errors() {
    type Edit = fn(&mut Fx);
    let rows: [(&str, Edit, ExecCmdErr); 7] = [
        ("EXE-221.1", |fx| fx.risk(A1, |r| r.hard = true), ExecCmdErr::Hard),
        (
            "EXE-221.3",
            |fx| naked_g7(fx, "1.05", "-1.0"),
            ExecCmdErr::NothingToFix,
        ),
        (
            "EXE-221.5",
            |fx| fx.lv(A1, G7, |l| l.open_links = 1),
            ExecCmdErr::NotQuiescent,
        ),
        (
            "EXE-221.6",
            |fx| fx.lv(A1, G7, |l| l.has_unknown = true),
            ExecCmdErr::NotQuiescent,
        ),
        (
            "EXE-221.7",
            |fx| fx.infl(A1, G7, 1, "0.1"),
            ExecCmdErr::NotQuiescent,
        ),
        ("EXE-221.8", |fx| fx.quote_down(P), ExecCmdErr::QuoteUnavailable),
        ("EXE-221.9", |fx| fx.quote_down(P), ExecCmdErr::QuoteUnavailable),
    ];
    for (row, edit, want) in rows {
        let mut fx = Fx::new();
        naked_g7(&mut fx, "1.00", "-1.5");
        if row == "EXE-221.9" {
            fx.cm(A1, G7).hold = MANUAL_HOLD;
        }
        edit(&mut fx);
        let (res, r) = fx.cmd_fix(A1, G7);
        assert_eq!(res, Err(want), "{row}");
        assert!(r.is_empty(), "{row}");
        if row == "EXE-221.9" {
            assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD, "{row}");
        }
    }
    let mut fx = Fx::new();
    assert_eq!(
        fx.cmd_fix(A1, hft_types::GroupId(0x0100_0099)).0,
        Err(ExecCmdErr::UnknownGroup),
        "EXE-221.2"
    );
    let mut insts = default_insts();
    insts[2].min_qty = dec("1.0", 8);
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    naked_g7(&mut fx, "1.00", "-1.5");
    assert_eq!(fx.cmd_fix(A1, G7).0, Err(ExecCmdErr::NothingToFix), "EXE-221.4");
}

#[test]
fn exe_222_cmd_cancels_autofix() {
    let mut fx = Fx::new();
    naked_g7(&mut fx, "1.00", "-1.5");
    fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
    fx.adv();
    fx.at(10_000);
    let (res, r) = fx.cmd_fix(A1, G7);
    res.unwrap();
    let rf = fx.placed(&r.orders[0]);
    fx.legs(A1, G7, &["1.00", "-1.3"]);
    fx.fill(rf, G7, "0.2", true);
    fx.at(130_000);
    fx.touch(P);
    let r = fx.adv();
    assert!(r.dec(14).is_empty() && r.orders.is_empty(), "step 2");
    assert_eq!(fx.cell(A1, G7).auto_fix.phase, AutoFixPhase::Cancelled as u8);
    fx.at(140_000);
    fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G7));
    fx.adv();
    fx.at(150_000);
    fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
    fx.adv();
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.due_tsc),
        (AutoFixPhase::Primed as u8, t(270_000))
    );
}
