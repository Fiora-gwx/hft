//! EXE-315–EXE-319, EXE-325–EXE-329: per-venue recovery gate and the gate-held-naked alarm.
use crate::common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{EngineOrigin, MarginLevel, TargetOrigin, TargetReason, TargetState};
use hft_types::{CapSource, Side, Tsc};

const HZ: u64 = 1_000_000_000_000;

fn recover(fx: &mut Fx) {
    let now = fx.now();
    fx.ex.recovery_end(now.tsc, now.tsc, HZ, HZ);
}

fn open(fx: &mut Fx, v: hft_types::VenueId) {
    let now = fx.now();
    fx.ex.on_venue_gate_open(A1, v, now);
}

fn step(fx: &mut Fx, ms: u64) -> Rec {
    fx.at(ms);
    fx.touch(S);
    fx.touch(P);
    fx.leg_adjust(A1, G7);
    fx.adv()
}

fn broken(fx: &mut Fx, since: Tsc, due: Tsc) {
    let c = fx.cm(A1, G7);
    c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    c.broken_since = since;
    c.auto_fix.phase = AutoFixPhase::Primed as u8;
    c.auto_fix.due_tsc = due;
}

#[test]
fn exe_315_per_venue_suppression() {
    let setup = |s: &str, p: &str| {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &[s, p]);
        fx.spot(A1, A, G7, s, s);
        recover(&mut fx);
        fx.at(100);
        open(&mut fx, A);
        fx
    };
    let mut fx = setup("1.50", "-1.0");
    let o = step(&mut fx, 100).ord(ExecOrderKind::HedgeFix);
    assert_eq!((o[0].intent.side, o[0].intent.inst), (Side::Sell, S), "EXE-315.1");
    let mut fx = setup("1.00", "-1.5");
    assert!(step(&mut fx, 100).orders.is_empty(), "EXE-315.2");
    assert_eq!(fx.cell(A1, G7).fix.first_tsc, t(100), "EXE-315.2");
    assert!(
        step(&mut fx, 10_101).states().iter().all(|s| s.1 != 4),
        "EXE-315.2"
    );
    // .3 residual trim of P.
    let mut insts = default_insts();
    insts[0].min_qty = dec("0.1", 8);
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    let mut pp = pair0();
    pp.residual = hft_strategy_sdk::ResidualPolicy::TrimExcess;
    inject(
        &mut fx,
        &target(
            A1,
            G7,
            &[(0, q("10.0")), (1, q("-10.0"))],
            hft_strategy_sdk::ExecSpec::PairOnFill(pp),
        ),
    );
    fx.cm(A1, G7).view.state = TargetState::Idle;
    fx.legs(A1, G7, &["10.0", "-10.06"]);
    recover(&mut fx);
    fx.at(100);
    open(&mut fx, A);
    assert!(
        step(&mut fx, 200).ord(ExecOrderKind::ResidualTrim).is_empty(),
        "EXE-315.3"
    );
    // .4 external deadline.
    let mut fx = setup("1.00", "-1.5");
    fx.cm(A1, G7).ext_deadline = t(200);
    assert!(step(&mut fx, 300).dec(10).is_empty(), "EXE-315.4");
    // .5 / .7 Broken.
    let mut fx = setup("1.00", "-1.5");
    broken(&mut fx, t(0), t(0));
    assert!(step(&mut fx, 200).dec(14).is_empty(), "EXE-315.5");
    let mut fx = setup("1.50", "-1.0");
    broken(&mut fx, t(0), t(0));
    assert!(step(&mut fx, 200).has_dec(14, pack(4, 1, 0)), "EXE-315.7");
    // .6 L3 on g8.
    let mut fx = setup("0", "0");
    fx.legs(A1, G8, &["-10"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    assert!(
        !step(&mut fx, 200).ord(ExecOrderKind::ForceStep).is_empty(),
        "EXE-315.6"
    );
}

#[test]
fn exe_316_venue_open_resets() {
    let mut fx = Fx::with(c0(), vec![g7(), g8(), gspec(G15, &[(S, A)])]);
    recover(&mut fx);
    {
        let c = fx.cm(A1, G7);
        c.fix.count = 1;
        c.fix.first_tsc = t(0);
        c.ext_deadline = t(3000);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = AutoFixPhase::Primed as u8;
        c.auto_fix.due_tsc = t(20_000);
        c.aux16 |= (1 << ExecTimerKind::External as u16) | (1 << ExecTimerKind::AutoFix as u16);
    }
    fx.cm(A1, G15).ext_deadline = t(3000);
    fx.at(40_000);
    fx.out.clear();
    open(&mut fx, B);
    assert!(fx.out.is_empty());
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.fix.count, c.fix.first_tsc, c.ext_deadline, c.auto_fix.due_tsc),
        (0, Tsc(0), t(45_000), t(160_000))
    );
    assert_eq!(fx.cell(A1, G15).ext_deadline, t(3000));
    assert_ne!(gate_row(&fx.snap(), A1).unwrap().venue_open & (1 << B.0), 0);
    let r = fx.adv();
    assert!(r.timer_set(ExecTimerKind::External).contains(&t(45_000)));
    assert!(r.timer_set(ExecTimerKind::AutoFix).contains(&t(160_000)));
}

