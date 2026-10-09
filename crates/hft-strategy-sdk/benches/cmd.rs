//! `CmdView::parse` and every `val` of a 31-value command (SDK-200).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::CmdView;

fn sdk_200_cmd_parse(c: &mut Criterion) {
    let mut p = vec![1u8, 1, 31, 0, 0, 0, 0, 0];
    for v in 0..31i64 {
        p.extend_from_slice(&v.to_le_bytes());
    }
    c.bench_function("sdk/cmd_parse", |b| {
        b.iter(|| {
            let Ok(v) = CmdView::parse(black_box(&p)) else {
                return 0;
            };
            (0..31u8).fold(0i64, |a, i| a.wrapping_add(v.val(i).unwrap_or(0)))
        })
    });
}

criterion_group!(benches, sdk_200_cmd_parse);
criterion_main!(benches);
