//! Rings: the SPSC core (in-process and shared memory share it), the byte ring and the shared
//! memory protocol.

pub(crate) mod bytes;
#[cfg(not(loom))]
pub(crate) mod shm;
pub(crate) mod spsc;

use std::alloc::Layout;
use std::ptr::NonNull;

use crate::RtErr;
use crate::mem::HugeRegion;
use crate::sync::{AtomicU64, Tracks};
use crate::wait::CachePadded;

/// The two cursor lines: `head` (next index to write, producer-owned) and `tail` (next index
/// to read, consumer-owned); monotonic u64, never wrap. Same layout in shared memory.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct Lines {
    pub(crate) prod: CachePadded<AtomicU64>,
    pub(crate) cons: CachePadded<AtomicU64>,
}

#[derive(Debug)]
enum AllocKind {
    Heap(Layout),
    Region(#[allow(dead_code)] HugeRegion),
}

/// In-process ring memory: `Lines` then slots (first slot 128 B aligned); freed when both ends
/// are dropped.
#[derive(Debug)]
pub(crate) struct RingAlloc {
    base: NonNull<u8>,
    slots_off: usize,
    kind: AllocKind,
    pub(crate) tracks: Tracks,
}

// SAFETY: the allocation is plain memory plus atomics; each end only touches what the SPSC
// protocol gives it.
unsafe impl Send for RingAlloc {}
// SAFETY: as above; shared access goes through atomics and the SPSC protocol.
unsafe impl Sync for RingAlloc {}

impl RingAlloc {
    pub(crate) fn new(
        cap: usize,
        elem_size: usize,
        elem_align: usize,
        backing: Backing,
    ) -> Result<RingAlloc, RtErr> {
        let align = elem_align.max(128);
        let slots_off = size_of::<Lines>().div_ceil(align) * align;
        let bytes = cap
            .checked_mul(elem_size)
            .and_then(|b| b.checked_add(slots_off))
            .ok_or(RtErr::NoMem)?;
        let (base, kind) = match backing {
            Backing::Heap => {
                let layout = Layout::from_size_align(bytes, align).map_err(|_| RtErr::NoMem)?;
                // SAFETY: `layout` has non-zero size (it includes the cursor lines).
                let p = unsafe { std::alloc::alloc(layout) };
                (NonNull::new(p).ok_or(RtErr::NoMem)?, AllocKind::Heap(layout))
            }
            Backing::Huge(page) => {
                let r = HugeRegion::anon(bytes, page, None)?;
                (
                    NonNull::new(r.as_ptr()).ok_or(RtErr::NoMem)?,
                    AllocKind::Region(r),
                )
            }
        };
        let lines = Lines {
            prod: CachePadded(AtomicU64::new(0)),
            cons: CachePadded(AtomicU64::new(0)),
        };
        // SAFETY: `base` is valid for `bytes ≥ size_of::<Lines>()` bytes and 128 B aligned.
        unsafe { base.as_ptr().cast::<Lines>().write(lines) };
        Ok(RingAlloc {
            base,
            slots_off,
            kind,
            tracks: Tracks::new(cap),
        })
    }

    pub(crate) fn lines(&self) -> NonNull<Lines> {
        self.base.cast()
    }

    pub(crate) fn slots<T>(&self) -> NonNull<T> {
        // SAFETY: `slots_off` is inside the allocation (or one past its end for zero slots).
        unsafe { self.base.add(self.slots_off).cast() }
    }
}

impl Drop for RingAlloc {
    fn drop(&mut self) {
        // SAFETY: the lines were written in `new` and are dropped exactly once here.
        unsafe { core::ptr::drop_in_place(self.base.as_ptr().cast::<Lines>()) };
        if let AllocKind::Heap(layout) = self.kind {
            // SAFETY: allocated in `new` with exactly this layout.
            unsafe { std::alloc::dealloc(self.base.as_ptr(), layout) };
        }
    }
}

pub use spsc::Backing;
