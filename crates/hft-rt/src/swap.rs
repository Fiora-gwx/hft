//! `EpochSwap` (optional): one publisher, ≤ 16 readers each with an ack line, 4 retire slots.
//!
//! Orders: `publish` swaps `current` then Release-stores `version`; a reader's Acquire load of
//! `version` therefore sees a `current` at least that new. A reader Release-stores the version
//! it acknowledges after it has stopped using older pointers; the reclaimer Acquire-loads every
//! ack before freeing a retired pointer whose retire version ≤ min(ack).

use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
#[allow(clippy::disallowed_types)] // allow-list: see the ring/handle owners below
use std::sync::Arc;

use crate::wait::CachePadded;

const RETIRE_SLOTS: usize = 4;
/// Maximum readers.
pub const MAX_SWAP_READERS: usize = 16;

struct Retired<T> {
    ptr: AtomicPtr<T>,
    version: AtomicU64,
}

struct Inner<T> {
    current: AtomicPtr<T>,
    version: AtomicU64,
    acks: Box<[CachePadded<AtomicU64>]>,
    retired: [Retired<T>; RETIRE_SLOTS],
}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        let cur = *self.current.get_mut();
        if !cur.is_null() {
            // SAFETY: `current` came from `Box::into_raw` and nobody else can use it now.
            drop(unsafe { Box::from_raw(cur) });
        }
        for r in self.retired.iter_mut() {
            let p = *r.ptr.get_mut();
            if !p.is_null() {
                // SAFETY: retired pointers came from `Box::into_raw` and are owned here.
                drop(unsafe { Box::from_raw(p) });
            }
        }
    }
}

// SAFETY: readers get `&T` across threads (T: Sync); values are dropped on the reclaimer or the
// last owner's thread (T: Send).
unsafe impl<T: Send + Sync> Send for Inner<T> {}
// SAFETY: as above.
unsafe impl<T: Send + Sync> Sync for Inner<T> {}

/// Namespace of the version exchange.
#[derive(Debug)]
pub struct EpochSwap<T>(core::marker::PhantomData<T>);

/// The single publisher.
#[allow(clippy::disallowed_types)] // allow-list: shared version-exchange owner, never cloned after construction
pub struct SwapPublisher<T> {
    inner: Arc<Inner<T>>,
}

/// One reader (≤ 16).
#[allow(clippy::disallowed_types)] // allow-list: shared version-exchange owner, never cloned after construction
pub struct SwapReader<T> {
    inner: Arc<Inner<T>>,
    idx: usize,
    seen: u64,
    ptr: *const T,
}

/// Housekeeping side that frees acknowledged old versions.
#[allow(clippy::disallowed_types)] // allow-list: shared version-exchange owner, never cloned after construction
pub struct SwapReclaimer<T> {
    inner: Arc<Inner<T>>,
}

// SAFETY: each handle is used by one thread at a time; shared state is atomics; T: Send + Sync.
unsafe impl<T: Send + Sync> Send for SwapReader<T> {}

impl<T> EpochSwap<T> {
    /// `readers` is clamped to `MAX_SWAP_READERS`.
    #[allow(clippy::disallowed_types)] // allow-list: shared version-exchange owner
    #[allow(clippy::new_ret_no_self)] // README signature: the three handles, no `EpochSwap` value
    pub fn new(initial: Box<T>, readers: usize) -> (SwapPublisher<T>, Vec<SwapReader<T>>, SwapReclaimer<T>) {
        let n = readers.min(MAX_SWAP_READERS);
        let p = Box::into_raw(initial);
        let inner = Arc::new(Inner {
            current: AtomicPtr::new(p),
            version: AtomicU64::new(0),
            acks: (0..n).map(|_| CachePadded(AtomicU64::new(0))).collect(),
            retired: core::array::from_fn(|_| Retired {
                ptr: AtomicPtr::new(ptr::null_mut()),
                version: AtomicU64::new(0),
            }),
        });
        let rs = (0..n)
            .map(|idx| SwapReader {
                inner: inner.clone(),
                idx,
                seen: 0,
                ptr: p,
            })
            .collect();
        (
            SwapPublisher { inner: inner.clone() },
            rs,
            SwapReclaimer { inner },
        )
    }
}

impl<T> SwapPublisher<T> {
    /// No allocation, no free; all retire slots full → `Err(next)`.
    pub fn publish(&mut self, next: Box<T>) -> Result<u64, Box<T>> {
        let i = &*self.inner;
        // Acquire pairs with the reclaimer's Release store of null (slot free again).
        let Some(slot) = i.retired.iter().find(|r| r.ptr.load(Ordering::Acquire).is_null()) else {
            return Err(next);
        };
        let v = i.version.load(Ordering::Relaxed).wrapping_add(1);
        let old = i.current.swap(Box::into_raw(next), Ordering::AcqRel);
        // Release pairs with readers' Acquire load of `version`.
        i.version.store(v, Ordering::Release);
        slot.version.store(v, Ordering::Relaxed);
        // Release pairs with the reclaimer's Acquire load of the slot pointer.
        slot.ptr.store(old, Ordering::Release);
        Ok(v)
    }
}

impl<T> SwapReader<T> {
    /// Once per loop iteration: one Acquire load of `version`.
    #[inline(always)]
    pub fn get(&mut self) -> &T {
        let i = &*self.inner;
        let v = i.version.load(Ordering::Acquire);
        if v != self.seen {
            self.ptr = i.current.load(Ordering::Acquire);
            self.seen = v;
            if let Some(a) = i.acks.get(self.idx) {
                // Release pairs with the reclaimer's Acquire load of the ack.
                a.store(v, Ordering::Release);
            }
        }
        // SAFETY: `ptr` is a published version ≥ `seen`; it is retired only by a later publish
        // and freed only after this reader acks a version ≥ that retire version, which happens
        // in a later `get` (after this borrow ends).
        unsafe { &*self.ptr }
    }
}

impl<T> SwapReclaimer<T> {
    /// Frees every retired version all readers have acknowledged; the number freed.
    pub fn reclaim(&mut self) -> usize {
        let i = &*self.inner;
        let min_ack = i
            .acks
            .iter()
            .map(|a| a.load(Ordering::Acquire))
            .min()
            .unwrap_or(u64::MAX);
        let mut freed = 0;
        for r in i.retired.iter() {
            let p = r.ptr.load(Ordering::Acquire);
            if p.is_null() || r.version.load(Ordering::Relaxed) > min_ack {
                continue;
            }
            // Release pairs with the publisher's Acquire search for a free slot.
            r.ptr.store(ptr::null_mut(), Ordering::Release);
            // SAFETY: every reader has moved past this version; the pointer came from
            // `Box::into_raw` and is now owned here alone.
            drop(unsafe { Box::from_raw(p) });
            freed += 1;
        }
        freed
    }
}

impl<T> core::fmt::Debug for SwapPublisher<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SwapPublisher").finish_non_exhaustive()
    }
}
impl<T> core::fmt::Debug for SwapReader<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SwapReader")
            .field("idx", &self.idx)
            .field("seen", &self.seen)
            .finish_non_exhaustive()
    }
}
impl<T> core::fmt::Debug for SwapReclaimer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SwapReclaimer").finish_non_exhaustive()
    }
}
