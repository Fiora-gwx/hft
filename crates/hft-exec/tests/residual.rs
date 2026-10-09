//! EXE-280–EXE-283: dust and residuals.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{ExecSpec, ExecUpdateKind, ResidualPolicy, TargetReason, TargetState, Urgency};
use hft_types::{CapSource, Side, Tsc};

#[test]
fn exe_280_dust_no_action() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10.00", "-10.0"));
    fx.legs(A1, G7, &["10.05", "-10.0"]);
    fx.spot(A1, A, G7, "10.05", "10.05");
    fx.touch(S);
    let r = fx.adv();
    assert!(r.states().contains(&(2, 1, 0)));
    assert!(r.upd().contains(&ExecUpdateKind::Done {
        cleared_by_engine: false
    }));
    for s in 1..=60u64 {
        fx.at(s * 1000);
        fx.touch(S);
        fx.leg_adjust(A1, G7);
        fx.adv();
    }
    assert!(fx.log.orders.is_empty());
    assert!(fx.log.dec(7).is_empty() && fx.log.dec(8).is_empty());
    assert_eq!(fx.cell(A1, G7).fix.first_tsc, Tsc(0));
}

fn resid_fx(policy: Option<ResidualPolicy>, target_policy: ResidualPolicy, s_target: &str) -> Fx {
    let mut fx = Fx::new();
    let mut pp = pair0();
    pp.residual = target_policy;
    inject(
        &mut fx,
        &target(
            A1,
            G7,
            &[(0, q(s_target)), (1, q("-10.0"))],
            ExecSpec::PairOnFill(pp),
        ),
    );
    fx.legs(A1, G7, &["10.06", "-10.0"]);
    fx.spot(A1, A, G7, "10.06", "10.06");
    if let Some(p) = policy {
        let mut l = link(1, A1, G7, LinkPhase::Done, LinkBrokenCause::None);
        l.residual = p;
        fx.link(&l);
    }
    fx
}

#[test]
fn exe_281_residual_policies() {
    let trims = |r: &Rec| r.ord(ExecOrderKind::ResidualTrim);
    // .1
    let mut fx = resid_fx(Some(ResidualPolicy::Defer), ResidualPolicy::Hold, "20.00");
    fx.touch(S);
    assert!(trims(&fx.adv()).is_empty(), "EXE-281.1");
    // .2
    let mut fx = resid_fx(Some(ResidualPolicy::Defer), ResidualPolicy::Hold, "10.00");
    fx.touch(S);
    assert!(fx.adv().orders.is_empty(), "EXE-281.2");
    // .3
    let mut fx = resid_fx(Some(ResidualPolicy::TrimExcess), ResidualPolicy::Hold, "10.00");
    fx.touch(S);
    let r = fx.adv();
    let o = trims(&r);
    assert_eq!(
        (
            o[0].kind.code(),
            o[0].intent.side,
            o[0].intent.inst,
            o[0].intent.qty
        ),
        (6, Side::Sell, S, q("0.06")),
        "EXE-281.3"
    );
    assert_eq!(r.dec(8).len(), 1, "EXE-281.3");
    // .4
    let mut insts = default_insts();
    insts[0].min_qty = dec("0.1", 8);
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    let mut pp = pair0();
    pp.residual = ResidualPolicy::TrimExcess;
    inject(
        &mut fx,
        &target(
            A1,
            G7,
            &[(0, q("10.00")), (1, q("-10.0"))],
            ExecSpec::PairOnFill(pp),
        ),
    );
    fx.legs(A1, G7, &["10.06", "-10.0"]);
    fx.spot(A1, A, G7, "10.06", "10.06");
    fx.touch(S);
    assert!(fx.adv().orders.is_empty(), "EXE-281.4");
    // .5
    let mut fx = resid_fx(Some(ResidualPolicy::Hold), ResidualPolicy::Hold, "10.00");
    fx.touch(S);
    assert!(fx.adv().orders.is_empty(), "EXE-281.5");
    // .6
    let mut fx = Fx::new();
    let mut pp = pair0();
    pp.residual = ResidualPolicy::Hold;
    inject(
        &mut fx,
        &target(
            A1,
            G7,
            &[(0, q("1.08")), (1, q("-1.0"))],
            ExecSpec::PairOnFill(pp),
        ),
    );
    fx.legs(A1, G7, &["1.08", "-1.0"]);
    fx.spot(A1, A, G7, "1.08", "1.08");
    fx.touch(S);
    let o = fx.adv().ord(ExecOrderKind::HedgeFix);
    assert_eq!(
        (o[0].intent.side, o[0].intent.qty),
        (Side::Sell, q("0.08")),
        "EXE-281.6"
    );
    // .7 / .8 / .9
    type Edit = fn(&mut Fx);
    let rows: [(&str, Edit); 3] = [
        ("EXE-281.7", |fx| fx.cm(A1, G7).hold = MANUAL_HOLD),
        ("EXE-281.8", |fx| fx.acct(A1).venue_open &= !(1u64 << A.0)),
        ("EXE-281.9", |fx| fx.quote_down(S)),
    ];
    for (row, edit) in rows {
        let mut fx = resid_fx(Some(ResidualPolicy::TrimExcess), ResidualPolicy::Hold, "10.00");
        edit(&mut fx);
        fx.touch(S);
        assert!(fx.adv().orders.is_empty(), "{row}");
        assert_eq!(fx.cell(A1, G7).fix.count, 0, "{row}");
    }
    // .10
    let mut fx = resid_fx(None, ResidualPolicy::TrimExcess, "10.00");
    fx.touch(S);
    assert_eq!(trims(&fx.adv()).len(), 1, "EXE-281.10");
}

