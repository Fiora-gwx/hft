//! NET-009, NET-041: compile-time rules (trybuild, one source file per step in `tests/ui/`).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[test]
fn net_009_masked_frame_cap_over_u16() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/net_009_cap_65536.rs");
    t.pass("tests/ui/net_009_cap_65535_pass.rs");
}

#[test]
fn net_041_frame_borrow_exclusive() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/net_041_frame_borrow.rs");
}
