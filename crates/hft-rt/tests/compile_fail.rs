//! Shared-ring element size and alignment limits are compile-time errors (RT-028); one source
//! file per table row in `tests/ui/`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

#[test]
fn rt_028_shm_elem_constraints() {
    let t = trybuild::TestCases::new();
    for row in [1, 4, 5, 6, 7] {
        t.compile_fail(format!("tests/ui/rt_028_{row}_*.rs"));
    }
    t.pass("tests/ui/rt_028_2_u64x1_pass.rs");
    t.pass("tests/ui/rt_028_3_u64x512_pass.rs");
}
