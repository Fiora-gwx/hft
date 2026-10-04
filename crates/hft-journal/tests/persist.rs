//! Persist: chunked copy, sync thresholds, sealing, lag, round order (JNL-044 – JNL-047, JNL-049,
//! JNL-050). Linux rows use tmpfs; elsewhere the heap backend.

mod common;

use common::harness::*;
use hft_journal::testing::{self, Op};
use hft_journal::*;
use sha2::{Digest, Sha256};

const MIB: u64 = 1 << 20;

fn masked(b: &[u8]) -> Vec<u8> {
    let mut v = b.to_vec();
    for x in v.iter_mut().take(3200).skip(3072) {
        *x = 0;
    }
    v
}

#[test]
fn jnl_044_chunked_copy() {
    let mut cfg = linux_c0();
    cfg.segment_bytes = 16 * MIB;
    let mut h = H::open0(cfg);
    h.persister(PersistConfig {
        chunk_bytes: 4 << 20,
        sync_bytes: 1 << 20,
        sync_interval_ms: 20,
    });
    h.push_to(10 * MIB as usize);
    let mut got = Vec::new();
    for i in 1..=3 {
        h.mono += 1_000_000;
        h.pers.poll(h.mono, h.utc);
        got.push(h.persisted(1));
        if i == 1 {
            let cold = testing::cold_stem(&h.cold_jdir(), UTC0, 1, 1);
            let jrn = cold.with_extension("jrn");
            assert!(jrn.to_string_lossy().contains("20261001"));
            assert_eq!(std::fs::metadata(&jrn).unwrap().len(), 16 * MIB);
        }
    }
    assert_eq!(got, vec![4 * MIB, 8 * MIB, 10 * MIB]);
    let jrn = h.cold_stem(1).with_extension("jrn");
    let cold = std::fs::read(jrn).unwrap();
    let hot = h.seg(1);
    let n = 10 * MIB as usize;
    assert_eq!(masked(&cold[..n]), masked(&hot[..n]));
}

/// Harness whose last `fdatasync` happened at `t0`; returns `(h, t0)`.
fn synced_at_t0(chunk: u32) -> (H, u64) {
    let mut cfg = linux_c0();
    cfg.segment_bytes = 16 * MIB;
    let mut h = H::open0(cfg);
    h.persister(PersistConfig {
        chunk_bytes: chunk,
        sync_bytes: 1 << 20,
        sync_interval_ms: 20,
    });
    let m0 = 1_000_000_000u64;
    let t0 = m0 + 20_000_000;
    // Copy everything but the last chunk at m0 (no sync), the rest at t0 (time-triggered sync).
    h.pers.poll(m0, h.utc);
    while h.persisted(1) + u64::from(chunk) < h.committed(1) {
        h.pers.poll(m0, h.utc);
    }
    let p = h.pers.poll(t0, h.utc);
    assert_eq!(p.syncs, 1);
    assert_eq!(h.synced(1), h.committed(1));
    (h, t0)
}

#[test]
fn jnl_045_sync_thresholds() {
    for (row, unsynced, dt, syncs) in [
        ("JNL-045.1", 1_048_568usize, 19_999_999u64, 0u32),
        ("JNL-045.2", 1_048_576, 1, 1),
        ("JNL-045.3", 8, 20_000_000, 1),
        ("JNL-045.4", 8, 19_999_999, 0),
    ] {
        let small = unsynced == 8;
        let (mut h, t0) = synced_at_t0(if small { 8 } else { 4 << 20 });
        let before = h.synced(1);
        let c = h.committed(1) as usize;
        h.push_to(c + if small { 48 } else { unsynced });
        testing::oplog_start();
        let p = h.pers.poll(t0 + dt, h.utc);
        let n = testing::oplog_take().iter().filter(|o| **o == Op::Sync).count();
        assert_eq!(h.persisted(1) - before, unsynced as u64, "{row}: unsynced bytes");
        assert_eq!((p.syncs, n as u32), (syncs, syncs), "{row}");
        if syncs == 0 {
            assert_eq!(h.synced(1), before, "{row}");
        } else {
            assert_eq!(h.synced(1), h.persisted(1), "{row}");
            let cold = std::fs::read(h.cold_stem(1).with_extension("jrn")).unwrap();
            assert_eq!(u64_at(&cold, 3144), h.synced(1), "{row}");
        }
    }
}

