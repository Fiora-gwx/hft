//! Rule 16: the short-horizon head (README "规则" 16). A small boosted-tree model predicts the
//! change of the Up token's mid over the next 3 s from 26 features of the reference markets and
//! the Up book; the strategy bids inside the spread when the prediction for a token is high,
//! cancels its lead bids when it turns, and gates rule 15 with it. Research: Python reference
//! `sim4.py` `feat` / `sim5.py` (out of repo). Everything is integer: features, thresholds and
//! leaf values are 1e-6 (`SCALE`) of their unit (cents, bp, shares, seconds); the model tables
//! are generated (`head_model.rs`).

use hft_feature::ONE;

use crate::fcev;
use crate::head_model as hm;

/// Features of one evaluation.
pub const NF: usize = 26;
/// Fixed point of features, thresholds and leaf values.
pub const SCALE: i64 = 1_000_000;

/// Feature indices (the generated model uses the same order).
pub mod ix {
    pub const AGE: usize = 0;
    pub const MID: usize = 1;
    pub const SPREAD: usize = 2;
    pub const BID_Q: usize = 3;
    pub const ASK_Q: usize = 4;
    pub const OBI1: usize = 5;
    pub const PM1: usize = 6;
    pub const PM5: usize = 7;
    pub const PM30: usize = 8;
    pub const DROP30: usize = 9;
    pub const UP5: usize = 10;
    pub const SIG: usize = 11;
    pub const D_TOK: usize = 12;
    pub const DELTA: usize = 13;
    /// Perpetual moves over 100 ms, 300 ms, 1 s, 5 s, 30 s; then the same five of the spot.
    pub const PERP: usize = 14;
    pub const SPOT: usize = 19;
    pub const E300: usize = 24;
    pub const E1000: usize = 25;
}

/// Look-backs of the reference moves (ms).
pub const RET_MS: [i64; 5] = [100, 300, 1_000, 5_000, 30_000];
const MS: i64 = 1_000_000;

/// Values in engine-time order with a look-up of the latest one at or before a time. `gap` > 0
/// keeps at most one value per `gap` ns (long look-backs).
#[derive(Clone, Debug)]
pub struct Ring {
    t: Vec<i64>,
    v: Vec<i64>,
    head: usize,
    n: usize,
    gap: i64,
    last_t: i64,
}

