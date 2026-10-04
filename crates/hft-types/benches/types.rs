//! Criterion benchmarks for the hft-types hot entries (TYP-170 – TYP-179); ids equal the
//! `基准名` fields in TESTS.md and the budget names in README "延迟与资源契约".

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_types::*;

fn typ_170_parse_px(c: &mut Criterion) {
    c.bench_function("types_parse_px", |b| {
        b.iter(|| parse_exact(black_box(b"104523.67"), black_box(-4)))
    });
}

fn typ_171_parse_qty(c: &mut Criterion) {
    c.bench_function("types_parse_qty", |b| {
        b.iter(|| parse_exact(black_box(b"0.01234000"), black_box(-8)))
    });
}

fn typ_172_parse_sci(c: &mut Criterion) {
    c.bench_function("types_parse_sci", |b| {
        b.iter(|| parse_exact(black_box(b"1.0E-4"), black_box(-9)))
    });
}

fn typ_173_write_fixed(c: &mut Criterion) {
    c.bench_function("types_write_fixed", |b| {
        let mut out = [0u8; 40];
        b.iter(|| write_fixed(black_box(1_045_236_700), black_box(-4), &mut out))
    });
}

fn typ_174_mul_qty(c: &mut Criterion) {
    let e = AssetExp {
        px_exp: -4,
        qty_exp: -8,
    };
    c.bench_function("types_mul_qty", |b| {
        b.iter(|| black_box(Px(1_045_236_700)).mul_qty(black_box(Qty(1_234_000)), e, RoundMode::HalfEven))
    });
}

fn typ_175_round(c: &mut Criterion) {
    c.bench_function("types_round", |b| {
        b.iter(|| {
            let p = black_box(Px(1_045_236_789)).round_to_tick(black_box(100), RoundMode::Floor);
            let q = black_box(Qty(1_234_567)).round_to_lot(black_box(1_000), RoundMode::TowardZero);
            (p, q)
        })
    });
}

fn typ_176_u256(c: &mut Criterion) {
    let v: U256Limbs = [0x0DE0_B6B3_A764_0001, 0, 0, 0]; // 10^18 + 1
    c.bench_function("types_u256", |b| {
        b.iter(|| qty_from_u256(black_box(&v), black_box(18), black_box(-8), RoundMode::Floor))
    });
}

fn typ_177_clid_encode(c: &mut Criterion) {
    let x = ClientOrderId {
        epoch: 0x1234,
        venue: VenueId(10),
        account: AccountId(3),
        slot: 0xA_BCDE,
        r#gen: 7,
    };
    c.bench_function("types/clid_encode", |b| {
        let mut out = [0u8; 24];
        b.iter(|| black_box(&x).encode(&mut out))
    });
}

fn typ_178_clid_decode(c: &mut Criterion) {
    c.bench_function("types/clid_decode", |b| {
        b.iter(|| ClientOrderId::decode(black_box(b"112340a03abcdedeadbeefa8")))
    });
}

fn typ_179_cap_cross_px(c: &mut Criterion) {
    let lim = HedgePxLimits {
        conn_liveness_ms: 1000,
        rr_max_cross: Rate(5_000_000),
    };
    c.bench_function("types_cap_cross_px", |b| {
        b.iter(|| {
            cap_cross_px(
                black_box(Side::Buy),
                black_box(Px(1_000_000)),
                black_box(Px(999_000)),
                black_box(Rate(10_000_000)),
                &lim,
                black_box(100),
            )
        })
    });
}

criterion_group!(
    benches,
    typ_170_parse_px,
    typ_171_parse_qty,
    typ_172_parse_sci,
    typ_173_write_fixed,
    typ_174_mul_qty,
    typ_175_round,
    typ_176_u256,
    typ_177_clid_encode,
    typ_178_clid_decode,
    typ_179_cap_cross_px
);
criterion_main!(benches);
