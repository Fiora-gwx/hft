//! SPSC core: cursors, cached opposite cursors and batches, generic over where the memory
//! lives (heap/huge-page allocation here, a shared file mapping in `shm`).
//!
//! Ordering pairs: the producer's Release store of `head` (`try_push`, `WriteBatch::commit`)
//! pairs with the consumer's Acquire load of `head` (`try_pop`, `read_batch`, `len_hint`); the
//! consumer's Release store of `tail` (`try_pop`, `ReadBatch::release`) pairs with the
//! producer's Acquire load of `tail` (`try_push` when apparently full, `write_batch`, `free_hint`).

use core::mem::MaybeUninit;
use core::ptr::NonNull;
#[allow(clippy::disallowed_types)] // allow-list: see the ring/handle owners below
use std::sync::Arc;

use super::{Lines, RingAlloc};
use crate::RtErr;
use crate::idle::Unparker;
use crate::mem::PageSize;
use crate::sync::{Ordering, Tracks};

/// Where an in-process ring's memory comes from.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Backing {
    Heap,
    Huge(PageSize),
}

/// Producer half of the core (also used by `ShmProducer`).
pub(crate) struct ProdCore<T> {
    lines: NonNull<Lines>,
    slots: NonNull<MaybeUninit<T>>,
    tracks: NonNull<Tracks>,
    mask: u64,
    cap: u64,
    head: u64,
    cached_tail: u64,
    pub(crate) wake: Option<Unparker>,
}

/// Consumer half of the core (also used by `ShmConsumer`).
pub(crate) struct ConsCore<T> {
    lines: NonNull<Lines>,
    slots: NonNull<MaybeUninit<T>>,
    tracks: NonNull<Tracks>,
    mask: u64,
    cap: u64,
    tail: u64,
    cached_head: u64,
}

impl<T> ProdCore<T> {
    /// # Safety
    /// `lines`, `slots` (`cap` elements, `cap` a power of two) and `tracks` stay valid for the
    /// life of the core, and no other producer exists for these lines.
    pub(crate) unsafe fn new(
        lines: NonNull<Lines>,
        slots: NonNull<T>,
        tracks: NonNull<Tracks>,
        cap: u64,
    ) -> Self {
        // SAFETY: `lines` is valid per the contract.
        let l = unsafe { lines.as_ref() };
        // Acquire: continue after a previous producer incarnation (shared memory) and see the
        // consumer's latest release.
        let head = l.prod.load(Ordering::Acquire);
        let cached_tail = l.cons.load(Ordering::Acquire);
        ProdCore {
            lines,
            slots: slots.cast(),
            tracks,
            mask: cap - 1,
            cap,
            head,
            cached_tail,
            wake: None,
        }
    }

    #[inline(always)]
    fn lines(&self) -> &Lines {
        // SAFETY: valid for the life of the core (constructor contract).
        unsafe { self.lines.as_ref() }
    }

    #[inline(always)]
    fn tracks(&self) -> &Tracks {
        // SAFETY: valid for the life of the core (constructor contract).
        unsafe { self.tracks.as_ref() }
    }

    #[inline(always)]
    fn notify(&self) {
        if let Some(u) = &self.wake {
            u.unpark();
        }
    }

    #[inline(always)]
    fn write_slot(&mut self, idx: u64, v: T) {
        let i = (idx & self.mask) as usize;
        self.tracks().write(i);
        // SAFETY: `i < cap`; the slot is free (index ≥ tail + 0 and < tail + cap), so the
        // consumer does not read it until the next Release of `head`.
        unsafe { self.slots.as_ptr().add(i).write(MaybeUninit::new(v)) };
    }

