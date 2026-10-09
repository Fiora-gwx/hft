//! `Ctx::place` and `Ctx::target` (SDK-196, SDK-197).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;

use common::*;
use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::*;
use hft_types::{Px, Qty, Side};

fn sdk_196_ctx_place(c: &mut Criterion) {
    let mut b = CtxBufs::new();
    let it = order(s3(0xFF01), G1, I1, Side::Buy, IntentClass::Open, Px(6_500_000), Qty(1_000), 0);
    c.bench_function("sdk/ctx_place", |bn| {
        bn.iter(|| {
            b.begin();
            let mut ctx = b.ctx();
            black_box(ctx.place(black_box(it)).is_ok())
        })
    });
}

fn sdk_197_ctx_target(c: &mut Criterion) {
    let mut b = CtxBufs::new();
    let t = pair_target(A1, G1, 1_000, 0);
    c.bench_function("sdk/ctx_target", |bn| {
        bn.iter(|| {
            b.begin();
            let mut ctx = b.ctx();
            black_box(ctx.target(black_box(t)).is_ok())
        })
    });
}

criterion_group!(benches, sdk_196_ctx_place, sdk_197_ctx_target);
criterion_main!(benches);
