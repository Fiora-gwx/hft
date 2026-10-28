//! The published UTC line: publisher rule, a simulated day on a slow counter, concurrent readers
//! (RT-169 – RT-171).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use hft_rt::{
    CalibConfig, CalibPair, ClockReads, DriftTracker, Tsc, TscClock, TscUtc, UtcClock, sample_pair_with,
};

const R: i64 = 1_791_072_000_000_000_000;
/// 1 GHz nominal (1 tick = 1 ns at the nominal rate).
const NOMINAL: u64 = 1_000_000_000_000;

fn pair(tsc: u64, realtime_ns: i64) -> CalibPair {
    CalibPair {
        tsc: Tsc(tsc),
        realtime_ns,
        phc_ns: 0,
        err_ns: 10,
    }
}

#[test]
fn rt_169_publisher_rule() {
    let boot = TscClock::from_hz_milli(NOMINAL, pair(0, R));
    // RT-169.1: the boot line before any update.
    let (r, mut p) = UtcClock::shared(&boot, 1_000);
    assert_eq!(r.line(), boot.tsc_utc(&boot.base), "RT-169.1");
    assert_eq!(r.utc_ns(Tsc(1_000)), R + 1_000, "RT-169.1");
    // RT-169.2: behind → the target line (forward step), measured rate used.
    let hz = 999_970_000_000;
    let l = p.update(&pair(1_000_000_000, R + 1_000_030_000), Some(hz));
    assert_eq!(
        l,
        TscClock::from_hz_milli(hz, boot.base).tsc_utc(&pair(1_000_000_000, R + 1_000_030_000)),
        "RT-169.2"
    );
    assert_eq!(r.line(), l, "RT-169.2");
    assert_eq!(r.utc_ns(Tsc(1_000_000_000)), R + 1_000_030_000, "RT-169.2");
    // RT-169.3: rate outside 1 000 ppm or None → boot rate.
    let mult_boot = boot.tsc_utc(&boot.base).mult;
    let l = p.update(
        &pair(2_000_000_000, R + 2_000_060_000),
        Some(NOMINAL * 1_002 / 1_000),
    );
    assert_eq!(l.mult, mult_boot, "RT-169.3");
    let l = p.update(&pair(3_000_000_000, R + 3_000_060_000), None);
    assert_eq!(l.mult, mult_boot, "RT-169.3");
    assert_eq!(l.base_utc_ns, R + 3_000_060_000, "RT-169.3");
    // RT-169.4: ahead by 100 µs → keeps its value at the pair, slowed by 100 ppm (100 µs over the
    // 1 s horizon), meets the target one horizon later; err grows by the lead.
    let c = l.utc_ns(Tsc(4_000_000_000));
    let l = p.update(&pair(4_000_000_000, c - 100_000), None);
    assert_eq!((l.base_tsc, l.base_utc_ns), (Tsc(4_000_000_000), c), "RT-169.4");
    assert_eq!(l.err_ns, 100_010, "RT-169.4");
    assert_eq!(l.mult, mult_boot - mult_boot / 10_000, "RT-169.4");
    let after = l.utc_ns(Tsc(5_000_000_000));
    assert!(
        (after - (c - 100_000 + 1_000_000_000)).abs() <= 1,
        "RT-169.4 {after}"
    );
    // RT-169.5: ahead by 10 ms → capped at 500 ppm (500 µs removed per second).
    let c = l.utc_ns(Tsc(5_000_000_000));
    let l = p.update(&pair(5_000_000_000, c - 10_000_000), None);
    assert_eq!(l.base_utc_ns, c, "RT-169.5");
    assert_eq!(l.mult, mult_boot - mult_boot * 500 / 1_000_000, "RT-169.5");
    assert_eq!(p.current(), l, "RT-169.5");
    // RT-169.6: a fixed clock is never republished.
    let f = UtcClock::fixed(boot.tsc_utc(&boot.base));
    assert_eq!(f.utc_ns(Tsc(7)), R + 7, "RT-169.6");
    // RT-169.7: the reader floor: an earlier t after a later one returns the previous result.
    assert_eq!(f.utc_ns(Tsc(5)), R + 7, "RT-169.7");
    let g = f.clone();
    assert_eq!(g.utc_ns(Tsc(9)), R + 9, "RT-169.7");
}

