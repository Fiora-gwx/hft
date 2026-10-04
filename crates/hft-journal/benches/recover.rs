//! Recovery of a committed but uncopied hot segment (JNL-116). Size: `HFT_BENCH_RECOVER_BYTES`
//! (default 1 GiB, the README budget); cold directory: `HFT_BENCH_COLD` (local NVMe/EBS) or a
//! temporary directory.

#[path = "../tests/common/harness.rs"]
mod harness;

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use harness::*;
use hft_types::Tsc;

fn cold() -> std::path::PathBuf {
    match std::env::var("HFT_BENCH_COLD") {
        Ok(p) => {
            let d = std::path::PathBuf::from(p).join(format!("recover-{}", std::process::id()));
            std::fs::create_dir_all(&d).unwrap();
            d
        }
        Err(_) => cold_dir(),
    }
}

fn jnl_116_journal_recover_1g(c: &mut Criterion) {
    let size: u64 = std::env::var("HFT_BENCH_RECOVER_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1 << 30);
    let mut g = c.benchmark_group("recover");
    g.sample_size(10);
    g.bench_function("journal_recover_1g", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let hot = mem_hot();
                let cold = cold();
                let mut cfg = c0(&hot, &cold);
                cfg.segment_bytes = size + (16 << 20);
                let mut h = H::open0(cfg);
                let w = h.w();
                let body = tvar(65_000, 0x42);
                while w.offset() < size {
                    w.append_var(hft_event::EventKind::T_VAR, F0, Tsc(1), &[&body])
                        .unwrap();
                    w.end_event();
                }
                w.publish();
                h.crash();
                let t = Instant::now();
                h.open().unwrap();
                total += t.elapsed();
                drop(h);
                hft_journal::testing::mem_purge(&hot);
                let _ = std::fs::remove_dir_all(&cold);
            }
            total
        })
    });
    g.finish();
}

criterion_group!(benches, jnl_116_journal_recover_1g);
criterion_main!(benches);
