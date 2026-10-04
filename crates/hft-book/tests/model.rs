//! `BTreeMap` reference book (BOOK-056, BOOK-060): same rules as README "实现说明" written
//! the obvious way — every visible level in one map per side, window = `[base, base + L)`,
//! the overflow area = visible levels outside the window, capped at `cap` by evicting the level
//! farthest from the window centre `base + W` (equal distance → the worse price).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(dead_code)]

use std::collections::BTreeMap;

use hft_book::{BookParams, Outcome};

pub const UNINIT: u8 = 0;
pub const LIVE: u8 = 1;
pub const STALE: u8 = 2;

pub const BID: usize = 0;
pub const ASK: usize = 1;

#[derive(Clone, Debug)]
pub struct Model {
    pub tick: i64,
    pub w: i64,
    pub l: i64,
    pub cap: usize,
    pub tol: u16,
    pub base: i64,
    pub state: u8,
    pub reason: u8,
    pub origin: u8,
    pub last_id: u64,
    pub crossed_run: u16,
    pub truncated: u32,
    /// Visible levels: tick index → qty.
    pub sides: [BTreeMap<i64, i64>; 2],
}

impl Model {
    pub fn new(p: &BookParams) -> Model {
        let w = i64::from(p.window);
        Model {
            tick: p.tick,
            w,
            l: (2 * w + 63) / 64 * 64,
            cap: usize::from(p.ovf_cap),
            tol: p.crossed_tolerance,
            base: 0,
            state: UNINIT,
            reason: 0,
            origin: 0,
            last_id: 0,
            crossed_run: 0,
            truncated: 0,
            sides: [BTreeMap::new(), BTreeMap::new()],
        }
    }

    fn in_win(&self, i: i64) -> bool {
        i >= self.base && i < self.base + self.l
    }

    pub fn best(&self, s: usize) -> Option<(i64, i64)> {
        let m = &self.sides[s];
        let e = if s == BID {
            m.last_key_value()
        } else {
            m.first_key_value()
        };
        e.map(|(i, q)| (*i, *q))
    }

    /// Visible levels as (px, qty), best first.
    pub fn levels(&self, s: usize) -> Vec<(i64, i64)> {
        let it = self.sides[s].iter().map(|(i, q)| (i * self.tick, *q));
        if s == BID {
            it.rev().collect()
        } else {
            it.collect()
        }
    }

    fn worse(&self, s: usize, a: i64, b: i64) -> bool {
        let c = self.base + self.w;
        let (da, db) = (a.abs_diff(c), b.abs_diff(c));
        da > db || (da == db && if s == BID { a < b } else { a > b })
    }

    /// Evicts the farthest out-of-window levels until at most `cap` remain; returns evictions.
    fn enforce(&mut self, s: usize) -> u32 {
        let (lo, hi) = (self.base, self.base + self.l);
        let out = self.sides[s].range(..lo).count() + self.sides[s].range(hi..).count();
        let mut n = 0;
        for _ in self.cap..out {
            // The farthest outside level is the lowest one below or the highest one above.
            let a = self.sides[s].range(..lo).next().map(|(i, _)| *i);
            let b = self.sides[s].range(hi..).next_back().map(|(i, _)| *i);
            let far = match (a, b) {
                (Some(a), Some(b)) => {
                    if self.worse(s, a, b) {
                        a
                    } else {
                        b
                    }
                }
                (a, b) => a.or(b).unwrap_or(0),
            };
            self.sides[s].remove(&far);
            self.truncated += 1;
            n += 1;
        }
        n
    }

    fn put(&mut self, s: usize, i: i64, q: i64) -> bool {
        if q == 0 {
            self.sides[s].remove(&i);
            return false;
        }
        self.sides[s].insert(i, q);
        !self.in_win(i) && self.enforce(s) > 0
    }

    fn go_stale(&mut self, r: u8) -> Outcome {
        let was = self.state == LIVE;
        self.state = STALE;
        self.reason = r;
        if was { Outcome::WENT_STALE } else { Outcome::NONE }
    }

    fn tops(&self) -> [Option<(i64, i64)>; 2] {
        [self.best(BID), self.best(ASK)]
    }

    fn top_bits(&self, before: [Option<(i64, i64)>; 2]) -> Outcome {
        let after = self.tops();
        let mut o = Outcome::NONE;
        if before[BID] != after[BID] {
            o |= Outcome::BID_TOP;
        }
        if before[ASK] != after[ASK] {
            o |= Outcome::ASK_TOP;
        }
        o
    }

