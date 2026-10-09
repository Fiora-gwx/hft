//! Zero allocation on the SDK hot entries and in the test strategies (SDK-013, SDK-014).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::alloc::Counting;
use common::*;
use hft_strategy_sdk::params::{body_view, encode_body};
use hft_strategy_sdk::testing::ev::Ev;
use hft_strategy_sdk::*;
use hft_types::{GroupId, Px, Qty, Side, Tsc};

#[global_allocator]
static A: Counting = Counting;

/// Runs `f` `n` times, starting a new event (`begin`) every `per` calls.
fn each(b: &mut CtxBufs, n: usize, per: usize, mut f: impl FnMut(&mut Ctx<'_>, usize)) {
    let mut i = 0;
    while i < n {
        b.begin();
        let mut ctx = b.ctx();
        for _ in 0..per.min(n - i) {
            f(&mut ctx, i);
            i += 1;
        }
    }
}

#[test]
fn sdk_013_hot_entries_zero_alloc() {
    let mut b = CtxBufs::new();
    let it = order(
        s3(0xFF01),
        G1,
        I1,
        Side::Buy,
        IntentClass::Open,
        Px(6_500_000),
        Qty(1_000),
        0,
    );
    let t = pair_target(A1, G1, 1_000, 0);
    let r = OrderRef::new(V1, A1, 1, 1);
    let cmd = [1u8, 1, 1, 0, 0, 0, 0, 0, 0, 0xCA, 0x9A, 0x3B, 0, 0, 0, 0];
    let body = aligned(
        &encode_body(
            &TP,
            3,
            &raw(tp_defaults(), &[(0, 0, 6_000_000), (2, 3, 50_000_000)]),
        )
        .unwrap(),
    );
    let body = bytes_of(&body, 88);
    let g = gt3();
    let bbo = quote(6_500_000);
    let clk = EngineClockAnchor {
        base_engine_ns: 0,
        base_tsc: Tsc(0),
        tsc_hz_milli: 3_000_000_000_000,
        utc_tsc: Tsc(0),
        utc_ns: UTC0,
    };
    let now = Now::new(Tsc(9_000_000_000), 1, 1, true, clk);
    let mut sink = 0u64;
    const N: usize = 10_000;

    alloc::start();
    each(&mut b, N, 16, |c, _| sink += u64::from(c.place(it).is_ok()));
    each(&mut b, N, 16, |c, _| sink += u64::from(c.target(t).is_ok()));
    each(&mut b, N, 16, |c, _| {
        sink += u64::from(c.clear_target(A1, G1).is_ok())
    });
    each(&mut b, N, 16, |c, _| sink += u64::from(c.cancel(r).is_ok()));
    each(&mut b, N, 16, |c, _| {
        sink += u64::from(c.amend(r, Px(1), Qty(1)).is_ok())
    });
    each(&mut b, N, 8, |c, i| {
        sink += u64::from(c.timer_at(Tsc(i as u64), 1, G1).is_ok())
    });
    each(&mut b, N, 8, |c, i| c.cancel_timer(TimerId(i as u64)));
    each(&mut b, N, 16, |c, i| {
        c.note(ECHO_FIRE, NoteLevel::Info, G1, I1, [i as u64; 8])
    });
    each(&mut b, N, 4, |c, i| {
        sink += u64::from(c.request_halt(G1, i as u16).is_ok())
    });
    each(&mut b, N, 16, |c, i| {
        sink += u64::from(c.outcome(IntentHandle(i as u16 % 16)).is_some())
    });
    for i in 0..N {
        let v = CmdView::parse(&cmd).unwrap();
        sink = sink.wrapping_add(v.val(0).unwrap_or(0) as u64 + u64::from(v.op()));
        sink += u64::from(bbo.quote_live(Tsc(i as u64), 6_000_000_000));
        let t = (now.ens_of(Tsc(i as u64)) as u64)
            .wrapping_add(now.tsc_of(i as i64).0)
            .wrapping_add(now.utc_of(Tsc(i as u64)) as u64);
        sink = sink.wrapping_add(t);
        sink += u64::from(g.gix_of(GroupId(0x0300_0001 + (i % 4) as u32)).unwrap_or(0));
        sink += u64::from(g.get((i % 3) as u16).id.0) + u64::from(g.len());
        sink += u64::from(body_view(body, 3).is_ok());
    }
    let (allocs, frees) = alloc::stop();
    assert_eq!((allocs, frees), (0, 0), "allocations / frees on the hot entries");
    assert_ne!(sink, 0);
}

#[test]
fn sdk_014_probe_on_event_zero_alloc() {
    let mut v = clocked_views();
    {
        let m = &mut v;
        m.bbo(
            I1,
            (Px(6_499_950), Qty(100_000_000)),
            (Px(6_500_050), Qty(100_000_000)),
        );
        m.bbo(
            I2,
            (Px(6_501_950), Qty(100_000_000)),
            (Px(6_502_050), Qty(100_000_000)),
        );
    }
    let t = gt3();
    let mut e = EchoProbe::new(s3(0xFF01), &tp(3, &[]), &t, &v.refview()).unwrap();
    let mut p = TargetProbe::new(s3(0xFF02), &tp2(3, &[]), &t, &v.refview()).unwrap();
    let now = Views::now(&v);
    let evs: Vec<Ev> = (0..N_VARIANTS)
        .map(|k| variant(k, u16::from(k) % 3, 1_000 + i64::from(k), now))
        .collect();
    let mut b = CtxBufs::new();
    let mut w = SnapshotWriter::new(256 * 1024);

    alloc::start();
    for ev in &evs {
        let er = ev.as_ref();
        for _ in 0..1_000 {
            b.begin();
            e.on_event(&er, &v, &mut b.ctx());
            b.begin();
            p.on_event(&er, &v, &mut b.ctx());
        }
    }
    let step1 = alloc::stop();
    assert_eq!(step1.0, 0, "step 1: allocations in on_event");

    alloc::start();
    for _ in 0..100 {
        w.clear();
        e.snapshot(&mut w);
        w.clear();
        p.snapshot(&mut w);
    }
    let step2 = alloc::stop();
    assert_eq!(step2.0, 0, "step 2: allocations in snapshot");
}
