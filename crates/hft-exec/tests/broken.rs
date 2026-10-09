//! EXE-237–EXE-262 (without the property case): Broken disposal and the auto bounded close-out.
mod common;
use common::*;
use hft_event::NoteLevel;
use hft_exec::decision::note;
use hft_exec::*;
use hft_strategy_sdk::{ExternalCause, LinkRole, MarginLevel, TargetState};
use hft_types::{CapSource, RejectBits, Side, Tif, Tsc};

/// g7 Broken since `at` (CapSet + advance), legs as given (spot inventory = S).
fn broken_at(fx: &mut Fx, at: u64, s: &str, p: &str) {
    fx.legs(A1, G7, &[s, p]);
    fx.spot(A1, A, G7, s, s);
    fx.at(at);
    fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
    fx.adv();
}

/// Advances g7 at `ms` (P and S touched).
fn step(fx: &mut Fx, ms: u64) -> Rec {
    fx.at(ms);
    fx.touch(P);
    fx.touch(S);
    fx.adv()
}

fn phase(fx: &Fx) -> u8 {
    fx.cell(A1, G7).auto_fix.phase
}

const RUNNING: u8 = AutoFixPhase::Running as u8;
const PRIMED: u8 = AutoFixPhase::Primed as u8;

#[test]
fn exe_237_broken_entry_timing() {
    type Enter = fn(&mut Fx);
    let rows: [(&str, Enter); 5] = [
        ("EXE-237.1", |fx| {
            fx.cm(A1, G7).fix.count = 3;
            fx.cm(A1, G7).fix.first_tsc = t(1000);
            fx.leg_adjust(A1, G7);
        }),
        ("EXE-237.2", |fx| {
            fx.link(&link(1, A1, G7, LinkPhase::Broken, LinkBrokenCause::Deadline));
        }),
        ("EXE-237.3", |fx| {
            let r = fx.adv();
            let o = r.ord(ExecOrderKind::HedgeFix)[0];
            fx.place_res(&o, PlaceResult::Rejected(RejectBits::HEDGE_LIMIT));
        }),
        ("EXE-237.4", |fx| {
            fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7))
        }),
        ("EXE-237.5", |fx| {
            fx.cm(A1, G7).hold = MANUAL_HOLD;
            fx.risk_ev(A1, cap_set(CapSource::BROKEN, A1, G7));
        }),
    ];
    for (row, enter) in rows {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["1.00", "-1.5"]);
        fx.at(1000);
        let mark = fx.log.timers.len();
        if row == "EXE-237.3" {
            fx.leg_adjust(A1, G7);
        }
        enter(&mut fx);
        fx.adv();
        let c = fx.cell(A1, G7);
        assert!(matches!(c.view.state, TargetState::Broken(_)), "{row}");
        assert_eq!(c.broken_since, t(1000), "{row}");
        if row == "EXE-237.5" {
            assert_eq!(c.auto_fix.phase, AutoFixPhase::WaitHold as u8, "{row}");
            continue;
        }
        assert_eq!(
            (c.auto_fix.phase, c.auto_fix.due_tsc),
            (PRIMED, t(121_000)),
            "{row}"
        );
        let af: Vec<_> = fx.log.timers[mark..]
            .iter()
            .filter(|x| x.kind() == Some(ExecTimerKind::AutoFix) && !x.cancel)
            .copied()
            .collect();
        assert_eq!(af.len(), 1, "{row}");
        assert_eq!((af[0].timer_id, af[0].at), (5 | 1 << 8, t(121_000)), "{row}");
    }
}

#[test]
fn exe_238_due_boundary() {
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    let r = step(&mut fx, 119_999);
    assert!(r.dec(14).is_empty() && r.orders.is_empty(), "EXE-238.1");
    fx.at(120_000);
    fx.fire(ExecTimerKind::AutoFix, A1, G7);
    let r = fx.adv();
    assert!(r.has_dec(14, pack(4, 1, 0)), "EXE-238.2");
    assert_eq!(r.ord(ExecOrderKind::AutoFix).len(), 1, "EXE-238.2");
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    let r = step(&mut fx, 120_001);
    assert!(r.has_dec(14, pack(4, 1, 0)), "EXE-238.3");
}

