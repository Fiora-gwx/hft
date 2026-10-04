//! Table tests for decimal parsing (TYP-040 – TYP-044, TYP-051).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)] // table rows are tuples

use hft_types::RoundMode::{Ceil, Floor, HalfEven, TowardZero};
use hft_types::*;

const SYNTAX: FxErr = FxErr::SYNTAX;
const OVERFLOW: FxErr = FxErr::OVERFLOW;
const PRECISION: FxErr = FxErr::PRECISION;
const EXP_RANGE: FxErr = FxErr::EXP_RANGE;

#[test]
fn typ_040_parse_syntax_errors() {
    let mut long = b"1.".to_vec();
    long.extend([b'0'; 39]);
    assert_eq!(long.len(), 41);
    let rows: Vec<(&str, Vec<u8>, FxErr)> = vec![
        ("TYP-040.1", b"".to_vec(), FxErr::EMPTY),
        ("TYP-040.2", b"+1".to_vec(), SYNTAX),
        ("TYP-040.3", b".5".to_vec(), SYNTAX),
        ("TYP-040.4", b"5.".to_vec(), SYNTAX),
        ("TYP-040.5", b"1e".to_vec(), SYNTAX),
        ("TYP-040.6", b"1e1234".to_vec(), SYNTAX),
        ("TYP-040.7", b"--1".to_vec(), SYNTAX),
        ("TYP-040.8", b"1.2.3".to_vec(), SYNTAX),
        ("TYP-040.9", b"NaN".to_vec(), SYNTAX),
        ("TYP-040.10", b"Infinity".to_vec(), SYNTAX),
        ("TYP-040.11", b"null".to_vec(), SYNTAX),
        ("TYP-040.12", b" 1".to_vec(), SYNTAX),
        ("TYP-040.13", b"1 ".to_vec(), SYNTAX),
        ("TYP-040.14", b"1,000".to_vec(), SYNTAX),
        ("TYP-040.15", b"1_000".to_vec(), SYNTAX),
        ("TYP-040.16", b"0x10".to_vec(), SYNTAX),
        ("TYP-040.17", b"-".to_vec(), SYNTAX),
        ("TYP-040.18", b"e5".to_vec(), SYNTAX),
        ("TYP-040.19", b"1e+".to_vec(), SYNTAX),
        ("TYP-040.20", b"1.e5".to_vec(), SYNTAX),
        ("TYP-040.21", b"-.5".to_vec(), SYNTAX),
        ("TYP-040.22", b"1e5.0".to_vec(), SYNTAX),
        ("TYP-040.23", b"1ee5".to_vec(), SYNTAX),
        ("TYP-040.24", vec![0xD9, 0xA1], SYNTAX),
        ("TYP-040.25", vec![0x31, 0x00], SYNTAX),
        ("TYP-040.26", long, SYNTAX),
    ];
    for (row, s, err) in rows {
        assert_eq!(parse_exact(&s, -4), Err(err), "{row} exact");
        assert_eq!(parse_round(&s, -4, HalfEven), Err(err), "{row} round");
    }
}

