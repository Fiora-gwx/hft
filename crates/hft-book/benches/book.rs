//! hft-book benchmarks (README "延迟与资源契约"; BOOK-090–BOOK-101).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_book::{BookParams, BookSet, L2Book, Top};
use hft_event::{L2SnapHdr, Level};
use hft_strategy_sdk::SnapshotWriter;
use hft_types::{InstId, Seq, Side};

#[path = "../tests/common/mod.rs"]
mod common;

use common::{aligned_mem, batch_hdr, bbo, delta, lv, refdata, snap_hdr};

const TICK: i64 = 10;
const MID: i64 = 100_000; // tick index

fn p2048() -> BookParams {
    BookParams {
        tick: TICK,
        window: 2048,
        ovf_cap: 64,
        crossed_tolerance: 0,
    }
}

/// Live W = 2048 book with `n` levels per side (1 tick apart) around `MID`, `last_id` = 1.
fn book(n: i64) -> L2Book {
    let mut b = L2Book::new(&p2048());
    let mut v: Vec<Level> = (0..n).map(|k| lv((MID - k) * TICK, 5)).collect();
    v.extend((1..=n).map(|k| lv((MID + k) * TICK, 5)));
    b.apply_snapshot(&snap_hdr(L2SnapHdr::EXCHANGE, 1, n as usize, n as usize), &v);
    b
}

fn book_090_delta_inwin(c: &mut Criterion) {
    let mut b = book(20);
    let mut id = 1u64;
    c.bench_function("book_delta_inwin", |x| {
        x.iter(|| {
            id += 1;
            let q = (id & 7) as i64 + 1;
            black_box(b.apply_delta(&delta(id - 1, id, Side::Buy, (MID - 5) * TICK, q, 0, true)))
        })
    });
}

fn book_091_batch_20(c: &mut Criterion) {
    let mut b = book(20);
    let mut id = 1u64;
    let mut lv20: Vec<Level> = (1..=10).map(|k| lv((MID - k) * TICK, 3)).collect();
    lv20.extend((2..=11).map(|k| lv((MID + k) * TICK, 3)));
    c.bench_function("book_batch_20", |x| {
        x.iter(|| {
            id += 1;
            let q = (id & 7) as i64 + 1;
            for l in lv20.iter_mut() {
                l.qty.0 = q;
            }
            black_box(b.apply_batch(&batch_hdr(id - 1, id, 10, 10), &lv20))
        })
    });
}

fn book_092_delta_best_remove(c: &mut Criterion) {
    // Best bid and the next bid 200 ticks (several bitmap words) below it.
    let mut b = L2Book::new(&p2048());
    let v = [
        lv(MID * TICK, 5),
        lv((MID - 200) * TICK, 5),
        lv((MID + 1) * TICK, 5),
    ];
    b.apply_snapshot(&snap_hdr(L2SnapHdr::EXCHANGE, 1, 2, 1), &v);
    let mut id = 1u64;
    c.bench_function("book_delta_best_remove", |x| {
        x.iter(|| {
            id += 1;
            black_box(b.apply_delta(&delta(id - 1, id, Side::Buy, MID * TICK, 0, 0, true)));
            id += 1;
            black_box(b.apply_delta(&delta(id - 1, id, Side::Buy, MID * TICK, 5, 0, true)))
        })
    });
}

fn book_093_delta_ovf(c: &mut Criterion) {
    let mut b = book(20);
    // 64 bids below the window fill the overflow area (cap 64).
    let ovf: Vec<Level> = (0..64).map(|k| lv((MID - 3000 - k) * TICK, 1)).collect();
    b.apply_batch(&batch_hdr(1, 2, 64, 0), &ovf);
    let mut id = 2u64;
    c.bench_function("book_delta_ovf", |x| {
        x.iter(|| {
            id += 1;
            let px = (MID - 3000 - (id & 63) as i64) * TICK;
            black_box(b.apply_delta(&delta(id - 1, id, Side::Buy, px, (id & 3) as i64 + 1, 0, true)))
        })
    });
}

fn book_094_top_apply(c: &mut Criterion) {
    let mut t = Top::ZERO;
    let mut q = [bbo(0, (1000, 1), (1010, 1), 1), bbo(0, (1000, 2), (1010, 1), 2)];
    let mut n = 0u64;
    c.bench_function("book_top_apply", |x| {
        x.iter(|| {
            n += 1;
            let b = &mut q[(n & 1) as usize];
            b.update_id = n;
            black_box(t.apply(b, Seq(n)))
        })
    });
}

fn book_095_best(c: &mut Criterion) {
    let b = book(20);
    c.bench_function("book_best", |x| {
        x.iter(|| black_box((black_box(&b).best_bid(), black_box(&b).best_ask())))
    });
}

