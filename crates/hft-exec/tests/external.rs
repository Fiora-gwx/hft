//! EXE-290–EXE-300, EXE-512: external fills.
mod common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{
    EngineOrigin, ExecUpdateKind, ExternalCause, StuckReason, TargetOrigin, TargetReason, TargetState,
};
use hft_types::{Side, Tsc};

fn xf(cause: ExternalCause) -> hft_strategy_sdk::ExternalFillView {
    xfill(G7, 1, P, B, Side::Buy, "0.5", cause)
}

fn engine_target(fx: &mut Fx) {
    inject(fx, &t0("8", "-8"));
    let c = fx.cm(A1, G7);
    c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
    c.view.intent.reason = TargetReason::MarginStress;
}

#[test]
fn exe_290_condition_table() {
    type Setup = fn(&mut Fx);
    let rows: [(&str, Setup, ExternalCause, Option<u64>, u32); 9] = [
        (
            "EXE-290.1",
            |fx| {
                let c = fx.cm(A1, G7);
                c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
                c.auto_fix.phase = AutoFixPhase::Primed as u8;
                c.auto_fix.due_tsc = t(100_000);
            },
            ExternalCause::Liquidation,
            None,
            pack(6, 1, 4),
        ),
        (
            "EXE-290.2",
            engine_target,
            ExternalCause::Adl,
            None,
            pack(6, 2, 3),
        ),
        (
            "EXE-290.3",
            |fx| fx.acct(A1).force_active = true,
            ExternalCause::Liquidation,
            None,
            pack(6, 1, 3),
        ),
        (
            "EXE-290.4",
            |fx| {
                inject(fx, &t0("10", "-10"));
                fx.acct(A1).flatten_lock = true;
            },
            ExternalCause::Liquidation,
            None,
            pack(6, 1, 3),
        ),
        (
            "EXE-290.5",
            |fx| inject(fx, &t0("10", "-10")),
            ExternalCause::Manual,
            None,
            pack(6, 3, 2),
        ),
        (
            "EXE-290.6",
            |fx| inject(fx, &t0("10", "-10")),
            ExternalCause::Liquidation,
            Some(6000),
            pack(6, 1, 1),
        ),
        ("EXE-290.7", |_| {}, ExternalCause::Adl, Some(6000), pack(6, 2, 1)),
        (
            "EXE-290.8",
            |fx| inject(fx, &t0("10", "-10")),
            ExternalCause::Unknown,
            Some(6000),
            pack(6, 5, 1),
        ),
        (
            "EXE-290.9",
            |fx| inject(fx, &t0("10", "-10")),
            ExternalCause::Delivery,
            Some(6000),
            pack(6, 4, 1),
        ),
    ];
    for (row, setup, cause, ret, detail) in rows {
        let mut fx = Fx::new();
        fx.at(1000);
        setup(&mut fx);
        let (d, mut r) = fx.ext(A1, &xf(cause));
        assert_eq!(d, ret.map_or(Tsc(0), t), "{row}");
        assert!(r.has_dec(9, detail), "{row}");
        r.extend(fx.adv());
        let c = fx.cell(A1, G7);
        match row {
            "EXE-290.1" => {
                assert!(matches!(c.view.state, TargetState::Broken(_)), "{row}");
                assert_eq!(
                    (c.auto_fix.due_tsc, c.ext_deadline),
                    (t(100_000), Tsc(0)),
                    "{row}"
                );
            }
            "EXE-290.2" => assert_eq!(
                c.view.intent,
                {
                    let mut x = t0("8", "-8");
                    x.reason = TargetReason::MarginStress;
                    x
                },
                "{row}"
            ),
            "EXE-290.5" => {
                assert_eq!(c.view.state, TargetState::Stuck(StuckReason::External), "{row}");
                assert_eq!(c.hold, MANUAL_HOLD, "{row}");
            }
            "EXE-290.6" | "EXE-290.8" | "EXE-290.9" => {
                assert!(r.states().contains(&(2, 3, pack(2, 6, 0))), "{row}");
                assert_eq!(r.timer_set(ExecTimerKind::External), vec![t(6000)], "{row}");
            }
            "EXE-290.7" => {
                assert!(r.states().is_empty(), "{row}");
                assert_eq!(c.view.state, TargetState::Idle, "{row}");
            }
            _ => {}
        }
    }
}