#[test]
fn exe_239_start_records() {
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.55");
    let r = step(&mut fx, 120_000);
    let a = fx.cell(A1, G7).auto_fix;
    assert_eq!(
        (
            a.phase,
            a.started_tsc,
            a.exec_ms,
            a.leg_ix,
            a.start_sign,
            a.budget
        ),
        (RUNNING, t(120_000), 0, 1, -1, q("0.5"))
    );
    assert_eq!(a.run_mark_tsc, t(120_000));
    let d = r.dec(14).into_iter().find(|d| d.detail == pack(4, 1, 0)).unwrap();
    assert_eq!((d.origin, d.leg_ix, d.target_qty), (6, 1, q("0.5")));
}

#[test]
fn exe_240_budget() {
    for (row, s, p, inv, free, want) in [
        ("EXE-240.1", "1.00", "-1.55", "1", "1", "0.5"),
        ("EXE-240.2", "1.555", "-1.0", "1.555", "1.555", "0.55"),
        ("EXE-240.3", "1.555", "-1.0", "0.40", "1.555", "0.40"),
        ("EXE-240.4", "1.555", "-1.0", "1.555", "0.10", "0.55"),
    ] {
        let mut fx = Fx::new();
        broken_at(&mut fx, 0, s, p);
        fx.spot(A1, A, G7, inv, free);
        let r = step(&mut fx, 120_000);
        assert_eq!(fx.cell(A1, G7).auto_fix.budget, q(want), "{row}");
        if row == "EXE-240.4" {
            assert_eq!(r.ord(ExecOrderKind::AutoFix)[0].intent.qty, q("0.10"), "{row}");
        }
    }
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.555", "-1.0");
    step(&mut fx, 120_000);
    fx.legs(A1, G7, &["2.00", "-1.0"]);
    fx.spot(A1, A, G7, "2", "2");
    step(&mut fx, 120_100);
    assert_eq!(fx.cell(A1, G7).auto_fix.budget, q("0.55"), "EXE-240.5");
}

#[test]
fn exe_241_start_wait_conditions() {
    type Cond = fn(&mut Fx, bool);
    let rows: [(&str, Cond); 11] = [
        ("EXE-241.1", |fx, on| fx.lv(A1, G7, |l| l.has_pending = on)),
        ("EXE-241.2", |fx, on| fx.lv(A1, G7, |l| l.has_unknown = on)),
        ("EXE-241.3", |fx, on| {
            fx.infl(A1, G7, 0, if on { "0.1" } else { "0" })
        }),
        ("EXE-241.4", |fx, on| {
            fx.lv(A1, G7, |l| l.open_links = u8::from(on))
        }),
        ("EXE-241.5", |fx, on| {
            if on {
                fx.acct(A1).venue_open &= !(1u64 << B.0);
            } else {
                let now = fx.now();
                fx.ex.on_venue_gate_open(A1, B, now);
            }
        }),
        ("EXE-241.6", |fx, on| {
            let c = fx.cm(A1, G7);
            if on {
                c.has_target = true;
                c.view.origin =
                    hft_strategy_sdk::TargetOrigin::Engine(hft_strategy_sdk::EngineOrigin::MarginStress);
                c.view.intent = t0("1.00", "-1.5");
                c.view.intent.reason = hft_strategy_sdk::TargetReason::MarginStress;
            } else {
                c.has_target = false;
            }
        }),
        ("EXE-241.7", |fx, on| {
            fx.cm(A1, G7).cap_block = if on { CAP_BROKEN | CAP_REORG } else { CAP_BROKEN }
        }),
        (
            "EXE-241.8",
            |fx, on| {
                if on { fx.quote_down(P) } else { fx.quote_up(P) }
            },
        ),
        ("EXE-241.9", |fx, on| fx.risk_for(A1, P, |r| r.rate_backoff = on)),
        ("EXE-241.10", |fx, on| {
            if on {
                fx.quote(P, "98.90", "101.10")
            } else {
                fx.quote(P, "99.80", "100.20")
            }
        }),
        ("EXE-241.11", |fx, on| {
            fx.legs(A1, G7, &["1.50", "-1.0"]);
            fx.spot(A1, A, G7, "1.50", if on { "0.005" } else { "1.50" });
        }),
    ];
    for (row, cond) in rows {
        let mut fx = Fx::new();
        broken_at(&mut fx, 0, "1.00", "-1.5");
        fx.at(100_000);
        cond(&mut fx, true);
        for ms in [120_000, 125_000, 129_999] {
            let r = step(&mut fx, ms);
            assert_eq!(phase(&fx), PRIMED, "{row} {ms}");
            assert!(r.dec(14).is_empty(), "{row} {ms}");
        }
        fx.at(130_000);
        cond(&mut fx, false);
        let r = step(&mut fx, 130_000);
        if row == "EXE-241.5" {
            assert_eq!(fx.cell(A1, G7).auto_fix.due_tsc, t(250_000), "{row}");
            assert!(r.dec(14).is_empty(), "{row}");
        } else {
            assert!(r.has_dec(14, pack(4, 1, 0)), "{row}");
        }
    }
}

