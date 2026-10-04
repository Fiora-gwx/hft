//! `ByteSpsc`: variable-length records `len: u32 | 0: u32 | payload | pad to 8 B`; a record that
//! does not fit before the end is preceded by a `len = WRAP` marker and starts again at 0.
//! Single record ≤ capacity/4, so an empty ring always accepts any legal record.
//!
//! Ordering pairs as in `spsc`: Release store of `head` in `commit` ↔ Acquire load in `peek`;
//! Release store of `tail` in `advance` ↔ Acquire load in `reserve`.

use core::ptr::NonNull;
use std::sync::Arc;

use super::{Lines, RingAlloc};
use crate::RtErr;
use crate::idle::Unparker;
use crate::ring::spsc::Backing;
use crate::sync::Ordering;

const WRAP: u32 = u32::MAX;
const HDR: u64 = 8;

#[inline(always)]
fn rec_size(len: u32) -> u64 {
    HDR + (len as u64).div_ceil(8) * 8
}

#[derive(Copy, Clone, Debug)]
struct Pending {
    wrap_at: Option<u64>,
    rec_off: u64,
    len: u32,
    adv: u64,
}

/// Byte-ring producer (exactly one; not `Clone`).
pub struct ByteProducer {
    lines: NonNull<Lines>,
    buf: NonNull<u8>,
    cap: u64,
    head: u64,
    cached_tail: u64,
    pending: Option<Pending>,
    wake: Option<Unparker>,
    _mem: Arc<RingAlloc>,
}

/// Byte-ring consumer (exactly one; not `Clone`).
pub struct ByteConsumer {
    lines: NonNull<Lines>,
    buf: NonNull<u8>,
    cap: u64,
    tail: u64,
    cached_head: u64,
    cur: Option<u64>,
    _mem: Arc<RingAlloc>,
}

// SAFETY: each end exclusively owns its side of the ring; the bytes are plain data.
unsafe impl Send for ByteProducer {}
// SAFETY: as above.
unsafe impl Send for ByteConsumer {}

/// `capacity_bytes` must be a power of two ≥ 64 (else `Capacity`).
pub fn byte_spsc(capacity_bytes: usize, mem: Backing) -> Result<(ByteProducer, ByteConsumer), RtErr> {
    if capacity_bytes < 64 || !capacity_bytes.is_power_of_two() {
        return Err(RtErr::Capacity);
    }
    let alloc = Arc::new(RingAlloc::new(capacity_bytes, 1, 8, mem)?);
    let cap = capacity_bytes as u64;
    Ok((
        ByteProducer {
            lines: alloc.lines(),
            buf: alloc.slots::<u8>(),
            cap,
            head: 0,
            cached_tail: 0,
            pending: None,
            wake: None,
            _mem: alloc.clone(),
        },
        ByteConsumer {
            lines: alloc.lines(),
            buf: alloc.slots::<u8>(),
            cap,
            tail: 0,
            cached_head: 0,
            cur: None,
            _mem: alloc,
        },
    ))
}

impl ByteProducer {
    #[inline(always)]
    fn lines(&self) -> &Lines {
        // SAFETY: kept alive by `_mem`.
        unsafe { self.lines.as_ref() }
    }

    /// Reserves `len` payload bytes. `len > capacity/4` → always `None`; no space → `None`.
    /// An uncommitted reservation is replaced by the next `reserve` (never published).
    #[inline]
    pub fn reserve(&mut self, len: u32) -> Option<&mut [u8]> {
        if len as u64 > self.cap / 4 {
            return None;
        }
        let need = rec_size(len);
        let pos = self.head & (self.cap - 1);
        let room = self.cap - pos;
        let (wrap_at, rec_off, adv) = if need <= room {
            (None, pos, need)
        } else {
            (Some(pos), 0, room + need)
        };
        if self.head.wrapping_sub(self.cached_tail) + adv > self.cap {
            // Acquire pairs with the consumer's Release store of `tail` in `advance`.
            self.cached_tail = self.lines().cons.load(Ordering::Acquire);
            if self.head.wrapping_sub(self.cached_tail) + adv > self.cap {
                self.pending = None;
                return None;
            }
        }
        self.pending = Some(Pending {
            wrap_at,
            rec_off,
            len,
            adv,
        });
        // SAFETY: [rec_off + 8, rec_off + 8 + len) lies inside the buffer (need ≤ cap - rec_off)
        // and in free space the consumer does not read until the next Release of `head`.
        Some(unsafe {
            core::slice::from_raw_parts_mut(self.buf.as_ptr().add((rec_off + HDR) as usize), len as usize)
        })
    }

