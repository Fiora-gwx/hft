//! EXE-506: `engine_reduce_pending` and `drain_ready`.
mod common;
use common::*;
use hft_exec::*;
use hft_strategy_sdk::{EngineOrigin, MarginLevel, TargetOrigin, TargetReason, TargetState};

fn engine(
    fx: &mut Fx,
    a: hft_types::AccountId,
    g: hft_types::GroupId,
    tg: &hft_strategy_sdk::TargetIntent,
    reason: TargetReason,
    state: TargetState,
) {
    inject(fx, tg);
    let c = fx.cm(a, g);
    c.view.origin = TargetOrigin::Engine(EngineOrigin::MarginStress);
    c.view.intent.reason = reason;
    c.view.state = state;
}

#[test]
fn exe_506_engine_reduce_pending() {
    type Setup = fn(&mut Fx);
    let rows: [(&str, Setup, bool, bool); 10] = [
        ("EXE-506.1", |_| {}, false, true),
        (
            "EXE-506.2",
            |fx| {
                fx.legs(A1, G8, &["-1.0"]);
                engine(
                    fx,
                    A1,
                    G8,
                    &t1("0"),
                    TargetReason::MarginStress,
                    TargetState::Working,
                );
            },
            true,
            false,
        ),
        (
            "EXE-506.3",
            |fx| {
                fx.legs(A1, G7, &["1", "-1"]);
                fx.quote_down(P);
                fx.flatten(A1);
                fx.adv();
            },
            true,
            false,
        ),
        (
            "EXE-506.4",
            |fx| {
                fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
                fx.perp(A1, B, P4, "-0.5");
                fx.quote_down(P4);
                fx.force(A1, B, "0.5");
                fx.adv();
            },
            true,
            false,
        ),
        (
            "EXE-506.5",
            |fx| {
                engine(
                    fx,
                    A1,
                    G7,
                    &t0("6", "-6"),
                    TargetReason::Hard,
                    TargetState::Working,
                )
            },
            true,
            false,
        ),
        (
            "EXE-506.6",
            |fx| {
                engine(
                    fx,
                    S1,
                    G9,
                    &taker(S1, G9, &[(0, "0")]),
                    TargetReason::Hard,
                    TargetState::Working,
                )
            },
            true,
            false,
        ),
        (
            "EXE-506.7",
            |fx| {
                fx.go(&t1("-10"));
            },
            false,
            false,
        ),
        (
            "EXE-506.8",
            |fx| fx.cm(A1, G7).ext_deadline = t(5000),
            false,
            false,
        ),
        (
            "EXE-506.9",
            |fx| {
                engine(
                    fx,
                    A1,
                    G8,
                    &t1("0"),
                    TargetReason::MarginStress,
                    TargetState::Idle,
                )
            },
            false,
            true,
        ),
        (
            "EXE-506.10",
            |fx| {
                let c = fx.cm(A1, G7);
                c.view.state = TargetState::Broken(hft_strategy_sdk::BrokenReason::LinkBroken);
                c.auto_fix.phase = AutoFixPhase::Primed as u8;
                c.auto_fix.due_tsc = t(100_000);
            },
            false,
            true,
        ),
    ];
    for (row, setup, pending, ready) in rows {
        let g9s = gspec(hft_types::GroupId(0x0300_0009), &[(P, B)]);
        let mut fx = Fx::with(c0(), vec![g7(), g8(), g9s]);
        if row == "EXE-506.6" {
            let g = hft_types::GroupId(0x0300_0009);
            engine(
                &mut fx,
                S1,
                g,
                &taker(S1, g, &[(0, "0")]),
                TargetReason::Hard,
                TargetState::Working,
            );
        } else {
            setup(&mut fx);
        }
        fx.out.clear();
        assert_eq!(fx.ex.engine_reduce_pending(), pending, "{row}");
        assert_eq!(fx.ex.drain_ready(), ready, "{row}");
        assert!(fx.out.is_empty(), "EXE-506.11");
    }
}