#[test]
fn exe_242_beyond_cross_boundary() {
    for (row, s, p, inst, bid, ask, start, lim) in [
        ("EXE-242.1", "1.00", "-1.5", P, "99.00", "101.00", true, "101.00"),
        ("EXE-242.2", "1.00", "-1.5", P, "98.99", "101.01", false, ""),
        ("EXE-242.3", "1.50", "-1.0", S, "99.00", "101.00", true, "99.00"),
        ("EXE-242.4", "1.50", "-1.0", S, "98.99", "101.01", false, ""),
    ] {
        let mut fx = Fx::new();
        broken_at(&mut fx, 0, s, p);
        fx.quote(inst, bid, ask);
        let r = step(&mut fx, 120_000);
        assert_eq!(r.has_dec(14, pack(4, 1, 0)), start, "{row}");
        if start {
            assert_eq!(r.ord(ExecOrderKind::AutoFix)[0].intent.px, px(lim), "{row}");
        }
    }
}

#[test]
fn exe_243_other_waits_keep_due() {
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    fx.at(50_000);
    fx.quote_down(P);
    let mut ms = 50_000;
    while ms < 150_000 {
        let r = step(&mut fx, ms);
        assert!(r.dec(14).is_empty());
        assert_eq!(fx.cell(A1, G7).auto_fix.due_tsc, t(120_000));
        ms += 10_000;
    }
    fx.at(150_000);
    fx.quote_up(P);
    assert!(step(&mut fx, 150_000).has_dec(14, pack(4, 1, 0)));
}

#[test]
fn exe_244_start_below_dust_done() {
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.04", "-1.0");
    let r = step(&mut fx, 120_000);
    assert_eq!(phase(&fx), AutoFixPhase::Done as u8);
    assert!(r.has_dec(14, pack(4, 3, 0)));
    let n = r.notes_n(note::EXEC_AUTO_FIX_DONE);
    assert_eq!(n[0].level, NoteLevel::P1);
    assert_eq!(
        &n[0].v[..6],
        &[0x01_0000_0701, 0, 0, q("0.04").0 as u64, q("0.04").0 as u64, 0]
    );
    assert!(r.orders.is_empty());
}

#[test]
fn exe_245_start_min_order_fail() {
    let mut insts = default_insts();
    insts[2].min_qty = dec("1.0", 8);
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    broken_at(&mut fx, 0, "1.00", "-1.5");
    let r = step(&mut fx, 120_000);
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.fail),
        (AutoFixPhase::Failed as u8, 2)
    );
    assert!(r.has_dec(14, pack(4, 4, 2)));
    let n = r.notes_n(note::EXEC_AUTO_FIX_FAILED);
    assert_eq!(n[0].level, NoteLevel::P0);
    assert_eq!(
        &n[0].v[..7],
        &[0x01_0000_0701, 1, 2, 0, 0, q("0.5").0 as u64, q("0.5").0 as u64]
    );
    assert!(r.orders.is_empty());
    assert!(matches!(c.view.state, TargetState::Broken(_)));
}

fn hard(fx: &mut Fx, on: bool) {
    fx.risk(A1, |r| r.hard = on);
    let e = if on {
        cap_set(CapSource::KILL, A1, GNONE)
    } else {
        cap_clr(CapSource::KILL, A1, GNONE)
    };
    fx.risk_ev(A1, e);
}

#[test]
fn exe_246_hard_pause() {
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    fx.at(60_000);
    hard(&mut fx, true);
    fx.adv();
    assert_eq!(phase(&fx), AutoFixPhase::WaitHard as u8, "EXE-246.1");
    assert!(step(&mut fx, 120_000).dec(14).is_empty(), "EXE-246.1");
    fx.at(200_000);
    hard(&mut fx, false);
    fx.adv();
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.due_tsc),
        (PRIMED, t(320_000)),
        "EXE-246.2"
    );
    // .3/.4 Running with a slice in flight.
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    let r = step(&mut fx, 120_000);
    let o = r.ord(ExecOrderKind::AutoFix)[0];
    let rf = fx.placed(&o);
    fx.at(130_000);
    hard(&mut fx, true);
    fx.adv();
    assert_eq!(phase(&fx), AutoFixPhase::WaitHard as u8, "EXE-246.3");
    fx.nofill(rf, G7);
    assert!(step(&mut fx, 130_500).orders.is_empty(), "EXE-246.3");
    fx.at(140_000);
    hard(&mut fx, false);
    fx.adv();
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.due_tsc),
        (PRIMED, t(260_000)),
        "EXE-246.4"
    );
    step(&mut fx, 260_000);
    assert_eq!(fx.cell(A1, G7).auto_fix.exec_ms, 0, "EXE-246.4");
    // .5 global HARD arrives as the account's Kill cap.
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    fx.at(60_000);
    fx.risk(A1, |r| r.hard = true);
    fx.risk_ev(A1, cap_set(CapSource::KILL, hft_types::AccountId(0), GNONE));
    fx.adv();
    assert_eq!(phase(&fx), AutoFixPhase::WaitHard as u8, "EXE-246.5");
}