fn book_096_iter_20(c: &mut Criterion) {
    let b = book(50);
    c.bench_function("book_iter_20", |x| {
        x.iter(|| {
            black_box(&b)
                .iter(Side::Buy)
                .take(20)
                .map(|l| l.qty.0)
                .sum::<i64>()
        })
    });
}

fn book_097_snapshot_1000(c: &mut Criterion) {
    let mut b = book(20);
    let mut v: Vec<Level> = (0..1000).map(|k| lv((MID - k) * TICK, 5)).collect();
    v.extend((1..=1000).map(|k| lv((MID + k) * TICK, 5)));
    let mut id = 1u64;
    c.bench_function("book_snapshot_1000", |x| {
        x.iter(|| {
            id += 1;
            black_box(b.apply_snapshot(&snap_hdr(L2SnapHdr::EXCHANGE, id, 1000, 1000), &v))
        })
    });
}

fn book_098_snapshot_topn20(c: &mut Criterion) {
    let mut b = L2Book::new(&p2048());
    let mut v: Vec<Level> = (0..20).map(|k| lv((MID - k) * TICK, 5)).collect();
    v.extend((1..=20).map(|k| lv((MID + k) * TICK, 5)));
    let mut id = 1u64;
    c.bench_function("book_snapshot_topn20", |x| {
        x.iter(|| {
            id += 1;
            let q = (id & 7) as i64 + 1;
            for l in v.iter_mut() {
                l.qty.0 = q;
            }
            black_box(b.apply_snapshot(&snap_hdr(L2SnapHdr::TOP_N, id, 20, 20), &v))
        })
    });
}

fn book_099_snapshot_sections(c: &mut Criterion) {
    let rd = refdata(2000, TICK, 2048, 0);
    let ids: Vec<InstId> = (1..=400).map(InstId).collect();
    let mut set = match BookSet::new_in(rd, &ids, aligned_mem(BookSet::bytes_needed(rd, &ids))) {
        Ok(s) => s,
        Err(e) => panic!("{e:?}"),
    };
    let v = [lv(MID * TICK, 5), lv((MID + 1) * TICK, 5)];
    for i in &ids {
        if let Some(b) = set.l2_mut(*i) {
            b.apply_snapshot(&snap_hdr(L2SnapHdr::EXCHANGE, 1, 1, 1), &v);
        }
    }
    for i in 1..=2000u32 {
        set.top_mut(InstId(i))
            .apply(&bbo(1, (1000, 1), (1010, 1), 1), Seq(u64::from(i)));
    }
    let mut w = SnapshotWriter::new(1 << 20);
    c.bench_function("book_snapshot_sections", |x| {
        x.iter(|| {
            w.clear();
            set.snapshot(&mut w);
            black_box(w.len())
        })
    });
}

fn book_100_recenter_2048(c: &mut Criterion) {
    // One message moves the book (20 levels per side) 1 500 ticks up or back down: the mid
    // leaves [W/2, 3W/2) every time and the window is shifted (|d| < L path).
    let side_lv = |m: i64, q: i64| {
        let mut v: Vec<Level> = (0..20).map(|k| lv((m - k) * TICK, q)).collect();
        v.extend((1..=20).map(|k| lv((m + k) * TICK, q)));
        v
    };
    let msg = |from: i64, to: i64| {
        let (old, new) = (side_lv(from, 0), side_lv(to, 5));
        let mut v: Vec<Level> = old[..20].to_vec();
        v.extend_from_slice(&new[..20]);
        v.extend_from_slice(&old[20..]);
        v.extend_from_slice(&new[20..]);
        v
    };
    let (go, back) = (msg(MID, MID + 1500), msg(MID + 1500, MID));
    let mut b = book(20);
    let mut id = 1u64;
    c.bench_function("book_recenter_2048", |x| {
        x.iter(|| {
            id += 1;
            let l = if id.is_multiple_of(2) { &go } else { &back };
            black_box(b.apply_batch(&batch_hdr(id - 1, id, 40, 40), l))
        })
    });
}

fn book_101_export_200(c: &mut Criterion) {
    let b = book(200);
    let mut out = vec![lv(0, 0); 400];
    c.bench_function("book_export_200", |x| {
        x.iter(|| black_box(black_box(&b).export(&mut out)))
    });
}

criterion_group!(
    benches,
    book_090_delta_inwin,
    book_091_batch_20,
    book_092_delta_best_remove,
    book_093_delta_ovf,
    book_094_top_apply,
    book_095_best,
    book_096_iter_20,
    book_097_snapshot_1000,
    book_098_snapshot_topn20,
    book_099_snapshot_sections,
    book_100_recenter_2048,
    book_101_export_200
);
criterion_main!(benches);