impl Ring {
    pub fn new(cap: usize, gap: i64) -> Ring {
        let cap = cap.max(2);
        Ring {
            t: vec![0; cap],
            v: vec![0; cap],
            head: 0,
            n: 0,
            gap,
            last_t: i64::MIN / 2,
        }
    }
    pub fn clear(&mut self) {
        self.head = 0;
        self.n = 0;
        self.last_t = i64::MIN / 2;
    }
    pub fn len(&self) -> usize {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    /// One value at engine time `t` (non-decreasing).
    pub fn push(&mut self, t: i64, v: i64) {
        if self.gap > 0 && self.n > 0 && t.saturating_sub(self.last_t) < self.gap {
            return;
        }
        let cap = self.t.len();
        if let (Some(a), Some(b)) = (self.t.get_mut(self.head), self.v.get_mut(self.head)) {
            *a = t;
            *b = v;
        }
        self.head = (self.head + 1) % cap;
        self.n = (self.n + 1).min(cap);
        self.last_t = t;
    }
    /// Entry `k` from the oldest (k < n).
    pub fn get(&self, k: usize) -> Option<(i64, i64)> {
        let cap = self.t.len();
        let i = (self.head + cap - self.n + k) % cap;
        Some((*self.t.get(i)?, *self.v.get(i)?))
    }
    /// The latest value.
    pub fn last(&self) -> Option<i64> {
        self.n.checked_sub(1).and_then(|k| self.get(k)).map(|(_, v)| v)
    }
    /// Number of entries with time ≤ `t`.
    fn count_le(&self, t: i64) -> usize {
        let (mut lo, mut hi) = (0usize, self.n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.get(mid).is_some_and(|(x, _)| x <= t) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    /// The value of the latest entry at or before `t`.
    pub fn at_or_before(&self, t: i64) -> Option<i64> {
        self.count_le(t)
            .checked_sub(1)
            .and_then(|k| self.get(k))
            .map(|(_, v)| v)
    }
    /// (max, min) over the entries with time ≥ `t` and the one entry before them (the value in
    /// effect at `t`); `None` when empty.
    pub fn range_from(&self, t: i64) -> Option<(i64, i64)> {
        if self.n == 0 {
            return None;
        }
        // First entry with time ≥ t, then one earlier.
        let first = if t == i64::MIN { 0 } else { self.count_le(t - 1) };
        let from = first.saturating_sub(1);
        let (mut hi, mut lo) = (i64::MIN, i64::MAX);
        for k in from..self.n {
            if let Some((_, v)) = self.get(k) {
                hi = hi.max(v);
                lo = lo.min(v);
            }
        }
        (hi >= lo).then_some((hi, lo))
    }
}

/// Head state of one strategy instance; not in the snapshot (rebuilt from the events).
#[derive(Clone, Debug)]
pub struct HeadState {
    /// Reference 2 × mids: the perpetual and the spot, exact and thinned (30 s look-back).
    pub perp: Ring,
    pub perp_slow: Ring,
    pub spot: Ring,
    pub spot_slow: Ring,
    /// Changes of the current window's Up token 2 × mid (raw price units).
    pub mid: Ring,
    pub win: i64,
    /// Engine time and value (1e-6 cents, Up orientation) of the last evaluation.
    pub last_eval: i64,
    pub last: Option<i64>,
    /// Per slot leg: engine time of its last head bid.
    pub placed: Vec<i64>,
}

impl Default for HeadState {
    fn default() -> HeadState {
        HeadState {
            perp: Ring::new(8_192, 0),
            perp_slow: Ring::new(4_096, 10 * MS),
            spot: Ring::new(8_192, 0),
            spot_slow: Ring::new(4_096, 10 * MS),
            mid: Ring::new(4_096, 0),
            win: i64::MIN,
            last_eval: i64::MIN / 2,
            last: None,
            placed: Vec::new(),
        }
    }
}

/// The current window's Up book top (raw units of its inst).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Top {
    pub bid: i64,
    pub bid_q: i64,
    pub ask: i64,
    pub ask_q: i64,
    /// 10^|px_exp| and 10^|qty_exp| of the inst.
    pub unit_px: i64,
    pub unit_qty: i64,
}

fn muldiv(a: i64, b: i64, c: i64) -> i64 {
    if c == 0 {
        return 0;
    }
    (i128::from(a) * i128::from(b) / i128::from(c)).clamp(i128::from(i64::MIN / 4), i128::from(i64::MAX / 4))
        as i64
}

impl HeadState {
    /// A reference quote (2 × mid > 0) at engine time `t`.
    pub fn on_ref(&mut self, perp: bool, t: i64, mid_x2: i64) {
        if mid_x2 <= 0 {
            return;
        }
        if perp {
            self.perp.push(t, mid_x2);
            self.perp_slow.push(t, mid_x2);
        } else {
            self.spot.push(t, mid_x2);
            self.spot_slow.push(t, mid_x2);
        }
    }

    /// Window `start`: the Up mid history restarts.
    pub fn window(&mut self, start: i64) {
        if self.win != start {
            self.win = start;
            self.mid.clear();
            self.last = None;
        }
    }

    /// The Up token's best bid + best ask (raw) after a book event; kept when it changed.
    pub fn on_mid(&mut self, t: i64, mid_x2: i64) {
        if self.mid.last() != Some(mid_x2) {
            self.mid.push(t, mid_x2);
        }
    }

    /// Move (1e-6 bp) of a reference over `w_ms`, exact ring up to 5 s and the thinned one beyond.
    fn ref_ret(exact: &Ring, slow: &Ring, t: i64, w_ms: i64) -> Option<i64> {
        let now = exact.last()?;
        let ring = if w_ms > 5_000 { slow } else { exact };
        let base = ring.at_or_before(t - w_ms * MS)?;
        (base > 0).then(|| muldiv(now - base, 10_000 * SCALE, base))
    }

    /// The 26 features at engine time `t` (window age `age_ms`), Up orientation; `fc` = rule
    /// 14's `(dist_up, sq, remf, sigma)` (1e-9). `None` while a reference look-back or the book
    /// top is missing.
    pub fn features(&self, t: i64, age_ms: i64, top: Top, fc: (i64, i64, i64, i64)) -> Option<[i64; NF]> {
        if top.bid <= 0 || top.ask <= top.bid || top.unit_px <= 0 || top.unit_qty <= 0 {
            return None;
        }
        let mut x = [0i64; NF];
        // Cents (1e-6) of a 2 × price: × 100 / unit_px / 2.
        let c2 = |px_x2: i64| muldiv(px_x2, 50 * SCALE, top.unit_px);
        let mid_x2 = top.bid + top.ask;
        x[ix::AGE] = age_ms * (SCALE / 1_000);
        x[ix::MID] = c2(mid_x2);
        x[ix::SPREAD] = c2(2 * (top.ask - top.bid));
        x[ix::BID_Q] = muldiv(top.bid_q, SCALE, top.unit_qty);
        x[ix::ASK_Q] = muldiv(top.ask_q, SCALE, top.unit_qty);
        let qs = top.bid_q.saturating_add(top.ask_q);
        x[ix::OBI1] = if qs > 0 {
            muldiv(top.bid_q - top.ask_q, SCALE, qs)
        } else {
            0
        };
        for (k, s) in [(ix::PM1, 1i64), (ix::PM5, 5), (ix::PM30, 30)] {
            x[k] = self
                .mid
                .at_or_before(t - s * 1_000 * MS)
                .map_or(0, |m| c2(mid_x2 - m));
        }
        let hi = self
            .mid
            .range_from(t - 30_000 * MS)
            .map_or(mid_x2, |(h, _)| h.max(mid_x2));
        let lo = self
            .mid
            .range_from(t - 5_000 * MS)
            .map_or(mid_x2, |(_, l)| l.min(mid_x2));
        x[ix::DROP30] = c2(hi - mid_x2);
        x[ix::UP5] = c2(mid_x2 - lo);
        let (dist, sq, remf, sig) = fc;
        let one = i128::from(ONE);
        let den = (i128::from(sig).max(i128::from(fcev::SIG_FLOOR)) * i128::from(sq) / one).max(1);
        let d =
            (i128::from(dist) * one / den).clamp(-i128::from(fcev::D_CLIP), i128::from(fcev::D_CLIP)) as i64;
        let delta = fcev::delta(dist, sq, remf, sig);
        x[ix::SIG] = sig / 1_000;
        x[ix::D_TOK] = d / 1_000;
        x[ix::DELTA] = delta / 1_000;
        for (j, w) in RET_MS.iter().enumerate() {
            x[ix::PERP + j] = Self::ref_ret(&self.perp, &self.perp_slow, t, *w)?;
            x[ix::SPOT + j] = Self::ref_ret(&self.spot, &self.spot_slow, t, *w)?;
        }
        x[ix::E300] = muldiv(x[ix::DELTA], x[ix::PERP + 1], SCALE);
        x[ix::E1000] = muldiv(x[ix::DELTA], x[ix::PERP + 2], SCALE);
        Some(x)
    }
}

/// The model's prediction (1e-6 cents: change of the Up mid over the next 3 s) for features `x`.
pub fn predict(x: &[i64; NF]) -> i64 {
    let mut sum = 0i64;
    for t in 0..hm::N_TREES {
        let (n0, l0) = (usize::from(hm::NODE_OFF[t]), usize::from(hm::LEAF_OFF[t]));
        let mut node = 0i32;
        // A tree with one leaf has no nodes: its root is leaf 0.
        if hm::NODE_OFF[t + 1] == hm::NODE_OFF[t] {
            sum = sum.saturating_add(hm::LEAF.get(l0).copied().unwrap_or(0));
            continue;
        }
        let leaf = loop {
            let i = n0 + node as usize;
            let (Some(f), Some(thr), Some(l), Some(r)) =
                (hm::FEAT.get(i), hm::THR.get(i), hm::LEFT.get(i), hm::RIGHT.get(i))
            else {
                break 0usize;
            };
            let v = x.get(usize::from(*f)).copied().unwrap_or(0);
            let next = if v <= *thr { *l } else { *r };
            if next < 0 {
                break (!next) as usize;
            }
            node = i32::from(next);
        };
        sum = sum.saturating_add(hm::LEAF.get(l0 + leaf).copied().unwrap_or(0));
    }
    sum
}

/// Price (raw units) of a bid inside the spread for an expected rise `e` (raw price units):
/// max(bid + 1 cent, min(bid + `maxtick` cents, ⌊mid + e − `m`⌋ to a whole cent, ask − 1 cent)),
/// `m` in cents (1e-9) — rule 15's model price with the expected repricing given directly.
pub fn inside_px(bid: i64, ask: i64, cent: i64, e: i64, m: i64, maxtick: i64) -> i64 {
    let one = 1_000_000_000i128;
    let cent = i128::from(cent.max(1));
    let m_raw = i128::from(m) * cent / one;
    let cap2 = i128::from(bid) + i128::from(ask) + 2 * i128::from(e) - 2 * m_raw;
    let cap = cap2.div_euclid(2 * cent) * cent;
    let lo = i128::from(bid) + cent;
    let hi = (i128::from(bid) + i128::from(maxtick) * cent)
        .min(cap)
        .min(i128::from(ask) - cent);
    lo.max(hi)
        .clamp(i128::from(i64::MIN / 4), i128::from(i64::MAX / 4)) as i64
}