#[test]
fn exe_247_hold_pause() {
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    fx.at(60_000);
    fx.ext(A1, &xfill(G7, 1, P, B, Side::Buy, "0.1", ExternalCause::Manual));
    assert_eq!(phase(&fx), AutoFixPhase::WaitHold as u8, "EXE-247.1");
    fx.at(300_000);
    let now = fx.now();
    fx.ex.note_strategy_action(A1, G7, now, &mut fx.out);
    fx.out.clear();
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.due_tsc),
        (PRIMED, t(420_000)),
        "EXE-247.2"
    );
    let mut fx = Fx::new();
    broken_at(&mut fx, 0, "1.00", "-1.5");
    let r = step(&mut fx, 120_000);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    fx.ext(A1, &xfill(G7, 1, P, B, Side::Buy, "0.1", ExternalCause::Manual));
    assert_eq!(phase(&fx), AutoFixPhase::WaitHold as u8, "EXE-247.3");
    fx.nofill(rf, G7);
    assert!(step(&mut fx, 121_000).orders.is_empty(), "EXE-247.3");
}

/// Recovered Broken g7 (due at t 20 000), gates closed at t 0.
fn recovered(fx: &mut Fx) {
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.broken_since = Tsc(1);
        c.auto_fix.phase = PRIMED;
        c.auto_fix.due_tsc = t(20_000);
    }
    let a = fx.acct(A1);
    a.gate_open = false;
    a.venue_open = 0;
    a.gate_closed_tsc = t(0);
}

#[test]
fn exe_248_recovery_full_window() {
    let mut fx = Fx::new();
    recovered(&mut fx);
    assert!(step(&mut fx, 30_000).dec(14).is_empty(), "EXE-248.1");
    fx.at(50_000);
    let now = fx.now();
    fx.ex.on_venue_gate_open(A1, B, now);
    assert_eq!(fx.cell(A1, G7).auto_fix.due_tsc, t(170_000), "EXE-248.2");
    assert!(step(&mut fx, 169_999).dec(14).is_empty(), "EXE-248.3");
    assert!(step(&mut fx, 170_000).has_dec(14, pack(4, 1, 0)), "EXE-248.4");
}

#[test]
fn exe_249_last_venue_open_wins() {
    let mut fx = Fx::new();
    recovered(&mut fx);
    fx.at(1000);
    let now = fx.now();
    fx.ex.on_venue_gate_open(A1, B, now);
    assert_eq!(fx.cell(A1, G7).auto_fix.due_tsc, t(121_000));
    fx.at(50_000);
    let now = fx.now();
    fx.ex.on_venue_gate_open(A1, A, now);
    assert_eq!(fx.cell(A1, G7).auto_fix.due_tsc, t(170_000));
    assert!(step(&mut fx, 121_000).dec(14).is_empty());
}

fn c_nofill() -> ExecParams {
    let mut p = c0();
    p.max_nofill = 200;
    p
}

/// Running from t 0 with the first slice ended without fill at t 1.
fn running0(fx: &mut Fx) {
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = PRIMED;
        c.auto_fix.due_tsc = t(0);
    }
    let r = step(fx, 0);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    fx.at(1);
    fx.nofill(rf, G7);
}

