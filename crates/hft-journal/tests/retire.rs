//! Hot segment retirement (JNL-051 – JNL-053). Linux rows use tmpfs; elsewhere the heap backend.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::harness::*;
use hft_journal::testing::{self, Op};
use hft_journal::*;

/// Segments 1–4 sealed, synced and with `.seal`; segment 5 active.
fn five(hot_keep: u8) -> H {
    let mut cfg = linux_c0();
    cfg.hot_keep = hot_keep;
    let mut h = H::open0(cfg);
    for _ in 0..4 {
        h.events(3);
        h.roll(RollReason::Hourly);
    }
    h.persist_all();
    h
}

fn retired(h: &H) -> Vec<u32> {
    (1..=4).map(|n| h.retired(n)).collect()
}

#[test]
fn jnl_051_retire_conditions() {
    let mut h = five(1);
    h.ret.poll(h.utc);
    assert_eq!(retired(&h)[2..], [1, 0], "JNL-051.1");

    let mut h = five(1);
    testing::poke(&h.seg_path(3), 3144, &100u64.to_le_bytes()).unwrap();
    h.ret.poll(h.utc);
    assert_eq!(retired(&h)[2], 0, "JNL-051.2");

    let mut h = five(1);
    std::fs::remove_file(h.cold_stem(3).with_extension("seal")).unwrap();
    h.ret.poll(h.utc);
    assert_eq!(retired(&h)[2], 0, "JNL-051.3");

    let mut h = five(0);
    h.ret.poll(h.utc);
    assert_eq!(retired(&h)[2..], [1, 1], "JNL-051.4");

    let mut h = five(2);
    h.ret.poll(h.utc);
    assert_eq!(retired(&h)[1..3], [1, 0], "JNL-051.5");
}

#[cfg(target_os = "linux")]
fn mapped(p: &std::path::Path) -> bool {
    std::fs::read_to_string("/proc/self/maps")
        .unwrap()
        .contains(p.to_str().unwrap())
}

#[test]
fn jnl_052_retire_grace() {
    let mut h = five(1);
    // Retire and remove segments 1 and 2 first, while segment 3 still lacks its `.seal`.
    let seal3 = h.cold_stem(3).with_extension("seal");
    let saved = std::fs::read(&seal3).unwrap();
    std::fs::remove_file(&seal3).unwrap();
    let t0 = h.utc;
    h.ret.poll(t0);
    assert_eq!(h.ret.poll(t0 + 2_000_000_000), 2);
    std::fs::write(&seal3, saved).unwrap();
    let t = t0 + 10_000_000_000;
    assert_eq!(h.ret.poll(t), 0);
    assert_eq!(h.retired(3), 1);
    let p3 = h.seg_path(3);
    assert_eq!(h.ret.poll(t + 1_999_999_999), 0, "JNL-052.1");
    assert!(testing::exists(&p3), "JNL-052.1");
    assert_eq!(h.ret.poll(t + 2_000_000_000), 1, "JNL-052.2");
    assert!(!testing::exists(&p3), "JNL-052.2");
    #[cfg(target_os = "linux")]
    assert!(!mapped(&p3), "JNL-052.2");
}

#[test]
fn jnl_053_writer_never_unmaps() {
    let mut h = H::bare(mem_c0(), true);
    h.open().unwrap();
    testing::oplog_start();
    h.roll(RollReason::Startup);
    for _ in 0..20 {
        h.events(1);
        h.roll(RollReason::Hourly);
    }
    let ops = testing::oplog_take();
    let n = ops.iter().filter(|o| matches!(o, Op::Unmap | Op::Unlink)).count();
    assert_eq!(n, 0);
    let tap = h.tap.as_mut().unwrap();
    let got: Vec<u64> = std::iter::from_fn(|| tap.try_pop().map(|r| r.segment_no)).collect();
    assert_eq!(got, (1..=20).collect::<Vec<_>>());
}
