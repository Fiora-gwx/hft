//! `body_view` container checks and zero copy (SDK-098, SDK-099).
//!
//! A per-thread counting allocator stands in for `hft-testkit::CountingAlloc` until that crate
//! exists; only allocations on the measuring thread while counting is enabled are counted.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)] // table-driven rows

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use common::*;
use hft_strategy_sdk::params::*;
use zerocopy::IntoBytes;

struct CountingAlloc;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

fn note() {
    let _ = ENABLED.try_with(|e| {
        if e.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

// SAFETY: every method forwards to `System` unchanged; the extra work touches const-initialized
// thread-locals, which never allocate.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// The SDK-094 88 B body.
fn body88() -> Vec<u8> {
    let r = raw(
        vec![5_000_000, 20_000, 100_000, 0, 1, 0],
        &[(2, 0, 7_000_000), (0, 0, 6_000_000), (0, 0, 6_000_000)],
    );
    encode_body(&TP, 3, &r).unwrap()
}

#[test]
fn sdk_098_body_view_checks() {
    type Edit = fn(&mut Vec<u8>);
    let rows: [(u32, Edit, Result<(usize, usize), ParamError>); 11] = [
        (1, |_| {}, Ok((6, 2))),
        (2, |b| b.truncate(87), Err(ParamError::BodyLen)),
        (3, |b| b.extend_from_slice(&[0; 8]), Err(ParamError::BodyLen)),
        (4, |b| b.clear(), Err(ParamError::BodyLen)),
        (5, |b| b[4] = 3, Err(ParamError::BodyLen)),
        (6, |b| b[2] = 1, Err(ParamError::Padding)),
        // Second override starts at 72; its `_pad` (u32 at +4) = 0x0100_0000.
        (7, |b| b[72 + 7] = 1, Err(ParamError::Padding)),
        (
            8,
            |b| {
                let (a, c) = b.split_at_mut(72);
                a[56..72].swap_with_slice(&mut c[..16]);
            },
            Err(ParamError::OverrideOrder { ix: 1 }),
        ),
        (
            9,
            |b| {
                b[72] = 0;
                b[73] = 0;
            },
            Err(ParamError::OverrideOrder { ix: 1 }),
        ),
        (
            10,
            |b| b[72] = 3,
            Err(ParamError::GroupIxOutOfRange { group_ix: 3 }),
        ),
        (
            11,
            |b| {
                *b = vec![0; 8];
            },
            Ok((0, 0)),
        ),
    ];
    for (row, edit, want) in rows {
        let mut b = body88();
        edit(&mut b);
        let al = aligned(&b);
        let got = body_view(bytes_of(&al, b.len()), 3).map(|v| (v.defaults.len(), v.overrides.len()));
        assert_eq!(got, want, "SDK-098.{row}");
    }
}

#[test]
fn sdk_099_body_view_zero_copy() {
    let mut b = ParamsBodyHdr {
        n_fields: 64,
        _pad: 0,
        n_overrides: 4096,
    }
    .as_bytes()
    .to_vec();
    b.extend_from_slice([0i64; 64].as_bytes());
    for g in 0..64u16 {
        for f in 0..64u16 {
            b.extend_from_slice(ParamOverride::new(g, f, 1).as_bytes());
        }
    }
    let al = aligned(&b);
    let buf = bytes_of(&al, b.len());
    COUNT.with(|c| c.set(0));
    ENABLED.with(|e| e.set(true));
    let v = black_box(body_view(black_box(buf), 64));
    ENABLED.with(|e| e.set(false));
    let v = v.unwrap();
    assert_eq!(v.defaults.as_ptr() as usize, buf.as_ptr() as usize + 8);
    assert_eq!(v.overrides.len(), 4096);
    assert_eq!(COUNT.with(Cell::get), 0);
}
