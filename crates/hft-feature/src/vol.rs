//! `RealizedVol` (README "节点库"): integer realized volatility of a mid, one return per UTC
//! second, a ring of the last `cap` returns and the running sum of their squares. Rates are 1e-9.

use crate::fixed::{ONE, isqrt};
use crate::{ColumnDesc, Events, Feature, FeatureRow, Unit};

#[derive(Debug)]
pub struct RealizedVol {
    ring: Box<[i64]>,
    n: usize,
    head: usize,
    sum_sq: u128,
    /// Twice the mid at the last sample (bid + ask, raw price units); 0 = none.
    last_mid_x2: i128,
    /// UTC second of the last sample.
    last_sec: i64,
}

impl RealizedVol {
    /// Room for `cap` returns (command path; the only allocation).
    pub fn new(cap: usize) -> RealizedVol {
        RealizedVol {
            ring: vec![0; cap.max(1)].into_boxed_slice(),
            n: 0,
            head: 0,
            sum_sq: 0,
            last_mid_x2: 0,
            last_sec: i64::MIN,
        }
    }

    pub fn capacity(&self) -> usize {
        self.ring.len()
    }

    pub fn samples(&self) -> usize {
        self.n
    }

    /// Relative change of `mid_x2` from the last sample, as a `Rate` (clamped to ±1); 0 without one.
    pub fn ret_from_sample(&self, mid_x2: i128) -> i64 {
        if self.last_mid_x2 <= 0 || mid_x2 <= 0 {
            return 0;
        }
        let r = (mid_x2 - self.last_mid_x2) * i128::from(ONE) / self.last_mid_x2;
        r.clamp(-i128::from(ONE), i128::from(ONE)) as i64
    }

    /// Handler: a quote with 2 × mid `mid_x2` at UTC second `sec`. The first one of a later second
    /// records the return from the previous sample (seconds without quotes add no sample).
    pub fn on_mid(&mut self, mid_x2: i128, sec: i64) {
        if mid_x2 <= 0 || sec <= self.last_sec {
            return;
        }
        if self.last_mid_x2 > 0 {
            let r = self.ret_from_sample(mid_x2);
            let cap = self.ring.len();
            if self.n == cap {
                let old = self.ring.get(self.head).copied().unwrap_or(0);
                self.sum_sq = self.sum_sq.saturating_sub(sq(old));
            } else {
                self.n += 1;
            }
            if let Some(slot) = self.ring.get_mut(self.head) {
                *slot = r;
            }
            self.sum_sq = self.sum_sq.saturating_add(sq(r));
            self.head = (self.head + 1) % cap;
        }
        self.last_mid_x2 = mid_x2;
        self.last_sec = sec;
    }

    /// Eval: sum and sum of absolute values of the last `k` returns (fewer while the ring holds
    /// fewer).
    pub fn recent(&self, k: usize) -> (i64, i64) {
        let cap = self.ring.len();
        let (mut sum, mut abs) = (0i64, 0i64);
        for back in 1..=k.min(self.n) {
            let r = self
                .ring
                .get((self.head + cap - back) % cap)
                .copied()
                .unwrap_or(0);
            sum = sum.saturating_add(r);
            abs = abs.saturating_add(r.saturating_abs());
        }
        (sum, abs)
    }

    /// Eval: per-second standard deviation of the returns (`Rate`); `None` below `min_samples`.
    pub fn sigma(&self, min_samples: usize) -> Option<i64> {
        if self.n == 0 || self.n < min_samples {
            return None;
        }
        Some(isqrt(self.sum_sq / self.n as u128) as i64)
    }
}

fn sq(r: i64) -> u128 {
    let a = u128::from(r.unsigned_abs());
    a * a
}

impl Feature for RealizedVol {
    /// (2 × mid, UTC second).
    type Input<'a> = (i128, i64);
    const EVENTS: Events = Events::BBO;
    const COLUMNS: &'static [ColumnDesc] = &[
        ColumnDesc {
            name: "sigma",
            unit: Unit::Ratio,
            exp: -9,
        },
        ColumnDesc {
            name: "samples",
            unit: Unit::Count,
            exp: 0,
        },
    ];
    fn on_event(&mut self, x: (i128, i64)) {
        self.on_mid(x.0, x.1);
    }
    fn export(&self, row: &mut FeatureRow<'_>) {
        row.put_opt(self.sigma(1));
        row.put(self.n as i64);
    }
}