#[test]
fn exe_250_exec_time() {
    for (row, at, want) in [
        ("EXE-250.1", 10_000, RUNNING),
        ("EXE-250.2", 10_001, AutoFixPhase::Failed as u8),
    ] {
        let mut fx = Fx::with(c_nofill(), vec![g7(), g8()]);
        running0(&mut fx);
        step(&mut fx, at);
        assert_eq!(phase(&fx), want, "{row}");
        if want != RUNNING {
            assert_eq!(fx.cell(A1, G7).auto_fix.fail, 7, "{row}");
        }
    }
    for (row, at, want) in [
        ("EXE-250.3", 39_999, RUNNING),
        ("EXE-250.4", 40_000, RUNNING),
        ("EXE-250.5", 40_001, AutoFixPhase::Failed as u8),
    ] {
        let mut fx = Fx::with(c_nofill(), vec![g7(), g8()]);
        running0(&mut fx);
        fx.at(1000);
        fx.quote_down(P);
        step(&mut fx, 1000);
        fx.at(31_000);
        fx.quote_up(P);
        let r = step(&mut fx, 31_000);
        for o in r.ord(ExecOrderKind::AutoFix) {
            let rf = fx.placed(&o);
            fx.nofill(rf, G7);
        }
        step(&mut fx, at);
        assert_eq!(phase(&fx), want, "{row}");
    }
    let mut fx = Fx::with(c_nofill(), vec![g7(), g8()]);
    running0(&mut fx);
    let mut ms = 500;
    let mut r = step(&mut fx, ms);
    while ms < 10_001 {
        for o in r.ord(ExecOrderKind::AutoFix) {
            fx.place_res(&o, PlaceResult::Rejected(RejectBits::RATE_BACKOFF));
        }
        ms = (ms + 500).min(10_001);
        r = step(&mut fx, ms);
    }
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.fail),
        (AutoFixPhase::Failed as u8, 7),
        "EXE-250.6"
    );
}

#[test]
fn exe_251_exec_time_across_restart() {
    let stop = t(1_000_000);
    let n_now = Tsc(T0 + 3_000_000_000);
    for (row, after, want) in [
        ("4999", 4999u64, RUNNING),
        ("5001", 5001, AutoFixPhase::Failed as u8),
    ] {
        let mut fx = Fx::new();
        fx.legs(A1, G7, &["1.00", "-1.5"]);
        {
            let c = fx.cm(A1, G7);
            c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
            c.auto_fix.phase = RUNNING;
            c.auto_fix.leg_ix = 1;
            c.auto_fix.start_sign = -1;
            c.auto_fix.budget = q("0.5");
            c.auto_fix.exec_ms = 4000;
            c.auto_fix.run_mark_tsc = Tsc(stop.0 - 1_000_000_000);
        }
        fx.ex
            .recovery_end(n_now, stop, 1_000_000_000_000, 1_000_000_000_000);
        let a = fx.cell(A1, G7).auto_fix;
        assert_eq!(
            (a.run_mark_tsc, a.exec_ms),
            (Tsc(n_now.0 - 1_000_000_000), 4000),
            "{row}"
        );
        fx.at_tsc(n_now);
        let now = fx.now();
        fx.ex.on_venue_gate_open(A1, B, now);
        fx.at_tsc(Tsc(n_now.0 + after * 1_000_000));
        fx.touch(P);
        fx.adv();
        assert_eq!(phase(&fx), want, "{row}");
    }
}

/// Running, start_sign −1, budget 0.5, done 0.2, previous slice ended.
fn running_mid(fx: &mut Fx, s: &str, p: &str) {
    fx.legs(A1, G7, &[s, p]);
    fx.spot(A1, A, G7, s, s);
    let c = fx.cm(A1, G7);
    c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
    c.auto_fix.phase = RUNNING;
    c.auto_fix.leg_ix = 1;
    c.auto_fix.start_sign = -1;
    c.auto_fix.budget = q("0.5");
    c.auto_fix.done_qty = q("0.2");
    c.auto_fix.n_slices = 1;
    c.auto_fix.run_mark_tsc = t(0);
}

#[test]
fn exe_252_per_slice_recheck() {
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.3");
    let r = step(&mut fx, 500);
    let o = r.ord(ExecOrderKind::AutoFix)[0];
    assert_eq!((o.intent.side, o.intent.qty), (Side::Buy, q("0.3")), "EXE-252.1");
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.04");
    let r = step(&mut fx, 500);
    assert_eq!(phase(&fx), AutoFixPhase::Done as u8, "EXE-252.2");
    assert_eq!(r.notes_n(note::EXEC_AUTO_FIX_DONE).len(), 1, "EXE-252.2");
    assert!(r.orders.is_empty(), "EXE-252.2");
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.40", "-1.3");
    let r = step(&mut fx, 500);
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.fail),
        (AutoFixPhase::Failed as u8, 8),
        "EXE-252.3"
    );
    assert!(r.orders.is_empty(), "EXE-252.3");
}

