//! Symbol-table lookups (REF-081 – REF-083); ids equal the `基准名` fields in TESTS.md.

#[path = "../tests/common/f0.rs"]
mod f0;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_refdata::RefData;
use hft_types::VenueId;

fn loaded() -> RefData {
    RefData::load(&f0::universe6(5000, 20).build(), None).unwrap_or_else(|r| panic!("{:?}", r.errors))
}

fn ref_081_refdata_symbol_get(c: &mut Criterion) {
    let rd = loaded();
    let t = rd.symbols(VenueId::BINANCE_USDM);
    black_box(t.prewarm());
    c.bench_function("refdata_symbol_get", |b| b.iter(|| t.get(black_box(b"A417USDT"))));
}

fn ref_082_refdata_symbol_get_miss(c: &mut Criterion) {
    let rd = loaded();
    let t = rd.symbols(VenueId::BINANCE_USDM);
    black_box(t.prewarm());
    c.bench_function("refdata_symbol_get_miss", |b| {
        b.iter(|| t.get(black_box(b"ZZZ417USDT")))
    });
}

fn ref_083_refdata_num_get(c: &mut Criterion) {
    let rd = loaded();
    let t = rd.symbols(VenueId::OKX_SWAP);
    black_box(t.prewarm());
    c.bench_function("refdata_num_get", |b| b.iter(|| t.get_num(black_box(2_503))));
}

criterion_group!(
    benches,
    ref_081_refdata_symbol_get,
    ref_082_refdata_symbol_get_miss,
    ref_083_refdata_num_get
);
criterion_main!(benches);
