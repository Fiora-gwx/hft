//! `CountingAlloc`: a global-allocator wrapper counting `alloc`/`alloc_zeroed`/`realloc` calls
//! per thread (`dealloc` is not counted), and `AllocScope`, the "allocations on this thread since
//! now" reading used by zero-allocation tests (hot-path-rules M1).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::marker::PhantomData;

thread_local! {
    // Const-initialized and without a destructor: reading it never allocates and works during
    // thread teardown.
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

#[inline]
fn bump() {
    let _ = ALLOCS.try_with(|c| c.set(c.get().wrapping_add(1)));
}

#[inline]
fn current() -> u64 {
    ALLOCS.try_with(Cell::get).unwrap_or(0)
}

/// Global allocator wrapper over `System`; install with `#[global_allocator]` in a test binary.
#[derive(Copy, Clone, Debug, Default)]
pub struct CountingAlloc;

impl CountingAlloc {
    /// Starts counting this thread's allocations from this moment.
    pub fn scope() -> AllocScope {
        AllocScope {
            start: current(),
            _thread_bound: PhantomData,
        }
    }

    /// Whether `CountingAlloc` is the installed global allocator (one boxed probe on this thread
    /// must be counted).
    pub fn is_installed() -> bool {
        let s = CountingAlloc::scope();
        let b = std::hint::black_box(Box::new(0u64));
        let n = s.allocs();
        drop(b);
        n >= 1
    }
}

/// Allocation count of the creating thread since `CountingAlloc::scope()`; not `Send`.
#[derive(Debug)]
pub struct AllocScope {
    start: u64,
    _thread_bound: PhantomData<*const ()>,
}

impl AllocScope {
    /// `alloc`/`alloc_zeroed`/`realloc` calls made by this thread since the scope began.
    pub fn allocs(&self) -> u64 {
        current().wrapping_sub(self.start)
    }
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged; the extra work
// only touches a const-initialized thread-local `Cell<u64>`, which never allocates.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump();
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
}