#[test]
fn exe_282_zero_target_tail() {
    for (row, net, stuck) in [
        ("EXE-282.1", "-0.08", true),
        ("EXE-282.2", "-0.05", false),
        ("EXE-282.3", "-0.04", false),
    ] {
        let mut fx = Fx::new();
        let mut tg = t1("0");
        tg.reason = TargetReason::Exit;
        tg.urgency = Urgency::Urgent;
        inject(&mut fx, &tg);
        fx.legs(A1, G8, &[net]);
        fx.touch(P);
        let r = fx.adv();
        assert!(r.orders.is_empty(), "{row}");
        if stuck {
            assert!(r.states().contains(&(2, 3, pack(2, 8, 0))), "{row}");
            assert_eq!(
                r.notes_n(hft_exec::decision::note::EXEC_STUCK)[0].level,
                hft_event::NoteLevel::P1,
                "{row}"
            );
        } else {
            assert!(r.states().contains(&(2, 1, 0)), "{row}");
        }
    }
}

#[test]
fn exe_283_after_write_off() {
    // .1
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.06", "-1.0"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Failed as u8;
        c.auto_fix.fail = 2;
    }
    fx.legs(A1, G7, &["1.00", "-1.0"]);
    fx.leg_adjust(A1, G7);
    assert!(fx.adv().orders.is_empty(), "EXE-283.1");
    fx.risk_ev(A1, cap_clr(CapSource::BROKEN, A1, G7));
    let r = fx.adv();
    assert!(r.states().contains(&(4, 1, 0)), "EXE-283.1");
    assert_eq!(fx.cell(A1, G7).auto_fix.phase, 0, "EXE-283.1");
    // .2
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10.00", "-10.0"));
    fx.legs(A1, G7, &["10.00", "-10.0"]);
    fx.leg_adjust(A1, G7);
    let r = fx.adv();
    assert!(
        r.states().contains(&(2, 1, 0)) && r.orders.is_empty(),
        "EXE-283.2"
    );
    assert!(
        r.upd().contains(&ExecUpdateKind::Done {
            cleared_by_engine: false
        }),
        "EXE-283.2"
    );
    // .3
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.0"]);
    fx.leg_adjust(A1, G7);
    let r = fx.adv();
    assert!(r.orders.is_empty() && r.decisions.is_empty(), "EXE-283.3");
}
