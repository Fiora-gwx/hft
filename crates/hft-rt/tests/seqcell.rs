//! SeqCell single-thread store/load (RT-043).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

use hft_rt::{SeqCell, Tsc, TscUtc};

#[test]
fn rt_043_seqcell_store_load() {
    let init = TscUtc {
        base_tsc: Tsc(0),
        base_utc_ns: 0,
        mult: 1 << 32,
        shift: 32,
        err_ns: 0,
    };
    let cell = SeqCell::new(init);
    let v = TscUtc {
        base_tsc: Tsc(5),
        err_ns: 7,
        ..init
    };
    cell.store(v);
    assert_eq!(cell.load(), v);
}
