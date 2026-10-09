//! `Ctx` capacities (SDK-145, SDK-146).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_strategy_sdk::ctx::{CAP_CANCEL, CAP_CANCEL_TIMER, CAP_HALT, CAP_NOTE, CAP_OPS, CAP_PLACE, CAP_TARGET, CAP_TIMER};
use hft_strategy_sdk::testing::{CtxHarness, Recorded};
use hft_strategy_sdk::*;
use hft_types::{Px, Qty, Side, Tsc};

fn it() -> OrderIntent {
    order(s3(0xFF01), G1, I1, Side::Buy, IntentClass::Open, Px(6_500_000), Qty(1_000), 0)
}

fn run(f: impl FnOnce(&mut Ctx<'_>)) -> Recorded {
    CtxHarness::new(&EchoProbe::DESC).run(f)
}

#[test]
fn sdk_145_ctx_capacities() {
    let r = run(|ctx| {
        for i in 0..16 {
            assert_eq!(ctx.place(it()).map(|_| ()), Ok(()), "place {i}");
        }
        assert_eq!(ctx.place(it()), Err(CtxFull));
    });
    assert_eq!((r.places.len(), r.full), (16, 1), "SDK-145.1");

    let r = run(|ctx| {
        for i in 0..64 {
            let ok = if i % 2 == 0 { ctx.target(pair_target(A1, G1, 1_000, 0)).map(|_| ()) } else { ctx.clear_target(A1, G1) };
            assert_eq!(ok, Ok(()), "target/clear {i}");
        }
        assert_eq!(ctx.clear_target(A1, G2), Err(CtxFull));
    });
    assert_eq!((r.targets.len() + r.clears.len(), r.full), (64, 1), "SDK-145.2");

    let h = OrderRef::new(V1, A1, 1, 1);
    let r = run(|ctx| {
        for i in 0..16 {
            let ok = if i % 2 == 0 { ctx.cancel(h) } else { ctx.amend(h, Px(6_500_100), Qty(500)) };
            assert_eq!(ok, Ok(()), "cancel/amend {i}");
        }
        assert_eq!(ctx.cancel(h), Err(CtxFull));
    });
    assert_eq!((r.cancels.len(), r.full), (16, 1), "SDK-145.3");

    let r = run(|ctx| {
        for i in 0..8 {
            assert!(ctx.timer_at(Tsc(1_000 + i), 1, G1).is_ok(), "timer {i}");
        }
        assert_eq!(ctx.timer_at(Tsc(9_999), 1, G1), Err(CtxFull));
    });
    assert_eq!((r.timers.len(), r.full), (8, 1), "SDK-145.4");

    let r = run(|ctx| {
        for i in 0..17u64 {
            ctx.note(ECHO_FIRE, NoteLevel::Info, G1, I1, [i, 0, 0, 0, 0, 0, 0, 0]);
        }
    });
    assert_eq!((r.notes.len(), r.full), (16, 1), "SDK-145.5");
    assert_eq!(r.notes[15].v[0], 15, "the 17th is dropped, accepted ones unchanged");

    let r = run(|ctx| {
        for i in 0..4u16 {
            assert_eq!(ctx.request_halt(G1, i), Ok(()), "halt {i}");
        }
        assert_eq!(ctx.request_halt(G1, 9), Err(CtxFull));
    });
    assert_eq!((r.halts.len(), r.full), (4, 1), "SDK-145.6");

    let r = run(|ctx| {
        for _ in 0..16 {
            ctx.place(it()).unwrap();
        }
        assert!(ctx.target(pair_target(A1, G1, 1_000, 0)).is_ok(), "SDK-145.7");
    });
    assert_eq!((r.places.len(), r.targets.len(), r.full), (16, 1, 0), "SDK-145.7");
}

#[test]
fn sdk_146_capacity_constants() {
    assert_eq!(
        (CAP_PLACE, CAP_TARGET, CAP_CANCEL, CAP_TIMER, CAP_NOTE, CAP_HALT, CAP_OPS),
        (16, 64, 16, 8, 16, 4, 128)
    );
    assert_eq!(CAP_CANCEL_TIMER, 8);
}
