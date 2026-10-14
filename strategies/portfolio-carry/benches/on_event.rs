//! CARRY-009: measure only the strategy call with fixed buffers and a hedged position.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    dead_code,
    unused_imports
)]
#[path = "../tests/signals.rs"]
mod scenario;
use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::MockViews;
use scenario::common::*;
use std::time::{Duration, Instant};

fn carry_009_hold(c: &mut Criterion) {
    let (mut s, mut v, _) = scenario::setup();
    let mut pos = GroupPosView::EMPTY;
    pos.n_legs = 2;
    pos.legs[0].qty = Qty(-10_000_000);
    pos.legs[1].qty = Qty(10_000_000);
    MockViews::group_pos(&mut v, A1, G4, pos);
    let mut bufs = CtxBufs::new();
    let e = EventRef::Bbo {
        inst: I2,
        gix: 0,
        b: v.bbo_of(I2),
    };
    c.bench_function("strategies/portfolio-carry/hold", |b| {
        b.iter_custom(|n| {
            let mut d = Duration::ZERO;
            for _ in 0..n {
                bufs.begin();
                let mut ctx = bufs.ctx();
                let start = Instant::now();
                s.on_event(&e, &v, &mut ctx);
                d += start.elapsed();
                std::hint::black_box(&ctx);
            }
            d
        })
    });
}
criterion_group!(benches, carry_009_hold);
criterion_main!(benches);
