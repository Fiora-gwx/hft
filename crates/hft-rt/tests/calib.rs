//! Calibration with injected readings, calibration pairs, drift tracking (RT-053 – RT-055, RT-168).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use std::collections::VecDeque;

use hft_rt::sample_pair_with;
use hft_rt::{
    CalibConfig, CalibPair, ClockErr, ClockReads, ClockVerdict, DriftTracker, PhcOffset, Tsc, TscClock,
};

const R: i64 = 1_791_072_000_000_000_000;

/// RT-053 fake: one clock `m` (ns) for mono and realtime, +50 ns per read; `sleep_ms(200)`
/// advances it by 2·10^8 − 400 ns so the first readings of a round's two sample groups (8 reads
/// each) are exactly 2·10^8 ns apart; the counter is `floor(m × hz_r / 10^12)` with the round's
/// frequency (round = counter reads / 32).
struct LinearFake {
    m: i64,
    hz: Vec<u64>,
    tsc_reads: usize,
    sleeps: u32,
}

impl ClockReads for LinearFake {
    fn tsc(&mut self) -> Tsc {
        let r = (self.tsc_reads / 32).min(self.hz.len() - 1);
        self.tsc_reads += 1;
        Tsc((self.m as u128 * self.hz[r] as u128 / 1_000_000_000_000) as u64)
    }
    fn mono_raw_ns(&mut self) -> i64 {
        let v = self.m;
        self.m += 50;
        v
    }
    fn realtime_ns(&mut self) -> i64 {
        let v = self.m;
        self.m += 50;
        v
    }
    fn sleep_ms(&mut self, ms: u32) {
        assert_eq!(ms, 200);
        self.sleeps += 1;
        self.m += 200_000_000 - 400;
    }
}

#[test]
fn rt_053_calibrate_with() {
    let cfg = CalibConfig {
        rounds: 5,
        samples: 8,
        phc_samples: 10,
        cpuid_tolerance_ppm: 100,
        step_threshold_ns: 50_000,
    };
    const F: u64 = 2_500_000_000_000;
    type Row = (&'static str, [u64; 5], Option<u64>, Result<Option<u64>, ClockErr>);
    let rows: [Row; 8] = [
        ("RT-053.1", [F; 5], None, Ok(Some(F))),
        (
            "RT-053.2",
            [
                2_500_000_500_000,
                2_499_999_500_000,
                F,
                2_500_000_100_000,
                2_499_999_900_000,
            ],
            None,
            Ok(Some(F)),
        ),
        ("RT-053.3", [F, F, F, F, 2_500_002_500_000], None, Ok(None)),
        (
            "RT-053.4",
            [F, F, F, F, 2_500_002_600_000],
            None,
            Err(ClockErr::Unstable),
        ),
        ("RT-053.5", [2_500_250_000_000; 5], Some(F), Ok(None)),
        (
            "RT-053.6",
            [2_500_252_500_000; 5],
            Some(F),
            Err(ClockErr::Mismatch),
        ),
        ("RT-053.7", [2_499_750_000_000; 5], Some(F), Ok(None)),
        (
            "RT-053.8",
            [F; 5],
            Some(2_400_000_000_000),
            Err(ClockErr::Mismatch),
        ),
    ];
    for (row, hz, nominal, want) in rows {
        let mut fake = LinearFake {
            m: 1_000_000_000,
            hz: hz.to_vec(),
            tsc_reads: 0,
            sleeps: 0,
        };
        let got = TscClock::calibrate_with(&mut fake, nominal, &cfg);
        match (got, want) {
            (Ok(c), Ok(w)) => {
                if let Some(w) = w {
                    assert_eq!(c.hz_milli, w, "{row}");
                }
            }
            (Err(e), Err(w)) => assert_eq!(e, w, "{row}"),
            (g, w) => panic!("{row}: got {:?}, want {w:?}", g.map(|c| c.hz_milli)),
        }
        if row == "RT-053.1" {
            assert_eq!(fake.sleeps, 5, "{row} sleep count");
        }
    }
}

/// RT-054 fake: scripted (tsc, realtime, tsc) triples.
struct Scripted {
    tsc: VecDeque<u64>,
    rt: VecDeque<i64>,
}

impl Scripted {
    fn new(triples: &[(u64, i64, u64)]) -> Scripted {
        let mut tsc = VecDeque::new();
        let mut rt = VecDeque::new();
        for &(a, r, b) in triples {
            tsc.push_back(a);
            rt.push_back(r);
            tsc.push_back(b);
        }
        Scripted { tsc, rt }
    }
}

impl ClockReads for Scripted {
    fn tsc(&mut self) -> Tsc {
        Tsc(self.tsc.pop_front().expect("script exhausted"))
    }
    fn mono_raw_ns(&mut self) -> i64 {
        unreachable!("sample_pair_with reads CLOCK_REALTIME only")
    }
    fn realtime_ns(&mut self) -> i64 {
        self.rt.pop_front().expect("script exhausted")
    }
    fn sleep_ms(&mut self, _: u32) {
        unreachable!()
    }
}

fn triples(widths: [u64; 8], fixed: &[(usize, (u64, i64, u64))]) -> Vec<(u64, i64, u64)> {
    let mut v: Vec<(u64, i64, u64)> = widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let a = 100_000 * (i as u64 + 1);
            (a, R + 1_000 * i as i64, a + w)
        })
        .collect();
    for &(i, t) in fixed {
        v[i] = t;
    }
    v
}

