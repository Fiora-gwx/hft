//! `TriggerPx` (TESTS.md SLB-012–SLB-015, SLB-018, SLB-019).

use hft_strat_lib::TriggerPx;
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use hft_types::{MAX_RAW, Px, Rate};

const NOW: i64 = 7_000_000_000;

fn upd(t: &mut TriggerPx, b: i64, s: i64, to: i64, tc: i64, tick: i64, now: i64) {
    t.update(Px(b), Px(s), Rate(to), Rate(tc), Px(tick), now);
}

/// SLB-012.1 state.
fn t12_1() -> TriggerPx {
    let mut t = TriggerPx::default();
    upd(&mut t, 6_000_000, 6_000_000, 500_000, -200_000, 10, NOW);
    t
}

#[test]
fn slb_012_update_rounding() {
    type Row = (&'static str, (i64, i64, i64, i64, i64), i64, i64);
    let rows: &[Row] = &[
        (
            "SLB-012.1",
            (6_000_000, 6_000_000, 500_000, -200_000, 10),
            6_003_000,
            5_998_800,
        ),
        (
            "SLB-012.2",
            (6_000_001, 6_000_001, 500_000, 500_000, 1),
            6_003_002,
            6_003_001,
        ),
        (
            "SLB-012.3",
            (6_000_000, 6_000_000, 505_000, 505_000, 100),
            6_003_100,
            6_003_000,
        ),
        (
            "SLB-012.4",
            (6_000_005, 6_000_005, 0, 0, 10),
            6_000_010,
            6_000_000,
        ),
        (
            "SLB-012.5",
            (MAX_RAW, MAX_RAW, 1_000_000_000, 0, 10),
            4_611_686_018_427_387_900,
            4_611_686_018_427_387_900,
        ),
    ];
    for &(row, (b, s, to, tc, tick), wb, ws) in rows {
        let mut t = TriggerPx::default();
        upd(&mut t, b, s, to, tc, tick, NOW);
        assert_eq!((t.buy_bid_fast, t.sell_ask_fast), (Px(wb), Px(ws)), "{row}");
        assert!(t.valid, "{row}");
        assert_eq!(t.slow_ens, NOW, "{row}");
    }
}

#[test]
fn slb_013_hit_boundaries() {
    let t = t12_1();
    let rows: &[(&str, bool, i64, bool)] = &[
        ("SLB-013.1", true, 6_002_990, false),
        ("SLB-013.2", true, 6_002_999, false),
        ("SLB-013.3", true, 6_003_000, true),
        ("SLB-013.4", true, 6_003_010, true),
        ("SLB-013.5", false, 5_998_810, false),
        ("SLB-013.6", false, 5_998_800, true),
        ("SLB-013.7", false, 5_998_790, true),
    ];
    for &(row, buy, px, want) in rows {
        let got = if buy {
            t.hit_buy(Px(px))
        } else {
            t.hit_sell(Px(px))
        };
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn slb_014_update_invalid_inputs() {
    let rows: &[(&str, i64, i64, i64, bool)] = &[
        ("SLB-014.1", 0, 6_000_000, 10, false),
        ("SLB-014.2", 6_000_000, -1, 10, false),
        ("SLB-014.3", 6_000_000, 6_000_000, 0, false),
        ("SLB-014.4", 6_000_000, 6_000_000, -10, false),
        ("SLB-014.5", 1, 1, 1, true),
    ];
    for &(row, b, s, tick, valid) in rows {
        let mut t = t12_1();
        assert!(t.valid);
        upd(&mut t, b, s, 500_000, -200_000, tick, NOW);
        assert_eq!(t.valid, valid, "{row}");
        if !valid {
            assert!(!t.hit_buy(Px(MAX_RAW)), "{row}");
            assert!(!t.hit_sell(Px(-MAX_RAW)), "{row}");
        }
    }
}

#[test]
fn slb_015_invalidate_then_update() {
    let mut t = t12_1();
    t.invalidate();
    assert!(!t.hit_buy(Px(MAX_RAW)));
    assert!(!t.hit_sell(Px(-MAX_RAW)));
    assert!(!t.valid);
    upd(&mut t, 6_000_000, 6_000_000, 500_000, -200_000, 10, 8_000_000_000);
    assert!(t.hit_buy(Px(6_003_000)));
    assert_eq!(t.slow_ens, 8_000_000_000);
}

fn snap(t: &TriggerPx) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64);
    t.snapshot(&mut w);
    w.as_bytes().to_vec()
}

#[test]
fn slb_018_snapshot_restore_invalid() {
    let t = t12_1();
    let b = snap(&t);
    assert_eq!(b.len(), 25);
    let mut want = Vec::new();
    want.extend_from_slice(&6_003_000i64.to_le_bytes());
    want.extend_from_slice(&5_998_800i64.to_le_bytes());
    want.push(1);
    want.extend_from_slice(&7_000_000_000i64.to_le_bytes());
    assert_eq!(b, want);

    let mut u = TriggerPx::default();
    assert_eq!(u.restore(&mut SnapshotReader::new(&b)), Ok(()));
    assert_eq!(u.buy_bid_fast, Px(6_003_000));
    assert_eq!(u.sell_ask_fast, Px(5_998_800));
    assert_eq!(u.slow_ens, 7_000_000_000);
    assert!(!u.valid);
    assert!(!u.hit_buy(Px(6_003_000)));

    upd(&mut u, 6_000_000, 6_000_000, 500_000, -200_000, 10, 9_000_000_000);
    assert!(u.hit_buy(Px(6_003_000)));
}

#[test]
fn slb_019_restore_truncated() {
    let b = snap(&t12_1());
    let mut t = TriggerPx::default();
    upd(&mut t, 6_000_000, 6_000_000, 505_000, 505_000, 100, NOW);
    let before = t;
    assert!(t.restore(&mut SnapshotReader::new(&b[..24])).is_err());
    assert_eq!(t, before);
    assert!(t.valid);
    assert_eq!(t.buy_bid_fast, Px(6_003_100));
}
