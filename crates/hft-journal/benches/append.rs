//! Append-side benchmarks (JNL-108 – JNL-110, JNL-113); ids equal the `基准名` in TESTS.md and the
//! README latency table. Heap backend here (any platform); on the reference machine the same code
//! runs with `HFT_BENCH_HOT` pointing at a hugetlbfs/tmpfs directory. Only the hot calls are
//! timed: rolls, persistence and retirement between batches are not.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "../tests/common/harness.rs"]
mod harness;

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use harness::*;
use hft_event::EventKind;
use hft_journal::*;
use hft_types::Tsc;

const BATCH: u64 = 1024;

fn hot() -> std::path::PathBuf {
    match std::env::var("HFT_BENCH_HOT") {
        Ok(p) => {
            let d = std::path::PathBuf::from(p).join(format!("bench-{}", std::process::id()));
            std::fs::create_dir_all(&d).unwrap();
            d
        }
        Err(_) => mem_hot(),
    }
}

fn big(kind_raw: bool) -> H {
    let (h, c) = (hot(), cold_dir());
    let mut cfg = if kind_raw { r0(&h, &c) } else { c0(&h, &c) };
    cfg.segment_bytes = 64 << 20;
    cfg.hot_keep = 0;
    H::open0(cfg)
}

/// Untimed housekeeping between batches: close the event, publish, roll and reclaim when due.
fn between(h: &mut H) {
    h.w().end_event();
    h.w().publish();
    if h.w().needs_roll(Tsc(0)).is_some() {
        h.roll(RollReason::Size);
        h.persist_all();
        h.utc += 3_000_000_000;
        h.ret.poll(h.utc);
        h.utc += 3_000_000_000;
        h.ret.poll(h.utc);
    }
}

/// Times `iters` calls of `f` in batches of `BATCH`, housekeeping outside the timed region.
fn batched(h: &mut H, iters: u64, mut f: impl FnMut(&mut JournalWriter, u64)) -> Duration {
    let mut total = Duration::ZERO;
    let mut done = 0;
    while done < iters {
        let n = (iters - done).min(BATCH);
        let w = h.w();
        let t = Instant::now();
        for i in 0..n {
            f(w, done + i);
        }
        total += t.elapsed();
        done += n;
        between(h);
    }
    total
}

fn jnl_108_journal_append_80b(c: &mut Criterion) {
    let mut h = big(false);
    let body = tin(7);
    c.bench_function("journal_append_80b", |b| {
        b.iter_custom(|iters| {
            batched(&mut h, iters, |w, i| {
                black_box(w.append(F0, Tsc(i), black_box(&body))).unwrap();
            })
        })
    });
}

fn jnl_109_journal_append_raw_512b(c: &mut Criterion) {
    let mut h = big(true);
    let body = raw_body(512 - 24, 0, 3);
    c.bench_function("journal_append_raw_512b", |b| {
        b.iter_custom(|iters| {
            batched(&mut h, iters, |w, i| {
                black_box(w.append_var(EventKind::RAW_RX, F0, Tsc(i), &[black_box(&body)])).unwrap();
            })
        })
    });
    c.bench_function("journal_append_raw_512b/reserve_var", |b| {
        b.iter_custom(|iters| {
            batched(&mut h, iters, |w, i| {
                let mut s = w.reserve_var(EventKind::RAW_RX, F0, Tsc(i), 512).unwrap();
                s.buf().copy_from_slice(black_box(&body));
                black_box(s.commit());
            })
        })
    });
}

/// Back-to-back `publish` calls (each Release-stores `last_seq` and `committed_len`).
fn publish_loop(h: &mut H, iters: u64) -> Duration {
    h.w().append(F0, Tsc(0), &tin(1)).unwrap();
    h.w().end_event();
    let w = h.w();
    let t = Instant::now();
    for _ in 0..iters {
        black_box(&mut *w).publish();
    }
    t.elapsed()
}

fn jnl_110_journal_publish(c: &mut Criterion) {
    let mut h = big(false);
    c.bench_function("journal_publish", |b| {
        b.iter_custom(|iters| publish_loop(&mut h, iters))
    });
    // With a tailing reader on another thread (tail_poll_us = 50) re-reading the writer row.
    let stop = Arc::new(AtomicBool::new(false));
    let mut rc = h.reader_cfg(StartAt::Tail);
    rc.tail_poll_us = 50;
    let s2 = stop.clone();
    let t = std::thread::spawn(move || {
        let mut r = JournalReader::open(&rc).unwrap();
        while !s2.load(Ordering::Relaxed) {
            if matches!(r.poll(), Next::Pending) {
                std::thread::yield_now();
            }
        }
    });
    c.bench_function("journal_publish/tailing_reader", |b| {
        b.iter_custom(|iters| publish_loop(&mut h, iters))
    });
    stop.store(true, Ordering::Relaxed);
    t.join().unwrap();
}

/// `append` + `end_event` per event (subtract `journal_append_80b` for `end_event` alone; a
/// per-call timer costs more than the call itself).
fn jnl_113_journal_end_event(c: &mut Criterion) {
    let mut h = big(false);
    let body = tin(1);
    c.bench_function("journal_end_event", |b| {
        b.iter_custom(|iters| {
            batched(&mut h, iters, |w, i| {
                w.append(F0, Tsc(i), black_box(&body)).unwrap();
                w.end_event();
            })
        })
    });
}

criterion_group!(
    benches,
    jnl_108_journal_append_80b,
    jnl_109_journal_append_raw_512b,
    jnl_110_journal_publish,
    jnl_113_journal_end_event
);
criterion_main!(benches);
