//! Table tests for fixed-point arithmetic (TYP-006 – TYP-025, TYP-160).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)] // table rows are tuples

use hft_types::RoundMode::{Ceil, Floor, HalfEven, TowardZero};
use hft_types::*;

const MODES: [RoundMode; 4] = [Floor, Ceil, TowardZero, HalfEven];

fn e(px_exp: i8, qty_exp: i8) -> AssetExp {
    AssetExp { px_exp, qty_exp }
}

#[test]
fn typ_006_same_type_ops() {
    assert_eq!(Px(2) + Px(3), Px(5), "TYP-006.1");
    assert_eq!(Px(2) - Px(3), Px(-1), "TYP-006.2");
    assert_eq!(-Px(5), Px(-5), "TYP-006.3");
    assert!(Px(1) < Px(2), "TYP-006.4");
    assert_eq!(Qty(-3) + Qty(3), Qty(0), "TYP-006.5");
    assert_eq!(
        Px(MAX_RAW) + Px(MAX_RAW),
        Px(9_223_372_036_854_775_806),
        "TYP-006.6"
    );
    assert_eq!(
        Px(-MAX_RAW) - Px(MAX_RAW),
        Px(-9_223_372_036_854_775_806),
        "TYP-006.7"
    );
}

#[test]
fn typ_010_mul_qty_round_modes() {
    // (row, p, [Floor, Ceil, TowardZero, HalfEven])
    let rows: [(&str, i64, [i64; 4]); 13] = [
        ("TYP-010.1", 1_000_000, [1, 1, 1, 1]),
        ("TYP-010.2", 1, [0, 1, 0, 0]),
        ("TYP-010.3", 499_999, [0, 1, 0, 0]),
        ("TYP-010.4", 500_000, [0, 1, 0, 0]),
        ("TYP-010.5", 500_001, [0, 1, 0, 1]),
        ("TYP-010.6", 1_500_000, [1, 2, 1, 2]),
        ("TYP-010.7", 2_500_000, [2, 3, 2, 2]),
        ("TYP-010.8", -1, [-1, 0, 0, 0]),
        ("TYP-010.9", -500_000, [-1, 0, 0, 0]),
        ("TYP-010.10", -1_500_000, [-2, -1, -1, -2]),
        ("TYP-010.11", -2_500_000, [-3, -2, -2, -2]),
        ("TYP-010.12", -2_500_001, [-3, -2, -2, -3]),
        ("TYP-010.13", 0, [0, 0, 0, 0]),
    ];
    for (row, p, want) in rows {
        for (m, w) in MODES.into_iter().zip(want) {
            assert_eq!(
                Px(p).mul_qty(Qty(1), e(-4, -8), m),
                (Notional(w), FxErr::OK),
                "{row} {m:?}"
            );
        }
    }
}

#[test]
fn typ_011_mul_qty_saturation() {
    let ok = FxErr::OK;
    let of = FxErr::OVERFLOW;
    let rows: [(&str, i64, i64, (i8, i8), (i64, FxErr)); 12] = [
        (
            "TYP-011.1",
            7,
            1_317_624_576_693_539_401,
            (0, -6),
            (9_223_372_036_854_775_807, ok),
        ),
        (
            "TYP-011.2",
            7,
            1_317_624_576_693_539_400,
            (0, -6),
            (9_223_372_036_854_775_800, ok),
        ),
        ("TYP-011.3", 7, 1_317_624_576_693_539_402, (0, -6), (i64::MAX, of)),
        (
            "TYP-011.4",
            -7,
            1_317_624_576_693_539_401,
            (0, -6),
            (-9_223_372_036_854_775_807, ok),
        ),
        (
            "TYP-011.5",
            -4,
            2_305_843_009_213_693_952,
            (0, -6),
            (-i64::MAX, of),
        ),
        ("TYP-011.6", MAX_RAW, MAX_RAW, (-4, -8), (i64::MAX, of)),
        ("TYP-011.7", MAX_RAW, -MAX_RAW, (-4, -8), (-i64::MAX, of)),
        ("TYP-011.8", 1, 1, (6, 6), (1_000_000_000_000_000_000, ok)),
        ("TYP-011.9", 10, 1, (6, 6), (i64::MAX, of)),
        ("TYP-011.10", 1, 1, (-19, -8), (0, FxErr::EXP_RANGE)),
        ("TYP-011.11", 1, 1, (-4, 7), (0, FxErr::EXP_RANGE)),
        ("TYP-011.12", 0, MAX_RAW, (-4, -8), (0, ok)),
    ];
    for (row, px, q, (pe, qe), (n, err)) in rows {
        assert_eq!(
            Px(px).mul_qty(Qty(q), e(pe, qe), TowardZero),
            (Notional(n), err),
            "{row}"
        );
    }
}

