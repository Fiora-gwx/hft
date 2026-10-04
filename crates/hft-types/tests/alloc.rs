//! Zero allocation in every public function (TYP-180).
//!
//! A per-thread counting allocator stands in for `hft-testkit::CountingAlloc` until that crate
//! exists; only allocations made on the measuring thread while counting is enabled are counted.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use hft_types::*;

struct CountingAlloc;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
}

fn note() {
    // `try_with`: thread-local storage may be gone during thread teardown.
    let _ = ENABLED.try_with(|e| {
        if e.get() {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    });
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged; the only
// extra work touches const-initialized thread-locals, which never allocate.
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

fn count() -> u64 {
    COUNT.with(|c| c.get())
}

/// One call of every public function with the first row of TYP-010 – TYP-142.
fn call_all() {
    let e = AssetExp {
        px_exp: -4,
        qty_exp: -8,
    };
    let m = RoundMode::Floor;
    black_box(black_box(Px(1_000_000)).mul_qty(Qty(1), e, m));
    let _ = black_box(Px::from_notional(
        Notional(5_000_000_000_000),
        Qty(5_000_000_000),
        e,
        RoundMode::HalfEven,
    ));
    black_box(black_box(Px(1_000_000_000)).mul_rate(Rate(100_000), m));
    black_box(black_box(Notional(1_000_000_000)).mul_rate(Rate(200_000), RoundMode::Ceil));
    black_box(black_box(Px(123)).round_to_tick(10, m));
    black_box(black_box(Qty(1999)).round_to_lot(1000, m));
    black_box(black_box(Qty(1999)).split_lot(1000));
    black_box(Px::mid_floor(Px(3), Px(4)));
    black_box(black_box(Px(1_000_100_000)).ratio(Px(1_000_000_000)));
    black_box(pair_lot(black_box(1), 10, 1, 1));
    let lim = HedgePxLimits {
        conn_liveness_ms: 1000,
        rr_max_cross: Rate(5_000_000),
    };
    black_box(cap_cross_px(
        Side::Buy,
        Px(1_000_000),
        Px(999_000),
        Rate(10_000_000),
        &lim,
        1,
    ));
    black_box(funding_settle_norm_ns(
        black_box(1_791_100_800_003_000_000),
        28_800,
        0,
    ));
    let _ = black_box(parse_exact(black_box(b"0.00000001"), -8));
    let _ = black_box(parse_round(black_box(b"1.005"), -2, m));
    let mut out = [0u8; 40];
    black_box(write_fixed(black_box(1), -4, &mut out));
    let _ = black_box(qty_from_u256(
        &black_box([1_000_000_000_000_000_000, 0, 0, 0]),
        18,
        -8,
        m,
    ));
    let _ = black_box(qty_to_u256(Qty(100_000_000), 18, -8));
    black_box(GroupId::compose(StrategyId(1), black_box(1)));
    let x = ClientOrderId {
        epoch: 1,
        venue: VenueId(1),
        account: AccountId(1),
        slot: 5,
        r#gen: 7,
    };
    let mut t = [0u8; 24];
    black_box(&x).encode(&mut t);
    let _ = black_box(ClientOrderId::decode(black_box(b"100010101000050000000702")));
    black_box(KillBits::account(black_box(0)));
    black_box(KillBits::account_hard(black_box(0)));
    black_box(KillBits::gw_mask(black_box(0)));
    black_box(KillBits::hard_mask(black_box(0)));
}

#[test]
fn typ_180_zero_alloc() {
    call_all(); // warm up anything lazily initialized
    ENABLED.with(|e| e.set(true));
    let c0 = count();
    for _ in 0..10_000 {
        call_all();
    }
    let c1 = count();
    // The counter itself works: one boxed value is one allocation.
    drop(black_box(Box::new(1u64)));
    let c2 = count();
    ENABLED.with(|e| e.set(false));
    assert_eq!(c1 - c0, 0);
    assert_eq!(c2 - c1, 1);
}