/// A host whose counter runs `ppm_slow` ppm slower than its nominal 1 GHz: CLOCK_MONOTONIC_RAW
/// is the counter at the nominal rate (as on Graviton), CLOCK_REALTIME is true time. Each read
/// advances true time by 20 ns.
struct SlowCounter {
    /// True nanoseconds since start.
    ns: i64,
    /// Counter ticks per true second (nominal 10^9) from `(seg_ns, seg_tsc)` on.
    ticks_per_s: u64,
    seg_ns: i64,
    seg_tsc: u64,
    /// Added to CLOCK_REALTIME (an NTP step).
    rt_step: i64,
}

impl SlowCounter {
    fn tsc_at(&self, ns: i64) -> Tsc {
        let d = (ns - self.seg_ns) as u128;
        Tsc(self.seg_tsc + (d * u128::from(self.ticks_per_s) / 1_000_000_000) as u64)
    }
    /// The counter's rate changes at the current instant (the counter stays continuous).
    fn set_rate(&mut self, ticks_per_s: u64) {
        self.seg_tsc = self.tsc_at(self.ns).0;
        self.seg_ns = self.ns;
        self.ticks_per_s = ticks_per_s;
    }
    fn truth(&self, ns: i64) -> i64 {
        R + ns + self.rt_step
    }
}

impl ClockReads for SlowCounter {
    fn tsc(&mut self) -> Tsc {
        self.ns += 20;
        self.tsc_at(self.ns)
    }
    fn mono_raw_ns(&mut self) -> i64 {
        self.ns += 20;
        self.tsc_at(self.ns).0 as i64
    }
    fn realtime_ns(&mut self) -> i64 {
        self.ns += 20;
        self.truth(self.ns)
    }
    fn sleep_ms(&mut self, ms: u32) {
        self.ns += i64::from(ms) * 1_000_000;
    }
}