    #[inline(always)]
    pub(crate) fn try_push(&mut self, v: T) -> Result<(), T> {
        if self.head.wrapping_sub(self.cached_tail) >= self.cap {
            // Acquire pairs with the consumer's Release store of `tail`.
            self.cached_tail = self.lines().cons.load(Ordering::Acquire);
            if self.head.wrapping_sub(self.cached_tail) >= self.cap {
                return Err(v);
            }
        }
        self.write_slot(self.head, v);
        self.head = self.head.wrapping_add(1);
        // Release pairs with the consumer's Acquire load of `head`.
        self.lines().prod.store(self.head, Ordering::Release);
        self.notify();
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn write_batch(&mut self, max: usize) -> WriteBatch<'_, T> {
        // Acquire pairs with the consumer's Release store of `tail`.
        self.cached_tail = self.lines().cons.load(Ordering::Acquire);
        let free = self.cap - self.head.wrapping_sub(self.cached_tail).min(self.cap);
        let avail = free.min(max as u64);
        WriteBatch {
            core: self,
            avail,
            n: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn free_hint(&self) -> usize {
        // Acquire pairs with the consumer's Release store of `tail`.
        let tail = self.lines().cons.load(Ordering::Acquire);
        (self.cap - self.head.wrapping_sub(tail).min(self.cap)) as usize
    }
}

impl<T> ConsCore<T> {
    /// # Safety
    /// Same as `ProdCore::new`, for the single consumer.
    pub(crate) unsafe fn new(
        lines: NonNull<Lines>,
        slots: NonNull<T>,
        tracks: NonNull<Tracks>,
        cap: u64,
    ) -> Self {
        // SAFETY: `lines` is valid per the contract.
        let l = unsafe { lines.as_ref() };
        let tail = l.cons.load(Ordering::Acquire);
        let cached_head = l.prod.load(Ordering::Acquire);
        ConsCore {
            lines,
            slots: slots.cast(),
            tracks,
            mask: cap - 1,
            cap,
            tail,
            cached_head,
        }
    }

    #[inline(always)]
    fn lines(&self) -> &Lines {
        // SAFETY: valid for the life of the core (constructor contract).
        unsafe { self.lines.as_ref() }
    }

    #[inline(always)]
    fn tracks(&self) -> &Tracks {
        // SAFETY: valid for the life of the core (constructor contract).
        unsafe { self.tracks.as_ref() }
    }

    #[inline(always)]
    pub(crate) fn try_pop(&mut self) -> Option<T> {
        if self.tail == self.cached_head {
            // Acquire pairs with the producer's Release store of `head`.
            self.cached_head = self.lines().prod.load(Ordering::Acquire);
            if self.tail == self.cached_head {
                return None;
            }
        }
        let i = (self.tail & self.mask) as usize;
        self.tracks().read(i);
        // SAFETY: `tail < head`, so the slot was written before the Release of `head` that the
        // Acquire above observed; `i < cap`.
        let v = unsafe { self.slots.as_ptr().add(i).read().assume_init() };
        self.tail = self.tail.wrapping_add(1);
        // Release pairs with the producer's Acquire load of `tail`.
        self.lines().cons.store(self.tail, Ordering::Release);
        Some(v)
    }

    #[inline(always)]
    pub(crate) fn read_batch(&mut self, max: usize) -> ReadBatch<'_, T> {
        // Acquire pairs with the producer's Release store of `head`.
        self.cached_head = self.lines().prod.load(Ordering::Acquire);
        let n = self
            .cached_head
            .wrapping_sub(self.tail)
            .min(max as u64)
            .min(self.cap);
        ReadBatch { core: self, n }
    }

    #[inline(always)]
    pub(crate) fn len_hint(&self) -> usize {
        // Acquire pairs with the producer's Release store of `head`.
        let head = self.lines().prod.load(Ordering::Acquire);
        head.wrapping_sub(self.tail).min(self.cap) as usize
    }
}

/// A batch of writes published by one Release store of `head` at `commit`; dropping it without
/// `commit` publishes nothing.
pub struct WriteBatch<'a, T> {
    core: &'a mut ProdCore<T>,
    avail: u64,
    n: u64,
}

impl<T> WriteBatch<'_, T> {
    /// Writes one element; `false` when the batch is full (nothing written).
    #[inline(always)]
    pub fn push(&mut self, v: T) -> bool {
        if self.n >= self.avail {
            return false;
        }
        let idx = self.core.head.wrapping_add(self.n);
        self.core.write_slot(idx, v);
        self.n += 1;
        true
    }

    /// One Release store of `head` (pairs with the consumer's Acquire load of `head`).
    #[inline(always)]
    pub fn commit(self) {
        if self.n == 0 {
            return;
        }
        self.core.head = self.core.head.wrapping_add(self.n);
        self.core.lines().prod.store(self.core.head, Ordering::Release);
        self.core.notify();
    }
}

impl<T> core::fmt::Debug for WriteBatch<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WriteBatch")
            .field("avail", &self.avail)
            .field("n", &self.n)
            .finish()
    }
}

/// A batch of reads (at most two slices when it wraps); `release` frees them with one Release
/// store of `tail`; dropping it without `release` re-delivers them.
pub struct ReadBatch<'a, T> {
    core: &'a mut ConsCore<T>,
    n: u64,
}

