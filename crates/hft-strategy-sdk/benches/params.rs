//! `params::body_view` with 4 096 overrides (SDK-204), `ParamSet::decode` of 64 groups × 64
//! fields (SDK-203) and `decode` + `validate` by group count (SDK-205).

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


#[path = "../tests/common/mod.rs"]
mod common;

use criterion::BenchmarkId;
use hft_strategy_sdk::params::{RawParams, encode_body};
use hft_strategy_sdk::{GroupTable, ParamSet};
use hft_types::GroupId;

hft_strategy_sdk::define_params! {
    pub struct P64(schema_id = 0xFF7F, version = 1, fields = P64Fields) {
        f00: Count [min = 0, max = 1_000, default = 7, per_group],
        f01: Count [min = 0, max = 1_000, default = 7, per_group],
        f02: Count [min = 0, max = 1_000, default = 7, per_group],
        f03: Count [min = 0, max = 1_000, default = 7, per_group],
        f04: Count [min = 0, max = 1_000, default = 7, per_group],
        f05: Count [min = 0, max = 1_000, default = 7, per_group],
        f06: Count [min = 0, max = 1_000, default = 7, per_group],
        f07: Count [min = 0, max = 1_000, default = 7, per_group],
        f08: Count [min = 0, max = 1_000, default = 7, per_group],
        f09: Count [min = 0, max = 1_000, default = 7, per_group],
        f10: Count [min = 0, max = 1_000, default = 7, per_group],
        f11: Count [min = 0, max = 1_000, default = 7, per_group],
        f12: Count [min = 0, max = 1_000, default = 7, per_group],
        f13: Count [min = 0, max = 1_000, default = 7, per_group],
        f14: Count [min = 0, max = 1_000, default = 7, per_group],
        f15: Count [min = 0, max = 1_000, default = 7, per_group],
        f16: Count [min = 0, max = 1_000, default = 7, per_group],
        f17: Count [min = 0, max = 1_000, default = 7, per_group],
        f18: Count [min = 0, max = 1_000, default = 7, per_group],
        f19: Count [min = 0, max = 1_000, default = 7, per_group],
        f20: Count [min = 0, max = 1_000, default = 7, per_group],
        f21: Count [min = 0, max = 1_000, default = 7, per_group],
        f22: Count [min = 0, max = 1_000, default = 7, per_group],
        f23: Count [min = 0, max = 1_000, default = 7, per_group],
        f24: Count [min = 0, max = 1_000, default = 7, per_group],
        f25: Count [min = 0, max = 1_000, default = 7, per_group],
        f26: Count [min = 0, max = 1_000, default = 7, per_group],
        f27: Count [min = 0, max = 1_000, default = 7, per_group],
        f28: Count [min = 0, max = 1_000, default = 7, per_group],
        f29: Count [min = 0, max = 1_000, default = 7, per_group],
        f30: Count [min = 0, max = 1_000, default = 7, per_group],
        f31: Count [min = 0, max = 1_000, default = 7, per_group],
        f32: Count [min = 0, max = 1_000, default = 7, per_group],
        f33: Count [min = 0, max = 1_000, default = 7, per_group],
        f34: Count [min = 0, max = 1_000, default = 7, per_group],
        f35: Count [min = 0, max = 1_000, default = 7, per_group],
        f36: Count [min = 0, max = 1_000, default = 7, per_group],
        f37: Count [min = 0, max = 1_000, default = 7, per_group],
        f38: Count [min = 0, max = 1_000, default = 7, per_group],
        f39: Count [min = 0, max = 1_000, default = 7, per_group],
        f40: Count [min = 0, max = 1_000, default = 7, per_group],
        f41: Count [min = 0, max = 1_000, default = 7, per_group],
        f42: Count [min = 0, max = 1_000, default = 7, per_group],
        f43: Count [min = 0, max = 1_000, default = 7, per_group],
        f44: Count [min = 0, max = 1_000, default = 7, per_group],
        f45: Count [min = 0, max = 1_000, default = 7, per_group],
        f46: Count [min = 0, max = 1_000, default = 7, per_group],
        f47: Count [min = 0, max = 1_000, default = 7, per_group],
        f48: Count [min = 0, max = 1_000, default = 7, per_group],
        f49: Count [min = 0, max = 1_000, default = 7, per_group],
        f50: Count [min = 0, max = 1_000, default = 7, per_group],
        f51: Count [min = 0, max = 1_000, default = 7, per_group],
        f52: Count [min = 0, max = 1_000, default = 7, per_group],
        f53: Count [min = 0, max = 1_000, default = 7, per_group],
        f54: Count [min = 0, max = 1_000, default = 7, per_group],
        f55: Count [min = 0, max = 1_000, default = 7, per_group],
        f56: Count [min = 0, max = 1_000, default = 7, per_group],
        f57: Count [min = 0, max = 1_000, default = 7, per_group],
        f58: Count [min = 0, max = 1_000, default = 7, per_group],
        f59: Count [min = 0, max = 1_000, default = 7, per_group],
        f60: Count [min = 0, max = 1_000, default = 7, per_group],
        f61: Count [min = 0, max = 1_000, default = 7, per_group],
        f62: Count [min = 0, max = 1_000, default = 7, per_group],
        f63: Count [min = 0, max = 1_000, default = 7, per_group],
    }
}

fn table(n: u32, cap: u16) -> GroupTable {
    let specs: Vec<_> = (1..=n).map(|k| common::gspec(GroupId(0x0300_0000 + k))).collect();
    GroupTable::new(&specs, cap)
}

fn sdk_203_params_decode(c: &mut Criterion) {
    let raw = RawParams {
        defaults: vec![7; 64],
        overrides: (0..64u16)
            .flat_map(|g| (0..64u16).map(move |f| ParamOverride::new(g, f, i64::from(f))))
            .collect(),
    };
    let body = common::aligned(&encode_body(&P64::SCHEMA, 64, &raw).unwrap());
    let bytes = body.as_bytes();
    let t = table(64, 64);
    assert!(P64::decode(bytes, 1, &t).is_ok());
    c.bench_function("sdk/params_decode", |b| b.iter(|| P64::decode(black_box(bytes), 1, &t).is_ok()));
}

fn sdk_205_params_validate(c: &mut Criterion) {
    let rd = common::gt3_fixture();
    let mut g = c.benchmark_group("params/validate");
    for n in [1u32, 8, 64] {
        let t = table(n, 64);
        let ov: Vec<(u16, u16, i64)> = (0..n as u16).map(|gix| (gix, 0, 6_000_000)).collect();
        let body = common::aligned(&encode_body(&common::TP, n as u16, &common::raw(common::tp_defaults(), &ov)).unwrap());
        let bytes = body.as_bytes();
        let p = common::TPParams::decode(bytes, 2, &t).unwrap();
        assert_eq!(p.validate(&t, &rd.view()), Ok(()));
        g.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let p = common::TPParams::decode(black_box(bytes), 2, &t).unwrap();
                p.validate(&t, &rd.view()).is_ok()
            })
        });
    }
    g.finish();
}

criterion_group!(benches, sdk_203_params_decode, sdk_204_body_view, sdk_205_params_validate);
criterion_main!(benches);
