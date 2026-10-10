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
