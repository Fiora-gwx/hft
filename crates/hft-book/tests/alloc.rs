//! BOOK-103: no allocation in any entry point after `new_in`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use common::{aligned_mem, batch_hdr, bbo, delta, lv, refdata, snap_hdr};
use hft_book::{BookSet, L2Book, Outcome};
use hft_event::{L2SnapHdr, Level};
use hft_strategy_sdk::SnapshotWriter;
use hft_types::{InstId, Seq, Side};

struct Counting;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static COUNT_HERE: Cell<bool> = const { Cell::new(false) };
}

fn note() {
    if COUNT_HERE.with(Cell::get) {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
    }
}

// SAFETY: forwards to the system allocator unchanged; only counts calls.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(p, l, n) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const W: i64 = 64; // book_window of the test refdata

fn levels(v: &[(i64, i64)]) -> Vec<Level> {
    v.iter().map(|&(p, q)| lv(p, q)).collect()
}

#[test]
fn book_103_zero_alloc_after_new_in() {
    let rd = refdata(3, 10, W as u32, 0);
    let ids = [InstId(1), InstId(2)];
    let mut set = match BookSet::new_in(rd, &ids, aligned_mem(BookSet::bytes_needed(rd, &ids))) {
        Ok(s) => s,
        Err(e) => panic!("{e:?}"),
    };
    let mut w = SnapshotWriter::new(1 << 16);
    let mut out = vec![lv(0, 0); 400];
    // Inputs, prepared before counting.
    let s0 = levels(&[(10000, 5), (9990, 3), (10010, 2), (10020, 4)]);
    let b20: Vec<(i64, i64)> = (0..10)
        .map(|k| (9980 - 10 * k, 2))
        .chain((0..10).map(|k| (10030 + 10 * k, 2)))
        .collect();
    let b20 = levels(&b20);
    let far: Vec<(i64, i64)> = (0..80).map(|k| (12000 + 10 * k, 1)).collect();
    let far = levels(&far);
    let up = levels(&[(10000 + 300 * W, 1), (10010 + 300 * W, 1)]);
    let back = levels(&[(10000, 5), (10010, 2)]);
    let topn: Vec<(i64, i64)> = (0..20)
        .map(|k| (5000 - 10 * k, 1))
        .chain((0..20).map(|k| (5010 + 10 * k, 1)))
        .collect();
    let topn = levels(&topn);
    let quotes = [bbo(0, (100, 1), (110, 1), 1), bbo(0, (101, 2), (111, 2), 2)];

    COUNT_HERE.with(|c| c.set(true));
    let c0 = ALLOCS.load(Ordering::Relaxed);
    let mut id = 0u64;
    let mut topn_id = 0u64;
    let mut acc = 0i64;
    let mut seen = Outcome::NONE;
    for i in 0..100_000u64 {
        let b: &mut L2Book = match set.l2_mut(InstId(1)) {
            Some(b) => b,
            None => break,
        };
        let o = match i % 10 {
            0 => {
                id += 1;
                b.apply_snapshot(&snap_hdr(L2SnapHdr::EXCHANGE, id, 2, 2), &s0)
            }
            1 => {
                let side = if i % 20 == 1 { Side::Buy } else { Side::Sell };
                let px = if side == Side::Buy { 9990 } else { 10020 };
                let o = b.apply_delta(&delta(id, id + 1, side, px, (i % 5) as i64 + 1, 0, false));
                let o = o | b.apply_delta(&delta(id, id + 1, side, px - 10, 1, 1, true));
                id += 1;
                o
            }
            2 => {
                id += 1;
                b.apply_batch(&batch_hdr(id - 1, id, 10, 10), &b20)
            }
            3 => {
                id += 1;
                b.apply_batch(&batch_hdr(id - 1, id, 0, 80), &far)
            }
            4 => {
                id += 1;
                b.apply_batch(&batch_hdr(id - 1, id, 1, 1), &up)
            }
            5 => {
                id += 1;
                b.apply_snapshot(&snap_hdr(L2SnapHdr::EXCHANGE, id, 1, 1), &back)
            }
            6 => {
                topn_id += 1;
                set.l2_mut(InstId(2)).map_or(Outcome::NONE, |t| {
                    t.apply_snapshot(&snap_hdr(L2SnapHdr::TOP_N, topn_id, 20, 20), &topn)
                })
            }
            7 => {
                let t = set.top_mut(InstId(3));
                Outcome(u16::from(t.apply(&quotes[(i % 2) as usize], Seq(i + 1)).0))
            }
            8 => {
                acc += b.best_bid().map_or(0, |l| l.px.0) + b.best_ask().map_or(0, |l| l.qty.0);
                acc += b.iter(Side::Buy).take(20).map(|l| l.qty.0).sum::<i64>();
                let (nb, na, _) = b.export(&mut out);
                acc += i64::from(nb + na) + (b.state_hash() as i64 & 1) + b.meta().base;
                Outcome::NONE
            }
            _ => {
                w.clear();
                set.snapshot(&mut w);
                acc += w.len() as i64;
                Outcome::NONE
            }
        };
        acc += i64::from(o.0);
        seen |= o;
    }
    let c1 = ALLOCS.load(Ordering::Relaxed);
    COUNT_HERE.with(|c| c.set(false));
    assert!(acc != 0);
    assert!(
        seen.contains(Outcome::RECENTERED | Outcome::TRUNCATED | Outcome::WENT_LIVE),
        "{seen:?}"
    );
    assert_eq!(c1 - c0, 0);
}
