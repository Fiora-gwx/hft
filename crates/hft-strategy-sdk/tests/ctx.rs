//! `Ctx` capacities (SDK-145, SDK-146).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_strategy_sdk::ctx::{
    CAP_CANCEL, CAP_CANCEL_TIMER, CAP_HALT, CAP_NOTE, CAP_OPS, CAP_PLACE, CAP_TARGET, CAP_TIMER,
};
use hft_strategy_sdk::testing::{CtxHarness, Recorded};
use hft_strategy_sdk::*;
use hft_types::{Px, Qty, Side, Tsc};

fn it() -> OrderIntent {
    order(
        s3(0xFF01),
        G1,
        I1,
        Side::Buy,
        IntentClass::Open,
        Px(6_500_000),
        Qty(1_000),
        0,
    )
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
            let ok = if i % 2 == 0 {
                ctx.target(pair_target(A1, G1, 1_000, 0)).map(|_| ())
            } else {
                ctx.clear_target(A1, G1)
            };
            assert_eq!(ok, Ok(()), "target/clear {i}");
        }
        assert_eq!(ctx.clear_target(A1, G2), Err(CtxFull));
    });
    assert_eq!((r.targets.len() + r.clears.len(), r.full), (64, 1), "SDK-145.2");

    let h = OrderRef::new(V1, A1, 1, 1);
    let r = run(|ctx| {
        for i in 0..16 {
            let ok = if i % 2 == 0 {
                ctx.cancel(h)
            } else {
                ctx.amend(h, Px(6_500_100), Qty(500))
            };
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
    assert_eq!(
        r.notes[15].v[0], 15,
        "the 17th is dropped, accepted ones unchanged"
    );

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
        (
            CAP_PLACE, CAP_TARGET, CAP_CANCEL, CAP_TIMER, CAP_NOTE, CAP_HALT, CAP_OPS
        ),
        (16, 64, 16, 8, 16, 4, 128)
    );
    assert_eq!(CAP_CANCEL_TIMER, 8);
}

#[test]
fn sdk_212_timer_admission_preserves_call_order() {
    use hft_strategy_sdk::ctx::{Op, TimerReq};

    let mut a = CtxBufs::new();
    let mut b = CtxBufs::new();
    let mut remaining = 2;
    assert_eq!(
        a.ctx_with_timer_quota(&mut remaining, &[])
            .timer_at(Tsc(10), 1, G1),
        Ok(TimerId(1))
    );
    assert_eq!(
        b.ctx_with_timer_quota(&mut remaining, &[])
            .timer_at(Tsc(20), 2, G1),
        Ok(TimerId(1))
    );
    // Reborrowing one instance's Ctx for another callback cannot reset the shared balance.
    assert_eq!(
        a.ctx_with_timer_quota(&mut remaining, &[])
            .timer_at(Tsc(30), 3, G1),
        Err(CtxFull)
    );
    assert_eq!((remaining, a.next_timer_id(), a.full_count()), (0, 2, 1));
    assert_eq!(a.ops().filter(|(_, op)| matches!(op, Op::Timer(_))).count(), 1);

    let live = [TimerReq {
        id: TimerId(11),
        at: Tsc(100),
        tag: 7,
        group: G1,
    }];
    let mut c = CtxBufs::new();
    c.set_next_timer_id(12);
    let mut remaining = 0;
    {
        let mut ctx = c.ctx_with_timer_quota(&mut remaining, &live);
        ctx.cancel_timer(TimerId(999));
        assert_eq!(ctx.timer_at(Tsc(30), 3, G1), Err(CtxFull));
        ctx.cancel_timer(TimerId(11));
        assert_eq!(ctx.timer_at(Tsc(30), 3, G1), Ok(TimerId(12)));
        ctx.cancel_timer(TimerId(11));
        assert_eq!(ctx.timer_at(Tsc(40), 4, G1), Err(CtxFull));
        ctx.cancel_timer(TimerId(12));
        assert_eq!(ctx.timer_at(Tsc(40), 4, G1), Ok(TimerId(13)));
    }
    assert_eq!((remaining, c.full_count()), (0, 2));

    // Cancelling an as-yet unallocated id is a no-op, not a credit and not a future cancel.
    let mut c = CtxBufs::new();
    let mut remaining = 1;
    {
        let mut ctx = c.ctx_with_timer_quota(&mut remaining, &[]);
        ctx.cancel_timer(TimerId(1));
        let id = ctx.timer_at(Tsc(10), 1, G1).unwrap();
        ctx.cancel_timer(id);
        assert_eq!(ctx.timer_at(Tsc(20), 2, G1), Ok(TimerId(2)));
        ctx.cancel_timer(id);
        assert_eq!(ctx.timer_at(Tsc(30), 3, G1), Err(CtxFull));
    }
    assert_eq!((remaining, c.next_timer_id()), (0, 3));

    // Per-event buffers remain bounded even when every accepted timer is cancelled immediately.
    let mut c = CtxBufs::new();
    let mut remaining = 1;
    for tag in 0..CAP_TIMER as u32 {
        let mut ctx = c.ctx_with_timer_quota(&mut remaining, &[]);
        let id = ctx.timer_at(Tsc(10), tag, G1).unwrap();
        ctx.cancel_timer(id);
    }
    assert_eq!(remaining, 1);
    assert_eq!(
        c.ctx_with_timer_quota(&mut remaining, &[])
            .timer_at(Tsc(20), 9, G1),
        Err(CtxFull)
    );
    assert_eq!((remaining, c.next_timer_id(), c.full_count()), (1, 9, 1));
    c.begin();
    assert_eq!(
        c.ctx_with_timer_quota(&mut remaining, &[])
            .timer_at(Tsc(20), 9, G1),
        Ok(TimerId(9))
    );

    let mut c = CtxBufs::new();
    let mut remaining = 0;
    for _ in 0..CAP_CANCEL_TIMER {
        c.ctx_with_timer_quota(&mut remaining, &live)
            .cancel_timer(TimerId(999));
    }
    c.ctx_with_timer_quota(&mut remaining, &live)
        .cancel_timer(TimerId(11));
    assert_eq!(
        (remaining, c.full_count()),
        (0, 1),
        "dropped cancel released quota"
    );

    let mut c = CtxBufs::new();
    c.set_next_timer_id(u64::MAX);
    let mut remaining = 1;
    assert_eq!(
        c.ctx_with_timer_quota(&mut remaining, &[])
            .timer_at(Tsc(10), 1, G1),
        Err(CtxFull)
    );
    assert_eq!((remaining, c.next_timer_id(), c.full_count()), (1, u64::MAX, 1));
}

#[test]
fn sdk_216_spend_buy_and_spend_qty() {
    use hft_types::{AssetExp, Notional, Tif};
    assert_eq!(OrderIntent::SPEND, 0x80, "SDK-216.1");
    assert_eq!(
        u16::from(OrderIntent::SPEND),
        hft_event::OrderFlags::SPEND.0,
        "SDK-216.1"
    );
    let o = OrderIntent::spend_buy(V1, A1, G1, I1, Tif::Ioc, Px(110), Qty(9_090_000), 7);
    assert_eq!(o.side, Side::Buy, "SDK-216.1");
    assert_eq!(o.class, IntentClass::Open, "SDK-216.1");
    assert_eq!(o.tif, Tif::Ioc, "SDK-216.1");
    assert_eq!(o.flags, OrderIntent::SPEND, "SDK-216.1");
    assert!(o.spend() && !o.reduce_only(), "SDK-216.1");
    assert!(o.link.get().is_none(), "SDK-216.1");
    assert_eq!((o.max_rest_ms, o.tag), (0, 7), "SDK-216.1");
    assert_eq!(
        (o.venue, o.account, o.group, o.inst),
        (V1, A1, G1, I1),
        "SDK-216.1"
    );
    assert_eq!((o.px, o.qty), (Px(110), Qty(9_090_000)), "SDK-216.1");
    let e = AssetExp {
        px_exp: -3,
        qty_exp: -6,
    };
    let one = Notional(1_000_000);
    assert_eq!(
        OrderIntent::spend_qty(one, Px(110), e, 10_000),
        Qty(9_090_000),
        "SDK-216.2"
    );
    assert_eq!(
        OrderIntent::spend_qty(one, Px(110), e, 1),
        Qty(9_090_909),
        "SDK-216.3"
    );
    assert_eq!(OrderIntent::spend_qty(one, Px(0), e, 1), Qty(0), "SDK-216.4");
    assert_eq!(
        OrderIntent::spend_qty(Notional(0), Px(110), e, 1),
        Qty(0),
        "SDK-216.4"
    );
    let bad = AssetExp {
        px_exp: 40,
        qty_exp: -6,
    };
    assert_eq!(OrderIntent::spend_qty(one, Px(110), bad, 1), Qty(0), "SDK-216.4");
}
