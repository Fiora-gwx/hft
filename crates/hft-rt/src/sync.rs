//! Atomics shim: `loom` types under `--cfg loom`, `core` types otherwise. Only the primitives
//! with loom models (SPSC core, `Mailbox`, `SeqCell`, `KillWord`, the park state) use it;
//! shared-memory layouts always use `core` atomics.

#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, fence};

#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, fence};

pub(crate) use core::sync::atomic::Ordering;

/// Spin-wait hint: `cpu_relax` normally, a loom yield inside a model.
#[inline(always)]
pub(crate) fn spin_hint() {
    #[cfg(loom)]
    loom::thread::yield_now();
    #[cfg(not(loom))]
    crate::wait::cpu_relax();
}

/// `UnsafeCell` with the loom access API (`with`/`with_mut`); loom checks the accesses for races.
#[cfg(not(loom))]
#[derive(Debug)]
pub(crate) struct Cell<T>(core::cell::UnsafeCell<T>);

#[cfg(not(loom))]
impl<T> Cell<T> {
    pub(crate) const fn new(v: T) -> Cell<T> {
        Cell(core::cell::UnsafeCell::new(v))
    }
    #[inline(always)]
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }
}

#[cfg(loom)]
#[derive(Debug)]
pub(crate) struct Cell<T>(loom::cell::UnsafeCell<T>);

#[cfg(loom)]
impl<T> Cell<T> {
    pub(crate) fn new(v: T) -> Cell<T> {
        Cell(loom::cell::UnsafeCell::new(v))
    }
    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        self.0.with_mut(f)
    }
}

/// Per-slot access trackers of the SPSC core: no-ops normally; under loom each slot access is
/// registered on a `loom::cell::UnsafeCell<()>` so a read not ordered after its write is caught.
#[derive(Debug)]
pub(crate) struct Tracks {
    #[cfg(loom)]
    cells: Box<[loom::cell::UnsafeCell<()>]>,
}

impl Tracks {
    #[cfg(not(loom))]
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) const EMPTY: Tracks = Tracks {};

    #[cfg_attr(not(loom), allow(unused_variables))]
    pub(crate) fn new(n: usize) -> Tracks {
        Tracks {
            #[cfg(loom)]
            cells: (0..n).map(|_| loom::cell::UnsafeCell::new(())).collect(),
        }
    }
    #[inline(always)]
    #[cfg_attr(not(loom), allow(unused_variables))]
    pub(crate) fn write(&self, i: usize) {
        #[cfg(loom)]
        if let Some(c) = self.cells.get(i) {
            c.with_mut(|_| ());
        }
    }
    #[inline(always)]
    #[cfg_attr(not(loom), allow(unused_variables))]
    pub(crate) fn read(&self, i: usize) {
        #[cfg(loom)]
        if let Some(c) = self.cells.get(i) {
            c.with(|_| ());
        }
    }
}
