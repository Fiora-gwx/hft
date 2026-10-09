//! Per-thread counting global allocator for the SDK tests (stands in for
//! `hft-testkit::CountingAlloc`, which depends on this crate): allocation/free counts and live /
//! peak bytes of the current thread while counting is enabled. A test binary installs it with
//! `#[global_allocator] static A: Counting = Counting;`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use hft_strategy_sdk::testing::AllocMeter;

pub struct Counting;

thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
    static FREES: Cell<u64> = const { Cell::new(0) };
    static LIVE: Cell<i64> = const { Cell::new(0) };
    static PEAK: Cell<i64> = const { Cell::new(0) };
}

fn on_alloc(n: usize) {
    let _ = ON.try_with(|on| {
        if on.get() {
            let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
            let _ = LIVE.try_with(|l| {
                l.set(l.get() + n as i64);
                let _ = PEAK.try_with(|p| p.set(p.get().max(l.get())));
            });
        }
    });
}

fn on_free(n: usize) {
    let _ = ON.try_with(|on| {
        if on.get() {
            let _ = FREES.try_with(|c| c.set(c.get() + 1));
            let _ = LIVE.try_with(|l| l.set(l.get() - n as i64));
        }
    });
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged; the extra work
// touches const-initialized thread-locals only, which never allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        on_alloc(layout.size());
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        on_alloc(layout.size());
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        on_alloc(new_size);
        on_free(layout.size());
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        on_free(layout.size());
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// Starts counting on this thread from zero.
pub fn start() {
    ALLOCS.with(|c| c.set(0));
    FREES.with(|c| c.set(0));
    LIVE.with(|c| c.set(0));
    PEAK.with(|c| c.set(0));
    ON.with(|c| c.set(true));
}

/// Stops counting; returns `(allocations, frees)`.
pub fn stop() -> (u64, u64) {
    ON.with(|c| c.set(false));
    (ALLOCS.with(Cell::get), FREES.with(Cell::get))
}

/// `(allocations, frees)` so far.
pub fn counts() -> (u64, u64) {
    (ALLOCS.with(Cell::get), FREES.with(Cell::get))
}

fn live() -> u64 {
    LIVE.with(Cell::get).max(0) as u64
}
fn peak() -> u64 {
    PEAK.with(Cell::get).max(0) as u64
}
fn reset_peak() {
    let l = LIVE.with(Cell::get);
    PEAK.with(|p| p.set(l));
}

/// The `ModelHarness` meter over this allocator (counting must be on: `start()`).
pub const METER: AllocMeter = AllocMeter {
    live,
    peak,
    reset_peak,
};
