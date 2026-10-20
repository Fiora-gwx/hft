//! Real strategy entry points; fixture and output inspection are outside the timed region. The
//! observer never sends an intent: `tick_fire` is its full recording path (a book event reads the
//! top of book), `tick_miss` a trade (one counter).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: bench code

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use book_observer::{BookObserver, BookObserverParams};
use common::{A1, G1, I1, I2, S1, fixture, g1, quote};
use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::params::{RawParams, SchemaView};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::{MockViews, ev};

fn setup() -> (BookObserver, MockViews) {
    let schema = BookObserver::DESC.schema;
    let defaults: Vec<i64> = (0..schema.n_fields())
        .filter_map(|i| schema.field(i).map(|f| f.default))
        .collect();
    let p = BookObserverParams::from_raw(
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
        1,
    );
    let mut views = MockViews::new(fixture());
    views.at_engine_ns(1_000_000_000);
    quote(&mut views, I1, 6_000_000);
    quote(&mut views, I2, 6_000_100);
    let strategy = BookObserver::new(
        StrategyInstance {
            strategy: S1,
            account: A1,
            kind_id: 6,
        },
        &p,
        &g1(),
        &views.refview(),
    )
    .unwrap();
    (strategy, views)
}

fn measure(c: &mut Criterion, name: &str, event: ev::Ev) {
    // Counters only grow; every sample takes the same path without a reset.
    let (mut strategy, views) = setup();
    let mut bufs = CtxBufs::new();
    c.bench_function(name, |b| {
        b.iter_custom(|n| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..n {
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
    let t = TradeView {
        px: Px(6_000_100),
        qty: Qty(1_000),
        side: Side::Buy,
        ex_ts_ns: 1,
    };
    measure(c, "strategies/book-observer/tick_miss", ev::trade(I2, 0, t));
    measure(c, "strategies/book-observer/tick_fire", ev::book(I1, 0));
    let r = OrderRef {
        venue: common::V1,
        account: A1,
        _pad: [0; 2],
        slot: 1,
        r#gen: 1,
    };
    measure(
        c,
        "strategies/book-observer/exec_update",
        ev::order(
            0,
            OrderUpdate {
                r,
                group: G1,
                tag: 1,
                kind: OrderUpdateKind::Cancelled,
            },
        ),
    );
    measure(
        c,
        "strategies/book-observer/risk_event",
        ev::risk(A1, RiskEvent::HedgeBroken { group: G1 }),
    );
}

criterion_group!(benches, on_event);
criterion_main!(benches);
