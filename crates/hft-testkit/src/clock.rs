//! Manual clocks for components that read the TSC (feed/gw runtimes, housekeeping, sim-gw
//! delivery times). The sequencer and strategies take time from events and need no stand-in.

use std::sync::atomic::{AtomicU64, Ordering};

use hft_rt::{CalibPair, ClockReads, TscClock, TscUtc};
use hft_types::Tsc;

/// A counter at a fixed `tsc_hz`, advanced only by the test. Implements hft-rt's `ClockReads`
/// (the TSC reading trait, also for `&ManualTsc`): `mono_raw_ns`/`realtime_ns` are the counter
/// converted to ns, `sleep_ms` advances the counter.
#[derive(Debug)]
pub struct ManualTsc {
    tsc_hz: u64,
    // Relaxed throughout: the value is the only shared state; readers need no other memory.
    now: AtomicU64,
    /// Number of reads (`now` and the `ClockReads` methods): `StrategyHarness` asserts a strategy
    /// never reads it.
    reads: AtomicU64,
}

impl ManualTsc {
    /// Counter at `start` ticks, running at `tsc_hz` ticks per second (`tsc_hz > 0`).
    pub fn new(tsc_hz: u64, start: u64) -> Self {
        assert!(tsc_hz > 0, "ManualTsc: tsc_hz must be > 0");
        ManualTsc {
            tsc_hz,
            now: AtomicU64::new(start),
            reads: AtomicU64::new(0),
        }
    }

    pub fn tsc_hz(&self) -> u64 {
        self.tsc_hz
    }

    /// Current reading (counted as a read).
    pub fn now(&self) -> Tsc {
        self.reads.fetch_add(1, Ordering::Relaxed);
        Tsc(self.now.load(Ordering::Relaxed))
    }

    /// Reads so far.
    pub fn reads(&self) -> u64 {
        self.reads.load(Ordering::Relaxed)
    }

    /// Advances by `⌊ns × tsc_hz / 10^9⌋` ticks (u128 intermediate, saturating).
    pub fn advance_ns(&self, ns: u64) {
        let ticks = self.ns_to_ticks(ns);
        let _ = self.now.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |t| {
            Some(t.saturating_add(ticks))
        });
    }

    /// Sets the reading (may move backwards).
    pub fn set(&self, tsc: u64) {
        self.now.store(tsc, Ordering::Relaxed);
    }

    /// `⌊ns × tsc_hz / 10^9⌋`, saturating.
    pub fn ns_to_ticks(&self, ns: u64) -> u64 {
        (ns as u128 * self.tsc_hz as u128 / 1_000_000_000).min(u64::MAX as u128) as u64
    }

    /// `⌊ticks × 10^9 / tsc_hz⌋`, saturating.
    pub fn ticks_to_ns(&self, ticks: u64) -> u64 {
        (ticks as u128 * 1_000_000_000 / self.tsc_hz as u128).min(u64::MAX as u128) as u64
    }

    /// A `TscClock` at exactly this counter's frequency with the given calibration base.
    pub fn tsc_clock(&self, base: CalibPair) -> TscClock {
        TscClock::from_hz_milli(self.tsc_hz.saturating_mul(1000), base)
    }

    fn now_ns(&self) -> i64 {
        self.ticks_to_ns(self.now().0).min(i64::MAX as u64) as i64
    }
}

impl ClockReads for ManualTsc {
    fn tsc(&mut self) -> Tsc {
        self.now()
    }
    fn mono_raw_ns(&mut self) -> i64 {
        self.now_ns()
    }
    fn realtime_ns(&mut self) -> i64 {
        self.now_ns()
    }
    fn sleep_ms(&mut self, ms: u32) {
        self.advance_ns(ms as u64 * 1_000_000);
    }
}

impl ClockReads for &ManualTsc {
    fn tsc(&mut self) -> Tsc {
        self.now()
    }
    fn mono_raw_ns(&mut self) -> i64 {
        self.now_ns()
    }
    fn realtime_ns(&mut self) -> i64 {
        self.now_ns()
    }
    fn sleep_ms(&mut self, ms: u32) {
        self.advance_ns(ms as u64 * 1_000_000);
    }
}

/// TSC → UTC through one fixed calibration pair, for `ClockStatus` and segment-header tests.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ManualUtc {
    pub pair_tsc: u64,
    pub pair_utc_ns: i64,
    pub tsc_hz: u64,
}

impl ManualUtc {
    /// Pair (`pair_tsc`, `pair_utc_ns`) at `tsc_hz` ticks per second (`tsc_hz > 0`).
    pub fn new(pair_tsc: u64, pair_utc_ns: i64, tsc_hz: u64) -> Self {
        assert!(tsc_hz > 0, "ManualUtc: tsc_hz must be > 0");
        ManualUtc {
            pair_tsc,
            pair_utc_ns,
            tsc_hz,
        }
    }

    /// `pair_utc_ns + (tsc − pair_tsc) × 10^9 / tsc_hz` (exact i128 arithmetic, truncation toward
    /// zero, saturating to i64).
    pub fn utc_ns(&self, tsc: u64) -> i64 {
        let d = tsc as i128 - self.pair_tsc as i128;
        let v = self.pair_utc_ns as i128 + d * 1_000_000_000 / self.tsc_hz as i128;
        v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
    }

    /// The same pair as hft-rt's `CalibPair` (`err_ns = 0`, no PHC).
    pub fn calib_pair(&self) -> CalibPair {
        CalibPair {
            tsc: Tsc(self.pair_tsc),
            realtime_ns: self.pair_utc_ns,
            phc_ns: 0,
            err_ns: 0,
        }
    }

    /// hft-rt's distributed conversion for this pair (mult/shift rounding as in `TscClock`).
    pub fn tsc_utc(&self) -> TscUtc {
        let pair = self.calib_pair();
        TscClock::from_hz_milli(self.tsc_hz.saturating_mul(1000), pair).tsc_utc(&pair)
    }
}