#[test]
fn exe_317_venue_open_idempotent() {
    let mut fx = Fx::new();
    recover(&mut fx);
    fx.at(1000);
    open(&mut fx, B);
    fx.adv();
    fx.cm(A1, G7).ext_deadline = t(7000);
    let x = fx.snap();
    fx.at(5000);
    open(&mut fx, B);
    assert_eq!(fx.cell(A1, G7).ext_deadline, t(7000));
    assert!(!fx.ex.has_pending());
    assert_eq!(fx.snap(), x);
}

#[test]
fn exe_318_gate_open_fills_venues() {
    let mut fx = Fx::new();
    recover(&mut fx);
    fx.at(1000);
    open(&mut fx, B);
    fx.cm(A1, G7).ext_deadline = t(8000);
    fx.cm(A1, G8).ext_deadline = t(8000);
    fx.at(9000);
    let now = fx.now();
    fx.ex.on_gate_open(A1, now);
    let g = gate_row(&fx.snap(), A1).unwrap();
    assert_eq!(g.gate_open, 1);
    assert_eq!(g.venue_open & ((1 << A.0) | (1 << B.0)), (1 << A.0) | (1 << B.0));
    assert_eq!(fx.cell(A1, G7).ext_deadline, t(14_000));
    assert_eq!(fx.cell(A1, G8).ext_deadline, t(8000));
}

#[test]
fn exe_319_recovery_end_closes_gate() {
    let mut fx = Fx::new();
    {
        let a = fx.acct(A1);
        a.gate_open = true;
        a.venue_open = 0b110;
        a.gate_closed_tsc = Tsc(5);
    }
    let snap = fx.snap();
    let mut ex = fx.restore(&snap);
    let n = Tsc(777_000_000);
    ex.recovery_end(n, t(0), HZ, HZ);
    fx.ex = ex;
    let s = fx.snap();
    for a in [A1, A2, S1] {
        let g = gate_row(&s, a).unwrap();
        assert_eq!((g.gate_open, g.venue_open, g.gate_closed_tsc), (0, 0, n));
    }
}

fn alarm_note(r: &Rec) -> Vec<hft_event::Note> {
    r.notes_n(note::EXEC_GATE_HELD_NAKED)
}

fn case1_fx() -> Fx {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    recover(&mut fx);
    fx
}

#[test]
fn exe_325_alarm_case1() {
    let mut fx = case1_fx();
    broken(&mut fx, Tsc(1), t(500_000));
    step(&mut fx, 0);
    fx.at(299_999);
    let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
    r.extend(fx.adv());
    assert!(alarm_note(&r).is_empty(), "EXE-325.1");
    fx.at(300_000);
    let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
    r.extend(fx.adv());
    let n = alarm_note(&r);
    assert_eq!(n[0].level, NoteLevel::P0, "EXE-325.2");
    assert_eq!(
        &n[0].v[..7],
        &[
            0x01_0000_0701,
            1,
            1,
            300_000,
            1,
            q("0.5").0 as u64,
            usd("50").0 as u64
        ],
        "EXE-325.2"
    );
    // .3
    let mut fx = case1_fx();
    step(&mut fx, 20_000);
    assert_eq!(fx.cell(A1, G7).fix.first_tsc, t(20_000));
    fx.at(320_000);
    let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
    r.extend(fx.adv());
    let n = alarm_note(&r);
    assert_eq!(
        (n.len(), n[0].v[1], n[0].v[2], n[0].v[3]),
        (1, 1, 0, 300_000),
        "EXE-325.3"
    );
    // .4
    let mut fx = case1_fx();
    broken(&mut fx, Tsc(1), t(500_000));
    step(&mut fx, 0);
    fx.at(200_000);
    let now = fx.now();
    fx.ex.on_gate_open(A1, now);
    fx.adv();
    fx.at(300_000);
    let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
    r.extend(fx.adv());
    assert!(alarm_note(&r).is_empty(), "EXE-325.4");
    // .5
    let mut fx = case1_fx();
    broken(&mut fx, Tsc(1), t(500_000));
    fx.cm(A1, G7).hold = MANUAL_HOLD;
    step(&mut fx, 0);
    fx.at(300_000);
    let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
    r.extend(fx.adv());
    assert!(alarm_note(&r).is_empty(), "EXE-325.5");
}

