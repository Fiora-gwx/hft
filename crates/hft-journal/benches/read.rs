//! Sequential reader scan over a sealed cold copy in the page cache (JNL-115).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "../tests/common/harness.rs"]
mod harness;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use harness::*;
use hft_journal::*;
use hft_types::Tsc;

fn jnl_115_journal_read_scan(c: &mut Criterion) {
    let mut cfg = c0(&mem_hot(), &cold_dir());
    cfg.segment_bytes = 64 << 20;
    let mut h = H::open0(cfg);
    let w = h.w();
    let mut i = 0u64;
    while w.needs_roll(Tsc(0)).is_none() {
        for _ in 0..64 {
            w.append(F0, Tsc(i), &tin(i as u8)).unwrap();
            w.append(F0, Tsc(i), &tout(i as u8)).unwrap();
            w.end_event();
            i += 1;
        }
        w.publish();
    }
    h.roll(RollReason::Size);
    h.persist_all();
    // Segment 1 is read from its cold copy.
    let stem = h.cold_stem(1);
    hft_journal::testing::unlink(&h.seg_path(1)).unwrap();
    let bytes = std::fs::metadata(stem.with_extension("jrn")).unwrap().len();
    let mut g = c.benchmark_group("read");
    g.throughput(Throughput::Bytes(bytes));
    g.sample_size(10);
    g.bench_function("journal_read_scan", |b| {
        b.iter(|| {
            let mut r = h.reader(StartAt::Oldest);
            let mut n = 0u64;
            while let Next::Record(_) | Next::Event(_) = r.poll() {
                n += 1;
                if r.position().segment_no > 1 {
                    break;
                }
            }
            n
        })
    });
    g.finish();
}

criterion_group!(benches, jnl_115_journal_read_scan);
criterion_main!(benches);