#[test]
fn typ_041_parse_exact_values() {
    let mut s22 = b"1.".to_vec();
    s22.extend([b'0'; 38]);
    assert_eq!(s22.len(), 40);
    let mut s23 = vec![b'0'; 27];
    s23.push(b'1');
    let rows: Vec<(&str, Vec<u8>, Result<i64, FxErr>)> = vec![
        ("TYP-041.1", b"0".to_vec(), Ok(0)),
        ("TYP-041.2", b"-0".to_vec(), Ok(0)),
        ("TYP-041.3", b"-0.000".to_vec(), Ok(0)),
        ("TYP-041.4", b"12.5".to_vec(), Ok(125_000)),
        ("TYP-041.5", b"00012.5".to_vec(), Ok(125_000)),
        ("TYP-041.6", b"-12.5".to_vec(), Ok(-125_000)),
        ("TYP-041.7", b"0.0001".to_vec(), Ok(1)),
        ("TYP-041.8", b"0.00010".to_vec(), Ok(1)),
        ("TYP-041.9", b"0.10000000".to_vec(), Ok(1_000)),
        ("TYP-041.10", b"0.00012345".to_vec(), Err(PRECISION)),
        ("TYP-041.11", b"1.0E-4".to_vec(), Ok(1)),
        ("TYP-041.12", b"0E-8".to_vec(), Ok(0)),
        ("TYP-041.13", b"1e3".to_vec(), Ok(10_000_000)),
        ("TYP-041.14", b"1E+3".to_vec(), Ok(10_000_000)),
        ("TYP-041.15", b"1e004".to_vec(), Ok(100_000_000)),
        ("TYP-041.16", b"12345e-4".to_vec(), Ok(12_345)),
        ("TYP-041.17", b"1e-5".to_vec(), Err(PRECISION)),
        ("TYP-041.18", b"1.5e-4".to_vec(), Err(PRECISION)),
        ("TYP-041.19", b"1e999".to_vec(), Err(OVERFLOW)),
        ("TYP-041.20", b"0e999".to_vec(), Ok(0)),
        ("TYP-041.21", b"-1e-999".to_vec(), Err(PRECISION)),
        ("TYP-041.22", s22, Ok(10_000)),
        ("TYP-041.23", s23, Ok(10_000)),
    ];
    for (row, s, want) in rows {
        assert_eq!(parse_exact(&s, -4), want, "{row}");
    }
}

#[test]
fn typ_042_parse_exact_bounds() {
    let rows: [(&str, &[u8], i8, Result<i64, FxErr>); 20] = [
        ("TYP-042.1", b"4611686018427387903", 0, Ok(MAX_RAW)),
        ("TYP-042.2", b"4611686018427387904", 0, Err(OVERFLOW)),
        (
            "TYP-042.3",
            b"4611686018427387902",
            0,
            Ok(4_611_686_018_427_387_902),
        ),
        ("TYP-042.4", b"-4611686018427387903", 0, Ok(-MAX_RAW)),
        ("TYP-042.5", b"-4611686018427387904", 0, Err(OVERFLOW)),
        ("TYP-042.6", b"46116860184.27387903", -8, Ok(MAX_RAW)),
        ("TYP-042.7", b"46116860184.27387904", -8, Err(OVERFLOW)),
        ("TYP-042.8", b"12345678901234567891", 0, Err(OVERFLOW)),
        ("TYP-042.9", b"0.12345678901234567891", -4, Err(OVERFLOW)),
        ("TYP-042.10", b"4611686018427387903000000", 6, Ok(MAX_RAW)),
        (
            "TYP-042.11",
            b"1000000000000000000000",
            3,
            Ok(1_000_000_000_000_000_000),
        ),
        ("TYP-042.12", b"1.000000000000000000000", 0, Ok(1)),
        ("TYP-042.13", b"1", -30, Err(OVERFLOW)),
        ("TYP-042.14", b"0.000000000000000000000000000001", -30, Ok(1)),
        ("TYP-042.15", b"1", -31, Err(EXP_RANGE)),
        ("TYP-042.16", b"1", 19, Err(EXP_RANGE)),
        ("TYP-042.17", b"1000000000000000000", 18, Ok(1)),
        ("TYP-042.18", b"1", 18, Err(PRECISION)),
        ("TYP-042.19", b"", 19, Err(EXP_RANGE)),
        ("TYP-042.20", b"x", -31, Err(EXP_RANGE)),
    ];
    for (row, s, exp, want) in rows {
        assert_eq!(parse_exact(s, exp), want, "{row}");
    }
}

#[test]
fn typ_043_wire_exponent() {
    let rows: [(&str, &[u8], i8, i64); 6] = [
        ("TYP-043.1", b"0.0123456", -9, 12_345_600),
        ("TYP-043.2", b"5", -6, 5_000_000),
        ("TYP-043.3", b"100000", -4, 1_000_000_000),
        ("TYP-043.4", b"10000000", -4, 100_000_000_000),
        ("TYP-043.5", b"0.00001", -8, 1_000),
        (
            "TYP-043.6",
            b"1000000000000000000000",
            3,
            1_000_000_000_000_000_000,
        ),
    ];
    for (row, s, exp, want) in rows {
        assert_eq!(parse_exact(s, exp), Ok(want), "{row}");
    }
}

