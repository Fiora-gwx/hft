//! TK-092: DST per-step overhead (`testkit/dst_step`): one W1 run of 2 000 steps on the toy
//! engine with every invariant on; Criterion reports the time per run, divide by 2 000.

use criterion::{Criterion, criterion_group, criterion_main};
use hft_testkit::dst::{DstConfig, RerunPolicy, ToyEngine, run};

fn tk_092_dst_step(c: &mut Criterion) {
    let mut cfg = DstConfig::new(1, "core");
    cfg.steps = 2_000;
    cfg.rerun = RerunPolicy::Never;
    c.bench_function("testkit/dst_step", |b| b.iter(|| run::<ToyEngine>(&cfg)));
}

criterion_group!(benches, tk_092_dst_step);
criterion_main!(benches);
