//! SDK-140 (crates/hft-strategy-sdk/TESTS.md, engine-side contract): every `Views` accessor is
//! allocation-free inside `on_event`. A separate test target because the counting global
//! allocator is process-wide.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

#[path = "common/mod.rs"]
mod common;

use std::cell::Cell;
use std::rc::Rc;

use hft_strategy_sdk::{EventRef, OrderRef};
use hft_testkit::alloc::CountingAlloc;
use hft_types::{GroupId, InstId};

use crate::common::*;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn seq_315_live_driver_buffers() {
    use hft_event::{FeedCmd, RecordFlags};
    use hft_sequencer::Driver;
    use hft_sequencer::driver::live::RingId;
    use hft_sequencer::types::OutRecord;
    use zerocopy::FromZeros;

    reset_script();
    let mut boot = b0();
    boot.rd.insts.iter_mut().find(|i| i.id == A1).unwrap().l2_depth = 20;
    let mut h = boot.boot();
    h.bring_up(false);
    SCRIPT.with(|s| s.borrow_mut().quiet = true);
    h.io().handed.reserve(20_000);
    h.io().feed.reserve(20_000);
    h.io().j.segments.last_mut().unwrap().records.reserve(20_000);
    h.s.driver_mut().taken_log.reserve(20_000);

    // Keep a subscribed funding row and a live L2 book while exercising the scheduler.
    let now = h.now();
    h.s.engine_mut().market.row_mut(B1).unwrap().next_funding_ns = UTC0 + 100 * 1_000_000_000;
    {
        let book = h.s.engine_mut().books.l2_mut(A1).expect("allocated L2 book");
        let mut hdr = hft_event::L2SnapHdr::new_zeroed();
        hdr.h.inst = A1;
        hdr.h.venue = A;
        hdr.snap_id = 1;
        hdr.origin = hft_event::L2SnapHdr::EXCHANGE;
        hdr.n_bids = 1;
        hdr.n_asks = 1;
        book.apply_snapshot(
            &hdr,
            &[
                hft_event::Level {
                    px: px("50000"),
                    qty: q("1"),
                },
                hft_event::Level {
                    px: px("50002"),
                    qty: q("1"),
                },
            ],
        );
        assert_eq!(book.state(), hft_book::BookState::Live);
    }
    h.run();
    for batch in 0..4u64 {
        for k in 0..70u64 {
            h.push(
                RingId::Bbo(0),
                &bbo(A1, A, 0, "50000", "50002", 10_000 + batch * 70 + k, now),
            );
        }
        let before = h.mark();
        let scope = CountingAlloc::scope();
        h.run();
        let allocations = scope.allocs();
        let sink_bodies = (h.mark() - before) as u64;
        eprintln!(
            "driver allocation profile: batch={batch} engine_allocs={} records={sink_bodies}",
            allocations - sink_bodies
        );
        assert_eq!(allocations, sink_bodies, "engine allocated in BBO batch {batch}");
        // The final record must retain the real final update, independent of buffer reuse.
        assert_eq!(h.s.engine().books.top(A1).update_id, 10_069 + batch * 70);
        assert!(h.j().records().any(|r| r.flags.contains(RecordFlags::CONFLATED)));
    }

    h.io().feed_free = Some(0);
    let cmd = FeedCmd::Subscribe { inst: A1, streams: 1 };
    h.s.driver_mut().emit(&OutRecord::FeedCtl(A, &cmd));
    h.io().feed_free = Some(1);
    let before = h.mark();
    let scope = CountingAlloc::scope();
    h.run();
    let allocations = scope.allocs();
    let sink_bodies = (h.mark() - before) as u64;
    eprintln!(
        "driver allocation profile: staged_feed engine_allocs={}",
        allocations - sink_bodies
    );
    assert_eq!(
        allocations, sink_bodies,
        "engine allocated while retrying FeedCmd"
    );
    assert_eq!(h.io_ref().feed.last(), Some(&(A, cmd)));

    let ring = hft_sequencer::OutRing::Venue(A);
    h.io().out_free.insert(ring, 0);
    let mut cmd = hft_event::GwCmd::set_epoch(1, false, 0);
    cmd.op = hft_event::GwOp::Query;
    cmd.account = AC1;
    let before = h.mark();
    let scope = CountingAlloc::scope();
    h.s.driver_mut().emit(&OutRecord::Out(ring, &cmd));
    h.run();
    let allocations = scope.allocs();
    assert_eq!(
        allocations,
        (h.mark() - before) as u64,
        "synthetic SendReport allocated"
    );
    let report = h
        .since(before)
        .into_iter()
        .find(|r| r.kind == hft_event::EventKind::SEND_REPORT)
        .unwrap();
    assert_eq!(
        report.read::<hft_event::SendReport>().unwrap().status,
        hft_event::SendStatus::RingFull
    );
}

