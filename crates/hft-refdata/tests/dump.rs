//! REF-078: readable dump and two-version diff (feature `tools`).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(feature = "tools")]

use crate::common;

use common::*;
use hft_refdata::{DiffItem, UidPolicy, diff, dump};

#[test]
fn ref_078_dump_and_diff() {
    let a = ok(&f0());
    let mut s = f0_spec();
    s.version = 11;
    s.account_mut(1).is_prod = false;
    s.account_mut(1).uid_policy = UidPolicy::Unverifiable;
    let b = ok(&s.build());
    let (t1, t2) = (dump(&a), dump(&a));
    assert_eq!(t1, t2);
    let section = t1.split("[accounts.acct-a]").nth(1).expect("account 1 section");
    let section = section.split("\n[").next().unwrap_or("");
    assert!(section.contains("uid_policy = \"verified\""), "{section}");
    let item = |path: &str, old: &str, new: &str| DiffItem {
        path: path.into(),
        old: old.into(),
        new: new.into(),
    };
    assert_eq!(
        diff(&a, &b),
        vec![
            item("accounts.acct-a.is_prod", "true", "false"),
            item("accounts.acct-a.uid_policy", "verified", "unverifiable"),
            item("version", "10", "11"),
        ]
    );
}
