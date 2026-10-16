//! EXE-380–EXE-385: gate rejections, backoff, execution failures, no-fill.
use crate::common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{MarginLevel, StuckReason, TargetState};
use hft_types::{CapSource, RejectBits, RunMode, Tsc};

#[test]
fn exe_380_gate_backoff() {
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10")).orders[0];
    fx.place_res(&o, PlaceResult::Rejected(RejectBits::RATE));
    let c = fx.cell(A1, G8);
    assert_eq!(
        (c.gated_bits, c.gated_since, c.wake_at, c.fail_streak),
        (RejectBits::RATE, t(0), t(500), 0),
        "EXE-380.1"
    );
    fx.at(499);
    fx.touch(P);
    assert!(fx.adv().orders.is_empty(), "EXE-380.2");
    fx.at(500);
    fx.touch(P);
    assert_eq!(fx.adv().orders.len(), 1, "EXE-380.3");
}

#[test]
fn exe_381_backoff_bounded() {
    let mut fx = Fx::new();
    let mut r = fx.go(&t1("-10"));
    let mut times = Vec::new();
    for ms in 0..10_000u64 {
        if ms > 0 {
            fx.at(ms);
            fx.touch(P);
            r = fx.adv();
        }
        for o in r.orders.iter().filter(|o| o.intent.group == G8) {
            times.push(ms);
            fx.place_res(o, PlaceResult::Rejected(RejectBits::KILLED));
        }
    }
    let want: Vec<u64> = (0..20).map(|k| k * 500).collect();
    assert_eq!(times, want);
}

#[test]
fn exe_381_rate_retry_uses_next_free_without_spinning() {
    for (kind, ready) in [(0, 3), (0, 750), (1, 3), (1, 750), (2, 3), (2, 750)] {
        let mut fx = Fx::new();
        let (group, o) = match kind {
            1 => {
                fx.legs(A1, G7, &["1.00", "-1.5"]);
                fx.leg_adjust(A1, G7);
                (G7, fx.adv().ord(ExecOrderKind::HedgeFix)[0])
            }
            2 => {
                fx.legs(A1, G8, &["-10"]);
                fx.flatten(A1);
                (G8, fx.adv().ord(ExecOrderKind::ForceStep)[0])
            }
            _ => (G8, fx.go(&t1("-10")).orders[0]),
        };
        fx.place_res(
            &o,
            PlaceResult::RateLimited {
                bits: RejectBits::RATE,
                retry_at: t(ready),
            },
        );
        assert_eq!(fx.cell(A1, group).wake_at, t(ready));
        for ms in 0..ready {
            fx.at(ms);
            fx.touch(P);
            assert!(fx.adv().orders.is_empty(), "premature retry at {ms}");
        }
        fx.at(ready);
        fx.touch(P);
        let retry = fx.adv();
        assert_eq!(retry.orders.len(), 1, "kind={kind} ready={ready}");
        fx.placed(&retry.orders[0]);
        assert!(fx.adv().orders.is_empty(), "one retry per wake");
    }
}

fn reject_n(fx: &mut Fx, n: u64, start: u64) -> u64 {
    let mut ms = start;
    for _ in 0..n {
        fx.at(ms);
        fx.touch(P);
        let r = fx.adv();
        let o = r.orders[0];
        let rf = fx.placed(&o);
        fx.exch_reject(rf, G8);
        ms += 300;
    }
    ms
}

#[test]
fn exe_382_slice_fails() {
    let mut fx = Fx::new();
    fx.set(I1, &t1("-10")).0.unwrap();
    reject_n(&mut fx, 2, 0);
    let c = fx.cell(A1, G8);
    assert_eq!(
        (c.fail_streak, c.view.state),
        (2, TargetState::Working),
        "EXE-382.1"
    );
    let mut fx = Fx::new();
    fx.set(I1, &t1("-10")).0.unwrap();
    let end = reject_n(&mut fx, 3, 0);
    assert!(fx.log.states().contains(&(2, 3, pack(2, 5, 0))), "EXE-382.2");
    assert_eq!(
        fx.log.notes_n(hft_exec::decision::note::EXEC_STUCK)[0].level,
        hft_event::NoteLevel::P1,
        "EXE-382.2"
    );
    let mut ms = end;
    while ms < end + 3_600_000 {
        fx.at(ms);
        fx.touch(P);
        assert!(fx.adv().orders.is_empty(), "EXE-382.3");
        ms += 60_000;
    }
    fx.clear(A1, G8);
    fx.set(I1, &t1("-10")).0.unwrap();
    let r = fx.adv();
    let c = fx.cell(A1, G8);
    assert_eq!(
        (c.view.state, c.fail_streak, r.orders.len()),
        (TargetState::Working, 0, 1),
        "EXE-382.4"
    );
    // .5 two EncodeError + one exchange reject.
    let mut fx = Fx::new();
    fx.set(I1, &t1("-10")).0.unwrap();
    let mut ms = 0;
    for k in 0..3 {
        fx.at(ms);
        fx.touch(P);
        let o = fx.adv().orders[0];
        let rf = fx.placed(&o);
        if k < 2 {
            fx.local_fail(rf, G8, 6);
        } else {
            fx.exch_reject(rf, G8);
        }
        ms += 300;
    }
    assert!(fx.log.states().contains(&(2, 3, pack(2, 5, 0))), "EXE-382.5");
    // .6 a partial fill resets the streak.
    let mut fx = Fx::new();
    fx.set(I1, &t1("-10")).0.unwrap();
    let ms = reject_n(&mut fx, 2, 0);
    fx.at(ms);
    fx.touch(P);
    let o = fx.adv().orders[0];
    let rf = fx.placed(&o);
    fx.legs(A1, G8, &["-1"]);
    fx.fill(rf, G8, "1", true);
    reject_n(&mut fx, 2, ms + 300);
    let c = fx.cell(A1, G8);
    assert_eq!(
        (c.fail_streak, c.view.state),
        (2, TargetState::Working),
        "EXE-382.6"
    );
    // .7 L3 RiskReject ×3 then a new step.
    let mut fx = Fx::new();
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    fx.legs(A1, G8, &["-10"]);
    fx.force(A1, B, "0.2");
    let mut r = fx.adv();
    for k in 0..3u64 {
        let o = r.ord(ExecOrderKind::ForceStep)[0];
        fx.place_res(&o, PlaceResult::Rejected(RejectBits::PX_BAND));
        fx.at(200 * (k + 1));
        fx.touch(P);
        r = fx.adv();
    }
    assert_eq!(
        fx.cell(A1, G8).view.state,
        TargetState::Stuck(StuckReason::SliceFails),
        "EXE-382.7"
    );
    fx.at(5000);
    fx.force(A1, B, "0.2");
    fx.adv();
    let c = fx.cell(A1, G8);
    assert_eq!(c.fail_streak, 0, "EXE-382.7");
    assert_ne!(
        c.view.state,
        TargetState::Stuck(StuckReason::SliceFails),
        "EXE-382.7"
    );
}

