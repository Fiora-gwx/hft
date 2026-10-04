//! `event_body!` compile-time assertions (EVT-170); one file per table row.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[test]
fn evt_170_event_body_static_asserts() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/evt_170_*.rs");
    t.pass("tests/compile_pass/evt_170_*.rs");
}