#[test]
fn seq_315_order_reports_reuse_hot_buffers() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(pair_first(hx, q("0.01")));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    let d = decisions(&h.since(m))[0].1;
    SCRIPT.with(|s| s.borrow_mut().quiet = true);
    // The rig records diagnostics in growable collections. Reserve those outside the
    // measured path. MemJournal still makes exactly one body Vec per appended record;
    // count that explicit test-sink cost separately from sequencer allocations.
    h.io().handed.reserve(4096);
    h.io().out.reserve(4096);
    h.io().push_committed.reserve(4096);
    h.io().j.segments.last_mut().unwrap().records.reserve(4096);
    for (what, cum) in [
        (hft_event::OrderEventKind::Ack, "0"),
        (hft_event::OrderEventKind::PartialFill, "0.004"),
        (hft_event::OrderEventKind::PartialFill, "0.004"),
        (hft_event::OrderEventKind::Fill, "0.01"),
        (hft_event::OrderEventKind::Reject, "0.01"),
    ] {
        let mut ev = order_event(&d, what, h.epoch());
        ev.cum_filled = q(cum);
        ev.avg_px = d.px;
        h.push(hft_sequencer::driver::live::RingId::Rep(0), &ev);
        let before = h.mark();
        let scope = CountingAlloc::scope();
        h.run();
        let allocations = scope.allocs();
        let sink_bodies = (h.mark() - before) as u64;
        assert_eq!(allocations, sink_bodies, "engine allocated on {what:?}");
    }
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.01"));
    SCRIPT.with(|s| s.borrow_mut().quiet = false);
}

/// `SeqRig` (crates/hft-sequencer/tests/sdk_contract/mod.rs `rig`): B0 with s1 on the three
/// `GT3` groups (inst 23 added), steady.
fn rig() -> H {
    reset_script();
    let mut b = b0();
    let gt2 = GroupId(0x0100_0002);
    let gt3 = GroupId(0x0100_0003);
    b.rd.insts.push(inst_spec(
        InstId(23),
        B,
        "F23_USDT",
        true,
        FILL,
        ("0.01", 1),
        ("0.001", 100_000),
    ));
    b.caps.max_timers = 512;
    b.params = vec![
        Spb::new(
            S1,
            AC1,
            TAKER,
            1,
            vec![
                gspec(G1),
                gspec_eth(gt2),
                group(gt3, FILL, &[leg(InstId(14), A), leg(InstId(23), B)]),
            ],
        ),
        Spb::s2(1),
        Spb::s9(1),
    ];
    let mut h = b.boot();
    h.steady();
    clear_log();
    h
}

#[test]
fn sdk_140_views_zero_alloc() {
    assert!(CountingAlloc::is_installed());
    let mut h = rig();
    // A resting order of s1 so that `order` reads a live slot.
    let r: Rc<Cell<Option<OrderRef>>> = Rc::new(Cell::new(None));
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50005")
        {
            let mut it = intent(
                hx,
                A1,
                hft_types::Side::Buy,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                px("49900"),
                q("0.001"),
            );
            it.group = G1;
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50005");
    h.run();
    clear_hook();
    let d = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.strategy == S1)
        .expect("gtc")
        .1;
    h.ack(&d);
    r.set(Some(OrderRef {
        venue: d.venue,
        account: d.account,
        _pad: [0; 2],
        slot: d.slot.0,
        r#gen: d.r#gen,
    }));
    // Every accessor once per a1 Bbo; the count covers the accessor calls only (the test set's
    // own delivery log allocates and is outside the assertion).
    let allocs = Rc::new(Cell::new(0u64));
    let calls = Rc::new(Cell::new(0u32));
    let sink = Rc::new(Cell::new(0u64));
    let (a2, c2, s2, r2) = (allocs.clone(), calls.clone(), sink.clone(), r.clone());
    let or = r2.get().expect("order ref");
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let sc = CountingAlloc::scope();
            let x = hx.v.sweep_all(AC1, G1, B1, B, or) ^ hx.v.sweep_all(AC1, G1, A1, A, or);
            let n = sc.allocs();
            a2.set(a2.get() + n);
            c2.set(c2.get() + 1);
            s2.set(s2.get() ^ x);
        }
    });
    for k in 0..1_000u32 {
        let ask = if k % 2 == 0 { "50001" } else { "50002" };
        h.quote(A1, "50000", ask);
        h.run();
    }
    clear_hook();
    assert_eq!(calls.get(), 1_000);
    assert_eq!(allocs.get(), 0, "allocations inside the Views accessors");
    std::hint::black_box(sink.get());
}
