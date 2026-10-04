//! Load of 5 000 insts over the six core venues with 20 accounts (REF-087).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "../tests/common/f0.rs"]
mod f0;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_refdata::RefData;

fn ref_087_refdata_load_5000(c: &mut Criterion) {
    let blob = f0::universe6(5000, 20).build();
    c.bench_function("refdata_load_5000", |b| {
        b.iter(|| RefData::load(black_box(&blob), None).is_ok())
    });
}

criterion_group!(benches, ref_087_refdata_load_5000);
criterion_main!(benches);
