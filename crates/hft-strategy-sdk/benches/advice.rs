//! `AdvicePayload::decode` of `BandAdvice` (SDK-199).

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

fn sdk_199_payload_decode(c: &mut Criterion) {
    let mut buf = [0u8; 16];
    let mut w = PayloadWriter::new(&mut buf, 16);
    BandAdvice {
        flags: 0,
        band: 6_500_000,
    }
    .encode(&mut w)
    .unwrap();
    let bytes = w.as_bytes().to_vec();
    c.bench_function("sdk/payload_decode", |b| {
        b.iter(|| BandAdvice::decode(black_box(&bytes)).is_ok())
    });
}

criterion_group!(benches, sdk_199_payload_decode);
criterion_main!(benches);