#[test]
fn exe_291_manual_notes() {
    type Setup = fn(&mut Fx);
    let rows: [(&str, Setup, bool, NoteLevel); 4] = [
        (
            "EXE-291.1",
            |fx| inject(fx, &t0("10", "-10")),
            true,
            NoteLevel::Info,
        ),
        (
            "EXE-291.2",
            |fx| {
                inject(fx, &t0("10", "-10"));
                fx.cm(A1, G7).hold = MANUAL_HOLD;
            },
            false,
            NoteLevel::P1,
        ),
        (
            "EXE-291.3",
            |fx| {
                let c = fx.cm(A1, G7);
                c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
                c.auto_fix.phase = AutoFixPhase::Primed as u8;
            },
            true,
            NoteLevel::Info,
        ),
        ("EXE-291.4", engine_target, false, NoteLevel::P1),
    ];
    for (row, setup, hold_note, lvl) in rows {
        let mut fx = Fx::new();
        setup(&mut fx);
        let mut f = xf(ExternalCause::Manual);
        f.px = px("100.10");
        let (_, r) = fx.ext(A1, &f);
        let h = r.notes_n(note::EXEC_MANUAL_HOLD);
        assert_eq!(!h.is_empty(), hold_note, "{row}");
        if hold_note {
            assert_eq!(h[0].level, NoteLevel::P1, "{row}");
        }
        let m = r.notes_n(note::EXEC_MANUAL_EXTERNAL);
        assert_eq!(m[0].level, lvl, "{row}");
        if row == "EXE-291.1" {
            assert_eq!(
                &m[0].v[..6],
                &[
                    1,
                    1,
                    21,
                    Side::Buy as u64,
                    q("0.5").0 as u64,
                    px("100.10").0 as u64
                ]
            );
        }
        if row == "EXE-291.3" {
            assert_eq!(fx.cell(A1, G7).auto_fix.phase, AutoFixPhase::WaitHold as u8);
        }
        if row == "EXE-291.4" {
            assert_eq!(fx.cell(A1, G7).hold, MANUAL_HOLD_DEFERRED);
        }
    }
}

#[test]
fn exe_292_group_none() {
    let none = || xfill(GNONE, 0, P, B, Side::Sell, "0.8", ExternalCause::Unknown);
    let mut fx = Fx::new();
    fx.perp(A1, B, P, "-0.8");
    let (d, r) = fx.ext(A1, &none());
    assert_eq!(d, Tsc(0), "EXE-292.1");
    assert!(r.is_empty() && !fx.ex.has_pending(), "EXE-292.1");
    let mut fx = Fx::new();
    fx.perp(A1, B, P, "-0.8");
    fx.acct(A1).flatten_lock = true;
    let (d, r) = fx.ext(A1, &none());
    assert!(d == Tsc(0) && r.is_empty(), "EXE-292.2");
    let r = fx.adv();
    assert!(
        r.decisions
            .iter()
            .any(|d| d.group == GNONE && d.action == 11 && d.detail == pack(5, 4, 0)),
        "EXE-292.2"
    );
    assert_eq!(
        r.notes_n(note::EXEC_UNGROUPED_REDUCE)[0].level,
        NoteLevel::P0,
        "EXE-292.2"
    );
    let o = r.orders.iter().find(|o| o.intent.group == GNONE).unwrap();
    assert_eq!(
        (o.intent.side, o.intent.qty, o.intent.reduce_only()),
        (Side::Buy, q("0.8"), true),
        "EXE-292.2"
    );
    let mut fx = Fx::new();
    fx.perp(A1, B, P, "-0.8");
    fx.acct(A1).force_active = true;
    let (d, _) = fx.ext(A1, &none());
    assert_eq!(d, Tsc(0), "EXE-292.3");
    assert!(
        fx.adv().orders.iter().all(|o| o.intent.group != GNONE),
        "EXE-292.3"
    );
}

fn paused(fx: &mut Fx) {
    inject(fx, &t0("10", "-10"));
    fx.at(1000);
    fx.ext(A1, &xf(ExternalCause::Liquidation));
    fx.adv();
    assert_eq!(fx.cell(A1, G7).ext_deadline, t(6000));
    fx.at(3000);
}

