//! U256 ↔ Qty (TYP-070, TYP-071).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)] // table rows are tuples

use hft_types::RoundMode::{Ceil, Floor, HalfEven, TowardZero};
use hft_types::*;

/// Decimal value → little-endian limbs (`u128` covers every row except 2^256 − 1).
fn limbs(v: u128) -> U256Limbs {
    [v as u64, (v >> 64) as u64, 0, 0]
}

#[test]
fn typ_070_qty_from_u256() {
    let e18 = 10u128.pow(18);
    let rows: [(&str, U256Limbs, u8, i8, RoundMode, Result<i64, FxErr>); 22] = [
        ("TYP-070.1", limbs(e18), 18, -8, Floor, Ok(100_000_000)),
        ("TYP-070.2", limbs(e18 + 1), 18, -8, Floor, Ok(100_000_000)),
        ("TYP-070.3", limbs(e18 + 1), 18, -8, Ceil, Ok(100_000_001)),
        ("TYP-070.4", limbs(e18 + 1), 18, -8, TowardZero, Ok(100_000_000)),
        ("TYP-070.5", limbs(e18 + 1), 18, -8, HalfEven, Ok(100_000_000)),
        ("TYP-070.6", limbs(5 * 10u128.pow(9)), 18, -8, HalfEven, Ok(0)),
        ("TYP-070.7", limbs(5 * 10u128.pow(9)), 18, -8, Ceil, Ok(1)),
        ("TYP-070.8", limbs(15 * 10u128.pow(9)), 18, -8, HalfEven, Ok(2)),
        ("TYP-070.9", limbs(10u128.pow(6)), 6, -8, Floor, Ok(100_000_000)),
        (
            "TYP-070.10",
            limbs(46_116_860_184_273_879),
            6,
            -8,
            Floor,
            Ok(4_611_686_018_427_387_900),
        ),
        (
            "TYP-070.11",
            limbs(46_116_860_184_273_880),
            6,
            -8,
            Floor,
            Err(FxErr::OVERFLOW),
        ),
        ("TYP-070.12", [u64::MAX; 4], 18, -8, Floor, Err(FxErr::OVERFLOW)),
        ("TYP-070.13", limbs(75 * 10u128.pow(35)), 30, 6, HalfEven, Ok(8)),
        ("TYP-070.14", limbs(75 * 10u128.pow(35)), 30, 6, Floor, Ok(7)),
        ("TYP-070.15", limbs(10u128.pow(19)), 19, 0, Floor, Ok(1)),
        ("TYP-070.16", limbs(2 * 10u128.pow(20)), 20, 0, Floor, Ok(2)),
        (
            "TYP-070.17",
            limbs(1),
            0,
            -18,
            Floor,
            Ok(1_000_000_000_000_000_000),
        ),
        ("TYP-070.18", limbs(1), 31, 6, Floor, Err(FxErr::EXP_RANGE)),
        ("TYP-070.19", limbs(1), 0, -19, Floor, Err(FxErr::EXP_RANGE)),
        ("TYP-070.20", limbs(0), 18, -8, Ceil, Ok(0)),
        ("TYP-070.21", limbs(MAX_RAW as u128), 0, 0, Floor, Ok(MAX_RAW)),
        ("TYP-070.22", [0, 1, 0, 0], 0, 0, Floor, Err(FxErr::OVERFLOW)),
    ];
    for (row, v, td, qe, m, want) in rows {
        assert_eq!(qty_from_u256(&v, td, qe, m), want.map(Qty), "{row}");
    }
}

#[test]
fn typ_071_qty_to_u256() {
    let rows: [(&str, i64, u8, i8, Result<U256Limbs, FxErr>); 7] = [
        ("TYP-071.1", 100_000_000, 18, -8, Ok(limbs(10u128.pow(18)))),
        ("TYP-071.2", -1, 18, -8, Err(FxErr::NEGATIVE)),
        ("TYP-071.3", 100_000_000, 6, -8, Ok(limbs(10u128.pow(6)))),
        ("TYP-071.4", 123, 6, -8, Err(FxErr::PRECISION)),
        (
            "TYP-071.5",
            MAX_RAW,
            30,
            6,
            Ok(mul_u128(MAX_RAW as u128, 10u128.pow(36))),
        ),
        ("TYP-071.6", 1, 31, 6, Err(FxErr::EXP_RANGE)),
        ("TYP-071.7", 0, 18, -8, Ok(limbs(0))),
    ];
    for (row, q, td, qe, want) in rows {
        assert_eq!(qty_to_u256(Qty(q), td, qe), want, "{row}");
    }
}

/// Full 256-bit product of two u128 values, by 32-bit schoolbook multiplication.
fn mul_u128(a: u128, b: u128) -> U256Limbs {
    let split = |x: u128| [x as u32, (x >> 32) as u32, (x >> 64) as u32, (x >> 96) as u32];
    let (a, b) = (split(a), split(b));
    let mut r = [0u64; 9];
    for i in 0..4 {
        let mut carry = 0u64;
        for j in 0..4 {
            let t = r[i + j] + a[i] as u64 * b[j] as u64 + carry;
            r[i + j] = t & 0xFFFF_FFFF;
            carry = t >> 32;
        }
        r[i + 4] += carry;
    }
    [
        r[0] | r[1] << 32,
        r[2] | r[3] << 32,
        r[4] | r[5] << 32,
        r[6] | r[7] << 32,
    ]
}
