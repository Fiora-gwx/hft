//! Table test for `write_fixed` (TYP-046).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_types::{MAX_RAW, write_fixed};

#[test]
fn typ_046_write_fixed() {
    let z29 = "0".repeat(29);
    let z18 = "0".repeat(18);
    let rows: Vec<(&str, i64, i8, String)> = vec![
        ("TYP-046.1", 0, -4, "0".into()),
        ("TYP-046.2", 1, -4, "0.0001".into()),
        ("TYP-046.3", -1, -4, "-0.0001".into()),
        ("TYP-046.4", 10_000, -4, "1".into()),
        ("TYP-046.5", 125_000, -4, "12.5".into()),
        ("TYP-046.6", -125_000, -4, "-12.5".into()),
        ("TYP-046.7", 1_000, -4, "0.1".into()),
        ("TYP-046.8", 12_340, -4, "1.234".into()),
        ("TYP-046.9", 5, 3, "5000".into()),
        ("TYP-046.10", MAX_RAW, 0, "4611686018427387903".into()),
        ("TYP-046.11", -MAX_RAW, -8, "-46116860184.27387903".into()),
        ("TYP-046.12", MAX_RAW, 6, "4611686018427387903000000".into()),
        ("TYP-046.13", 1, -18, "0.000000000000000001".into()),
        ("TYP-046.14", 1, -30, format!("0.{z29}1")),
        ("TYP-046.15", -1, -30, format!("-0.{z29}1")),
        ("TYP-046.16", i64::MAX, 18, format!("9223372036854775807{z18}")),
        ("TYP-046.17", i64::MIN, 18, format!("-9223372036854775808{z18}")),
    ];
    for (row, raw, exp, want) in rows {
        let mut out = [0xAAu8; 40];
        let n = write_fixed(raw, exp, &mut out);
        assert_eq!(&out[..n], want.as_bytes(), "{row}");
        assert!(
            out[n..].iter().all(|&b| b == 0xAA),
            "{row}: bytes past n untouched"
        );
    }
    assert_eq!(write_fixed(1, -30, &mut [0; 40]), 32, "TYP-046.14 n");
    assert_eq!(write_fixed(-1, -30, &mut [0; 40]), 33, "TYP-046.15 n");
    assert_eq!(write_fixed(i64::MAX, 18, &mut [0; 40]), 37, "TYP-046.16 n");
    assert_eq!(write_fixed(i64::MIN, 18, &mut [0; 40]), 38, "TYP-046.17 n");
    for (row, exp) in [("TYP-046.18", 19i8), ("TYP-046.19", -31)] {
        let mut out = [0xAAu8; 40];
        assert_eq!(write_fixed(1, exp, &mut out), 0, "{row}");
        assert_eq!(out, [0xAA; 40], "{row}");
    }
}
