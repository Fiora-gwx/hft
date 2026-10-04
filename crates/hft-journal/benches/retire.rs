//! Effect of hot-segment retirement on a hot thread (JNL-114): ring hop latency measured on the
//! bench thread while another thread rolls, persists and retires segments; the `/baseline` id is
//! the same hop without retirement.

#[path = "../tests/common/harness.rs"]
mod harness;

use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use criterion::{Criterion, criterion_group, criterion_main};
use harness::*;
use hft_journal::*;
use hft_rt::{Backing, spsc};

fn hop(c: &mut Criterion, id: &str) {
    let (mut tx, mut rx) = spsc::<u64>(1024, Backing::Heap).unwrap();
    let mut n = 0u64;
    c.bench_function(id, |b| {
        b.iter(|| {
            n += 1;
            let _ = tx.try_push(black_box(n));
            black_box(rx.try_pop())
        })
    });
}

fn jnl_114_journal_retire_tlb(c: &mut Criterion) {
    hop(c, "journal_retire_tlb/baseline");
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let t = std::thread::spawn(move || {
        let hot = match std::env::var("HFT_BENCH_HOT") {
            Ok(p) => {
                let d = std::path::PathBuf::from(p).join(format!("retire-{}", std::process::id()));
                std::fs::create_dir_all(&d).unwrap();
                d
            }
            Err(_) => mem_hot(),
        };
        let mut cfg = c0(&hot, &cold_dir());
        cfg.segment_bytes = 1 << 20;
        cfg.hot_keep = 0;
        let mut h = H::open0(cfg);
        while !s2.load(Ordering::Relaxed) {
            h.events(100);
            h.roll(RollReason::Hourly);
            h.persist_all();
            h.utc += 3_000_000_000;
            h.ret.poll(h.utc);
            h.utc += 3_000_000_000;
            h.ret.poll(h.utc);
        }
    });
    hop(c, "journal_retire_tlb");
    stop.store(true, Ordering::Relaxed);
    t.join().unwrap();
}

criterion_group!(benches, jnl_114_journal_retire_tlb);
criterion_main!(benches);
