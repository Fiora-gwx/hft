//! `HedgePxLimits` and `cap_cross_px` (TYP-120, TYP-121).
#![allow(clippy::type_complexity)] // table rows are tuples

use hft_types::{HedgePxLimits, MAX_RAW, Px, Rate, Side, cap_cross_px};

#[test]
fn typ_120_hedge_px_limits_value() {
    let a = HedgePxLimits {
        conn_liveness_ms: 1000,
        rr_max_cross: Rate(5_000_000),
    };
    let b = a;
    assert!(a == b);
    assert_eq!(a.conn_liveness_ms, 1000, "a still usable after copy");
    let c = HedgePxLimits {
        conn_liveness_ms: 999,
        ..a
    };
    assert!(a != c);
    let s = format!("{a:?}");
    assert!(s.contains("conn_liveness_ms: 1000"), "{s}");
    assert!(s.contains("rr_max_cross"), "{s}");
}

#[test]
fn typ_121_cap_cross_px() {
    use Side::{Buy, Sell};
    let rows: [(&str, Side, i64, i64, i64, i64, i64, i64); 20] = [
        (
            "TYP-121.1",
            Buy,
            1_000_000,
            999_000,
            10_000_000,
            5_000_000,
            1,
            1_003_995,
        ),
        (
            "TYP-121.2",
            Buy,
            1_000_000,
            999_000,
            1_000_000,
            5_000_000,
            1,
            1_001_000,
        ),
        (
            "TYP-121.3",
            Buy,
            1_000_000,
            1_000_000,
            5_000_000,
            5_000_000,
            1,
            1_005_000,
        ),
        (
            "TYP-121.4",
            Buy,
            1_000_000,
            999_000,
            1_234_000,
            5_000_000,
            100,
            1_001_200,
        ),
        (
            "TYP-121.5",
            Sell,
            1_000_000,
            1_001_000,
            10_000_000,
            5_000_000,
            100,
            996_000,
        ),
        (
            "TYP-121.6",
            Sell,
            1_000_000,
            1_001_000,
            1_000_000,
            5_000_000,
            1,
            999_000,
        ),
        ("TYP-121.7", Buy, 1_000_000, 999_000, 0, 5_000_000, 1, 1_000_000),
        (
            "TYP-121.8",
            Buy,
            1_000_000,
            900_000,
            10_000_000,
            5_000_000,
            1,
            904_500,
        ),
        ("TYP-121.9", Buy, 3, 1_000, 500_000_000, 10_000_000, 1, 4),
        ("TYP-121.10", Sell, 3, 1, 500_000_000, 10_000_000, 1, 2),
        ("TYP-121.11", Buy, 0, 1_000, 1_000_000, 5_000_000, 1, 0),
        ("TYP-121.12", Buy, 1_000, 0, 1_000_000, 5_000_000, 1, 0),
        ("TYP-121.13", Sell, -1, 1_000, 1_000_000, 5_000_000, 1, 0),
        ("TYP-121.14", Buy, 1_000, -1, 1_000_000, 5_000_000, 1, 0),
        ("TYP-121.15", Buy, 1_000_000, 999_000, -1, 5_000_000, 1, 0),
        ("TYP-121.16", Buy, 1_000_000, 999_000, 1_000_000, -1, 1, 0),
        ("TYP-121.17", Buy, 1_000_000, 999_000, 1_000_000, 5_000_000, 0, 0),
        (
            "TYP-121.18",
            Sell,
            1_000_000,
            1_000_000,
            1_000_000_000,
            1_000_000_000,
            1,
            0,
        ),
        (
            "TYP-121.19",
            Sell,
            1_000_000,
            1_000_000,
            1_500_000_000,
            1_500_000_000,
            1,
            0,
        ),
        (
            "TYP-121.20",
            Buy,
            MAX_RAW,
            MAX_RAW,
            1_000_000_000,
            1_000_000_000,
            10,
            4_611_686_018_427_387_900,
        ),
    ];
    for (row, side, touch, mid, mc, rr, tick, want) in rows {
        let lim = HedgePxLimits {
            conn_liveness_ms: 1000,
            rr_max_cross: Rate(rr),
        };
        assert_eq!(
            cap_cross_px(side, Px(touch), Px(mid), Rate(mc), &lim, tick),
            Px(want),
            "{row}"
        );
    }
}
