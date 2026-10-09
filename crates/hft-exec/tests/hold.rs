//! EXE-310–EXE-314: manual_hold.
mod common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{
    EngineOrigin, ExternalCause, MarginLevel, ResidualPolicy, StuckReason, TargetOrigin, TargetReason,
    TargetState,
};
use hft_types::{Side, Tsc};

fn manual(fx: &mut Fx) -> Rec {
    fx.ext(A1, &xfill(G7, 1, P, B, Side::Buy, "0.5", ExternalCause::Manual))
        .1
}

#[test]
fn exe_310_manual_hold_set() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["1.00", "-1.0"]);
    {
        let c = fx.cm(A1, G7);
        c.fix.count = 1;
        c.fix.first_tsc = t(0);
        c.ext_deadline = t(6000);
        c.aux16 |= (1 << ExecTimerKind::Fix as u16) | (1 << ExecTimerKind::External as u16);
    }
    fx.at(2000);
    fx.legs(A1, G7, &["1.00", "-0.5"]);
    let r = manual(&mut fx);
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.hold, c.fix, c.ext_deadline),
        (MANUAL_HOLD, FixState::ZERO, Tsc(0))
    );
    let cancels: Vec<_> = r.timers.iter().filter(|x| x.cancel).map(|x| x.kind()).collect();
    assert!(cancels.contains(&Some(ExecTimerKind::Fix)) && cancels.contains(&Some(ExecTimerKind::External)));
    let n = r.notes_n(note::EXEC_MANUAL_HOLD);
    assert_eq!(n[0].level, NoteLevel::P1);
    assert_eq!(
        &n[0].v[..7],
        &[
            0x01_0000_0701,
            1,
            1,
            21,
            Side::Buy as u64,
            q("0.5").0 as u64,
            q("0.5").0 as u64
        ]
    );
    assert!(r.has_dec(9, pack(6, 3, 2)) && r.has_dec(9, pack(6, 0, 5)));
}

fn l3_engine(fx: &mut Fx) {
    inject(fx, &t0("8", "-8"));
    let c = fx.cm(A1, G7);
    c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
    c.view.intent.reason = TargetReason::MarginStress;
    fx.acct(A1).force_active = true;
}

#[test]
fn exe_311_deferred_hold() {
    // .1 L3 then L2.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["9", "-9"]);
    l3_engine(&mut fx);
    let r = manual(&mut fx);
    assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD_DEFERRED, "EXE-311.1");
    assert!(r.notes_n(note::EXEC_MANUAL_HOLD).is_empty(), "EXE-311.1");
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    let r = fx.adv();
    assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD, "EXE-311.1");
    assert_eq!(
        r.notes_n(note::EXEC_MANUAL_HOLD)[0].level,
        NoteLevel::P1,
        "EXE-311.1"
    );
    // .2 HardAlign reached.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["9", "-9"]);
    inject(&mut fx, &t0("6", "-6"));
    {
        let c = fx.cm(A1, G7);
        c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
        c.view.intent.reason = TargetReason::Hard;
        c.serial = SERIAL_FALLBACK;
    }
    manual(&mut fx);
    assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD_DEFERRED, "EXE-311.2");
    fx.legs(A1, G7, &["6", "-6"]);
    fx.touch(S);
    let r = fx.adv();
    assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD, "EXE-311.2");
    assert_eq!(r.notes_n(note::EXEC_MANUAL_HOLD).len(), 1, "EXE-311.2");
    // .3 flatten target reached.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1", "-1"]);
    fx.flatten(A1);
    let r = fx.adv();
    for o in r.orders.clone() {
        let rf = fx.placed(&o);
        fx.nofill(rf, o.intent.group);
    }
    manual(&mut fx);
    assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD_DEFERRED, "EXE-311.3");
    fx.legs(A1, G7, &["0", "0"]);
    fx.at(1000);
    fx.touch(S);
    fx.adv();
    assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD, "EXE-311.3");
    // .4 strategy action clears the deferred bit.
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["9", "-9"]);
    l3_engine(&mut fx);
    manual(&mut fx);
    let now = fx.now();
    fx.ex.note_strategy_action(A1, G7, now, &mut fx.out);
    fx.out.clear();
    assert_eq!(fx.cell(A1, G7).hold, 0, "EXE-311.4");
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    fx.adv();
    assert_eq!(fx.cell(A1, G7).hold, 0, "EXE-311.4");
}

