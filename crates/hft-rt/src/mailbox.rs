//! Ownership hand-off (`Mailbox`), single-writer value cell (`SeqCell`) and the kill word.

use core::mem::MaybeUninit;

use crate::idle::Unparker;
use crate::sync::{AtomicU8, AtomicU32, AtomicU64, Cell, Ordering, fence, spin_hint};
use crate::wait::CachePadded;

/// System-wide 32-bit kill word (bit layout `hft_types::KillBits`; behaviour in hft-risk).
pub type KillWord = CachePadded<AtomicU32>;

#[cfg(not(loom))]
const _: () = assert!(size_of::<KillWord>() == 128);

const EMPTY: u8 = 0;
const FULL: u8 = 1;

struct MailLine<T> {
    state: AtomicU8,
    slot: Cell<MaybeUninit<T>>,
}

/// Single-slot owned hand-off: `state` (Empty/Full) + the value, on one padded line.
pub struct Mailbox<T: Send> {
    line: CachePadded<MailLine<T>>,
    split: core::sync::atomic::AtomicBool,
}

// SAFETY: the slot is accessed only by the one sender (while Empty) and the one receiver
// (while Full), ordered by the Release/Acquire pairs on `state`; values cross threads (T: Send).
unsafe impl<T: Send> Sync for Mailbox<T> {}

impl<T: Send> Default for Mailbox<T> {
    fn default() -> Mailbox<T> {
        Mailbox::new()
    }
}

impl<T: Send> Mailbox<T> {
    pub fn new() -> Mailbox<T> {
        Mailbox {
            line: CachePadded(MailLine {
                state: AtomicU8::new(EMPTY),
                slot: Cell::new(MaybeUninit::uninit()),
            }),
            split: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The one sender and the one receiver. Startup only; a second call is a logic error and
    /// panics (two senders would race on the slot).
    pub fn split(&'static self) -> (MailSender<T>, MailReceiver<T>) {
        let again = self.split.swap(true, core::sync::atomic::Ordering::AcqRel);
        assert!(!again, "Mailbox::split called twice");
        (MailSender { mb: self, wake: None }, MailReceiver { mb: self })
    }
}

impl<T: Send> Drop for Mailbox<T> {
    fn drop(&mut self) {
        if self.line.state.load(Ordering::Acquire) == FULL {
            // SAFETY: Full means the slot holds an initialized value nobody took; `&mut self`
            // excludes every other access.
            self.line.slot.with_mut(|p| unsafe { (*p).assume_init_drop() });
        }
    }
}

/// Sending end (exactly one; not `Clone`).
pub struct MailSender<T: Send + 'static> {
    mb: &'static Mailbox<T>,
    wake: Option<Unparker>,
}

/// Receiving end (exactly one; not `Clone`).
pub struct MailReceiver<T: Send + 'static> {
    mb: &'static Mailbox<T>,
}

impl<T: Send> MailSender<T> {
    /// Slot full → `Err(v)` unchanged; never blocks.
    #[inline]
    pub fn try_send(&mut self, v: T) -> Result<(), T> {
        let l = &self.mb.line;
        // Acquire pairs with the receiver's Release store of Empty (its read of the old value
        // happens before this write).
        if l.state.load(Ordering::Acquire) != EMPTY {
            return Err(v);
        }
        // SAFETY: Empty: the receiver does not touch the slot until it sees Full.
        l.slot.with_mut(|p| unsafe { (*p).write(v) });
        // Release pairs with the receiver's Acquire load in `try_take`.
        l.state.store(FULL, Ordering::Release);
        if let Some(u) = &self.wake {
            u.unpark();
        }
        Ok(())
    }

    /// Wake hook (startup, once); see `Producer::set_wake`.
    pub fn set_wake(&mut self, u: Option<Unparker>) {
        self.wake = u;
    }
}

impl<T: Send> MailReceiver<T> {
    /// One Acquire read; Full → moves the value out and marks Empty.
    #[inline]
    pub fn try_take(&mut self) -> Option<T> {
        let l = &self.mb.line;
        // Acquire pairs with the sender's Release store of Full.
        if l.state.load(Ordering::Acquire) != FULL {
            return None;
        }
        // SAFETY: Full: the value is initialized and the sender does not touch the slot until
        // it sees Empty.
        let v = l.slot.with_mut(|p| unsafe { (*p).assume_init_read() });
        // Release pairs with the sender's Acquire load in `try_send`.
        l.state.store(EMPTY, Ordering::Release);
        Some(v)
    }
}

