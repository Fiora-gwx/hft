//! Waiting and liveness: `CachePadded`, `cpu_relax`, `Backoff`, `Heartbeat`, `HeartbeatWatch`.

use core::sync::atomic::{AtomicU8, AtomicU16, AtomicU64, Ordering};

/// Pads and aligns `T` to 128 B (two 64 B lines: covers the adjacent-line prefetcher).
#[repr(C, align(128))]
#[derive(Default, Debug)]
pub struct CachePadded<T>(pub T);
const _: () = assert!(size_of::<CachePadded<u64>>() == 128 && align_of::<CachePadded<u64>>() == 128);

impl<T> core::ops::Deref for CachePadded<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> core::ops::DerefMut for CachePadded<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

/// One spin-wait back-off instruction: aarch64 `ISB`, x86 `PAUSE` (via `spin_loop`).
#[inline(always)]
pub fn cpu_relax() {
    #[cfg(target_arch = "aarch64")]
    // SAFETY: `isb` only flushes the pipeline; it touches no memory, stack or flags.
    unsafe {
        core::arch::asm!("isb", options(nomem, nostack, preserves_flags));
    }
    #[cfg(not(target_arch = "aarch64"))]
    core::hint::spin_loop();
}

/// Warm/cold-thread back-off: `spin_limit` × `cpu_relax`, then `yield_now` per call; never sleeps.
#[derive(Debug)]
pub struct Backoff {
    spin_limit: u32,
    spins: u32,
}

impl Backoff {
    pub fn new(spin_limit: u32) -> Backoff {
        Backoff { spin_limit, spins: 0 }
    }

    pub fn snooze(&mut self) {
        if self.spins < self.spin_limit {
            self.spins += 1;
            cpu_relax();
        } else {
            std::thread::yield_now();
        }
    }

    pub fn reset(&mut self) {
        self.spins = 0;
    }
}

/// Heartbeat thread kinds (`Heartbeat.kind`).
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HbKind {
    Sequencer = 1,
    Feed = 2,
    Gw = 3,
    Persist = 4,
    Housekeeping = 5,
    Advisor = 6,
    Ctl = 7,
    Ext = 8,
    ChainWorker = 9,
    Mixed = 10,
}

/// One 128 B padded line, single writer. Identity fields are atomics because the line lives in a
/// block shared with other processes and is written through `&` at claim time.
#[repr(C, align(128))]
#[derive(Debug)]
pub struct Heartbeat {
    count: AtomicU64,
    pub name: [AtomicU8; 16],
    pub core: AtomicU16,
    pub kind: AtomicU8,
    pub _pad: [u8; 101],
}

const _: () = assert!(size_of::<Heartbeat>() == 128);

impl Default for Heartbeat {
    fn default() -> Heartbeat {
        Heartbeat::new()
    }
}

impl Heartbeat {
    pub const fn new() -> Heartbeat {
        Heartbeat {
            count: AtomicU64::new(0),
            name: [const { AtomicU8::new(0) }; 16],
            core: AtomicU16::new(0),
            kind: AtomicU8::new(0),
            _pad: [0; 101],
        }
    }

    /// +1; single writer: Relaxed read of its own value, Release store (pairs with `read`).
    #[inline(always)]
    pub fn beat(&self) {
        let c = self.count.load(Ordering::Relaxed);
        self.count.store(c.wrapping_add(1), Ordering::Release);
    }

    /// Acquire (pairs with the Release store in `beat`).
    #[inline(always)]
    pub fn read(&self) -> u64 {
        self.count.load(Ordering::Acquire)
    }

    /// The thread name bytes (zero padded).
    pub fn name_bytes(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        for (o, b) in out.iter_mut().zip(self.name.iter()) {
            *o = b.load(Ordering::Acquire);
        }
        out
    }

    pub(crate) fn set_identity(&self, name: &str, core: u16, kind: u8) {
        let bytes = name.as_bytes();
        for (i, b) in self.name.iter().enumerate() {
            b.store(bytes.get(i).copied().unwrap_or(0), Ordering::Relaxed);
        }
        self.core.store(core, Ordering::Relaxed);
        self.kind.store(kind, Ordering::Release);
    }
}

/// `HeartbeatWatch::check` result.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    Advanced,
    Stalled { for_checks: u32 },
}

/// Stall detector over one `Heartbeat` (housekeeping, kill-switch, watchtower).
#[derive(Debug, Default)]
pub struct HeartbeatWatch {
    last: Option<u64>,
    stalled: u32,
}

impl HeartbeatWatch {
    pub fn new() -> HeartbeatWatch {
        HeartbeatWatch {
            last: None,
            stalled: 0,
        }
    }

    /// First call records the baseline (`Advanced`); then a changed count → `Advanced`, an
    /// unchanged one → `Stalled { consecutive unchanged checks }`.
    pub fn check(&mut self, hb: &Heartbeat) -> Progress {
        let now = hb.read();
        match self.last {
            Some(prev) if prev == now => {
                self.stalled = self.stalled.saturating_add(1);
                Progress::Stalled {
                    for_checks: self.stalled,
                }
            }
            _ => {
                self.last = Some(now);
                self.stalled = 0;
                Progress::Advanced
            }
        }
    }
}