#[test]
fn typ_044_parse_round() {
    let rows: [(&str, &[u8], i8, RoundMode, Result<i64, FxErr>); 29] = [
        ("TYP-044.1", b"1.005", -2, Floor, Ok(100)),
        ("TYP-044.2", b"1.005", -2, Ceil, Ok(101)),
        ("TYP-044.3", b"1.005", -2, TowardZero, Ok(100)),
        ("TYP-044.4", b"1.005", -2, HalfEven, Ok(100)),
        ("TYP-044.5", b"1.015", -2, HalfEven, Ok(102)),
        ("TYP-044.6", b"-1.005", -2, Floor, Ok(-101)),
        ("TYP-044.7", b"-1.005", -2, Ceil, Ok(-100)),
        ("TYP-044.8", b"-1.005", -2, TowardZero, Ok(-100)),
        ("TYP-044.9", b"-1.005", -2, HalfEven, Ok(-100)),
        ("TYP-044.10", b"-1.015", -2, HalfEven, Ok(-102)),
        ("TYP-044.11", b"1.0050000001", -2, HalfEven, Ok(101)),
        ("TYP-044.12", b"1.00499999", -2, HalfEven, Ok(100)),
        ("TYP-044.13", b"0.001", -2, Ceil, Ok(1)),
        ("TYP-044.14", b"0.001", -2, Floor, Ok(0)),
        ("TYP-044.15", b"-0.001", -2, Floor, Ok(-1)),
        ("TYP-044.16", b"-0.001", -2, Ceil, Ok(0)),
        ("TYP-044.17", b"0.00012345", -9, HalfEven, Ok(123_450)),
        ("TYP-044.18", b"-0.000375", -9, HalfEven, Ok(-375_000)),
        ("TYP-044.19", b"1.0E-4", -9, HalfEven, Ok(100_000)),
        ("TYP-044.20", b"0.12345678901234567891", -4, HalfEven, Ok(1_235)),
        ("TYP-044.21", b"2.5", 0, HalfEven, Ok(2)),
        ("TYP-044.22", b"3.5", 0, HalfEven, Ok(4)),
        ("TYP-044.23", b"4611686018427387903.4", 0, Floor, Ok(MAX_RAW)),
        ("TYP-044.24", b"4611686018427387903.4", 0, Ceil, Err(OVERFLOW)),
        ("TYP-044.25", b"1e999", -4, Floor, Err(OVERFLOW)),
        ("TYP-044.26", b"1e-999", -4, Ceil, Ok(1)),
        ("TYP-044.27", b"1e-999", -4, Floor, Ok(0)),
        ("TYP-044.28", b"-1e-999", -4, Floor, Ok(-1)),
        ("TYP-044.29", b"1", -31, HalfEven, Err(EXP_RANGE)),
    ];
    for (row, s, exp, m, want) in rows {
        assert_eq!(parse_round(s, exp, m), want, "{row}");
    }
}

#[test]
fn typ_051_swar_chunk_boundaries() {
    let rows: [(&str, &[u8], i8, Result<i64, FxErr>); 11] = [
        ("TYP-051.1", b"1234567", 0, Ok(1_234_567)),
        ("TYP-051.2", b"12345678", 0, Ok(12_345_678)),
        ("TYP-051.3", b"123456789", 0, Ok(123_456_789)),
        ("TYP-051.4", b"1234567890123456", 0, Ok(1_234_567_890_123_456)),
        ("TYP-051.5", b"12345678901234567", 0, Ok(12_345_678_901_234_567)),
        ("TYP-051.6", b"1234567:", 0, Err(SYNTAX)),
        ("TYP-051.7", b"1234567/", 0, Err(SYNTAX)),
        ("TYP-051.8", b"12345678.12345678", -8, Ok(1_234_567_812_345_678)),
        ("TYP-051.9", b"0.0000000012345678", -16, Ok(12_345_678)),
        ("TYP-051.10", b"9999999999999999999", 0, Err(OVERFLOW)),
        ("TYP-051.11", b"99999999", 0, Ok(99_999_999)),
    ];
    for (row, s, exp, want) in rows {
        assert_eq!(parse_exact(s, exp), want, "{row}");
    }
}