#[test]
fn typ_013_from_notional() {
    let rows: [(&str, i64, i64, (i8, i8), RoundMode, Result<Px, FxErr>); 18] = [
        (
            "TYP-013.1",
            5_000_000_000_000,
            5_000_000_000,
            (-4, -8),
            HalfEven,
            Ok(Px(1_000_000_000)),
        ),
        ("TYP-013.2", 1, 3, (-4, -8), Floor, Ok(Px(333_333))),
        ("TYP-013.3", 1, 3, (-4, -8), Ceil, Ok(Px(333_334))),
        ("TYP-013.4", 1, 3, (-4, -8), HalfEven, Ok(Px(333_333))),
        ("TYP-013.5", 5, 10_000_000, (-4, -8), HalfEven, Ok(Px(0))),
        ("TYP-013.6", 15, 10_000_000, (-4, -8), HalfEven, Ok(Px(2))),
        ("TYP-013.7", 25, 10_000_000, (-4, -8), HalfEven, Ok(Px(2))),
        ("TYP-013.8", -15, 10_000_000, (-4, -8), Floor, Ok(Px(-2))),
        ("TYP-013.9", -15, 10_000_000, (-4, -8), Ceil, Ok(Px(-1))),
        ("TYP-013.10", -15, 10_000_000, (-4, -8), TowardZero, Ok(Px(-1))),
        ("TYP-013.11", -15, 10_000_000, (-4, -8), HalfEven, Ok(Px(-2))),
        ("TYP-013.12", 15, -10_000_000, (-4, -8), HalfEven, Ok(Px(-2))),
        ("TYP-013.13", 5, 0, (-4, -8), HalfEven, Err(FxErr::DIV_ZERO)),
        (
            "TYP-013.14",
            4_611_686_018_427_387_903,
            1_000_000,
            (-4, -8),
            TowardZero,
            Ok(Px(MAX_RAW)),
        ),
        (
            "TYP-013.15",
            4_611_686_018_427_387_904,
            1_000_000,
            (-4, -8),
            TowardZero,
            Err(FxErr::OVERFLOW),
        ),
        (
            "TYP-013.16",
            -4_611_686_018_427_387_903,
            1_000_000,
            (-4, -8),
            TowardZero,
            Ok(Px(-MAX_RAW)),
        ),
        (
            "TYP-013.17",
            i64::MAX,
            MAX_RAW,
            (-18, -18),
            HalfEven,
            Err(FxErr::OVERFLOW),
        ),
        ("TYP-013.18", 1, 1, (-19, -8), HalfEven, Err(FxErr::EXP_RANGE)),
    ];
    for (row, n, q, (pe, qe), m, want) in rows {
        assert_eq!(
            Px::from_notional(Notional(n), Qty(q), e(pe, qe), m),
            want,
            "{row}"
        );
    }
}

