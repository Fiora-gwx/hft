//! Clock math: additivity within 1 ns, monotonic, equal to the i128 reference (RT-060).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use hft_rt::{CalibPair, Tsc, TscClock};
use proptest::prelude::*;

const BASE_TSC: u64 = 1 << 52;

fn hz() -> impl Strategy<Value = u64> {
    // log-uniform over [1e9, 5e12] mHz
    (0u32..=1000).prop_map(|k| {
        let lo = 1e9f64.ln();
        let hi = 5e12f64.ln();
        (lo + (hi - lo) * k as f64 / 1000.0).exp() as u64
    })
}

fn t() -> impl Strategy<Value = u64> {
    (BASE_TSC - 1_000_000_000_000_000)..=(BASE_TSC + 1_000_000_000_000_000)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]
    #[test]
    fn rt_060_clock_math(hz in hz(), a in t(), b in t(), c in t()) {
        let mut v = [a, b, c];
        v.sort_unstable();
        let [a, b, c] = v;
        let base = CalibPair { tsc: Tsc(BASE_TSC), realtime_ns: 1_791_072_000_000_000_000, phc_ns: 0, err_ns: 0 };
        let clk = TscClock::from_hz_milli(hz, base);
        let ac = clk.delta_ns(Tsc(a), Tsc(c));
        let sum = clk.delta_ns(Tsc(a), Tsc(b)) + clk.delta_ns(Tsc(b), Tsc(c));
        prop_assert!(ac.abs_diff(sum) <= 1, "{} vs {}", ac, sum);
        prop_assert!(clk.delta_ns(Tsc(a), Tsc(b)) <= ac);
        let mult = clk.tsc_utc(&base).mult as i128;
        for x in [a, b, c] {
            let want = base.realtime_ns as i128 + (((x as i128 - BASE_TSC as i128) * mult) >> 32);
            prop_assert_eq!(clk.to_utc_ns(Tsc(x)) as i128, want);
        }
    }
}