    fn recenter(&mut self) -> Outcome {
        let (bb, ba) = (self.best(BID), self.best(ASK));
        let m = match (bb, ba) {
            (Some(b), Some(a)) => (b.0 + a.0).div_euclid(2),
            (Some(b), None) => b.0,
            (None, Some(a)) => a.0,
            (None, None) => return Outcome::NONE,
        };
        let rel = m - self.base;
        let band = rel < self.w / 2 || rel >= 3 * self.w / 2;
        let ovf = |b: Option<(i64, i64)>| b.is_some_and(|(i, _)| !self.in_win(i));
        if !(band || ovf(bb) || ovf(ba)) || m - self.w == self.base {
            return Outcome::NONE;
        }
        self.base = m - self.w;
        let t = self.enforce(BID) + self.enforce(ASK);
        if t > 0 {
            Outcome::RECENTERED | Outcome::TRUNCATED
        } else {
            Outcome::RECENTERED
        }
    }

    /// One whole message (`apply_batch` semantics); levels are (side, px, qty).
    pub fn apply_msg(&mut self, prev: u64, last: u64, lv: &[(usize, i64, i64)]) -> Outcome {
        if self.origin == 3 {
            return Outcome::INVALID;
        }
        if self.state != LIVE {
            return Outcome::NOT_LIVE;
        }
        if last <= self.last_id {
            return Outcome::DUPLICATE;
        }
        if prev != self.last_id {
            return Outcome::GAP | self.go_stale(1);
        }
        if !lv.iter().all(|&(_, p, q)| p > 0 && q >= 0 && p % self.tick == 0) {
            return Outcome::INVALID | self.go_stale(5);
        }
        let before = self.tops();
        let mut out = Outcome::APPLIED;
        for &(s, p, q) in lv {
            if self.put(s, p / self.tick, q) {
                out |= Outcome::TRUNCATED;
            }
        }
        self.last_id = last;
        match (self.best(BID), self.best(ASK)) {
            (Some(b), Some(a)) if b.0 >= a.0 => {
                self.crossed_run += 1;
                out |= Outcome::CROSSED;
                if self.crossed_run > self.tol {
                    out |= self.go_stale(2);
                }
            }
            _ => self.crossed_run = 0,
        }
        out |= self.recenter();
        out | self.top_bits(before)
    }

    /// Exchange (1) or TopN (3) snapshot; bids and asks as (px, qty).
    pub fn apply_snapshot(
        &mut self,
        origin: u8,
        id: u64,
        bids: &[(i64, i64)],
        asks: &[(i64, i64)],
    ) -> Outcome {
        if origin == 3 && id != 0 && id <= self.last_id {
            return Outcome::DUPLICATE;
        }
        let live: Vec<(usize, i64, i64)> = bids
            .iter()
            .map(|&(p, q)| (BID, p, q))
            .chain(asks.iter().map(|&(p, q)| (ASK, p, q)))
            .filter(|l| l.2 != 0)
            .collect();
        if !live
            .iter()
            .all(|&(_, p, q)| p > 0 && q >= 0 && p % self.tick == 0)
        {
            return Outcome::INVALID | self.go_stale(5);
        }
        let bb = live.iter().filter(|l| l.0 == BID).map(|l| l.1 / self.tick).max();
        let ba = live.iter().filter(|l| l.0 == ASK).map(|l| l.1 / self.tick).min();
        if let (Some(b), Some(a)) = (bb, ba)
            && b >= a
        {
            return Outcome::INVALID | self.go_stale(2);
        }
        self.sides = [BTreeMap::new(), BTreeMap::new()];
        let m = match (bb, ba) {
            (Some(b), Some(a)) => Some((b + a).div_euclid(2)),
            (b, a) => b.or(a),
        };
        if let Some(m) = m {
            let rel = m - self.base;
            if !(origin == 3 && rel >= self.w / 2 && rel < 3 * self.w / 2) {
                self.base = m - self.w;
            }
        }
        let mut out = Outcome::APPLIED;
        for (s, p, q) in live {
            if self.put(s, p / self.tick, q) {
                out |= Outcome::TRUNCATED;
            }
        }
        self.last_id = id;
        self.origin = origin;
        self.crossed_run = 0;
        self.reason = 0;
        if self.state != LIVE {
            self.state = LIVE;
            out |= Outcome::WENT_LIVE;
        }
        out
    }

    /// Prices of visible bids at or above tick index `mid` and asks at or below it.
    pub fn crossing(&self, mid: i64) -> (Vec<i64>, Vec<i64>) {
        let b = self.sides[BID].range(mid..).map(|(i, _)| i * self.tick).collect();
        let a = self.sides[ASK]
            .range(..=mid)
            .map(|(i, _)| i * self.tick)
            .collect();
        (b, a)
    }
}