#[test]
fn typ_014_px_mul_rate() {
    let rows: [(&str, i64, i64, RoundMode, i64); 17] = [
        ("TYP-014.1", 1_000_000_000, 100_000, Floor, 1_000_100_000),
        ("TYP-014.2", 1_000_000_000, -100_000, Ceil, 999_900_000),
        ("TYP-014.3", 1, 500_000_000, Floor, 1),
        ("TYP-014.4", 1, 500_000_000, Ceil, 2),
        ("TYP-014.5", 1, 500_000_000, TowardZero, 1),
        ("TYP-014.6", 1, 500_000_000, HalfEven, 2),
        ("TYP-014.7", 5, -500_000_000, HalfEven, 2),
        ("TYP-014.8", -1, 500_000_000, Floor, -2),
        ("TYP-014.9", -1, 500_000_000, Ceil, -1),
        ("TYP-014.10", -1, 500_000_000, TowardZero, -1),
        ("TYP-014.11", -1, 500_000_000, HalfEven, -2),
        ("TYP-014.12", 123, 0, Floor, 123),
        ("TYP-014.13", 123, -1_000_000_000, Floor, 0),
        (
            "TYP-014.14",
            2_305_843_009_213_693_951,
            1_000_000_000,
            Floor,
            4_611_686_018_427_387_902,
        ),
        (
            "TYP-014.15",
            2_305_843_009_213_693_952,
            1_000_000_000,
            Floor,
            MAX_RAW,
        ),
        (
            "TYP-014.16",
            -2_305_843_009_213_693_952,
            1_000_000_000,
            Floor,
            -MAX_RAW,
        ),
        ("TYP-014.17", MAX_RAW, i64::MAX, HalfEven, MAX_RAW),
    ];
    for (row, px, r, m, want) in rows {
        assert_eq!(Px(px).mul_rate(Rate(r), m), Px(want), "{row}");
    }
}

#[test]
fn typ_015_notional_mul_rate() {
    let rows: [(&str, i64, i64, RoundMode, i64); 13] = [
        ("TYP-015.1", 1_000_000_000, 200_000, Ceil, 200_000),
        ("TYP-015.2", 1_000_001, 375_000, Ceil, 376),
        ("TYP-015.3", 1_000_001, 375_000, Floor, 375),
        ("TYP-015.4", 1, 500_000_000, Floor, 0),
        ("TYP-015.5", 1, 500_000_000, Ceil, 1),
        ("TYP-015.6", 1, 500_000_000, HalfEven, 0),
        ("TYP-015.7", -3, 500_000_000, Floor, -2),
        ("TYP-015.8", -3, 500_000_000, Ceil, -1),
        ("TYP-015.9", -3, 500_000_000, TowardZero, -1),
        ("TYP-015.10", -3, 500_000_000, HalfEven, -2),
        ("TYP-015.11", i64::MAX, 1_000_000_000, Floor, i64::MAX),
        ("TYP-015.12", i64::MAX, 2_000_000_000, Floor, i64::MAX),
        ("TYP-015.13", i64::MAX, -2_000_000_000, Floor, -i64::MAX),
    ];
    for (row, n, r, m, want) in rows {
        assert_eq!(Notional(n).mul_rate(Rate(r), m), Notional(want), "{row}");
    }
}

#[test]
fn typ_016_round_to_tick() {
    let rows: [(&str, i64, [i64; 4]); 11] = [
        ("TYP-016.1", 123, [120, 130, 120, 120]),
        ("TYP-016.2", 125, [120, 130, 120, 120]),
        ("TYP-016.3", 135, [130, 140, 130, 140]),
        ("TYP-016.4", 120, [120, 120, 120, 120]),
        ("TYP-016.5", 0, [0, 0, 0, 0]),
        ("TYP-016.6", -123, [-130, -120, -120, -120]),
        ("TYP-016.7", -125, [-130, -120, -120, -120]),
        ("TYP-016.8", -135, [-140, -130, -130, -140]),
        (
            "TYP-016.9",
            4_611_686_018_427_387_891,
            [
                4_611_686_018_427_387_890,
                4_611_686_018_427_387_900,
                4_611_686_018_427_387_890,
                4_611_686_018_427_387_890,
            ],
        ),
        (
            "TYP-016.10",
            4_611_686_018_427_387_901,
            [4_611_686_018_427_387_900; 4],
        ),
        (
            "TYP-016.11",
            -4_611_686_018_427_387_901,
            [-4_611_686_018_427_387_900; 4],
        ),
    ];
    for (row, px, want) in rows {
        for (m, w) in MODES.into_iter().zip(want) {
            assert_eq!(Px(px).round_to_tick(10, m), Px(w), "{row} {m:?}");
        }
    }
}

