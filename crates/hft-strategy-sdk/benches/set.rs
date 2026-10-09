//! `StrategySet` dispatch overhead (SDK-198): one Bbo through `ProbeSet::on_event` and the same
//! call made directly, for comparison.

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
use hft_strategy_sdk::testing::ev;
use hft_strategy_sdk::*;

hft_strategy_sdk::strategy_set! {
    pub struct ProbeSet {
        EchoProbe,
        TargetProbe,
    }
}

fn sdk_198_set_dispatch(c: &mut Criterion) {
    let v = clocked_views();
    let t = gt3();
    // A wide quote: the probe evaluates its condition but does not fire (no cooldown state).
    let mut b = quote(6_500_000);
    b.ask_px.0 += 1_000_000;
    let e = ev::bbo(I1, 0, b);
    let er = e.as_ref();
    let p = tp(3, &[]);
    let mut direct = EchoProbe::new(s3(0xFF01), &p, &t, &v.refview()).unwrap();
    let mut via = ProbeSet::create(s3(0xFF01), &__strategy_set::Params::EchoProbe(p.clone()), &t, &v.refview()).unwrap();
    let mut bufs = CtxBufs::new();
    let mut g = c.benchmark_group("sdk/set_dispatch");
    g.bench_function("set", |bn| {
        bn.iter(|| {
            bufs.begin();
            ProbeSet::on_event(&mut via, black_box(&er), &v, &mut bufs.ctx());
        })
    });
    g.bench_function("direct", |bn| {
        bn.iter(|| {
            bufs.begin();
            direct.on_event(black_box(&er), &v, &mut bufs.ctx());
        })
    });
    g.finish();
}

criterion_group!(benches, sdk_198_set_dispatch);
criterion_main!(benches);