#[test]
fn exe_383_nofill() {
    let run = |n: usize| {
        let mut fx = Fx::new();
        fx.set(I1, &t1("-10")).0.unwrap();
        let mut ms = 0u64;
        for _ in 0..n {
            fx.at(ms);
            fx.touch(P);
            let o = fx.adv().orders[0];
            let rf = fx.placed(&o);
            fx.nofill(rf, G8);
            ms += 300;
        }
        fx
    };
    let fx = run(4);
    assert_eq!(
        (fx.cell(A1, G8).nofill_streak, fx.cell(A1, G8).view.state),
        (4, TargetState::Working),
        "EXE-383.1"
    );
    let mut fx = run(4);
    fx.at(1200);
    fx.touch(P);
    let o = fx.adv().orders[0];
    let rf = fx.placed(&o);
    let r = fx.nofill(rf, G8);
    assert!(r.states().contains(&(2, 3, pack(2, 3, 0))), "EXE-383.2");
    fx.at(3199);
    fx.touch(P);
    assert!(fx.adv().orders.is_empty(), "EXE-383.3");
    fx.at(3200);
    fx.touch(P);
    assert_eq!(fx.adv().orders.len(), 1, "EXE-383.4");
    let mut fx = run(4);
    fx.at(1200);
    fx.touch(P);
    let o = fx.adv().orders[0];
    let rf = fx.placed(&o);
    fx.fill(rf, G8, "1", true);
    let c = fx.cell(A1, G8);
    assert_eq!((c.nofill_streak, c.fail_streak), (0, 0), "EXE-383.5");
}

#[test]
fn exe_384_wake_reset() {
    type Ev = fn(&mut Fx);
    let rows: [(&str, Ev); 3] = [
        ("EXE-384.1", |fx| {
            let now = fx.now();
            fx.ex
                .on_mode(A1, G8, RunMode::CloseOnly, RunMode::Live, now, &mut fx.out);
            fx.out.clear();
        }),
        ("EXE-384.2", |fx| {
            fx.risk_ev(A1, cap_clr(CapSource::KILL, A1, GNONE))
        }),
        ("EXE-384.3", |fx| {
            fx.risk_ev(A1, cap_clr(CapSource::UNKNOWN_TIMEOUT, A1, GNONE))
        }),
    ];
    for (row, ev) in rows {
        let mut fx = Fx::new();
        let o = fx.go(&t1("-10")).orders[0];
        fx.place_res(&o, PlaceResult::Rejected(RejectBits::RATE));
        assert_eq!(fx.cell(A1, G8).wake_at, t(500));
        fx.at(100);
        ev(&mut fx);
        assert!(fx.ex.has_pending(), "{row}");
        fx.at(300);
        assert_eq!(fx.adv().orders.len(), 1, "{row}");
    }
}

#[test]
fn exe_385_local_send_fail() {
    for (row, status, gated) in [
        ("EXE-385.1", 2u32, true),
        ("EXE-385.2", 3, true),
        ("EXE-385.3", 5, true),
        ("EXE-385.4", 7, true),
        ("EXE-385.5", 6, false),
    ] {
        let mut fx = Fx::new();
        let o = fx.go(&t1("-10")).orders[0];
        let rf = fx.placed(&o);
        fx.local_fail(rf, G8, status);
        let c = fx.cell(A1, G8);
        if gated {
            assert_eq!((c.fail_streak, c.wake_at), (0, t(500)), "{row}");
        } else {
            assert_eq!(c.fail_streak, 1, "{row}");
        }
        let _ = Tsc(0);
    }
}