#[test]
fn typ_017_nonpositive_step() {
    assert_eq!(Px(123).round_to_tick(0, Floor), Px(0), "TYP-017.1");
    assert_eq!(Px(123).round_to_tick(-10, Ceil), Px(0), "TYP-017.2");
    assert_eq!(Px(123).round_to_tick(1, HalfEven), Px(123), "TYP-017.3");
    assert_eq!(Qty(1999).round_to_lot(0, TowardZero), Qty(0), "TYP-017.4");
    assert_eq!(Qty(1999).round_to_lot(-5, TowardZero), Qty(0), "TYP-017.5");
    assert_eq!(Qty(1999).split_lot(0), (Qty(0), Qty(1999)), "TYP-017.6");
    assert_eq!(Qty(-1999).split_lot(-1000), (Qty(0), Qty(-1999)), "TYP-017.7");
}

#[test]
fn typ_018_round_to_lot() {
    let rows: [(&str, i64, [i64; 4]); 7] = [
        ("TYP-018.1", 1999, [1000, 2000, 1000, 2000]),
        ("TYP-018.2", -1999, [-2000, -1000, -1000, -2000]),
        ("TYP-018.3", 1500, [1000, 2000, 1000, 2000]),
        ("TYP-018.4", 2500, [2000, 3000, 2000, 2000]),
        ("TYP-018.5", -2500, [-3000, -2000, -2000, -2000]),
        ("TYP-018.6", 0, [0, 0, 0, 0]),
        ("TYP-018.7", MAX_RAW, [4_611_686_018_427_387_000; 4]),
    ];
    for (row, q, want) in rows {
        for (m, w) in MODES.into_iter().zip(want) {
            assert_eq!(Qty(q).round_to_lot(1000, m), Qty(w), "{row} {m:?}");
        }
    }
}

#[test]
fn typ_019_split_lot() {
    let rows: [(&str, i64, (i64, i64)); 6] = [
        ("TYP-019.1", 1999, (1000, 999)),
        ("TYP-019.2", -1999, (-1000, -999)),
        ("TYP-019.3", 999, (0, 999)),
        ("TYP-019.4", 2000, (2000, 0)),
        ("TYP-019.5", 0, (0, 0)),
        ("TYP-019.6", MAX_RAW, (4_611_686_018_427_387_000, 903)),
    ];
    for (row, q, (a, r)) in rows {
        assert_eq!(Qty(q).split_lot(1000), (Qty(a), Qty(r)), "{row}");
    }
}

#[test]
fn typ_021_mid_floor() {
    let rows: [(&str, i64, i64, i64); 10] = [
        ("TYP-021.1", 3, 4, 3),
        ("TYP-021.2", 4, 4, 4),
        ("TYP-021.3", -3, -4, -4),
        ("TYP-021.4", -3, 4, 0),
        ("TYP-021.5", -4, 3, -1),
        ("TYP-021.6", MAX_RAW, MAX_RAW, MAX_RAW),
        ("TYP-021.7", MAX_RAW, MAX_RAW - 1, 4_611_686_018_427_387_902),
        ("TYP-021.8", -MAX_RAW, -MAX_RAW, -MAX_RAW),
        ("TYP-021.9", MAX_RAW, -MAX_RAW, 0),
        ("TYP-021.10", i64::MAX, i64::MAX, i64::MAX),
    ];
    for (row, a, b, want) in rows {
        assert_eq!(Px::mid_floor(Px(a), Px(b)), Px(want), "{row}");
    }
}

#[test]
fn typ_022_ratio() {
    let rows: [(&str, i64, i64, i64); 12] = [
        ("TYP-022.1", 1_000_100_000, 1_000_000_000, 100_000),
        ("TYP-022.2", 999_900_000, 1_000_000_000, -100_000),
        ("TYP-022.3", 5, 5, 0),
        ("TYP-022.4", 2, 3, -333_333_333),
        ("TYP-022.5", 2_000_000_001, 2_000_000_000, 0),
        ("TYP-022.6", 2_000_000_003, 2_000_000_000, 2),
        ("TYP-022.7", 1_999_999_999, 2_000_000_000, 0),
        ("TYP-022.8", 1_999_999_997, 2_000_000_000, -2),
        ("TYP-022.9", 100, 0, 0),
        ("TYP-022.10", 100, -5, 0),
        ("TYP-022.11", MAX_RAW, 1, i64::MAX),
        ("TYP-022.12", -MAX_RAW, 1, -i64::MAX),
    ];
    for (row, a, b, want) in rows {
        assert_eq!(Px(a).ratio(Px(b)), Rate(want), "{row}");
    }
}

