//! Dimensioned hdrhistograms with cardinality control (README "直方图"), `Quantiles`.

use std::collections::HashMap;
use std::hash::Hash;

use hdrhistogram::Histogram;
use serde::Serialize;

/// Minimum sample count for p99 / p99.9 (latency-budget §5: `n < 1,000` no p99, `n < 10,000` no
/// p99.9).
pub const P99_MIN_N: u64 = 1_000;
pub const P999_MIN_N: u64 = 10_000;

/// Histogram parameters (`TelemetryConfig`: 3 significant digits, 1 ns – 10 s).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct HistParams {
    pub sig_digits: u8,
    pub min_ns: u64,
    pub max_ns: u64,
}

impl HistParams {
    /// A fresh, fixed-range histogram (never resizes). Invalid parameters fall back to the
    /// README defaults (3 digits, 1 ns – 10 s).
    pub fn new_hist(&self) -> Histogram<u64> {
        let min = self.min_ns.max(1);
        Histogram::new_with_bounds(min, self.max_ns.max(2 * min), self.sig_digits.min(5))
            .or_else(|_| Histogram::new_with_bounds(1, 10_000_000_000, 3))
            .expect("constant histogram bounds are valid")
    }
}

/// p50/p90/p99/p99.9/max/count of one series (README "WindowReport"). p99 needs `count ≥ 1,000`
/// and p99.9 `count ≥ 10,000` (latency-budget §5); `max` is the exact maximum.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Quantiles {
    pub count: u64,
    pub p50: i64,
    pub p90: i64,
    pub p99: Option<i64>,
    pub p999: Option<i64>,
    pub max: i64,
}

impl Quantiles {
    /// Quantiles of `h` (non-empty) whose exact maximum is `max`.
    pub fn of(h: &Histogram<u64>, max: i64) -> Quantiles {
        let n = h.len();
        let q = |p: f64| (h.value_at_quantile(p).min(i64::MAX as u64) as i64).min(max);
        Quantiles {
            count: n,
            p50: q(0.5),
            p90: q(0.9),
            p99: (n >= P99_MIN_N).then(|| q(0.99)),
            p999: (n >= P999_MIN_N).then(|| q(0.999)),
            max,
        }
    }
}

/// One histogram with its exact maximum.
#[derive(Clone, Debug)]
pub struct Acc {
    pub h: Histogram<u64>,
    pub max: i64,
}

impl Acc {
    pub fn new(p: &HistParams) -> Acc {
        Acc {
            h: p.new_hist(),
            max: i64::MIN,
        }
    }

    /// Records a value ≥ 0 (values above the range saturate at the top).
    pub fn record(&mut self, v: i64) {
        self.h.saturating_record(v.max(0) as u64);
        self.max = self.max.max(v);
    }

    pub fn quantiles(&self) -> Option<Quantiles> {
        (!self.h.is_empty()).then(|| Quantiles::of(&self.h, self.max))
    }

    pub fn reset(&mut self) {
        self.h.reset();
        self.max = i64::MIN;
    }
}

/// One series: the window histogram and the daily accumulation.
#[derive(Clone, Debug)]
pub struct Series<K> {
    pub key: K,
    pub win: Acc,
    pub day: Acc,
}

/// Bytes of one series (two histograms of `p`) — the `telemetry/series_bytes` quantity.
pub fn series_bytes(p: &HistParams) -> usize {
    let h = p.new_hist();
    size_of::<Series<[u8; 16]>>() + 2 * h.distinct_values() * size_of::<u64>()
}

/// Series keyed by `K`, at most `max_series` regular keys; later new keys go to `overflow(key)`.
#[derive(Clone, Debug)]
pub struct SeriesSet<K> {
    params: HistParams,
    max_series: u32,
    index: HashMap<K, usize>,
    series: Vec<Series<K>>,
    /// Regular (non-overflow) series created so far.
    regular: usize,
}

impl<K: Copy + Eq + Hash + Ord> SeriesSet<K> {
    pub fn new(params: HistParams, max_series: u32) -> SeriesSet<K> {
        SeriesSet {
            params,
            max_series,
            index: HashMap::new(),
            series: Vec::new(),
            regular: 0,
        }
    }

    /// Records `v` under `key`; when `key` is new and `max_series` regular series exist already,
    /// records under `overflow(&key)` instead and returns `true`.
    pub fn record(&mut self, key: K, overflow: impl FnOnce(&K) -> K, v: i64) -> bool {
        let (k, spilled) = if self.index.contains_key(&key) || self.regular < self.max_series as usize {
            (key, false)
        } else {
            (overflow(&key), true)
        };
        let i = match self.index.get(&k) {
            Some(&i) => i,
            None => {
                if !spilled {
                    self.regular += 1;
                }
                self.series.push(Series {
                    key: k,
                    win: Acc::new(&self.params),
                    day: Acc::new(&self.params),
                });
                self.index.insert(k, self.series.len() - 1);
                self.series.len() - 1
            }
        };
        let s = &mut self.series[i];
        s.win.record(v);
        s.day.record(v);
        spilled
    }

    /// Window quantiles of every non-empty series (sorted by key), then clears the windows.
    pub fn take_window(&mut self) -> Vec<(K, Quantiles)> {
        let mut out: Vec<(K, Quantiles)> = self
            .series
            .iter()
            .filter_map(|s| s.win.quantiles().map(|q| (s.key, q)))
            .collect();
        out.sort_by_key(|a| a.0);
        for s in &mut self.series {
            s.win.reset();
        }
        out
    }

    /// Daily quantiles of every non-empty series (sorted by key), then clears the days.
    pub fn take_day(&mut self) -> Vec<(K, Quantiles)> {
        let mut out: Vec<(K, Quantiles)> = self
            .series
            .iter()
            .filter_map(|s| s.day.quantiles().map(|q| (s.key, q)))
            .collect();
        out.sort_by_key(|a| a.0);
        for s in &mut self.series {
            s.day.reset();
        }
        out
    }
}
