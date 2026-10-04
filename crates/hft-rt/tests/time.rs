//! TSC conversions and counter reads (RT-050 – RT-052, RT-056, RT-057, RT-059).
#![cfg(not(loom))]

use hft_rt::{CalibPair, SeqCell, Tsc, TscClock, TscUtc, tsc_now, tsc_ordered};

const R: i64 = 1_791_072_000_000_000_000;

#[test]
fn rt_050_tsc_utc() {
    let rows: [(&str, u64, u64, i64); 4] = [
        ("RT-050.1", 1 << 32, 2000, 1_791_072_000_000_001_000),
        ("RT-050.2", 1 << 32, 500, 1_791_071_999_999_999_500),
        ("RT-050.3", 1 << 32, 1000, 1_791_072_000_000_000_000),
        ("RT-050.4", 40 << 32, 1025, 1_791_072_000_000_001_000),
    ];
    for (row, mult, t, want) in rows {
        let u = TscUtc {
            base_tsc: Tsc(1000),
            base_utc_ns: R,
            mult,
            shift: 32,
            err_ns: 0,
        };
        assert_eq!(u.utc_ns(Tsc(t)), want, "{row}");
    }
}

#[test]
fn rt_051_delta_ns() {
    let base = CalibPair::default();
    let rows: [(&str, u64, u64, u64, u64); 6] = [
        ("RT-051.1", 1_000_000_000_000, 0, 1000, 1000),
        ("RT-051.2", 25_000_000_000, 0, 25, 1000),
        ("RT-051.3", 3_000_000_000_000, 0, 3_000_000_000, 999_999_999),
        ("RT-051.4", 1_000_000_000_000, 100, 50, 0),
        ("RT-051.5", 1_000_000_000_000, 0, u64::MAX, u64::MAX),
        ("RT-051.6", 1_000_000_000_000, 7, 7, 0),
    ];
    for (row, hz, from, to, want) in rows {
        let c = TscClock::from_hz_milli(hz, base);
        assert_eq!(c.delta_ns(Tsc(from), Tsc(to)), want, "{row}");
    }
    let mult = |hz| TscClock::from_hz_milli(hz, base).tsc_utc(&base).mult;
    assert_eq!(mult(25_000_000_000), 171_798_691_840, "RT-051.2 mult");
    assert_eq!(mult(3_000_000_000_000), 1_431_655_765, "RT-051.3 mult");
}

#[test]
fn rt_052_to_utc_ns() {
    let base = CalibPair {
        tsc: Tsc(1000),
        realtime_ns: R,
        phc_ns: 0,
        err_ns: 50,
    };
    let c = TscClock::from_hz_milli(1_000_000_000_000, base);
    let rows: [(&str, u64, i64); 3] = [
        ("RT-052.1", 1500, 1_791_072_000_000_000_500),
        ("RT-052.2", 0, 1_791_071_999_999_999_000),
        ("RT-052.3", 1000, 1_791_072_000_000_000_000),
    ];
    for (row, t, want) in rows {
        assert_eq!(c.to_utc_ns(Tsc(t)), want, "{row}");
    }
}

#[test]
fn rt_056_tsc_monotonic() {
    for read in [tsc_now as fn() -> Tsc, tsc_ordered] {
        let first = read();
        let mut prev = first;
        for _ in 0..1_000_000 {
            let t = read();
            assert!(t >= prev, "counter went backwards: {prev:?} -> {t:?}");
            prev = t;
        }
        assert!(prev > first);
    }
}

#[cfg(target_arch = "aarch64")]
#[test]
fn rt_057_aarch64_nominal_hz() {
    let f = hft_rt::tsc_hz_nominal();
    assert!(matches!(f, Some(v) if v > 0), "{f:?}");
}

#[test]
fn rt_059_tsc_utc_matches_clock() {
    let base = CalibPair {
        tsc: Tsc(4_000),
        realtime_ns: R,
        phc_ns: 0,
        err_ns: 10,
    };
    let clock = TscClock::from_hz_milli(25_000_000_000, base);
    let cell = SeqCell::new(clock.tsc_utc(&base));
    cell.store(clock.tsc_utc(&base));
    for t in [0u64, 4_000, 4_025, 1_000_000_000_000] {
        assert_eq!(clock.to_utc_ns(Tsc(t)), cell.load().utc_ns(Tsc(t)), "t = {t}");
    }
    assert_eq!(cell.load().utc_ns(Tsc(4_025)), 1_791_072_000_000_001_000);
}
