//! TK-100: the checkers match docs/tests/invariants.md one to one.

use std::collections::BTreeSet;

use hft_testkit::dst::invariants::{CHECKERS, InvariantSet};

#[test]
fn tk_100_checkers_match_invariants_md() {
    let md = include_str!("../../../docs/tests/invariants.md");
    let e: BTreeSet<u8> = md
        .lines()
        .filter_map(|l| l.strip_prefix("### INV-"))
        .filter_map(|l| l.split_whitespace().next()?.parse().ok())
        .collect();
    let set: BTreeSet<u8> = InvariantSet::all().iter().map(|i| i.0).collect();
    let table: BTreeSet<u8> = CHECKERS.iter().map(|c| c.id.0).collect();
    assert_eq!(set, table);
    // INV-10 is the optional chain extension: no checker in a non-chain build.
    assert!(!e.contains(&10) && !set.contains(&10));
    assert_eq!(set, e, "one checker per INV-n");
    let src = include_str!("../src/dst/invariants.rs");
    for n in &e {
        let prefix = format!("pub fn inv_{n}_");
        assert_eq!(src.matches(&prefix).count(), 1, "exactly one inv_{n}_… function");
        let reg = format!("\"INV-{n}\"");
        assert_eq!(src.matches(&reg).count(), 1, "INV-{n} registered once");
    }
}