#[test]
fn exe_253_slice_qty() {
    let set = |fx: &mut Fx, s: &str, p: &str, leg: u8, sign: i8, budget: &str, done: &str| {
        fx.legs(A1, G7, &[s, p]);
        let c = fx.cm(A1, G7);
        c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
        c.auto_fix.phase = RUNNING;
        c.auto_fix.leg_ix = leg;
        c.auto_fix.start_sign = sign;
        c.auto_fix.budget = q(budget);
        c.auto_fix.done_qty = q(done);
        c.auto_fix.run_mark_tsc = t(0);
    };
    for (row, s, p, leg, sign, budget, done, inv, free, want) in [
        (
            "EXE-253.1",
            "1.00",
            "-1.3",
            1u8,
            -1i8,
            "0.5",
            "0.2",
            "1",
            "1",
            Some("0.3"),
        ),
        (
            "EXE-253.2",
            "1.00",
            "-21.0",
            1,
            -1,
            "20.0",
            "0",
            "1",
            "1",
            Some("9.9"),
        ),
        (
            "EXE-253.3",
            "1.00",
            "-1.5",
            1,
            -1,
            "0.5",
            "0.4",
            "1",
            "1",
            Some("0.1"),
        ),
        (
            "EXE-253.4",
            "1.55",
            "-1.0",
            0,
            1,
            "0.55",
            "0",
            "0.55",
            "0.30",
            Some("0.30"),
        ),
        (
            "EXE-253.5",
            "1.55",
            "-1.0",
            0,
            1,
            "0.55",
            "0",
            "0.55",
            "0.005",
            None,
        ),
    ] {
        let mut fx = Fx::new();
        set(&mut fx, s, p, leg, sign, budget, done);
        fx.spot(A1, A, G7, inv, free);
        let r = step(&mut fx, 500);
        let o = r.ord(ExecOrderKind::AutoFix);
        match want {
            Some(x) => assert_eq!(o[0].intent.qty, q(x), "{row}"),
            None => {
                assert!(o.is_empty(), "{row}");
                let a = fx.cell(A1, G7).auto_fix;
                assert_eq!((a.phase, a.run_mark_tsc), (RUNNING, Tsc(0)), "{row}");
            }
        }
    }
    let mut insts = default_insts();
    insts[2].min_qty = dec("0.2", 8);
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    set(&mut fx, "1.00", "-1.1", 1, -1, "0.5", "0.4");
    step(&mut fx, 500);
    let c = fx.cell(A1, G7);
    assert_eq!(
        (c.auto_fix.phase, c.auto_fix.fail),
        (AutoFixPhase::Failed as u8, 2),
        "EXE-253.6"
    );
}

#[test]
fn exe_254_autofix_order_fields() {
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    fx.cm(A1, G7).auto_fix.done_qty = q("0");
    let r = step(&mut fx, 500);
    let o = r.ord(ExecOrderKind::AutoFix)[0];
    assert_eq!(
        (
            o.kind.code(),
            o.origin,
            o.intent.side,
            o.intent.inst,
            o.intent.qty,
            o.hedge_cap_qty
        ),
        (10, ExecOrigin::Engine, Side::Buy, P, q("0.5"), q("0.6"))
    );
    assert_eq!(
        (o.intent.tif, o.intent.link.get().unwrap().role),
        (Tif::Ioc, LinkRole::HedgeFix)
    );
    assert!(o.intent.reduce_only());
    let d = r.dec(14).into_iter().find(|d| d.detail == pack(4, 2, 0)).unwrap();
    assert_eq!((d.origin, d.qty), (6, q("0.5")));
}

#[test]
fn exe_255_slice_spacing() {
    for (row, partial, at, order) in [("EXE-255.1", true, 199u64, false), ("EXE-255.2", true, 200, true)] {
        let mut fx = Fx::new();
        running_mid(&mut fx, "1.00", "-1.5");
        let r = step(&mut fx, 0);
        let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
        if partial {
            fx.at(1);
            fx.legs(A1, G7, &["1.00", "-1.4"]);
            fx.fill(rf, G7, "0.1", true);
        }
        assert_eq!(
            !step(&mut fx, at).ord(ExecOrderKind::AutoFix).is_empty(),
            order,
            "{row}"
        );
    }
    for (row, at, order) in [("EXE-255.3", 499u64, false), ("EXE-255.4", 500, true)] {
        let mut fx = Fx::new();
        running_mid(&mut fx, "1.00", "-1.5");
        let r = step(&mut fx, 0);
        let rr = fx.place_res(
            &r.ord(ExecOrderKind::AutoFix)[0],
            PlaceResult::Rejected(RejectBits::RATE_BACKOFF),
        );
        assert!(
            rr.decisions
                .iter()
                .any(|d| d.action == 14 && d.detail == pack(4, 2, 0) && d.gate == RejectBits::RATE_BACKOFF.0)
        );
        assert_eq!(
            !step(&mut fx, at).ord(ExecOrderKind::AutoFix).is_empty(),
            order,
            "{row}"
        );
    }
}