    #[inline(always)]
    fn write_hdr(&mut self, off: u64, len: u32) {
        let mut h = [0u8; 8];
        h[..4].copy_from_slice(&len.to_le_bytes());
        // SAFETY: `off` is 8 B aligned and `off + 8 ≤ cap` (records and markers are 8 B
        // multiples and the buffer length is a power of two ≥ 64), in free space.
        unsafe { core::ptr::copy_nonoverlapping(h.as_ptr(), self.buf.as_ptr().add(off as usize), 8) };
    }

    /// Publishes the pending reservation with one Release store of `head`; no-op without one.
    #[inline]
    pub fn commit(&mut self) {
        let Some(p) = self.pending.take() else { return };
        if let Some(w) = p.wrap_at {
            self.write_hdr(w, WRAP);
        }
        self.write_hdr(p.rec_off, p.len);
        self.head = self.head.wrapping_add(p.adv);
        // Release pairs with the consumer's Acquire load of `head` in `peek`.
        self.lines().prod.store(self.head, Ordering::Release);
        if let Some(u) = &self.wake {
            u.unpark();
        }
    }

    /// Wake hook (startup, once); see `Producer::set_wake`.
    pub fn set_wake(&mut self, u: Option<Unparker>) {
        self.wake = u;
    }
}

impl ByteConsumer {
    #[inline(always)]
    fn lines(&self) -> &Lines {
        // SAFETY: kept alive by `_mem`.
        unsafe { self.lines.as_ref() }
    }

    #[inline(always)]
    fn read_len(&self, off: u64) -> u32 {
        let mut b = [0u8; 4];
        // SAFETY: `off + 4 ≤ cap`; the header was published by the observed Release of `head`.
        unsafe { core::ptr::copy_nonoverlapping(self.buf.as_ptr().add(off as usize), b.as_mut_ptr(), 4) };
        u32::from_le_bytes(b)
    }

    /// The next record's payload, or `None` when empty; wrap markers are never delivered.
    #[inline]
    pub fn peek(&mut self) -> Option<&[u8]> {
        loop {
            if self.tail == self.cached_head {
                // Acquire pairs with the producer's Release store of `head` in `commit`.
                self.cached_head = self.lines().prod.load(Ordering::Acquire);
                if self.tail == self.cached_head {
                    self.cur = None;
                    return None;
                }
            }
            let pos = self.tail & (self.cap - 1);
            let len = self.read_len(pos);
            if len == WRAP {
                self.tail = self.tail.wrapping_add(self.cap - pos);
                continue;
            }
            self.cur = Some(rec_size(len));
            // SAFETY: the record [pos, pos + rec_size) was published with `head`; it stays
            // untouched until `advance` releases it, which needs `&mut self` (ends this borrow).
            return Some(unsafe {
                core::slice::from_raw_parts(self.buf.as_ptr().add((pos + HDR) as usize), len as usize)
            });
        }
    }

    /// Releases the current record (peeking first if needed); no-op when empty.
    #[inline]
    pub fn advance(&mut self) {
        if self.cur.is_none() && self.peek().is_none() {
            return;
        }
        if let Some(sz) = self.cur.take() {
            self.tail = self.tail.wrapping_add(sz);
            // Release pairs with the producer's Acquire load of `tail` in `reserve`.
            self.lines().cons.store(self.tail, Ordering::Release);
        }
    }
}

impl core::fmt::Debug for ByteProducer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ByteProducer")
            .field("cap", &self.cap)
            .field("head", &self.head)
            .finish_non_exhaustive()
    }
}

impl core::fmt::Debug for ByteConsumer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ByteConsumer")
            .field("cap", &self.cap)
            .field("tail", &self.tail)
            .finish_non_exhaustive()
    }
}
