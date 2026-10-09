//! `Now` time conversions (SDK-125, SDK-126).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_strategy_sdk::{EngineClockAnchor, Now, time};
use hft_types::Tsc;
use proptest::prelude::*;

const U0: i64 = 1_759_276_800_000_000_000;

#[test]
fn sdk_125_now_conversions() {
    // tsc_per_ms = 3 000 000 (3 GHz); base_engine_ns 47e9 at tsc 0.
    let clk = EngineClockAnchor {
        base_engine_ns: 47_000_000_000,
        base_tsc: Tsc(0),
        tsc_hz_milli: 3_000_000_000_000,
        utc_tsc: Tsc(9_000_000_000),
        utc_ns: U0,
    };
    let now = Now::new(Tsc(9_000_000_000), 500, 100, true, clk);
    assert_eq!(
        (now.engine_ns, now.utc_ns, now.tsc_per_ms, now.boot_seq),
        (50_000_000_000, U0, 3_000_000, 100)
    );
    assert_eq!(now.ens_of(now.tsc), 50_000_000_000, "SDK-125.1");
    assert_eq!(now.utc_of(now.tsc), U0, "SDK-125.2");
    assert_eq!(
        now.tsc_of(now.engine_ns + 1_000_000_000),
        Tsc(12_000_000_000),
        "SDK-125.3"
    );
    let e = now.engine_ns + 123_456_789;
    assert!((now.ens_of(now.tsc_of(e)) - e).abs() <= 1, "SDK-125.4");
    assert_eq!(now.tsc_of(46_999_999_999), Tsc(0), "SDK-125.5");
    assert_eq!(time::ms(1_500), 1_500_000_000, "SDK-125.6");
    assert_eq!(time::ms(0), 0, "SDK-125.7");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20_000))]

    #[test]
    fn sdk_126_time_roundtrip(
        hz in 1_000_000_000u64..=4_000_000_000,
        base in 0i64..=1_000_000_000_000_000,
        t in 0u64..=(1u64 << 50),
        de in 0i64..=1_000_000_000_000_000,
    ) {
        let clk = EngineClockAnchor { base_engine_ns: base, base_tsc: Tsc(0), tsc_hz_milli: hz * 1_000, utc_tsc: Tsc(0), utc_ns: 0 };
        let now = Now::new(Tsc(0), 1, 1, true, clk);
        let tsc_per_ms = hz / 1_000;
        let back = now.tsc_of(now.ens_of(Tsc(t))).0;
        prop_assert!(back.abs_diff(t) <= tsc_per_ms.div_ceil(1_000_000));
        let e = base + de;
        prop_assert!((now.ens_of(now.tsc_of(e)) - e).abs() <= 1);
    }
}