#[test]
fn rt_170_simulated_day() {
    let mut src = SlowCounter {
        ns: 0,
        ticks_per_s: 999_970_000,
        seg_ns: 0,
        seg_tsc: 1_000_000,
        rt_step: 0,
    };
    let cfg = CalibConfig {
        rounds: 5,
        samples: 8,
        phc_samples: 10,
        cpuid_tolerance_ppm: 100,
        step_threshold_ns: 50_000,
    };
    let boot = TscClock::calibrate_with(&mut src, Some(NOMINAL), &cfg).unwrap();
    // Calibrated against MONOTONIC_RAW: the nominal rate, 30 ppm off the true one.
    assert_eq!(boot.hz_milli, NOMINAL, "RT-170 boot rate");
    let (reader, mut publisher) = UtcClock::shared(&boot, 1_000);
    let mut tracker = DriftTracker::new(cfg.step_threshold_ns);
    let start = src.ns;
    let (mut worst_first, mut worst_after, mut worst_slew, mut worst_steady) = (0i64, 0i64, 0i64, 0i64);
    let mut prev = i64::MIN;
    let mut ticks = 0u32;
    while src.ns - start < 86_400_000_000_000 {
        // Ten readings across the next second (before this tick's update for the first one).
        for k in 0..10 {
            let ns = src.ns + k * 100_000_000;
            let v = reader.utc_ns(src.tsc_at(ns));
            assert!(v >= prev, "RT-170 monotone at {ns}");
            prev = v;
            let err = (v - src.truth(ns)).abs();
            let since = ns - start;
            if ticks == 0 {
                worst_first = worst_first.max(err);
            } else if (64_800_000_000_000..64_810_000_000_000).contains(&since) {
                worst_slew = worst_slew.max(err);
            } else {
                worst_after = worst_after.max(err);
                if ticks >= 2 {
                    worst_steady = worst_steady.max(err);
                }
            }
        }
        src.sleep_ms(1_000);
        // 12 h: the counter's true rate moves by another 1 ppm.
        if src.ns - start >= 43_200_000_000_000 && src.ticks_per_s == 999_970_000 {
            src.set_rate(999_969_000);
        }
        // 18 h: CLOCK_REALTIME steps back 2 ms (the line slews, never steps back).
        if src.ns - start >= 64_800_000_000_000 && src.rt_step == 0 {
            src.rt_step = -2_000_000;
        }
        let p = sample_pair_with(&mut src, boot.hz_milli, None, 8);
        tracker.push(p);
        publisher.update(&p, tracker.hz_milli());
        ticks += 1;
    }
    // The boot line alone: ~2.6 s behind after a day.
    let boot_err = src.truth(src.ns) - boot.to_utc_ns(src.tsc_at(src.ns));
    assert!(boot_err > 2_500_000_000, "RT-170 boot line {boot_err}");
    assert!(worst_first <= 1_000_000, "RT-170 first second {worst_first}");
    assert!(worst_after <= 50_000, "RT-170 after first update {worst_after}");
    // From the second update on, the measured rate: within 1 µs (incl. the 1 ppm change).
    assert!(worst_steady <= 1_000, "RT-170 steady {worst_steady}");
    // The 2 ms step back of CLOCK_REALTIME at 18 h is slewed out at 500 ppm (4 s); the line is
    // never more than the step ahead, and within 50 µs again 10 s later (worst_after).
    assert!(worst_slew <= 2_001_000, "RT-170 slew window {worst_slew}");
    let end = (reader.utc_ns(src.tsc_at(src.ns)) - src.truth(src.ns)).abs();
    assert!(end <= 50_000, "RT-170 end of day {end}");
}

#[test]
fn rt_171_concurrent_readers() {
    let boot = TscClock::from_hz_milli(NOMINAL, pair(0, R));
    let (proto, mut publisher) = UtcClock::shared(&boot, 1);
    // The writer's counter: advanced after each update (readers only see published ticks).
    let now = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let clock = proto.clone();
            let (now, done) = (now.clone(), done.clone());
            std::thread::spawn(move || {
                let (mut prev, mut prev_raw, mut worst_back, mut lines) =
                    (i64::MIN, i64::MIN, 0i64, Vec::new());
                let mut n = 0u64;
                while !done.load(Ordering::Acquire) || n < 1_000 {
                    let t = Tsc(now.load(Ordering::Acquire));
                    let line = clock.line();
                    let raw = line.utc_ns(t);
                    if prev_raw != i64::MIN && raw < prev_raw {
                        worst_back = worst_back.max(prev_raw - raw);
                    }
                    prev_raw = prev_raw.max(raw);
                    let v = clock.utc_ns(t);
                    assert!(v >= prev, "RT-171 monotone");
                    prev = v;
                    if n.is_multiple_of(64) {
                        lines.push(line);
                    }
                    n += 1;
                }
                (worst_back, lines)
            })
        })
        .collect();
    let mut published: Vec<TscUtc> = vec![publisher.current()];
    let mut t = 0u64;
    for k in 0..20_000i64 {
        t += 1_000_000;
        // Alternating ±5 µs of pair error: forward steps and slews in turn.
        let jitter = if k % 2 == 0 { 5_000 } else { -5_000 };
        published.push(publisher.update(&pair(t, R + t as i64 + jitter), Some(NOMINAL)));
        now.store(t, Ordering::Release);
    }
    done.store(true, Ordering::Release);
    for h in readers {
        let (worst_back, lines) = h.join().unwrap();
        // The floor only hides sub-µs steps of the published line (slope changes within one
        // publication's latency).
        assert!(worst_back <= 1_000, "RT-171 raw step back {worst_back}");
        for l in lines {
            assert!(published.contains(&l), "RT-171 torn or unknown line {l:?}");
        }
    }
}
