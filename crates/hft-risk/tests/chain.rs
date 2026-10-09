//! Check-chain cases (RSK-020–RSK-045, RSK-240).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
use common::*;
use hft_types::{Side, Tif};

#[test]
fn rsk_020_single_open_passes() {
    let mut w = r0();
    let it = open(1, G1, A1, Side::Buy, "0.01", "50000.00", Tif::Gtc);
    let a = w.eval(&it);
    let b = w.eval(&it);
    assert!(a.is_ok());
    assert_eq!(a, b);
}
