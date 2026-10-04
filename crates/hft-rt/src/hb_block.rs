//! `HeartbeatBlock`: the 4 KiB shared liveness block (`/dev/shm/hft/<instance>/heartbeat`).

use core::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::path::Path;

use crate::RtErr;
use crate::wait::{CachePadded, Heartbeat};

/// Housekeeping line (zone 1).
#[repr(C)]
#[derive(Debug)]
pub struct HkLine {
    /// `*b"HFTHB\0\x01\0"` read as little-endian u64; written last at creation.
    pub magic: u64,
    pub layout_version: u32,
    pub pid: u32,
    pub instance_id: u8,
    pub _rsv: u8,
    /// Starting=1 Warmup=2 Running=3 Draining=4 Stopped=5.
    pub state: AtomicU8,
    /// Claimed zone 2.. slots.
    pub n_slots: AtomicU8,
    pub _p: [u8; 4],
    pub boot_id: [u8; 16],
    pub started_utc_ns: i64,
    pub hk_utc_ns: AtomicI64,
}

const _: () = assert!(size_of::<HkLine>() == 56);

impl HkLine {
    pub const MAGIC: u64 = u64::from_le_bytes(*b"HFTHB\0\x01\0");
    pub const LAYOUT_VERSION: u32 = 1;
    pub const STARTING: u8 = 1;
    pub const WARMUP: u8 = 2;
    pub const RUNNING: u8 = 3;
    pub const DRAINING: u8 = 4;
    pub const STOPPED: u8 = 5;
}

/// 4096 B: zone 0 sequencer, zone 1 housekeeping, zones 2..31 other threads.
#[repr(C, align(128))]
#[derive(Debug)]
pub struct HeartbeatBlock {
    pub beat: Heartbeat,
    pub hk: CachePadded<HkLine>,
    pub slots: [Heartbeat; 30],
}

const _: () = assert!(size_of::<HeartbeatBlock>() == 4096);