#[test]
fn rt_054_sample_pair_with() {
    const HZ: u64 = 1_000_000_000_000;
    let w = [500, 300, 120, 900, 120, 700, 800, 1000];
    let third = (2, (10_000, R, 10_120));

    let mut s = Scripted::new(&triples(w, &[third]));
    let p = sample_pair_with(&mut s, HZ, None, 8);
    assert_eq!(
        p,
        CalibPair {
            tsc: Tsc(10_060),
            realtime_ns: R,
            phc_ns: 0,
            err_ns: 60
        },
        "RT-054.1"
    );

    let mut s = Scripted::new(&triples(w, &[third]));
    let phc = PhcOffset {
        phc_ns: R + 37,
        sys_ns: R + 5,
        err_ns: 40,
    };
    let p = sample_pair_with(&mut s, HZ, Some(&phc), 8);
    assert_eq!(
        p,
        CalibPair {
            tsc: Tsc(10_060),
            realtime_ns: R,
            phc_ns: R + 32,
            err_ns: 100
        },
        "RT-054.2"
    );

    let r2 = R + 777;
    let mut w3 = w;
    w3[2] = 121;
    let mut s = Scripted::new(&triples(
        w3,
        &[(2, (10_000, R, 10_121)), (4, (20_000, r2, 20_120))],
    ));
    let p = sample_pair_with(&mut s, HZ, None, 8);
    assert_eq!(
        (p.tsc, p.realtime_ns, p.err_ns),
        (Tsc(20_060), r2, 60),
        "RT-054.3"
    );

    let mut s = Scripted::new(&triples([121; 8], &[(0, (5_000, R, 5_121))]));
    let p = sample_pair_with(&mut s, HZ, None, 8);
    assert_eq!((p.tsc, p.err_ns), (Tsc(5_060), 61), "RT-054.4");
}

#[test]
fn rt_055_drift_tracker() {
    use ClockVerdict::*;
    let pair = |k: u64, off: i64| CalibPair {
        tsc: Tsc(k * 1_000_000_000),
        realtime_ns: R + k as i64 * 1_000_000_000 + off,
        phc_ns: 0,
        err_ns: 0,
    };
    let run = |offs: &[i64]| -> Vec<ClockVerdict> {
        let mut t = DriftTracker::new(50_000);
        offs.iter()
            .enumerate()
            .map(|(k, o)| t.push(pair(k as u64 + 1, *o)))
            .collect()
    };
    assert_eq!(run(&[0, 0, 0]), vec![Unsynced, Unsynced, Synced], "RT-055.1");
    assert_eq!(run(&[0, 0, 50_000]), vec![Unsynced, Unsynced, Synced], "RT-055.2");
    assert_eq!(
        run(&[0, 0, 50_001]),
        vec![Unsynced, Unsynced, Stepped { jump_ns: 50_001 }],
        "RT-055.3"
    );
    assert_eq!(
        run(&[0, 0, -50_001]),
        vec![Unsynced, Unsynced, Stepped { jump_ns: -50_001 }],
        "RT-055.4"
    );
    assert_eq!(
        run(&[0, 0, 50_001, 50_001, 50_001]),
        vec![Unsynced, Unsynced, Stepped { jump_ns: 50_001 }, Unsynced, Synced],
        "RT-055.5"
    );
    let mut offs = vec![0i64; 61];
    offs.push(49_999);
    let got = run(&offs);
    let mut want = vec![Unsynced, Unsynced];
    want.extend(std::iter::repeat_n(Synced, 59));
    want.push(Synced);
    assert_eq!(got, want, "RT-055.6");
}

#[test]
fn rt_168_drift_tracker_rate() {
    // Counter 30 ppm slow against CLOCK_REALTIME: 999 970 000 ticks per real second.
    let hz: u64 = 999_970_000_000;
    let pair = |k: i64, jitter: i64| CalibPair {
        tsc: Tsc(5_000 + (k as u128 * hz as u128 / 1_000) as u64),
        realtime_ns: R + k * 1_000_000_000 + jitter,
        phc_ns: 0,
        err_ns: 0,
    };
    let mut t = DriftTracker::new(50_000);
    assert_eq!(t.hz_milli(), None, "RT-168.1");
    t.push(pair(0, 0));
    assert_eq!(t.hz_milli(), None, "RT-168.1");
    t.push(pair(1, 0));
    assert_eq!(t.hz_milli(), Some(hz), "RT-168.2");
    for k in 2..100 {
        t.push(pair(k, 0));
    }
    assert_eq!(t.hz_milli(), Some(hz), "RT-168.3");
    // ±40 ns of pair jitter over the 60-pair window: within 0.01 ppm of the true rate.
    let mut t = DriftTracker::new(50_000);
    for k in 0..60 {
        t.push(pair(k, [40, -40, 0, 25, -25][k as usize % 5]));
    }
    let got = t.hz_milli().unwrap();
    assert!(got.abs_diff(hz) * 100_000_000 <= hz, "RT-168.4 {got}");
    // A counter that does not advance has no rate.
    let mut t = DriftTracker::new(u32::MAX);
    for k in 0..3 {
        t.push(CalibPair {
            tsc: Tsc(7),
            realtime_ns: R + k * 1_000_000_000,
            phc_ns: 0,
            err_ns: 0,
        });
    }
    assert_eq!(t.hz_milli(), None, "RT-168.5");
}