#[test]
fn exe_256_retry_not_fail() {
    for (row, bits) in [
        ("EXE-256.1", RejectBits::RATE_BACKOFF),
        ("EXE-256.3", RejectBits::CLOSE_ONLY),
    ] {
        let mut fx = Fx::new();
        running_mid(&mut fx, "1.00", "-1.5");
        let r = step(&mut fx, 0);
        let rr = fx.place_res(&r.ord(ExecOrderKind::AutoFix)[0], PlaceResult::Rejected(bits));
        assert_eq!(phase(&fx), RUNNING, "{row}");
        assert!(rr.notes_n(note::EXEC_AUTO_FIX_FAILED).is_empty(), "{row}");
        assert_eq!(fx.cell(A1, G7).gated_bits, bits, "{row}");
    }
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    let r = step(&mut fx, 0);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    let rr = fx.local_fail(rf, G7, 5);
    assert_eq!(phase(&fx), RUNNING, "EXE-256.2");
    assert!(rr.notes_n(note::EXEC_AUTO_FIX_FAILED).is_empty(), "EXE-256.2");
    // .4/.5/.6
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    let mut ms = 0;
    for k in 0..5 {
        let r = step(&mut fx, ms);
        let o = r.ord(ExecOrderKind::AutoFix);
        assert_eq!(o.len(), 1, "EXE-256.4 slice {k}");
        let rf = fx.placed(&o[0]);
        fx.at(ms + 1);
        fx.nofill(rf, G7);
        ms += 200;
    }
    // After 5 no-fills: next slice only 2000 ms after the last one (t 801).
    assert!(step(&mut fx, 1000).orders.is_empty(), "EXE-256.5");
    assert!(
        matches!(fx.cell(A1, G7).view.state, TargetState::Broken(_)),
        "EXE-256.5"
    );
    assert_eq!(
        step(&mut fx, 2801).ord(ExecOrderKind::AutoFix).len(),
        1,
        "EXE-256.5"
    );
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    fx.cm(A1, G7).nofill_streak = 4;
    let r = step(&mut fx, 0);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    fx.legs(A1, G7, &["1.00", "-1.4"]);
    fx.fill(rf, G7, "0.1", true);
    assert_eq!(fx.cell(A1, G7).nofill_streak, 0, "EXE-256.6");
}

#[test]
fn exe_257_end_table() {
    type Ev = fn(&mut Fx, &ExecOrder);
    let rows: [(&str, Ev, u8); 4] = [
        (
            "EXE-257.1",
            |fx, o| {
                let rf = fx.placed(o);
                fx.exch_reject(rf, G7);
            },
            4,
        ),
        (
            "EXE-257.2",
            |fx, o| {
                let rf = fx.placed(o);
                fx.local_fail(rf, G7, 6);
            },
            4,
        ),
        (
            "EXE-257.3",
            |fx, o| {
                fx.place_res(o, PlaceResult::Rejected(RejectBits::HEDGE_LIMIT));
            },
            9,
        ),
        (
            "EXE-257.4",
            |fx, o| {
                fx.place_res(o, PlaceResult::Rejected(RejectBits::PX_BAND));
            },
            9,
        ),
    ];
    for (row, ev, fail) in rows {
        let mut fx = Fx::new();
        running_mid(&mut fx, "1.00", "-1.5");
        let r = step(&mut fx, 0);
        let o = r.ord(ExecOrderKind::AutoFix)[0];
        ev(&mut fx, &o);
        let c = fx.cell(A1, G7);
        assert_eq!(
            (c.auto_fix.phase, c.auto_fix.fail),
            (AutoFixPhase::Failed as u8, fail),
            "{row}"
        );
        let n = fx.log.notes_n(note::EXEC_AUTO_FIX_FAILED);
        assert!(fx.log.has_dec(14, pack(4, 4, u16::from(fail))), "{row}");
        if row == "EXE-257.3" {
            assert_eq!(n[0].v[3], 1 << 26, "{row}");
        }
        if row == "EXE-257.4" {
            assert_eq!(n[0].v[3], 1 << 9, "{row}");
        }
    }
    // .5 MinOrder (not balance).
    let mut insts = default_insts();
    insts[2].min_qty = dec("0.6", 8);
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    running_mid(&mut fx, "1.00", "-1.5");
    step(&mut fx, 0);
    assert_eq!(fx.cell(A1, G7).auto_fix.fail, 2, "EXE-257.5");
    // .6 Residual.
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.1");
    fx.cm(A1, G7).auto_fix.done_qty = q("0.5");
    step(&mut fx, 0);
    assert_eq!(fx.cell(A1, G7).auto_fix.fail, 6, "EXE-257.6");
    // .7 Deadline.
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    step(&mut fx, 10_001);
    assert_eq!(fx.cell(A1, G7).auto_fix.fail, 7, "EXE-257.7");
    // .8 SignFlip.
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.40", "-1.3");
    step(&mut fx, 0);
    assert_eq!(fx.cell(A1, G7).auto_fix.fail, 8, "EXE-257.8");
}