#[test]
fn typ_023_pair_lot() {
    let rows: [(&str, i64, i64, u32, u32, Option<i64>); 21] = [
        ("TYP-023.1", 1, 10, 1, 1, Some(10)),
        ("TYP-023.2", 10, 1, 1, 1, Some(10)),
        ("TYP-023.3", 4, 10, 1, 1, Some(20)),
        ("TYP-023.4", 7, 7, 1, 1, Some(7)),
        ("TYP-023.5", 6, 4, 1, 1, Some(12)),
        ("TYP-023.6", 1, 5, 1, 2, Some(10)),
        ("TYP-023.7", 3, 5, 1, 2, Some(30)),
        ("TYP-023.8", 1, 5, 2, 1, Some(5)),
        ("TYP-023.9", 4, 6, 3, 2, Some(4)),
        ("TYP-023.10", 1, 1, 4_294_967_295, 4_294_967_295, Some(1)),
        ("TYP-023.11", 1, MAX_RAW, 1, 1, Some(MAX_RAW)),
        (
            "TYP-023.12",
            2,
            2_305_843_009_213_693_951,
            1,
            1,
            Some(4_611_686_018_427_387_902),
        ),
        ("TYP-023.13", 3, 2_305_843_009_213_693_951, 1, 1, None),
        ("TYP-023.14", 2_147_483_647, 2_305_843_009_213_693_951, 1, 1, None),
        ("TYP-023.15", 1, 4_611_686_018_427_387_904, 1, 1, None),
        ("TYP-023.16", 0, 10, 1, 1, None),
        ("TYP-023.17", 10, 0, 1, 1, None),
        ("TYP-023.18", -1, 10, 1, 1, None),
        ("TYP-023.19", 10, -1, 1, 1, None),
        ("TYP-023.20", 1, 1, 0, 1, None),
        ("TYP-023.21", 1, 1, 1, 0, None),
    ];
    for (row, lf, lh, num, den, want) in rows {
        assert_eq!(pair_lot(lf, lh, num, den), want.map(Qty), "{row}");
    }
}

#[test]
fn typ_025_side_helpers() {
    assert_eq!(Side::Buy.price_round(), Floor, "TYP-025.1");
    assert_eq!(Side::Sell.price_round(), Ceil, "TYP-025.2");
    assert_eq!(Side::Buy.sign(), 1, "TYP-025.3");
    assert_eq!(Side::Sell.sign(), -1, "TYP-025.4");
    assert_eq!(Side::Buy.opposite(), Side::Sell, "TYP-025.5");
    assert_eq!(Side::Sell.opposite(), Side::Buy, "TYP-025.6");
    const R: RoundMode = Side::Buy.price_round();
    assert_eq!(R, Floor, "TYP-025.7");
}

#[test]
fn typ_160_range_table_examples() {
    let m = TowardZero;
    assert_eq!(
        Px(1_000_000_000).mul_qty(Qty(5_000_000_000), e(-4, -8), m),
        (Notional(5_000_000_000_000), FxErr::OK),
        "TYP-160.1"
    );
    assert_eq!(
        Px(10_000_000).mul_qty(Qty(100_000_000_000_000), e(-12, -3), m),
        (Notional(1_000_000_000_000), FxErr::OK),
        "TYP-160.2"
    );
    assert_eq!(
        Px(100_000_000).mul_qty(Qty(10_000_000_000_000), e(-18, 0), m),
        (Notional(1_000_000_000), FxErr::OK),
        "TYP-160.3"
    );
    assert_eq!(
        parse_exact(b"1000000000000000000000", 3),
        Ok(1_000_000_000_000_000_000),
        "TYP-160.4"
    );
}
