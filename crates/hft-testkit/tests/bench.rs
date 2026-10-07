//! TK-090, TK-091: `HistBench` in smoke mode.

use std::hint::black_box;

use hft_testkit::{BenchMode, CountingAlloc, HistBench};

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

#[test]
fn tk_090_histbench_warmup() {
    assert!(CountingAlloc::is_installed());
    let b = HistBench::smoke("tk_090").iters(5_000);
    assert_eq!((b.warmup, b.mode), (1_000, BenchMode::Smoke));
    let mut calls = 0u64;
    let out = b.run(|| calls = black_box(calls + 1)).unwrap();
    assert_eq!(calls, 6_000);
    assert_eq!(out.n, 5_000);
    assert_eq!(out.hist.len(), 5_000);
    assert_eq!(out.warmup, 1_000);
    assert_eq!(out.allocs_after_warmup, 0);
    assert_eq!(out.pinned, None, "smoke mode does not pin");
}

#[test]
fn tk_091_histbench_allocs() {
    assert!(CountingAlloc::is_installed());
    let b = HistBench::smoke("tk_091").warmup(1_000).iters(5_000);
    let mut calls = 0u64;
    let mut kept: Vec<Box<u64>> = Vec::with_capacity(1);
    let out = b
        .run(|| {
            calls += 1;
            if calls == 3_000 {
                kept.push(black_box(Box::new(calls)));
            }
        })
        .unwrap();
    assert_eq!(calls, 6_000);
    assert_eq!(out.allocs_after_warmup, 1);
    drop(kept);
}