#[test]
fn exe_258_failure_final() {
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.3");
    let r = step(&mut fx, 0);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    let r = fx.exch_reject(rf, G7);
    let n = r.notes_n(note::EXEC_AUTO_FIX_FAILED);
    assert_eq!(n[0].level, NoteLevel::P0);
    assert_eq!(
        &n[0].v[..7],
        &[
            0x01_0000_0701,
            1,
            4,
            0,
            q("0.2").0 as u64,
            q("0.5").0 as u64,
            q("0.3").0 as u64
        ]
    );
    assert!(r.has_dec(14, pack(4, 4, 4)));
    assert!(matches!(
        fx.ex.view(A1, G7).unwrap().state,
        TargetState::Broken(_)
    ));
    let mut ms = 10_000;
    while ms <= 86_400_000 {
        let r = step(&mut fx, ms);
        assert!(r.dec(14).is_empty() && r.orders.is_empty());
        ms += 3_600_000;
    }
}

#[test]
fn exe_259_inflight_not_counted() {
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    fx.cm(A1, G7).auto_fix.done_qty = q("0");
    let r = step(&mut fx, 0);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    fx.infl(A1, G7, 1, "0.5");
    let r = step(&mut fx, 100);
    assert!(!r.has_dec(14, pack(4, 3, 0)) && r.orders.is_empty());
    fx.infl(A1, G7, 1, "0");
    fx.at(150);
    let r = fx.nofill(rf, G7);
    assert!(!r.has_dec(14, pack(4, 3, 0)));
    let r = step(&mut fx, 400);
    assert_eq!(r.ord(ExecOrderKind::AutoFix).len(), 1);
}

#[test]
fn exe_260_engine_target_takeover() {
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    fx.cm(A1, G7).auto_fix.exec_ms = 3000;
    let r = step(&mut fx, 0);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.force(A1, B, "0.2");
    fx.adv();
    fx.at(10);
    fx.nofill(rf, G7);
    let r = step(&mut fx, 20);
    assert_eq!(phase(&fx), PRIMED);
    assert!(r.ord(ExecOrderKind::AutoFix).is_empty());
    for o in r.ord(ExecOrderKind::ForceStep) {
        let rf = fx.placed(&o);
        fx.nofill(rf, G7);
    }
    fx.legs(A1, G7, &["1.00", "-1.4"]);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L2);
    fx.risk_ev(A1, ms_margin(MarginLevel::L2));
    fx.at(500);
    fx.adv();
    let r = step(&mut fx, 600);
    assert!(fx.log.has_dec(14, pack(4, 1, 0)) || r.has_dec(14, pack(4, 1, 0)));
    let a = fx.cell(A1, G7).auto_fix;
    assert_eq!((a.budget, a.exec_ms, a.done_qty), (q("0.4"), 0, q("0")));
}

#[test]
fn exe_262_done_waits_capcleared() {
    let mut fx = Fx::new();
    running_mid(&mut fx, "1.00", "-1.5");
    fx.cm(A1, G7).auto_fix.done_qty = q("0");
    let r = step(&mut fx, 0);
    let rf = fx.placed(&r.ord(ExecOrderKind::AutoFix)[0]);
    fx.legs(A1, G7, &["1.00", "-1.03"]);
    fx.fill(rf, G7, "0.47", true);
    let r = fx.adv();
    assert_eq!(phase(&fx), AutoFixPhase::Done as u8);
    assert!(matches!(
        fx.ex.view(A1, G7).unwrap().state,
        TargetState::Broken(_)
    ));
    assert!(r.orders.is_empty());
    step(&mut fx, 1000);
    assert_eq!(fx.log.notes_n(note::EXEC_AUTO_FIX_DONE).len(), 1);
}
