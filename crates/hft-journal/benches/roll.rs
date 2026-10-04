//! Roll benchmarks (JNL-111, JNL-112).

#[path = "../tests/common/harness.rs"]
mod harness;

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use harness::*;
use hft_event::{EventKind, SnapshotHdr};
use hft_journal::*;
use hft_types::Tsc;
use zerocopy::{FromZeros, IntoBytes};

/// Frees sealed old segments (persist, seal, retire past the grace) outside the timed region.
fn reclaim(h: &mut H) {
    h.persist_all();
    h.utc += 3_000_000_000;
    h.ret.poll(h.utc);
    h.utc += 3_000_000_000;
    h.ret.poll(h.utc);
}

fn jnl_111_journal_roll_switch(c: &mut Criterion) {
    let mut cfg = c0(&mem_hot(), &cold_dir());
    cfg.segment_bytes = 64 << 10;
    cfg.hot_keep = 0;
    let mut h = H::open0(cfg);
    c.bench_function("journal_roll_switch", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                h.events(1);
                assert_eq!(h.prep.poll(), PrepState::Prepared);
                let t = Instant::now();
                let bw = h.try_roll(RollReason::Hourly).unwrap();
                total += t.elapsed();
                bw.finish().unwrap();
                if i % 64 == 63 {
                    reclaim(&mut h);
                }
            }
            reclaim(&mut h);
            total
        })
    });
}

fn jnl_112_journal_boot_400books(c: &mut Criterion) {
    let mut cfg = c0(&mem_hot(), &cold_dir());
    cfg.segment_bytes = 16 << 20;
    cfg.hot_keep = 0;
    let mut h = H::open0(cfg);
    // 400 books × 200 levels as L2Snapshot{Keyframe} + a 200 KB StateSnapshot ≈ 1.5 MB.
    let mut book = vec![0u8; 72 + 200 * 16];
    book[64] = 2;
    book[68..70].copy_from_slice(&100u16.to_le_bytes());
    book[70..72].copy_from_slice(&100u16.to_le_bytes());
    let mut sh = SnapshotHdr::new_zeroed();
    sh.purpose = 1;
    sh.n_books = 400;
    let mut snap = sh.as_bytes().to_vec();
    snap.extend(vec![0x5Au8; 200_000]);
    c.bench_function("journal_boot_400books", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                h.events(1);
                assert_eq!(h.prep.poll(), PrepState::Prepared);
                let t = Instant::now();
                let mut bw = h.try_roll(RollReason::Hourly).unwrap();
                bw.append_var(EventKind::STATE_SNAPSHOT, Tsc(1), &[&snap])
                    .unwrap();
                for _ in 0..400 {
                    bw.append_var(EventKind::L2_SNAPSHOT, Tsc(1), &[&book]).unwrap();
                }
                bw.finish().unwrap();
                total += t.elapsed();
                reclaim(&mut h);
            }
            total
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = jnl_111_journal_roll_switch, jnl_112_journal_boot_400books
}
criterion_main!(benches);
