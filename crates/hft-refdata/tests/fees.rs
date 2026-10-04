//! Fee resolution precedence (REF-052).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_refdata::{FeeOverrideSpec, FeePair, InstKind};
use hft_types::{AccountId, InstId};

fn ov(inst: u32, maker: i64, taker: i64) -> FeeOverrideSpec {
    FeeOverrideSpec {
        account: AccountId(1),
        venue: V6,
        inst: InstId(inst),
        fee: fee(maker, taker),
    }
}

fn fees_with(overrides: &[FeeOverrideSpec], account: u8, inst_id: u32) -> FeePair {
    let mut s = f0_spec();
    s.insts.push(inst(
        6,
        "ETH-USDT",
        V6,
        InstKind::Spot,
        ETH,
        ("0.1", 10_000_000),
        ("0.00000001", 1),
        ("0.00001", 1_000),
    ));
    s.fee_overrides = overrides.to_vec();
    ok(&s.build()).fees(AccountId(account), InstId(inst_id))
}

#[test]
fn ref_052_fee_precedence() {
    let all = ov(0, 300_000, 700_000);
    let one = ov(4, 400_000, 900_000);
    assert_eq!(fees_with(&[], 1, 4), fee(800_000, 1_000_000), "REF-052.1");
    assert_eq!(fees_with(&[], 2, 4), fee(600_000, 800_000), "REF-052.2");
    assert_eq!(fees_with(&[all], 1, 4), fee(300_000, 700_000), "REF-052.3");
    assert_eq!(fees_with(&[all, one], 1, 4), fee(400_000, 900_000), "REF-052.4");
    assert_eq!(fees_with(&[all, one], 1, 6), fee(300_000, 700_000), "REF-052.5");
    assert_eq!(
        fees_with(&[ov(4, -50_000, 500_000)], 1, 4).maker.0,
        -50_000,
        "REF-052.6"
    );
    assert_eq!(fees_with(&[all], 2, 4), fee(600_000, 800_000), "REF-052.7");
}
