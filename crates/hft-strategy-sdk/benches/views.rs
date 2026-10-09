//! `BboView::quote_live` and `Now` time conversions (SDK-201, SDK-202).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::{BboView, EngineClockAnchor, Now};
use hft_types::Tsc;

fn sdk_201_quote_live(c: &mut Criterion) {
    let b = BboView {
        conn_up: true,
        conn_live_tsc: Tsc(99_000_000_000),
        rx_tsc: Tsc(99_000_000_000),
        ..BboView::EMPTY
    };
    c.bench_function("sdk/quote_live", |bn| {
        bn.iter(|| black_box(b).quote_live(black_box(Tsc(100_000_000_000)), black_box(6_000_000_000)))
    });
}

fn sdk_202_time_conv(c: &mut Criterion) {
    let clk = EngineClockAnchor {
        base_engine_ns: 47_000_000_000,
        base_tsc: Tsc(0),
        tsc_hz_milli: 3_000_000_000_000,
        utc_tsc: Tsc(9_000_000_000),
        utc_ns: 1_759_276_800_000_000_000,
    };
    let now = Now::new(Tsc(9_000_000_000), 1, 1, true, clk);
    c.bench_function("sdk/time_conv", |b| {
        b.iter(|| {
            let t = black_box(Tsc(12_345_678_901));
            let e = now.ens_of(t);
            (e, now.tsc_of(black_box(e + 1_000)), now.utc_of(t))
        })
    });
}

criterion_group!(benches, sdk_201_quote_live, sdk_202_time_conv);
criterion_main!(benches);