#[test]
fn jnl_046_seal() {
    let mut h = H::open0(linux_c0());
    h.write_to_seq(50);
    h.roll(RollReason::Hourly);
    let s = h.utc;
    h.persist_all();
    let c = h.committed(1);
    assert_eq!(h.synced(1), c);
    let stem = h.cold_stem(1);
    let cold = std::fs::read(stem.with_extension("jrn")).unwrap();
    assert_eq!(cold.len() as u64, c);
    let seal = std::fs::read(stem.with_extension("seal")).unwrap();
    assert_eq!(seal.len(), 128);
    let r: SealRecord = zerocopy::FromBytes::read_from_bytes(&seal[..]).unwrap();
    let sha: [u8; 32] = Sha256::digest(masked(&cold)).into();
    assert_eq!(&r.magic, b"HFTSEAL1");
    assert_eq!(
        (r.segment_no, r.first_seq, r.last_seq, r.len, r.sealed_utc_ns),
        (1, 1, 51, c, s)
    );
    assert_eq!((r.sha256, r.reason, r.flags), (sha, 1, 0));
    assert_eq!(r.crc32c, crc32c::crc32c(&seal[..124]));
}

#[test]
fn jnl_047_no_seal_before_synced() {
    let mut h = H::open0(linux_c0());
    h.write_to_seq(50);
    h.roll(RollReason::Hourly);
    h.persister(PersistConfig {
        chunk_bytes: 4096,
        sync_bytes: 1 << 20,
        sync_interval_ms: 20,
    });
    h.mono += 1_000_000;
    h.pers.poll(h.mono, h.utc);
    assert!(h.synced(1) < h.committed(1));
    let stem = h.cold_stem(1);
    assert!(!stem.with_extension("seal").exists());
    assert_eq!(
        std::fs::metadata(stem.with_extension("jrn")).unwrap().len(),
        1_048_576
    );
}

#[test]
fn jnl_049_lag() {
    let hot = linux_hot();
    let cold = cold_dir();
    let mut a = H::open0(c0(&hot, &cold));
    let mut b = H::open0(r0(&hot, &cold));
    b.w()
        .append_var(
            hft_event::EventKind::RAW_RX,
            F0,
            hft_types::Tsc(1),
            &[&raw_body(100, 0, 1)],
        )
        .unwrap();
    b.w().publish();
    a.push_to(60_000);
    let mut p = Persister::new(PersistConfig::default());
    p.add(&a.cfg);
    p.add(&b.cfg);
    p.poll(0, UTC0);
    p.poll(30_000_000, UTC0);
    assert_eq!((a.synced(1), b.synced(1)), (60_000, b.committed(1)));
    a.push_to(100_000);
    let lag = p.lag();
    let v: Vec<(&str, u64)> = lag.iter().map(|l| (l.name.as_str(), l.bytes)).collect();
    assert_eq!(v, vec![("seq", 40_000), ("raw-feed-binance-usdm", 0)]);
}

#[test]
fn jnl_050_round_order() {
    let mut h = H::open0(mem_c0());
    h.events(5);
    h.roll(RollReason::Hourly);
    h.events(5);
    h.roll(RollReason::Hourly);
    h.persist_all();
    h.events(5);
    testing::oplog_start();
    h.prep.poll();
    h.mono += 25_000_000;
    h.pers.poll(h.mono, h.utc);
    h.ret.poll(h.utc);
    let ops: Vec<&str> = testing::oplog_take()
        .iter()
        .filter_map(|o| match o {
            Op::Prepare(_) => Some("prepare"),
            Op::Copy(..) => Some("copy"),
            Op::Sync => Some("sync"),
            Op::Retire(_) => Some("retire"),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec!["prepare", "copy", "sync", "retire"]);
}