#[test]
fn exe_293_handled() {
    let mut fx = Fx::new();
    paused(&mut fx);
    let mut tg = t0("10", "-10");
    tg.tag = 0x11;
    let (_, r) = fx.set(I1, &tg);
    assert_eq!(fx.cell(A1, G7).ext_deadline, Tsc(0), "EXE-293.1");
    assert!(
        r.timers
            .iter()
            .any(|x| x.cancel && x.kind() == Some(ExecTimerKind::External)),
        "EXE-293.1"
    );
    assert!(r.dec(2).len() == 1 && r.has_dec(9, pack(6, 0, 5)), "EXE-293.1");
    assert_eq!(fx.cell(A1, G7).view.state, TargetState::Working, "EXE-293.1");
    let mut fx = Fx::new();
    paused(&mut fx);
    let r = fx.clear(A1, G7);
    assert!(r.dec(3).len() == 1 && r.has_dec(9, pack(6, 0, 5)), "EXE-293.2");
    assert_eq!(fx.cell(A1, G7).view.state, TargetState::Idle, "EXE-293.2");
    let mut fx = Fx::new();
    paused(&mut fx);
    let now = fx.now();
    fx.ex.note_strategy_action(A1, G7, now, &mut fx.out);
    let r = Rec::take(&mut fx.out);
    assert!(
        r.has_dec(9, pack(6, 0, 5)) && r.states().contains(&(3, 2, 0)),
        "EXE-293.3"
    );
    let mut fx = Fx::new();
    paused(&mut fx);
    let (_, r) = fx.set(I1, &t0("10", "-10"));
    assert!(r.decisions.is_empty(), "EXE-293.4");
    assert_eq!(fx.cell(A1, G7).ext_deadline, t(6000), "EXE-293.4");
}

fn liq_s_excess(fx: &mut Fx) {
    inject(fx, &t0("10", "-10"));
    fx.at(1000);
    fx.legs(A1, G7, &["1.00", "-0.5"]);
    fx.spot(A1, A, G7, "1", "1");
    fx.ext(A1, &xf(ExternalCause::Liquidation));
    fx.adv();
}

#[test]
fn exe_294_deadline_default() {
    let mut fx = Fx::new();
    liq_s_excess(&mut fx);
    fx.at(5999);
    fx.touch(S);
    let r = fx.adv();
    assert!(r.orders.is_empty() && r.dec(10).is_empty(), "EXE-294.1");
    fx.at(6000);
    fx.fire(ExecTimerKind::External, A1, G7);
    let r = fx.adv();
    assert!(r.dec(3).iter().any(|d| d.eflags == 2), "EXE-294.2");
    assert!(
        r.upd().contains(&ExecUpdateKind::Done {
            cleared_by_engine: true
        }),
        "EXE-294.2"
    );
    let o = r.ord(ExecOrderKind::ExternalDefault);
    assert_eq!(
        (
            o[0].kind.code(),
            o[0].intent.side,
            o[0].intent.inst,
            o[0].intent.qty,
            o[0].hedge_cap_qty
        ),
        (7, Side::Sell, S, q("0.50"), q("0.51")),
        "EXE-294.2"
    );
    assert_eq!(r.dec(10).len(), 1, "EXE-294.2");
    assert_eq!(
        r.notes_n(note::EXEC_EXTERNAL_DEFAULT)[0].level,
        NoteLevel::P0,
        "EXE-294.2"
    );
    assert_eq!(fx.cell(A1, G7).ext_deadline, Tsc(0), "EXE-294.2");
}

#[test]
fn exe_296_default_suppression() {
    // .1
    let mut fx = Fx::new();
    liq_s_excess(&mut fx);
    fx.lv(A1, G7, |l| l.has_unknown = true);
    fx.at(6000);
    fx.touch(S);
    assert!(fx.adv().orders.is_empty(), "EXE-296.1");
    fx.lv(A1, G7, |l| l.has_unknown = false);
    fx.at(8000);
    fx.touch(S);
    assert_eq!(fx.adv().ord(ExecOrderKind::ExternalDefault).len(), 1, "EXE-296.1");
    // .2
    let mut fx = Fx::new();
    liq_s_excess(&mut fx);
    fx.quote_down(S);
    let mut ms = 6000;
    while ms <= 16_000 {
        fx.at(ms);
        fx.touch(S);
        let r = fx.adv();
        assert!(r.orders.is_empty(), "EXE-296.2 t {ms}");
        ms += 1000;
    }
    fx.at(16_001);
    fx.touch(S);
    assert!(fx.adv().states().contains(&(1, 4, pack(3, 3, 0))), "EXE-296.2");
    // .3
    let mut fx = Fx::new();
    liq_s_excess(&mut fx);
    let mut ms = 6000;
    let mut broken = false;
    for _ in 0..3 {
        fx.at(ms);
        fx.touch(S);
        let r = fx.adv();
        broken |= r.states().iter().any(|s| s.1 == 4);
        if let Some(o) = r.orders.first().copied() {
            let rf = fx.placed(&o);
            fx.exch_reject(rf, G7);
        }
        ms += 1000;
    }
    fx.at(ms);
    fx.touch(S);
    let r = fx.adv();
    broken |= r.states().contains(&(1, 4, pack(3, 3, 0)));
    assert!(broken, "EXE-296.3");
}