#[test]
fn exe_312_release_actions() {
    type Act = fn(&mut Fx) -> Rec;
    let rows: [(&str, Act, Option<u64>); 8] = [
        (
            "EXE-312.1",
            |fx| {
                let mut tg = t0("10", "-10");
                tg.tag = 0x11;
                fx.set(I1, &tg).1
            },
            Some(1),
        ),
        (
            "EXE-312.2",
            |fx| {
                let now = fx.now();
                fx.ex.note_strategy_action(A1, G7, now, &mut fx.out);
                Rec::take(&mut fx.out)
            },
            Some(1),
        ),
        (
            "EXE-312.3",
            |fx| {
                fx.legs(A1, G7, &["1.00", "-1.5"]);
                fx.cmd_fix(A1, G7).1
            },
            Some(2),
        ),
        (
            "EXE-312.4",
            |fx| {
                fx.legs(A1, G7, &["10", "-10"]);
                fx.spot(A1, A, G7, "10", "10");
                fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
                fx.force(A1, B, "0.2");
                fx.adv()
            },
            Some(3),
        ),
        (
            "EXE-312.5",
            |fx| {
                let now = fx.now();
                fx.ex.clear_account(A1, now, &mut fx.out);
                fx.adv()
            },
            Some(4),
        ),
        ("EXE-312.6", |fx| fx.clear(A1, G7), None),
        ("EXE-312.7", |fx| fx.set(I1, &t0("10", "-10")).1, None),
        ("EXE-312.8", |fx| fx.set(I1, &t0("-1", "-10")).1, None),
    ];
    for (row, act, reason) in rows {
        let mut fx = Fx::new();
        inject(&mut fx, &t0("10", "-10"));
        {
            let c = fx.cm(A1, G7);
            c.hold = MANUAL_HOLD;
            c.fix.first_tsc = t(1);
        }
        fx.at(50_000);
        let r = act(&mut fx);
        let n = r.notes_n(note::EXEC_MANUAL_HOLD);
        match reason {
            Some(x) => {
                assert_eq!(fx.cell(A1, G7).hold & MANUAL_HOLD, 0, "{row}");
                assert_eq!(n[0].level, NoteLevel::Info, "{row}");
                assert_eq!(&n[0].v[..3], &[0x01_0000_0701, 0, x], "{row}");
            }
            None => {
                assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD, "{row}");
                assert!(n.is_empty(), "{row}");
            }
        }
        if row == "EXE-312.1" {
            assert_eq!(fx.cell(A1, G7).fix.first_tsc, Tsc(0), "{row}");
        }
    }
    let mut fx = Fx::new();
    {
        let c = fx.cm(A1, G7);
        c.hold = MANUAL_HOLD;
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::WaitHold as u8;
    }
    fx.at(50_000);
    let now = fx.now();
    fx.ex.note_strategy_action(A1, G7, now, &mut fx.out);
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.hold, c.auto_fix.phase, c.auto_fix.due_tsc),
        (0, AutoFixPhase::Primed as u8, t(170_000)),
        "EXE-312.9"
    );
}

#[test]
fn exe_313_hold_suppression() {
    let base = || {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["1.00", "-1.5"]);
        fx.spot(A1, A, G7, "1", "1");
        fx.cm(A1, G7).hold = MANUAL_HOLD;
        fx
    };
    let mut fx = base();
    for k in 1..=60u64 {
        fx.at(k * 60_000);
        fx.leg_adjust(A1, G7);
        fx.adv();
    }
    assert!(fx.log.ord(ExecOrderKind::HedgeFix).is_empty(), "EXE-313.1");
    assert_eq!(fx.cell(A1, G7).fix.first_tsc, Tsc(0), "EXE-313.1");
    assert!(fx.log.states().iter().all(|s| s.1 != 4), "EXE-313.1");
    let mut fx = Fx::new();
    let mut pp = pair0();
    pp.residual = ResidualPolicy::TrimExcess;
    inject(
        &mut fx,
        &target(
            A1,
            G7,
            &[(0, q("10")), (1, q("-10"))],
            hft_strategy_sdk::ExecSpec::PairOnFill(pp),
        ),
    );
    fx.legs(A1, G7, &["10.06", "-10.0"]);
    fx.spot(A1, A, G7, "10.06", "10.06");
    fx.cm(A1, G7).hold = MANUAL_HOLD;
    fx.touch(S);
    assert!(fx.adv().ord(ExecOrderKind::ResidualTrim).is_empty(), "EXE-313.2");
    let mut fx = base();
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    let r = fx.adv();
    assert!(!r.ord(ExecOrderKind::ForceStep).is_empty(), "EXE-313.3");
    assert_eq!(r.notes_n(note::EXEC_MANUAL_HOLD)[0].v[2], 3, "EXE-313.3");
    let mut fx = base();
    fx.flatten(A1);
    assert!(!fx.adv().ord(ExecOrderKind::ForceStep).is_empty(), "EXE-313.4");
    let mut fx = base();
    assert_eq!(
        fx.cmd_fix(A1, G7).1.ord(ExecOrderKind::CmdHedgeFix).len(),
        1,
        "EXE-313.5"
    );
    let _ = StuckReason::External;
}

#[test]
fn exe_314_hold_snapshot() {
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G10, &[(S, A), (P, B)])]);
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    fx.cm(A1, G7).hold = MANUAL_HOLD;
    fx.legs(A1, G10, &["9", "-9"]);
    inject(
        &mut fx,
        &target(
            A1,
            G10,
            &[(0, q("8")), (1, q("-8"))],
            hft_strategy_sdk::ExecSpec::PairOnFill(pair0()),
        ),
    );
    {
        let c = fx.cm(A1, G10);
        c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
        c.view.intent.reason = TargetReason::MarginStress;
        c.hold = MANUAL_HOLD_DEFERRED;
    }
    fx.acct(A1).force_active = true;
    let snap = fx.snap();
    assert_eq!(cell_row(&snap, A1, G7).unwrap().flags & 4, 4, "EXE-314.1");
    assert_eq!(cell_row(&snap, A1, G10).unwrap().flags & 16, 16, "EXE-314.2");
    let ex = fx.restore(&snap);
    fx.ex = ex;
    fx.leg_adjust(A1, G7);
    assert!(fx.adv().ord(ExecOrderKind::HedgeFix).is_empty(), "EXE-314.1");
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    fx.adv();
    assert_eq!(fx.cell(A1, G10).hold, MANUAL_HOLD, "EXE-314.2");
}
