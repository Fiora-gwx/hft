//! `HistBench`: the histogram benchmark driver outside Criterion (benchmarks README "实现说明"):
//! warm-up, then one TSC read before and after every iteration recorded into a preallocated
//! hdrhistogram (3 significant digits, 1 ns – 10 s), with `CountingAlloc` counting this thread's
//! allocations after the warm-up. Reference mode pins the thread to an isolated core; smoke mode
//! (shared CI runners) does not pin.

use hdrhistogram::Histogram;
use hft_rt::{CalibConfig, Tsc, TscClock, pin_current_thread, tsc_hz_nominal, tsc_ordered};

use crate::alloc::CountingAlloc;

/// Warm-up iterations of smoke mode (benchmarks README).
pub const SMOKE_WARMUP: u64 = 1_000;
/// Warm-up iterations of reference mode (≥ 100,000; benchmarks README).
pub const REFERENCE_WARMUP: u64 = 100_000;
/// Measured iterations of smoke mode (benchmarks README "CI 冒烟").
pub const SMOKE_ITERS: u64 = 10_000;

/// Run mode.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BenchMode {
    /// Shared runner: no pinning, short warm-up; numbers are reported only.
    Smoke,
    /// Reference machine: pinned to `cpu`.
    Reference { cpu: u16 },
}

/// Driver configuration.
#[derive(Clone, Debug)]
pub struct HistBench {
    pub name: String,
    pub mode: BenchMode,
    pub warmup: u64,
    pub iters: u64,
    /// Counter frequency in milli-Hz; `None` = nominal (`tsc_hz_nominal`) or one calibration.
    pub hz_milli: Option<u64>,
}

/// Result of one run (fields of the benchmarks README output format that the driver owns).
#[derive(Clone, Debug)]
pub struct BenchOutput {
    pub name: String,
    pub mode: BenchMode,
    pub n: u64,
    pub warmup: u64,
    pub p50_ns: u64,
    pub p90_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub max_ns: u64,
    /// Allocations of the benchmark thread after the warm-up (`CountingAlloc` must be the global
    /// allocator of the binary; otherwise 0).
    pub allocs_after_warmup: u64,
    /// Core the thread was pinned to (`None` in smoke mode).
    pub pinned: Option<u16>,
    pub hist: Histogram<u64>,
}

impl HistBench {
    /// Smoke mode: warm-up 1,000, 10,000 iterations, no pinning.
    pub fn smoke(name: &str) -> HistBench {
        HistBench {
            name: name.to_string(),
            mode: BenchMode::Smoke,
            warmup: SMOKE_WARMUP,
            iters: SMOKE_ITERS,
            hz_milli: None,
        }
    }

    /// Reference mode on isolated core `cpu`: warm-up 100,000.
    pub fn reference(name: &str, cpu: u16, iters: u64) -> HistBench {
        HistBench {
            name: name.to_string(),
            mode: BenchMode::Reference { cpu },
            warmup: REFERENCE_WARMUP,
            iters,
            hz_milli: None,
        }
    }

    pub fn iters(mut self, n: u64) -> HistBench {
        self.iters = n;
        self
    }

    pub fn warmup(mut self, n: u64) -> HistBench {
        self.warmup = n;
        self
    }

    pub fn hz_milli(mut self, hz_milli: u64) -> HistBench {
        self.hz_milli = Some(hz_milli);
        self
    }

    fn resolve_hz_milli(&self) -> u64 {
        if let Some(h) = self.hz_milli.filter(|&h| h != 0) {
            return h;
        }
        if let Some(hz) = tsc_hz_nominal() {
            return hz.saturating_mul(1000);
        }
        let cfg = CalibConfig {
            rounds: 1,
            ..CalibConfig::default()
        };
        TscClock::calibrate(&cfg).map_or(1_000_000_000_000, |c| c.hz_milli)
    }

    /// Runs `f` `warmup + iters` times; the last `iters` are timed. Returns `Err` only when
    /// pinning fails in reference mode.
    pub fn run(&self, mut f: impl FnMut()) -> Result<BenchOutput, String> {
        let pinned = match self.mode {
            BenchMode::Smoke => None,
            BenchMode::Reference { cpu } => {
                pin_current_thread(cpu).map_err(|e| format!("pin to cpu {cpu}: {e:?}"))?;
                Some(cpu)
            }
        };
        let clock = TscClock::from_hz_milli(self.resolve_hz_milli(), Default::default());
        let mut hist = Histogram::<u64>::new_with_bounds(1, 10_000_000_000, 3)
            .map_err(|e| format!("histogram: {e:?}"))?;
        for _ in 0..self.warmup {
            f();
        }
        let scope = CountingAlloc::scope();
        for _ in 0..self.iters {
            let t0: Tsc = tsc_ordered();
            f();
            let t1: Tsc = tsc_ordered();
            hist.saturating_record(clock.delta_ns(t0, t1));
        }
        let allocs_after_warmup = scope.allocs();
        let q = |p: f64| hist.value_at_quantile(p);
        Ok(BenchOutput {
            name: self.name.clone(),
            mode: self.mode,
            n: hist.len(),
            warmup: self.warmup,
            p50_ns: q(0.5),
            p90_ns: q(0.9),
            p99_ns: q(0.99),
            p999_ns: q(0.999),
            max_ns: hist.max(),
            allocs_after_warmup,
            pinned,
            hist,
        })
    }
}