#[test]
fn exe_297_recovery_deadline_reset() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-0.5"]);
    fx.spot(A1, A, G7, "1", "1");
    fx.cm(A1, G7).ext_deadline = t(2000);
    {
        let a = fx.acct(A1);
        a.gate_open = false;
        a.venue_open = 0;
    }
    fx.at(3000);
    fx.touch(S);
    assert!(fx.adv().dec(10).is_empty());
    fx.at(10_000);
    let now = fx.now();
    fx.ex.on_venue_gate_open(A1, A, now);
    let r = fx.adv();
    assert_eq!(fx.cell(A1, G7).ext_deadline, t(15_000));
    assert!(r.timer_set(ExecTimerKind::External).contains(&t(15_000)));
    fx.at(14_999);
    fx.touch(S);
    assert!(fx.adv().dec(10).is_empty());
    fx.at(15_000);
    fx.fire(ExecTimerKind::External, A1, G7);
    assert_eq!(fx.adv().dec(10).len(), 1);
}

#[test]
fn exe_298_nominal_deadline() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.acct(A1).venue_open = 0;
    fx.at(1000);
    let (d, _) = fx.ext(A1, &xf(ExternalCause::Liquidation));
    assert_eq!(d, t(6000));
    assert_eq!(fx.cell(A1, G7).ext_deadline, t(6000));
}

#[test]
fn exe_299_manual_no_timer() {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["1.00", "-0.5"]);
    fx.spot(A1, A, G7, "1", "1");
    fx.ext(A1, &xf(ExternalCause::Manual));
    for s in 1..=3600u64 {
        fx.at(s * 1000);
        fx.touch(S);
        fx.adv();
    }
    assert!(fx.log.dec(10).is_empty() && fx.log.dec(7).is_empty() && fx.log.dec(8).is_empty());
    assert!(fx.log.orders.is_empty());
    let c = fx.cell(A1, G7);
    assert_eq!(c.ext_deadline, Tsc(0));
    assert_eq!(c.view.state, TargetState::Stuck(StuckReason::External));
}

#[test]
fn exe_300_adl_during_l3() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["10.00", "-10.0"]);
    fx.spot(A1, A, G7, "10", "10");
    engine_target(&mut fx);
    fx.acct(A1).force_active = true;
    fx.legs(A1, G7, &["10.00", "-8.5"]);
    let (d, _) = fx.ext(A1, &xf(ExternalCause::Adl));
    assert_eq!(d, Tsc(0));
    let r = fx.adv();
    let mut want = t0("8", "-8");
    want.reason = TargetReason::MarginStress;
    assert_eq!(fx.cell(A1, G7).view.intent, want);
    let p: Vec<_> = r.orders.iter().filter(|o| o.intent.inst == P).collect();
    assert_eq!((p[0].intent.side, p[0].intent.qty), (Side::Buy, q("0.5")));
    assert!(r.ord(ExecOrderKind::ExternalDefault).is_empty());
}

#[test]
fn exe_512_deadline_within_threshold() {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.0"]);
    fx.cm(A1, G7).ext_deadline = t(6000);
    fx.at(6000);
    fx.touch(S);
    fx.leg_adjust(A1, G7);
    let r = fx.adv();
    let d = r.dec(10);
    assert_eq!((d.len(), d[0].qty), (1, q("0")));
    assert_eq!(r.notes_n(note::EXEC_EXTERNAL_DEFAULT)[0].level, NoteLevel::P1);
    assert!(r.orders.is_empty());
    assert_eq!(fx.cell(A1, G7).ext_deadline, Tsc(0));
}
