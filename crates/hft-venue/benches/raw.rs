//! VEN-353: one 300 B `RawRx` append (README "延迟与资源契约").
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: benchmark driver

#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;

use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_net::ConnId;
use hft_venue::{RawLog, raw_what};
use support::feedh::clock;
use support::jrn::raw_journal;
use support::rd::V7;

fn ven_353_raw_append(c: &mut Criterion) {
    let payload = [b'r'; 300];
    let mut seg = 0u64;
    let (w, mut j) = raw_journal("raw-bench", 64);
    let mut raw = RawLog::open(w, V7, clock());
    let mut t = 1u64;
    c.bench_function("venue_raw_append", |b| {
        b.iter(|| {
            t += 1;
            raw.rx(ConnId(0), raw_what::WS_TEXT, 0, 1, 0, 0, &payload, t);
            // A fresh journal when the segment fills (outside the steady state).
            if raw.counters().append_err[0] > 0 {
                seg += 1;
                let (w2, j2) = raw_journal("raw-bench", 64);
                raw = RawLog::open(w2, V7, clock());
                j = j2;
            }
        })
    });
    std::hint::black_box((seg, &j));
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(2)).warm_up_time(Duration::from_millis(300));
    targets = ven_353_raw_append
);
criterion_main!(benches);
