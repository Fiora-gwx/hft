//! Property test for `funding_settle_norm_ns` (TYP-131).

use hft_types::funding_settle_norm_ns;
use proptest::prelude::*;

const NS: i128 = 1_000_000_000;

/// i128 reference: lower boundary by remainder, nearest with ties to the earlier one, and the
/// other neighbour when the nearest is outside i64.
fn reference(ts: i64, iv: u32, an: u32) -> (i64, bool) {
    if iv == 0 {
        return (ts, false);
    }
    let (i, a, t) = (iv as i128 * NS, an as i128 * NS, ts as i128);
    let r = (t - a).rem_euclid(i);
    let lo = t - r;
    let hi = lo + i;
    let (near, far) = if 2 * r <= i { (lo, hi) } else { (hi, lo) };
    let ok = |v: i128| (i64::MIN as i128..=i64::MAX as i128).contains(&v);
    if ok(near) {
        (near as i64, false)
    } else {
        (far as i64, true)
    }
}

fn ts() -> impl Strategy<Value = i64> {
    // 2020-01-01 .. 2040-01-01 UTC in ns
    prop_oneof![
        any::<i64>(),
        1_577_836_800_000_000_000i64..=2_208_988_800_000_000_000
    ]
}

fn interval() -> impl Strategy<Value = u32> {
    prop_oneof![
        prop::sample::select(vec![0u32, 3_600, 14_400, 28_800]),
        0u32..=604_800
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn typ_131_funding_norm_matches_reference(ts in ts(), iv in interval(), an in any::<u32>()) {
        let got = funding_settle_norm_ns(ts, iv, an);
        let (want, fallback) = reference(ts, iv, an);
        prop_assert_eq!(got, want);
        if iv > 0 {
            prop_assert_eq!(got as i128 % NS, 0);
            prop_assert_eq!((got as i128 / NS - an as i128).rem_euclid(iv as i128), 0);
            if !fallback {
                prop_assert!((got as i128 - ts as i128).abs() <= iv as i128 * 500_000_000);
            }
        }
    }
}
