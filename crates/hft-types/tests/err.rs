//! `FxErr` values and aggregation (TYP-155, TYP-156).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_types::{FxErr, parse_exact};

#[test]
fn typ_155_fx_err_values() {
    assert_eq!(FxErr::OK, FxErr(0), "TYP-155.1");
    assert_eq!(FxErr::SYNTAX, FxErr(1), "TYP-155.2");
    assert_eq!(FxErr::EMPTY, FxErr(2), "TYP-155.3");
    assert_eq!(FxErr::PRECISION, FxErr(4), "TYP-155.4");
    assert_eq!(FxErr::OVERFLOW, FxErr(8), "TYP-155.5");
    assert_eq!(FxErr::NEGATIVE, FxErr(16), "TYP-155.6");
    assert_eq!(FxErr::DIV_ZERO, FxErr(32), "TYP-155.7");
    assert_eq!(FxErr::EXP_RANGE, FxErr(64), "TYP-155.8");
    assert_eq!(size_of::<FxErr>(), 1, "TYP-155.9");
}

#[test]
fn typ_156_fx_err_aggregate() {
    let mut e = FxErr::OK;
    let mut first = None;
    for (i, s) in [&b"1.5"[..], b"", b"x"].into_iter().enumerate() {
        match parse_exact(s, -4) {
            Ok(v) if i == 0 => first = Some(v),
            Ok(v) => panic!("field {i} unexpectedly parsed to {v}"),
            Err(b) => e |= b,
        }
    }
    assert_eq!(first, Some(15_000));
    assert_eq!(e, FxErr(3));
    assert_eq!(e, FxErr::EMPTY | FxErr::SYNTAX);
    assert_eq!(e | FxErr::OVERFLOW, FxErr(11));
}
