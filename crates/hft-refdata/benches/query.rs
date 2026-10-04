//! Array-indexed queries (REF-084, REF-085); ids equal the `基准名` fields in TESTS.md.

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
use hft_types::{InstId, VenueId};

fn loaded() -> RefData {
    RefData::load(&f0::universe6(5000, 20).build(), None).unwrap_or_else(|r| panic!("{:?}", r.errors))
}

fn ref_084_refdata_inst(c: &mut Criterion) {
    let rd = loaded();
    c.bench_function("refdata_inst", |b| {
        b.iter(|| rd.inst(black_box(InstId(2_417))).tick)
    });
}

fn ref_085_refdata_caps(c: &mut Criterion) {
    let rd = loaded();
    c.bench_function("refdata_caps", |b| {
        b.iter(|| rd.caps(black_box(VenueId::OKX_SWAP)).flags)
    });
}

criterion_group!(benches, ref_084_refdata_inst, ref_085_refdata_caps);
criterion_main!(benches);