impl<T> ReadBatch<'_, T> {
    pub fn slices(&self) -> (&[T], &[T]) {
        let c = &*self.core;
        let start = (c.tail & c.mask) as usize;
        let n = self.n as usize;
        let first = n.min(c.cap as usize - start);
        for k in 0..n {
            c.tracks().read((start + k) & c.mask as usize);
        }
        let base = c.slots.as_ptr().cast::<T>();
        // SAFETY: indices [tail, tail + n) were published by the Release of `head` observed in
        // `read_batch`; the producer will not overwrite them until `tail` moves past them, which
        // needs `release(self)` and so ends this borrow. Both ranges are inside the slot array.
        unsafe {
            (
                core::slice::from_raw_parts(base.add(start), first),
                core::slice::from_raw_parts(base, n - first),
            )
        }
    }

    /// One Release store of `tail` (pairs with the producer's Acquire load of `tail`).
    #[inline(always)]
    pub fn release(self) {
        if self.n == 0 {
            return;
        }
        self.core.tail = self.core.tail.wrapping_add(self.n);
        self.core.lines().cons.store(self.core.tail, Ordering::Release);
    }
}

impl<T> core::fmt::Debug for ReadBatch<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReadBatch").field("n", &self.n).finish()
    }
}

/// In-process producer: exactly one per ring; `Send`, not `Sync`, not `Clone`.
#[allow(clippy::disallowed_types)] // allow-list: shared ring memory owner, never cloned after construction
pub struct Producer<T> {
    core: ProdCore<T>,
    _mem: Arc<RingAlloc>,
}

/// In-process consumer: exactly one per ring; `Send`, not `Sync`, not `Clone`.
#[allow(clippy::disallowed_types)] // allow-list: shared ring memory owner, never cloned after construction
pub struct Consumer<T> {
    core: ConsCore<T>,
    _mem: Arc<RingAlloc>,
}

// SAFETY: the producer exclusively owns the producer side of the ring; elements are moved
// across threads, hence `T: Send`.
unsafe impl<T: Send> Send for Producer<T> {}
// SAFETY: as above for the consumer side.
unsafe impl<T: Send> Send for Consumer<T> {}

/// Creates an in-process ring; `capacity` must be a power of two ≥ 2 (else `Capacity`).
#[allow(clippy::disallowed_types)] // allow-list: shared ring memory owner, never cloned after construction
pub fn spsc<T: Copy>(capacity: usize, mem: Backing) -> Result<(Producer<T>, Consumer<T>), RtErr> {
    if capacity < 2 || !capacity.is_power_of_two() {
        return Err(RtErr::Capacity);
    }
    let alloc = Arc::new(RingAlloc::new(capacity, size_of::<T>(), align_of::<T>(), mem)?);
    let tracks = NonNull::from(&alloc.tracks);
    let cap = capacity as u64;
    // SAFETY: the allocation holds the lines and `capacity` slots and is kept alive by the
    // `Arc` in both ends; one producer and one consumer are created.
    let (p, c) = unsafe {
        (
            ProdCore::new(alloc.lines(), alloc.slots::<T>(), tracks, cap),
            ConsCore::new(alloc.lines(), alloc.slots::<T>(), tracks, cap),
        )
    };
    Ok((
        Producer {
            core: p,
            _mem: alloc.clone(),
        },
        Consumer { core: c, _mem: alloc },
    ))
}

impl<T: Copy> Producer<T> {
    /// Full → `Err(v)`; never blocks.
    #[inline(always)]
    pub fn try_push(&mut self, v: T) -> Result<(), T> {
        self.core.try_push(v)
    }

    #[inline(always)]
    pub fn write_batch(&mut self, max: usize) -> WriteBatch<'_, T> {
        self.core.write_batch(max)
    }

    /// Free slots; may be smaller than the truth, never larger.
    #[inline(always)]
    pub fn free_hint(&self) -> usize {
        self.core.free_hint()
    }

    /// Wake hook (startup, once): `Some` only when the consumer may park and this producer is
    /// not on a `Spin` thread.
    pub fn set_wake(&mut self, u: Option<Unparker>) {
        self.core.wake = u;
    }
}

impl<T: Copy> Consumer<T> {
    #[inline(always)]
    pub fn try_pop(&mut self) -> Option<T> {
        self.core.try_pop()
    }

    #[inline(always)]
    pub fn read_batch(&mut self, max: usize) -> ReadBatch<'_, T> {
        self.core.read_batch(max)
    }

    /// Readable elements; may be smaller than the truth, never larger.
    #[inline(always)]
    pub fn len_hint(&self) -> usize {
        self.core.len_hint()
    }
}

impl<T> core::fmt::Debug for Producer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Producer")
            .field("cap", &self.core.cap)
            .field("head", &self.core.head)
            .finish_non_exhaustive()
    }
}

impl<T> core::fmt::Debug for Consumer<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Consumer")
            .field("cap", &self.core.cap)
            .field("tail", &self.core.tail)
            .finish_non_exhaustive()
    }
}
