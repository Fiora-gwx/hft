//! Real strategy entry points; fixture/reset and output inspection are outside the timed region.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: bench code

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use binary_maker::fast::{QState, Quote};
use binary_maker::{BinaryMaker, BinaryMakerParams};
use common::*;
use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::ctx::Op;
use hft_strategy_sdk::params::{RawParams, SchemaView};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::{MockViews, ev};

/// The parameters of tests/scenario.rs: defaults with `enabled = 1`, `vol_min_samples` = 60.
fn params() -> BinaryMakerParams {
    let schema = BinaryMaker::DESC.schema;
    let mut defaults: Vec<i64> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i).map(|f| f.default))
        .collect();
    let ix = |name: &str| {
        (0..schema.n_fields())
            .position(|i| schema.field(i).is_some_and(|f| f.name == name))
            .unwrap()
    };
    defaults[ix("enabled")] = 1;
    defaults[ix("vol_min_samples")] = 60;
    BinaryMakerParams::from_raw(
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
        7,
    )
}

/// The scenario warm-up (BMK-003): 400 s of 1 bp reference moves ending at second 399, books of
/// pair 1 at 0.450/0.470, clock at 399.5 s (window 300..600 is pair 1). No quote is live.
fn fixture() -> (BinaryMaker, MockViews) {
    let p = params();
    let groups = groups();
    let mut views = MockViews::new(common::fixture());
    let mut strategy = BinaryMaker::new(
        StrategyInstance {
            strategy: S1,
            account: A1,
            kind_id: 7,
        },
        &p,
        &groups,
        &views.refview(),
    )
    .unwrap();
    let mut bufs = CtxBufs::new();
    for k in 0..400i64 {
        views.at_engine_ns(k * 1_000_000_000);
        ref_quote(&mut views, if k % 2 == 0 { 10_000_000 } else { 10_001_000 });
        let e = ev::bbo(R, 6, views.bbo_of(R));
        bufs.begin();
        let mut ctx = bufs.ctx();
        strategy.on_event(&e.as_ref(), &views, &mut ctx);
    }
    for i in [SLOTS[2], SLOTS[3]] {
        slot_book(&mut views, i, 450, 470);
    }
    views.at_engine_ns(399_500_000_000);
    ref_quote(&mut views, 10_000_500);
    (strategy, views)
}

/// Both quotes of pair 1 live at 0.450 (OMS slots 11 and 12).
fn live(strategy: &mut BinaryMaker) {
    for (i, slot) in [(2usize, 11u32), (3, 12)] {
        let q = &mut strategy.fast.quotes[i];
        q.state = QState::Live;
        q.r = Some(order_ref(slot));
        q.px = 450;
    }
}

fn order_ref(slot: u32) -> OrderRef {
    OrderRef {
        venue: VB,
        account: A1,
        _pad: [0; 2],
        slot,
        r#gen: 1,
    }
}

/// Times `event`; `places` is the number of places one sample makes (checked once, untimed).
fn measure(
    c: &mut Criterion,
    name: &str,
    mut strategy: BinaryMaker,
    views: MockViews,
    event: ev::Ev,
    places: usize,
) {
    // The quote table is the decision state; restoring it makes every sample take the same path
    // (the volatility ring gets no new sample within the same second).
    let quotes: Vec<Quote> = strategy.fast.quotes.clone();
    let mut bufs = CtxBufs::new();
    bufs.begin();
    strategy.on_event(&event.as_ref(), &views, &mut bufs.ctx());
    let made = bufs.ops().filter(|(_, op)| matches!(op, Op::Place(_))).count();
    assert_eq!(made, places, "{name}");
    c.bench_function(name, |b| {
        b.iter_custom(|n| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..n {
                strategy.fast.quotes.copy_from_slice(&quotes);
                bufs.begin();
                let mut ctx = bufs.ctx();
                let event = event.as_ref();
                let start = Instant::now();
                strategy.on_event(black_box(&event), black_box(&views), &mut ctx);
                elapsed += start.elapsed();
                black_box(ctx);
            }
            elapsed
        });
    });
}

fn on_event(c: &mut Criterion) {
    // Reference quote with both quotes already resting at the best bid: nothing to do.
    let (mut strategy, views) = fixture();
    live(&mut strategy);
    let event = ev::bbo(R, 6, views.bbo_of(R));
    measure(c, "strategies/binary-maker/tick_miss", strategy, views, event, 0);

    // The same reference quote with no live quote: two post-only bids.
    let (strategy, views) = fixture();
    let event = ev::bbo(R, 6, views.bbo_of(R));
    measure(c, "strategies/binary-maker/tick_fire", strategy, views, event, 2);

    // The Up quote is cancelled by the venue: it is placed again.
    let (mut strategy, views) = fixture();
    live(&mut strategy);
    let event = ev::order(
        2,
        OrderUpdate {
            r: order_ref(11),
            group: gid(2),
            tag: binary_maker::fast::tag_of(2),
            kind: OrderUpdateKind::Cancelled,
        },
    );
    measure(
        c,
        "strategies/binary-maker/exec_update",
        strategy,
        views,
        event,
        1,
    );

    // A risk event: only the pending place outcomes are read.
    let (mut strategy, views) = fixture();
    live(&mut strategy);
    measure(
        c,
        "strategies/binary-maker/risk_event",
        strategy,
        views,
        ev::risk(A1, RiskEvent::HedgeBroken { group: gid(2) }),
        0,
    );
}

criterion_group!(benches, on_event);
criterion_main!(benches);
