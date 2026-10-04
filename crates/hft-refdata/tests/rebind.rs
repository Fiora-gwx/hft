//! Binding changes need an idle account; `ref_inst` must be subscribable (REF-064, REF-065).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_refdata::InstStatus;
use hft_refdata::LoadErrCode::{BindingChangedWithPosition, RefInstNotSubscribed};
use hft_types::{AccountId, InstId, StrategyId};

fn with_bindings(version: u64, b1: u8, b2: u8) -> hft_refdata::RefData {
    let mut s = f0_spec();
    s.version = version;
    s.account_mut(1).bound_instance = StrategyId(b1);
    s.account_mut(2).bound_instance = StrategyId(b2);
    ok(&s.build())
}

#[test]
fn ref_064_check_rebind() {
    let prev = with_bindings(10, 1, 2);
    let errs_of =
        |new: &hft_refdata::RefData, prev: &hft_refdata::RefData, busy: &dyn Fn(AccountId) -> bool| {
            new.check_rebind(prev, busy)
                .err()
                .map(|r| r.errors)
                .unwrap_or_default()
        };
    let bcp = |row: u32| e(BindingChangedWithPosition, 4, row);
    let only1 = |a: AccountId| a == AccountId(1);
    let all = |_: AccountId| true;
    let none = |_: AccountId| false;
    assert_eq!(
        errs_of(&with_bindings(11, 3, 2), &prev, &only1),
        vec![bcp(1)],
        "REF-064.1"
    );
    assert_eq!(
        errs_of(&with_bindings(11, 3, 2), &prev, &none),
        vec![],
        "REF-064.2"
    );
    assert_eq!(
        errs_of(&with_bindings(11, 1, 2), &prev, &all),
        vec![],
        "REF-064.3"
    );
    assert_eq!(
        errs_of(&with_bindings(11, 0, 2), &prev, &only1),
        vec![bcp(1)],
        "REF-064.4"
    );
    let prev0 = with_bindings(10, 0, 2);
    assert_eq!(
        errs_of(&with_bindings(11, 1, 2), &prev0, &only1),
        vec![bcp(1)],
        "REF-064.5"
    );
    assert_eq!(
        errs_of(&with_bindings(11, 3, 1), &prev, &all),
        vec![bcp(1), bcp(2)],
        "REF-064.6"
    );
}

#[test]
fn ref_065_check_ref_inst() {
    let with = |status: InstStatus| {
        let mut s = f0_spec();
        s.inst_mut(2).status = status;
        ok(&s.build())
    };
    let code = |r: Result<(), hft_refdata::LoadErr>| r.err().map(|e| e.code);
    let cases = [
        ("REF-065.1", InstStatus::Trading, None),
        ("REF-065.2", InstStatus::ReduceOnly, None),
        ("REF-065.3", InstStatus::Halted, Some(RefInstNotSubscribed)),
        ("REF-065.4", InstStatus::Delisted, Some(RefInstNotSubscribed)),
        ("REF-065.5", InstStatus::PreTrading, Some(RefInstNotSubscribed)),
        ("REF-065.6", InstStatus::Maintenance, Some(RefInstNotSubscribed)),
    ];
    for (case, status, want) in cases {
        assert_eq!(code(with(status).check_ref_inst(InstId(2))), want, "{case}");
    }
    let rd = ok(&f0());
    assert_eq!(
        code(rd.check_ref_inst(InstId(99))),
        Some(RefInstNotSubscribed),
        "REF-065.7"
    );
    assert_eq!(
        code(rd.check_ref_inst(InstId(0))),
        Some(RefInstNotSubscribed),
        "REF-065.8"
    );
}