impl HeartbeatBlock {
    /// Startup: `O_CREAT|O_TRUNC`, mode 0640, pre-touch, magic written last. The block starts
    /// with `state = Starting`, `n_slots = 0` and all counts 0. The mapping is never unmapped.
    pub fn create(path: &Path, instance_id: u8) -> Result<&'static HeartbeatBlock, RtErr> {
        imp::create(path, instance_id)
    }

    /// Read-only mapping (`PROT_READ`); magic mismatch → `BadMagic`, layout version ≠ 1 →
    /// `BadVersion(v)`.
    pub fn open_ro(path: &Path) -> Result<&'static HeartbeatBlock, RtErr> {
        imp::open_ro(path)
    }

    /// Zone 0 (handed to `Sequencer::boot`).
    pub fn sequencer(&'static self) -> &'static Heartbeat {
        &self.beat
    }

    /// Assembly-time, single thread, on a block from `create`: the next free zone; full → `None`;
    /// names longer than 16 bytes are cut by bytes.
    pub fn claim(&'static self, name: &str, core: u16, kind: u8) -> Option<&'static Heartbeat> {
        let n = self.hk.n_slots.load(Ordering::Acquire) as usize;
        let slot = self.slots.get(n)?;
        slot.set_identity(name, core, kind);
        // Release pairs with readers' Acquire of `n_slots` (identity visible first).
        self.hk.n_slots.store(n as u8 + 1, Ordering::Release);
        Some(slot)
    }

    /// Release store of `hk.state` (pairs with readers' Acquire).
    pub fn set_state(&self, s: u8) {
        self.hk.state.store(s, Ordering::Release);
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::sys::{boot_id, clock_ns, cpath, errno};

    pub(super) fn create(path: &Path, instance_id: u8) -> Result<&'static HeartbeatBlock, RtErr> {
        let c = cpath(path).ok_or(RtErr::Os(libc::EINVAL))?;
        // SAFETY: valid NUL-terminated path.
        let fd = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_TRUNC | libc::O_CLOEXEC,
                0o640 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(RtErr::Os(errno()));
        }
        let res = (|| {
            // SAFETY: `fd` is open; 0640 regardless of umask.
            if unsafe { libc::fchmod(fd, 0o640) } != 0 {
                return Err(RtErr::Os(errno()));
            }
            // SAFETY: `fd` is open for writing.
            if unsafe { libc::ftruncate(fd, 4096) } != 0 {
                return Err(RtErr::Os(errno()));
            }
            // SAFETY: shared read-write mapping of the 4 KiB file.
            let p = unsafe {
                libc::mmap(
                    core::ptr::null_mut(),
                    4096,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    fd,
                    0,
                )
            };
            if p == libc::MAP_FAILED {
                return Err(RtErr::Os(errno()));
            }
            Ok(p.cast::<HeartbeatBlock>())
        })();
        // SAFETY: closed once; the mapping outlives the descriptor.
        unsafe { libc::close(fd) };
        let b = res?;
        let now = clock_ns(libc::CLOCK_REALTIME);
        // SAFETY: the mapping is 4096 B, page aligned, zero-filled (O_TRUNC + ftruncate) and not
        // yet shared with any reader that trusts it (magic is still 0); fields are written
        // through the raw pointer before any reference is created.
        unsafe {
            core::ptr::write_bytes(b.cast::<u8>(), 0, 4096);
            let hk = &raw mut (*b).hk.0;
            (*hk).layout_version = HkLine::LAYOUT_VERSION;
            (*hk).pid = std::process::id();
            (*hk).instance_id = instance_id;
            (*hk).boot_id = boot_id();
            (*hk).started_utc_ns = now;
            (*hk).state = AtomicU8::new(HkLine::STARTING);
            (*hk).hk_utc_ns = AtomicI64::new(now);
            core::sync::atomic::fence(Ordering::Release);
            core::ptr::write_volatile(&raw mut (*hk).magic, HkLine::MAGIC);
            Ok(&*b)
        }
    }

    pub(super) fn open_ro(path: &Path) -> Result<&'static HeartbeatBlock, RtErr> {
        let c = cpath(path).ok_or(RtErr::Os(libc::EINVAL))?;
        // SAFETY: valid NUL-terminated path.
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(RtErr::Os(errno()));
        }
        // SAFETY: zeroed `stat` is a valid out-buffer; `fd` is open.
        let mut st: libc::stat = unsafe { core::mem::zeroed() };
        // SAFETY: as above.
        let ok = unsafe { libc::fstat(fd, &mut st) } == 0;
        let p = if ok && st.st_size >= 4096 {
            // SAFETY: read-only shared mapping of the first 4 KiB.
            unsafe {
                libc::mmap(
                    core::ptr::null_mut(),
                    4096,
                    libc::PROT_READ,
                    libc::MAP_SHARED,
                    fd,
                    0,
                )
            }
        } else {
            libc::MAP_FAILED
        };
        let e = errno();
        // SAFETY: closed once.
        unsafe { libc::close(fd) };
        if !ok {
            return Err(RtErr::Os(e));
        }
        if st.st_size < 4096 {
            return Err(RtErr::BadMagic);
        }
        if p == libc::MAP_FAILED {
            return Err(RtErr::Os(e));
        }
        // SAFETY: 4096 B readable mapping, never unmapped; every bit pattern is a valid
        // `HeartbeatBlock` (integers, byte arrays, atomics).
        let b: &'static HeartbeatBlock = unsafe { &*p.cast::<HeartbeatBlock>() };
        // SAFETY: plain read of the magic field of the mapped header.
        let magic = unsafe { core::ptr::read_volatile(&raw const b.hk.0.magic) };
        let unmap = || {
            // SAFETY: the 4 KiB mapping created above; no reference escapes on this path.
            unsafe { libc::munmap(p, 4096) };
        };
        if magic != HkLine::MAGIC {
            unmap();
            return Err(RtErr::BadMagic);
        }
        core::sync::atomic::fence(Ordering::Acquire);
        let v = b.hk.layout_version;
        if v != HkLine::LAYOUT_VERSION {
            unmap();
            return Err(RtErr::BadVersion(v));
        }
        Ok(b)
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;
    use crate::sys::ENOSYS;

    pub(super) fn create(_: &Path, _: u8) -> Result<&'static HeartbeatBlock, RtErr> {
        Err(RtErr::Os(ENOSYS))
    }
    pub(super) fn open_ro(_: &Path) -> Result<&'static HeartbeatBlock, RtErr> {
        Err(RtErr::Os(ENOSYS))
    }
}
