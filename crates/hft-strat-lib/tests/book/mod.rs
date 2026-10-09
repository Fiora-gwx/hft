//! Test book implementing SDK `BookRead` (TESTS.md fixture `BK1`): levels listed from the touch,
//! counting `best`/`levels` calls and the last `out.len()`.
#![allow(dead_code)]

use std::cell::Cell;

use hft_strategy_sdk::prelude::Level;
use hft_strategy_sdk::{BookRead, BookStatus};
use hft_types::{Px, Qty, Side, Tsc};

#[derive(Clone, Debug)]
pub struct TestBook {
    pub status: BookStatus,
    /// Best first.
    pub bids: Vec<Level>,
    /// Best first.
    pub asks: Vec<Level>,
    pub best_calls: Cell<usize>,
    pub levels_calls: Cell<usize>,
    pub last_out_len: Cell<usize>,
}

pub fn lv(px: i64, qty: i64) -> Level {
    Level {
        px: Px(px),
        qty: Qty(qty),
    }
}

impl TestBook {
    pub fn live(bids: &[(i64, i64)], asks: &[(i64, i64)]) -> TestBook {
        TestBook {
            status: BookStatus::Live,
            bids: bids.iter().map(|&(p, q)| lv(p, q)).collect(),
            asks: asks.iter().map(|&(p, q)| lv(p, q)).collect(),
            best_calls: Cell::new(0),
            levels_calls: Cell::new(0),
            last_out_len: Cell::new(0),
        }
    }

    fn side(&self, s: Side) -> &[Level] {
        match s {
            Side::Buy => &self.bids,
            Side::Sell => &self.asks,
        }
    }
}

impl BookRead for TestBook {
    fn status(&self) -> BookStatus {
        self.status
    }
    fn upd_tsc(&self) -> Tsc {
        Tsc(0)
    }
    fn best(&self, s: Side) -> Option<Level> {
        self.best_calls.set(self.best_calls.get() + 1);
        self.side(s).first().copied()
    }
    fn levels(&self, s: Side, out: &mut [Level]) -> usize {
        self.levels_calls.set(self.levels_calls.get() + 1);
        self.last_out_len.set(out.len());
        let src = self.side(s);
        let n = src.len().min(out.len());
        out[..n].copy_from_slice(&src[..n]);
        n
    }
}

/// `BK1`: asks (6 001 000, 0.5), (6 001 100, 1), (6 001 500, 2); bids (6 000 000, 0.5),
/// (5 999 900, 1), (5 999 500, 2) (qty exponent −8).
pub fn bk1() -> TestBook {
    TestBook::live(
        &[
            (6_000_000, 50_000_000),
            (5_999_900, 100_000_000),
            (5_999_500, 200_000_000),
        ],
        &[
            (6_001_000, 50_000_000),
            (6_001_100, 100_000_000),
            (6_001_500, 200_000_000),
        ],
    )
}