#[test]
fn exe_326_alarm_case2() {
    type Cond = fn(&mut Fx);
    let rows: [(&str, Cond, Option<u64>); 8] = [
        ("EXE-326.2", |fx| fx.quote_down(P), Some(4)),
        ("EXE-326.3", |fx| fx.lv(A1, G7, |l| l.has_unknown = true), Some(2)),
        (
            "EXE-326.4",
            |fx| fx.cm(A1, G7).cap_block = CAP_BROKEN | CAP_REORG,
            Some(3),
        ),
        (
            "EXE-326.5",
            |fx| fx.risk_for(A1, P, |r| r.rate_backoff = true),
            Some(5),
        ),
        ("EXE-326.6", |fx| fx.quote(P, "98.90", "101.10"), Some(6)),
        (
            "EXE-326.7",
            |fx| {
                fx.legs(A1, G7, &["1.50", "-1.0"]);
                fx.spot(A1, A, G7, "1.50", "0.005");
            },
            Some(7),
        ),
        (
            "EXE-326.8",
            |fx| {
                let c = fx.cm(A1, G7);
                c.has_target = true;
                c.view.intent = t0("1", "-1.5");
                c.view.intent.reason = TargetReason::MarginStress;
                c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
            },
            None,
        ),
        ("EXE-326.9", |fx| fx.quote_down(P), None),
    ];
    for (row, cond, want) in rows {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["1.00", "-1.5"]);
        fx.spot(A1, A, G7, "1", "1");
        fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
        fx.adv();
        fx.at(100_000);
        cond(&mut fx);
        if row == "EXE-326.9" {
            step(&mut fx, 120_000);
            fx.at(400_000);
            fx.quote_up(P);
            step(&mut fx, 400_000);
        }
        if row == "EXE-326.2" {
            fx.at(419_999);
            let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
            r.extend(fx.adv());
            assert!(alarm_note(&r).is_empty(), "EXE-326.1");
        }
        fx.at(420_000);
        let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
        r.extend(fx.adv());
        let n = alarm_note(&r);
        match want {
            Some(code) => {
                assert_eq!(n.len(), 1, "{row}");
                assert_eq!((n[0].v[1], n[0].v[2], n[0].v[3]), (2, code, 300_000), "{row}");
            }
            None => assert!(n.is_empty(), "{row}"),
        }
    }
}

#[test]
fn exe_327_alarm_case1_wins() {
    let mut fx = case1_fx();
    fx.quote_down(P);
    fx.at(1000);
    open(&mut fx, B);
    broken(&mut fx, t(1000), t(121_000));
    let mut ms = 1000;
    while ms <= 1_000_000 {
        fx.at(ms);
        for x in fx.log.timers.clone() {
            let _ = x;
        }
        if ms == 300_000 || ms == 421_000 {
            fx.fire(ExecTimerKind::Alarm, A1, G7);
        }
        fx.touch(P);
        fx.adv();
        ms += 1000;
    }
    let n = alarm_note(&fx.log);
    assert_eq!(n.len(), 1);
    assert_eq!(n[0].v[1], 1);
}

#[test]
fn exe_328_alarm_timer() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    broken(&mut fx, t(0), t(120_000));
    fx.quote_down(P);
    step(&mut fx, 120_000);
    assert!(
        fx.log.timer_set(ExecTimerKind::Alarm).contains(&t(420_000)),
        "EXE-328.1"
    );
    assert!(
        fx.log
            .timer(ExecTimerKind::Alarm)
            .iter()
            .all(|x| x.timer_id == 6 | 1 << 8)
    );
    fx.at(300_000);
    let r = fx.fire(ExecTimerKind::Alarm, A1, G7);
    assert_eq!(r.timer_set(ExecTimerKind::Alarm), vec![t(420_000)], "EXE-328.4");
    assert!(alarm_note(&r).is_empty(), "EXE-328.4");
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    broken(&mut fx, t(0), t(120_000));
    fx.quote_down(P);
    step(&mut fx, 120_000);
    fx.at(200_000);
    fx.ext(
        A1,
        &xfill(
            G7,
            1,
            P,
            B,
            Side::Buy,
            "0.1",
            hft_strategy_sdk::ExternalCause::Manual,
        ),
    );
    fx.at(250_000);
    let now = fx.now();
    fx.ex.note_strategy_action(A1, G7, now, &mut fx.out);
    fx.out.clear();
    let r = fx.adv();
    let al = r.timer(ExecTimerKind::Alarm);
    assert!(
        al.len() >= 2 && al[0].cancel && !al[1].cancel && al[1].at == t(670_000),
        "EXE-328.2"
    );
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    broken(&mut fx, t(0), t(120_000));
    fx.quote_down(P);
    let r = step(&mut fx, 430_000);
    assert!(r.timer_set(ExecTimerKind::Alarm).is_empty(), "EXE-328.3");
}

#[test]
fn exe_329_alarm_after_restore() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    broken(&mut fx, t(0), t(120_000));
    let n = t(1_000_000);
    fx.ex.recovery_end(n, t(500_000), HZ, HZ);
    let timers: Vec<_> = fx.ex.recovery_timers().collect();
    assert!(timers.iter().any(|x| x.kind() == Some(ExecTimerKind::Alarm)
        && x.group == G7
        && x.at == Tsc(n.0 + 300_000_000_000)));
    fx.at_tsc(Tsc(n.0 + 300_000_000_000));
    let mut r = fx.fire(ExecTimerKind::Alarm, A1, G7);
    r.extend(fx.adv());
    let notes = alarm_note(&r);
    assert_eq!((notes.len(), notes[0].v[1]), (1, 1));
}
