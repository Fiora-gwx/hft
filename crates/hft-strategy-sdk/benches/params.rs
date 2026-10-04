//! `params::body_view` with 4 096 overrides (SDK-204).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::params::{ParamOverride, ParamsBodyHdr, body_view};
use zerocopy::IntoBytes;

/// 8-byte aligned body: 64 defaults and 64 groups × 64 fields of overrides.
fn body_4096() -> Vec<u64> {
    let hdr = ParamsBodyHdr {
        n_fields: 64,
        _pad: 0,
        n_overrides: 4096,
    };
    let mut b = hdr.as_bytes().to_vec();
    b.extend_from_slice([7i64; 64].as_bytes());
    for g in 0..64u16 {
        for f in 0..64u16 {
            b.extend_from_slice(ParamOverride::new(g, f, i64::from(f)).as_bytes());
        }
    }
    let mut v = vec![0u64; b.len() / 8];
    v.as_mut_bytes().copy_from_slice(&b);
    v
}

fn sdk_204_body_view(c: &mut Criterion) {
    let body = body_4096();
    let bytes = body.as_bytes();
    assert!(body_view(bytes, 64).is_ok());
    c.bench_function("sdk/body_view", |b| {
        b.iter(|| body_view(black_box(bytes), 64).is_ok())
    });
}

criterion_group!(benches, sdk_204_body_view);
criterion_main!(benches);