impl<T: Send> core::fmt::Debug for Mailbox<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Mailbox").finish_non_exhaustive()
    }
}
impl<T: Send> core::fmt::Debug for MailSender<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MailSender").finish_non_exhaustive()
    }
}
impl<T: Send> core::fmt::Debug for MailReceiver<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MailReceiver").finish_non_exhaustive()
    }
}

/// Single-writer seqlock: `store` from exactly one thread (calling convention), `load` from any.
///
/// Writer: seq → odd (Relaxed), `fence(Release)`, data, seq → even (Release).
/// Reader: seq (Acquire), data, `fence(Acquire)`, seq again (Relaxed); retry if odd or changed.
/// The writer's fence pairs with the reader's fence; the final Release with the first Acquire.
pub struct SeqCell<T: Copy> {
    seq: AtomicU64,
    #[cfg(not(loom))]
    data: core::cell::UnsafeCell<T>,
    // Under loom the data is copied word by word through atomics (loom flags any racing
    // non-atomic access; the protocol is the same).
    #[cfg(loom)]
    words: Box<[AtomicU64]>,
    #[cfg(loom)]
    _t: core::marker::PhantomData<T>,
}

// SAFETY: concurrent readers only copy the data out under the seqlock protocol and discard torn
// copies; the single writer is a calling convention documented on `store`.
unsafe impl<T: Copy + Send> Sync for SeqCell<T> {}

impl<T: Copy> SeqCell<T> {
    pub fn new(v: T) -> SeqCell<T> {
        SeqCell {
            seq: AtomicU64::new(0),
            #[cfg(not(loom))]
            data: core::cell::UnsafeCell::new(v),
            #[cfg(loom)]
            words: {
                let w = Self::to_words(v);
                w.into_iter().map(AtomicU64::new).collect()
            },
            #[cfg(loom)]
            _t: core::marker::PhantomData,
        }
    }

    #[cfg(loom)]
    fn to_words(v: T) -> Vec<u64> {
        let mut w = vec![0u64; size_of::<T>().div_ceil(8)];
        // SAFETY: `w` has at least size_of::<T>() bytes; T: Copy is plain data.
        unsafe {
            core::ptr::copy_nonoverlapping(
                (&v as *const T).cast::<u8>(),
                w.as_mut_ptr().cast::<u8>(),
                size_of::<T>(),
            )
        };
        w
    }

    /// Single writer only.
    #[inline]
    pub fn store(&self, v: T) {
        let s = self.seq.load(Ordering::Relaxed);
        self.seq.store(s.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        #[cfg(not(loom))]
        // SAFETY: single writer; readers detect the concurrent write via `seq` and retry.
        unsafe {
            core::ptr::write_volatile(self.data.get(), v)
        };
        #[cfg(loom)]
        for (a, w) in self.words.iter().zip(Self::to_words(v)) {
            a.store(w, Ordering::Relaxed);
        }
        self.seq.store(s.wrapping_add(2), Ordering::Release);
    }

    /// A consistent copy (retries torn reads); no allocation.
    #[inline]
    pub fn load(&self) -> T {
        loop {
            if let Some(v) = self.try_load() {
                return v;
            }
            spin_hint();
        }
    }

    /// One read attempt: `None` when a store was in progress (torn or odd sequence).
    #[doc(hidden)]
    #[inline]
    pub fn try_load(&self) -> Option<T> {
        let s1 = self.seq.load(Ordering::Acquire);
        if s1 & 1 != 0 {
            return None;
        }
        #[cfg(not(loom))]
        // SAFETY: a volatile copy; a torn copy is discarded by the seq check below.
        let v = unsafe { core::ptr::read_volatile(self.data.get()) };
        #[cfg(loom)]
        let v = {
            let w: Vec<u64> = self.words.iter().map(|a| a.load(Ordering::Relaxed)).collect();
            // SAFETY: `w` holds size_of::<T>() bytes written from a valid T.
            unsafe { core::ptr::read_unaligned(w.as_ptr().cast::<T>()) }
        };
        fence(Ordering::Acquire);
        (self.seq.load(Ordering::Relaxed) == s1).then_some(v)
    }
}

impl<T: Copy> core::fmt::Debug for SeqCell<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SeqCell").finish_non_exhaustive()
    }
}
