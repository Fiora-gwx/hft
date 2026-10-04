//! Random message streams shared by the property tests (BOOK-048, BOOK-060, BOOK-079).
//! Generated values are unitless; `Stream::next` turns them into prices around a moving mid.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(dead_code)]

use proptest::prelude::*;

pub const TICK: i64 = 10;

/// Raw generated operation.
#[derive(Clone, Debug)]
pub enum Op {
    /// Mid moves by `small + jump × L`, then one message with `lv` = (is_ask, raw offset, qty).
    /// `bad` = (position, kind) of one illegal level.
    Msg {
        small: i64,
        jump: i64,
        lv: Vec<(bool, u32, i64)>,
        bad: Option<(usize, u8)>,
    },
    /// Exchange snapshot around the mid: (raw offset, qty) per side.
    Snap {
        bids: Vec<(u32, i64)>,
        asks: Vec<(u32, i64)>,
    },
}

fn levels(max: usize) -> impl Strategy<Value = Vec<(bool, u32, i64)>> {
    prop::collection::vec((any::<bool>(), any::<u32>(), 0i64..=5), 1..=max)
}

/// `bad_pct` percent of messages carry one illegal level; `snap_pct` per mille are snapshots.
pub fn op(max_levels: usize, bad_pct: u32, snap_permille: u32) -> impl Strategy<Value = Op> {
    let jump = prop_oneof![97 => Just(0i64), 3 => prop_oneof![-3i64..=-1, 1i64..=3]];
    let bad =
        (0u32..100, any::<usize>(), 0u8..3).prop_map(move |(p, at, k)| (p < bad_pct).then_some((at, k)));
    let msg = (-3i64..=3, jump, levels(max_levels), bad).prop_map(|(small, jump, lv, bad)| Op::Msg {
        small,
        jump,
        lv,
        bad,
    });
    let side = || prop::collection::vec((any::<u32>(), 0i64..=5), 0..=20);
    let snap = (side(), side()).prop_map(|(bids, asks)| Op::Snap { bids, asks });
    (0u32..1000, msg, snap).prop_map(move |(r, m, s)| if r < snap_permille { s } else { m })
}

/// Concrete operation: prices are raw (tick 10).
#[derive(Clone, Debug)]
pub enum Concrete {
    /// (side 0 = bid / 1 = ask, px, qty), bids first.
    Msg {
        prev: u64,
        last: u64,
        lv: Vec<(usize, i64, i64)>,
    },
    Snap {
        id: u64,
        bids: Vec<(i64, i64)>,
        asks: Vec<(i64, i64)>,
    },
}

impl Concrete {
    pub fn n_bids(&self) -> usize {
        match self {
            Concrete::Msg { lv, .. } => lv.iter().filter(|l| l.0 == 0).count(),
            Concrete::Snap { bids, .. } => bids.len(),
        }
    }
}

/// Turns raw operations into prices around a moving mid (tick index) and chained ids.
#[derive(Clone, Debug)]
pub struct Stream {
    pub w: i64,
    pub l: i64,
    pub mid: i64,
    pub id: u64,
    /// Apply the generated `jump × L` mid moves.
    pub jumps: bool,
}

impl Stream {
    pub fn new(window: u32) -> Stream {
        let w = i64::from(window);
        Stream {
            w,
            l: (2 * w + 63) / 64 * 64,
            mid: 10_000_000,
            id: 100,
            jumps: true,
        }
    }

    fn off(&self, raw: u32, span: i64) -> i64 {
        if raw.is_multiple_of(3) {
            i64::from(raw / 3 % 8) + 1
        } else {
            i64::from(raw) % span + 1
        }
    }

    pub fn snap(&mut self, bids: &[(u32, i64)], asks: &[(u32, i64)]) -> Concrete {
        self.id += 10;
        let span = 2 * self.w;
        Concrete::Snap {
            id: self.id,
            bids: bids
                .iter()
                .map(|&(r, q)| ((self.mid - self.off(r, span)) * TICK, q))
                .collect(),
            asks: asks
                .iter()
                .map(|&(r, q)| ((self.mid + self.off(r, span)) * TICK, q))
                .collect(),
        }
    }

    /// `span_w` = price range in multiples of W on each side of the mid.
    pub fn next(&mut self, op: &Op, span_w: i64) -> Concrete {
        self.next_del(op, span_w, |_| (Vec::new(), Vec::new()))
    }

    /// As `next`; `del(mid)` gives bid and ask prices the message also deletes (levels that
    /// would cross the new mid), appended after the generated levels of each side.
    pub fn next_del(&mut self, op: &Op, span_w: i64, del: impl Fn(i64) -> (Vec<i64>, Vec<i64>)) -> Concrete {
        match op {
            Op::Snap { bids, asks } => self.snap(bids, asks),
            Op::Msg { small, jump, lv, bad } => {
                self.mid += small + if self.jumps { jump * self.l } else { 0 };
                let span = span_w * self.w;
                let mut v: Vec<(usize, i64, i64)> = lv
                    .iter()
                    .map(|&(ask, r, q)| {
                        let o = self.off(r, span);
                        if ask {
                            (1, (self.mid + o) * TICK, q)
                        } else {
                            (0, (self.mid - o) * TICK, q)
                        }
                    })
                    .collect();
                let (db, da) = del(self.mid);
                v.extend(db.into_iter().map(|p| (0, p, 0)));
                v.extend(da.into_iter().map(|p| (1, p, 0)));
                v.sort_by_key(|l| l.0);
                if let Some((at, k)) = bad {
                    let n = v.len();
                    let e = &mut v[at % n];
                    match k {
                        0 => e.1 += 5,
                        1 => e.1 = 0,
                        _ => e.2 = -1,
                    }
                }
                let prev = self.id;
                self.id += 1;
                Concrete::Msg {
                    prev,
                    last: self.id,
                    lv: v,
                }
            }
        }
    }
}
