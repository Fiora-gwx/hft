//! `SpreadCalc` (TESTS.md SLB-001–SLB-003).

use hft_strat_lib::SpreadCalc;
use hft_types::{MAX_RAW, Px, Rate};

#[test]
fn slb_001_buy_spread() {
    let rows: &[(&str, i64, i64, i64)] = &[
        ("SLB-001.1", 6_003_000, 6_000_000, 500_000),
        ("SLB-001.2", 6_000_001, 6_000_000, 167),
        ("SLB-001.3", 2_000_000_001, 2_000_000_000, 0),
        ("SLB-001.4", 2_000_000_003, 2_000_000_000, 2),
        ("SLB-001.5", 5_997_000, 6_000_000, -500_000),
        ("SLB-001.6", 6_000_000, 0, 0),
        ("SLB-001.7", 6_000_000, -1, 0),
        ("SLB-001.8", MAX_RAW, 1, i64::MAX),
        ("SLB-001.9", -MAX_RAW, 1, -i64::MAX),
    ];
    for &(row, bid, slow, want) in rows {
        assert_eq!(SpreadCalc::buy(Px(bid), Px(slow)), Rate(want), "{row}");
    }
}

#[test]
fn slb_002_sell_spread() {
    let rows: &[(&str, i64, i64, i64)] = &[
        ("SLB-002.1", 5_998_800, 6_000_000, -200_000),
        ("SLB-002.2", 6_000_000, 6_000_000, 0),
        ("SLB-002.3", 6_001_000, 6_000_000, 166_667),
        ("SLB-002.4", 6_000_000, 0, 0),
    ];
    for &(row, ask, slow, want) in rows {
        assert_eq!(SpreadCalc::sell(Px(ask), Px(slow)), Rate(want), "{row}");
    }
}

#[test]
fn slb_003_net_saturating() {
    let rows: &[(&str, i64, i64, i64)] = &[
        ("SLB-003.1", 500_000, 300_000, 200_000),
        ("SLB-003.2", 300_000, 500_000, -200_000),
        ("SLB-003.3", i64::MIN + 1, 2, i64::MIN),
        ("SLB-003.4", i64::MAX, -1, i64::MAX),
        ("SLB-003.5", 0, i64::MIN, i64::MAX),
    ];
    for &(row, s, c, want) in rows {
        assert_eq!(SpreadCalc::net(Rate(s), Rate(c)), Rate(want), "{row}");
    }
}
