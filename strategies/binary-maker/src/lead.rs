//! Rule 15: the lead quote (README "规则" 15; research: Python reference `sim2.py`
//! `perp_triggers`, `Strat.on_lead`, `Strat.on_perp_against`). A move of the perpetual reference's
//! mid over a short look-back places a post-only bid one cent inside the spread on the token it
//! favours; an adverse move cancels that token's lead bid. 1e-9 fixed point throughout.

use hft_strategy_sdk::prelude::OrderRef;

/// Perpetual mid updates kept (≥ 3 s at 2 000 updates/s).
pub const RING: usize = 8_192;

/// The perpetual's 2 × mid per update in engine time order.
#[derive(Clone, Debug)]
pub struct PerpRing {
    t: Box<[i64; RING]>,
    v: Box<[i64; RING]>,
    head: usize,
    n: usize,
}

impl Default for PerpRing {
    fn default() -> PerpRing {
        PerpRing {
            t: Box::new([0; RING]),
            v: Box::new([0; RING]),
            head: 0,
            n: 0,
        }
    }
}

impl PerpRing {
    /// One update at engine time `t` (non-decreasing) with 2 × mid `mid_x2` (> 0).
    pub fn push(&mut self, t: i64, mid_x2: i64) {
        if let (Some(a), Some(b)) = (self.t.get_mut(self.head), self.v.get_mut(self.head)) {
            *a = t;
            *b = mid_x2;
        }
        self.head = (self.head + 1) % RING;
        self.n = (self.n + 1).min(RING);
    }
    /// Entry `k` from the oldest (k < n).
    fn get(&self, k: usize) -> Option<(i64, i64)> {
        let i = (self.head + RING - self.n + k) % RING;
        Some((*self.t.get(i)?, *self.v.get(i)?))
    }
    /// The 2 × mid of the latest update at or before `t`; `None` when every kept update is later.
    pub fn at_or_before(&self, t: i64) -> Option<i64> {
        // Number of entries with time ≤ t (times are non-decreasing).
        let (mut lo, mut hi) = (0usize, self.n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.get(mid).is_some_and(|(x, _)| x <= t) {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo.checked_sub(1).and_then(|k| self.get(k)).map(|(_, v)| v)
    }
}

/// Relative change (1e-9) of `now` against `base`; 0 when `base` ≤ 0.
pub fn ret(now: i64, base: i64) -> i64 {
    if base <= 0 {
        return 0;
    }
    ((i128::from(now) - i128::from(base)) * 1_000_000_000 / i128::from(base)) as i64
}

/// A move of at least `x` (1e-9) over the look-back with its own cool-down per direction.
#[derive(Clone, Copy, Debug)]
pub struct Trigger {
    /// Engine time of the last trigger, up (index 0) and down (index 1).
    pub last: [i64; 2],
}

impl Default for Trigger {
    fn default() -> Trigger {
        Trigger {
            last: [i64::MIN / 2; 2],
        }
    }
}

impl Trigger {
    /// The direction (+1 / −1) when `|r| ≥ x` and the same direction last fired at least
    /// `cool_ns` ago (the trigger is recorded); `None` otherwise.
    pub fn fire(&mut self, r: i64, x: i64, t: i64, cool_ns: i64) -> Option<i64> {
        if x <= 0 || r.abs() < x {
            return None;
        }
        let k = usize::from(r < 0);
        let last = self.last.get_mut(k)?;
        if t.saturating_sub(*last) < cool_ns {
            return None;
        }
        *last = t;
        Some(if r > 0 { 1 } else { -1 })
    }
}

/// The lead order of one slot leg: the place handle until its outcome, then its order ref.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LeadLeg {
    pub handle: Option<u16>,
    pub r: Option<OrderRef>,
    /// Engine time of the place.
    pub placed: i64,
}

/// Rule 15 state; not in the snapshot.
#[derive(Clone, Debug, Default)]
pub struct LeadState {
    pub ring: PerpRing,
    /// The lead trigger and the perp-cancel trigger; both restart with each window.
    pub lead: Trigger,
    pub pc: Trigger,
    pub win: i64,
    /// Per slot leg (quote order).
    pub legs: Vec<LeadLeg>,
    /// Per slot leg: rule-14 entries are blocked until this engine time (perp cancel).
    pub block: Vec<i64>,
}

impl LeadState {
    /// Window `start`: the cool-downs restart (the research computes them per window).
    pub fn window(&mut self, start: i64) {
        if self.win != start {
            self.win = start;
            self.lead = Trigger::default();
            self.pc = Trigger::default();
        }
    }
}
