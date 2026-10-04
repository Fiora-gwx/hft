//! Step ordering of `LoadReport` (REF-010, REF-011).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_refdata::LoadErrCode::{Ids, Structure};
use hft_types::StrategyId;

#[test]
fn ref_010_stop_after_failing_step() {
    let mut s = f0_spec();
    s.account_mut(2).label = "acct-a".into();
    renumber_assets(&mut s, &[1, 2, 4, 5, 6]);
    let mut b = s.build();
    b[8..10].copy_from_slice(&2u16.to_le_bytes());
    let r = load(&b).expect_err("must fail");
    assert_eq!(r.errors, vec![e(Structure, 0, 0)]);
    assert_eq!(r.code(), Structure);
    assert_eq!(r.code() as u16, 1);
}

#[test]
fn ref_011_collect_within_step() {
    let mut s = f0_spec();
    renumber_assets(&mut s, &[1, 2, 3, 5, 6]);
    s.instances[2].id = StrategyId(4);
    let r = load(&s.build()).expect_err("must fail");
    assert_eq!(r.errors, vec![e(Ids, 1, 3), e(Ids, 15, 2)]);
    assert_eq!(r.code(), Ids);
    assert_eq!(r.code() as u16, 3);
}
