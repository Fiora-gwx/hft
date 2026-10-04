//! Shared-memory SPSC ring: a 4 KiB header page (`ShmRingHdr`) followed by the in-process
//! layout (cursor lines, then slots). Create/attach/liveness/close protocol per README
//! "实现说明". Linux only; elsewhere `create`/`attach` return `ShmErr::Io`.

use core::sync::atomic::{AtomicU32, AtomicU64};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::ptr::NonNull;

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[cfg(target_os = "linux")]
use super::Lines;
use super::spsc::{ConsCore, ProdCore, ReadBatch, WriteBatch};
use crate::mem::PageSize;

/// Which end of a shared ring this process takes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RingSide {
    Producer,
    Consumer,
}

/// Shared-ring errors.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShmErr {
    Io = 1,
    BadMagic = 2,
    VersionMismatch = 3,
    LayoutMismatch = 4,
    SideTaken = 5,
    Closed = 6,
    NotReady = 7,
}

/// The peer's liveness as read from the header (one Acquire read each, no system call).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PeerStatus {
    pub alive: bool,
    pub pid: u32,
    pub epoch: u64,
    pub last_beat: u64,
}

/// One end's liveness line (header lines 1 and 2).
#[repr(C, align(128))]
#[derive(Debug)]
pub struct PeerLine {
    pub pid: AtomicU32,
    pub _p: u32,
    /// `/proc/<pid>/stat` start time (guards against pid reuse).
    pub start: AtomicU64,
    pub beat: AtomicU64,
    pub _pad: [u8; 104],
}
const _: () = assert!(size_of::<PeerLine>() == 128 && align_of::<PeerLine>() == 128);

/// The 4 KiB header page. Line 0 is read-only after creation.
#[repr(C, align(128))]
#[derive(Debug)]
pub struct ShmRingHdr {
    pub magic: [u8; 8],
    pub version: u32,
    /// Creating = 0, Ready = 1, Closed = 2.
    pub state: AtomicU32,
    pub layout_id: u64,
    pub elem_size: u32,
    pub elem_align: u32,
    pub capacity: u64,
    pub epoch: u64,
    pub boot_id: [u8; 16],
    pub creator_pid: u32,
    pub _p0: [u8; 60],
    pub producer: PeerLine,
    pub consumer: PeerLine,
    pub _rest: [u8; 4096 - 384],
}

const _: () = assert!(size_of::<ShmRingHdr>() == 4096);
const _: () = assert!(core::mem::offset_of!(ShmRingHdr, producer) == 128);
const _: () = assert!(core::mem::offset_of!(ShmRingHdr, consumer) == 256);

impl ShmRingHdr {
    pub const MAGIC: [u8; 8] = *b"HFTRING1";
    pub const VERSION: u32 = 1;
    pub const CREATING: u32 = 0;
    pub const READY: u32 = 1;
    pub const CLOSED: u32 = 2;
}

/// Offset of the cursor lines from the start of the file.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINES_OFF: usize = 4096;
/// Offset of the first slot from the start of the file.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const SLOTS_OFF: usize = 4096 + 256;

/// Namespace for creating and attaching shared rings.
#[derive(Debug)]
pub struct ShmSpsc;

/// The end returned by `create`/`attach`.
#[derive(Debug)]
pub enum ShmEnd<T> {
    Producer(ShmProducer<T>),
    Consumer(ShmConsumer<T>),
}

impl<T> ShmEnd<T> {
    pub fn into_producer(self) -> Option<ShmProducer<T>> {
        match self {
            ShmEnd::Producer(p) => Some(p),
            ShmEnd::Consumer(_) => None,
        }
    }

    pub fn into_consumer(self) -> Option<ShmConsumer<T>> {
        match self {
            ShmEnd::Consumer(c) => Some(c),
            ShmEnd::Producer(_) => None,
        }
    }
}

/// Element constraints, checked at compile time (monomorphization).
struct ElemCheck<T>(core::marker::PhantomData<T>);
impl<T> ElemCheck<T> {
    const OK: () = assert!(
        size_of::<T>() >= 8
            && size_of::<T>() <= 4096
            && size_of::<T>().is_multiple_of(8)
            && align_of::<T>() <= 8,
        "ShmSpsc element: size 8 B – 4 KiB, a multiple of 8, align ≤ 8"
    );
}

impl ShmSpsc {
    /// Creates (or replaces) the ring file at `path` and takes `side`.
    pub fn create<T>(
        path: &Path,
        capacity: usize,
        page: PageSize,
        side: RingSide,
        layout_id: u64,
    ) -> Result<ShmEnd<T>, ShmErr>
    where
        T: FromBytes + IntoBytes + KnownLayout + Immutable + Copy,
    {
        let () = ElemCheck::<T>::OK;
        imp::create::<T>(path, capacity, page, side, layout_id)
    }

    /// Attaches to an existing ring file; never waits.
    pub fn attach<T>(path: &Path, side: RingSide, layout_id: u64) -> Result<ShmEnd<T>, ShmErr>
    where
        T: FromBytes + IntoBytes + KnownLayout + Immutable + Copy,
    {
        let () = ElemCheck::<T>::OK;
        imp::attach::<T>(path, side, layout_id)
    }
}

/// The mapping and identity of one end (unmapped on drop after the clean-exit step).
#[derive(Debug)]
struct ShmMap {
    #[cfg(target_os = "linux")]
    base: NonNull<u8>,
    #[cfg(target_os = "linux")]
    len: usize,
    #[cfg(target_os = "linux")]
    dev_ino: (u64, u64),
    #[cfg(target_os = "linux")]
    path: PathBuf,
    side: RingSide,
    creator: bool,
    attached: bool,
}

impl ShmMap {
    #[cfg(target_os = "linux")]
    fn hdr(&self) -> &ShmRingHdr {
        // SAFETY: the mapping starts with a 4 KiB header page and lives as long as `self`.
        unsafe { self.base.cast::<ShmRingHdr>().as_ref() }
    }
    #[cfg(not(target_os = "linux"))]
    fn hdr(&self) -> &ShmRingHdr {
        unreachable_hdr()
    }

    fn own(&self) -> &PeerLine {
        match self.side {
            RingSide::Producer => &self.hdr().producer,
            RingSide::Consumer => &self.hdr().consumer,
        }
    }

    fn other(&self) -> &PeerLine {
        match self.side {
            RingSide::Producer => &self.hdr().consumer,
            RingSide::Consumer => &self.hdr().producer,
        }
    }

    fn peer(&self) -> PeerStatus {
        use core::sync::atomic::Ordering;
        let h = self.hdr();
        let o = self.other();
        // Acquire pairs with the creator's Release store of `state` and the peer's Release
        // stores of its pid/beat.
        let ready = h.state.load(Ordering::Acquire) == ShmRingHdr::READY;
        let pid = o.pid.load(Ordering::Acquire);
        PeerStatus {
            alive: ready && pid != 0,
            pid,
            epoch: h.epoch,
            last_beat: o.beat.load(Ordering::Acquire),
        }
    }

    fn beat(&self) {
        use core::sync::atomic::Ordering;
        let b = &self.own().beat;
        // Single writer: Relaxed read of own value, Release store (pairs with `peer`).
        b.store(b.load(Ordering::Relaxed).wrapping_add(1), Ordering::Release);
    }

    fn replaced(&self) -> bool {
        imp::replaced(self)
    }
}

#[cfg(not(target_os = "linux"))]
fn unreachable_hdr() -> &'static ShmRingHdr {
    // Off Linux no `ShmMap` is ever constructed (create/attach fail first); this exists only so
    // the accessors type-check. A leaked zeroed header keeps it panic-free.
    use std::sync::OnceLock;
    static H: OnceLock<usize> = OnceLock::new();
    let p = *H.get_or_init(|| {
        let b: Box<core::mem::MaybeUninit<ShmRingHdr>> = Box::new_zeroed();
        Box::into_raw(b) as usize
    });
    // SAFETY: an all-zero `ShmRingHdr` is valid (integers, byte arrays and atomics); leaked.
    unsafe { &*(p as *const ShmRingHdr) }
}

impl Drop for ShmMap {
    fn drop(&mut self) {
        use core::sync::atomic::Ordering;
        if self.creator {
            // Clean exit of the creator: Release pairs with the peer's Acquire in `peer`/attach.
            self.hdr().state.store(ShmRingHdr::CLOSED, Ordering::Release);
        } else if self.attached {
            self.own().pid.store(0, Ordering::Release);
        }
        imp::unmap(self);
    }
}

/// Shared-ring producer.
#[derive(Debug)]
pub struct ShmProducer<T> {
    core: ProdCore<T>,
    map: ShmMap,
}

/// Shared-ring consumer.
#[derive(Debug)]
pub struct ShmConsumer<T> {
    core: ConsCore<T>,
    map: ShmMap,
}

// SAFETY: the end exclusively owns its side of the mapping; elements are plain bytes.
unsafe impl<T: Send> Send for ShmProducer<T> {}
// SAFETY: as above.
unsafe impl<T: Send> Send for ShmConsumer<T> {}

impl<T> core::fmt::Debug for ProdCore<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ProdCore")
    }
}

impl<T> core::fmt::Debug for ConsCore<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ConsCore")
    }
}

impl<T: Copy> ShmProducer<T> {
    #[inline(always)]
    pub fn try_push(&mut self, v: T) -> Result<(), T> {
        self.core.try_push(v)
    }
    #[inline(always)]
    pub fn write_batch(&mut self, max: usize) -> WriteBatch<'_, T> {
        self.core.write_batch(max)
    }
    #[inline(always)]
    pub fn free_hint(&self) -> usize {
        self.core.free_hint()
    }
    pub fn peer(&self) -> PeerStatus {
        self.map.peer()
    }
    pub fn beat(&self) {
        self.map.beat()
    }
    /// Warm: `stat` the path; a different inode or a missing path → `true`.
    pub fn replaced(&self) -> bool {
        self.map.replaced()
    }
    pub fn header(&self) -> &ShmRingHdr {
        self.map.hdr()
    }
}

impl<T: Copy> ShmConsumer<T> {
    #[inline(always)]
    pub fn try_pop(&mut self) -> Option<T> {
        self.core.try_pop()
    }
    #[inline(always)]
    pub fn read_batch(&mut self, max: usize) -> ReadBatch<'_, T> {
        self.core.read_batch(max)
    }
    #[inline(always)]
    pub fn len_hint(&self) -> usize {
        self.core.len_hint()
    }
    pub fn peer(&self) -> PeerStatus {
        self.map.peer()
    }
    pub fn beat(&self) {
        self.map.beat()
    }
    pub fn replaced(&self) -> bool {
        self.map.replaced()
    }
    pub fn header(&self) -> &ShmRingHdr {
        self.map.hdr()
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use core::sync::atomic::Ordering;

    use super::*;
    use crate::sync::Tracks;
    use crate::sys::{boot_id, cpath, proc_start};

    static NO_TRACKS: Tracks = Tracks::EMPTY;

    fn file_len<T>(capacity: usize, page: PageSize) -> Option<usize> {
        let raw = capacity.checked_mul(size_of::<T>())?.checked_add(SLOTS_OFF)?;
        page.round_up(raw)
    }

    fn map_fd(fd: i32, len: usize) -> Result<NonNull<u8>, ShmErr> {
        // SAFETY: shared read-write mapping of an open file; the kernel validates arguments.
        let p = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if p == libc::MAP_FAILED {
            return Err(ShmErr::Io);
        }
        NonNull::new(p.cast::<u8>()).ok_or(ShmErr::Io)
    }

    fn fstat(fd: i32) -> Result<libc::stat, ShmErr> {
        // SAFETY: zeroed `stat` is a valid out-buffer.
        let mut st: libc::stat = unsafe { core::mem::zeroed() };
        // SAFETY: `fd` is open, `st` writable.
        if unsafe { libc::fstat(fd, &mut st) } != 0 {
            return Err(ShmErr::Io);
        }
        Ok(st)
    }

    fn old_epoch(path: &Path) -> u64 {
        let Ok(b) = std::fs::read(path) else { return 1 };
        let Some(hdr) = b.get(..48) else { return 1 };
        if hdr[..8] != ShmRingHdr::MAGIC {
            return 1;
        }
        let mut e = [0u8; 8];
        e.copy_from_slice(&hdr[40..48]);
        u64::from_le_bytes(e).wrapping_add(1).max(1)
    }

    /// Claims `line` for this process: free (pid 0) or held by a dead `(pid, start)`.
    fn claim(line: &PeerLine) -> Result<(), ShmErr> {
        let me = std::process::id();
        let my_start = proc_start(me).unwrap_or(0);
        let cur = line.pid.load(Ordering::Acquire);
        if cur != 0 {
            let start = line.start.load(Ordering::Acquire);
            if proc_start(cur) == Some(start) {
                return Err(ShmErr::SideTaken);
            }
        }
        // AcqRel: two attachers race on the same line; exactly one CAS wins.
        if line
            .pid
            .compare_exchange(cur, me, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(ShmErr::SideTaken);
        }
        line.start.store(my_start, Ordering::Release);
        line.beat.store(0, Ordering::Release);
        Ok(())
    }

    fn ends<T: Copy>(map: ShmMap, capacity: u64) -> ShmEnd<T> {
        // SAFETY: the mapping holds the header page, the cursor lines at LINES_OFF and
        // `capacity` slots at SLOTS_OFF (checked by the caller); it lives as long as `map`,
        // which the returned end owns; this process holds the pid line of `map.side`.
        unsafe {
            let lines = map.base.add(LINES_OFF).cast::<Lines>();
            let slots = map.base.add(SLOTS_OFF).cast::<T>();
            let tracks = NonNull::from(&NO_TRACKS);
            match map.side {
                RingSide::Producer => ShmEnd::Producer(ShmProducer {
                    core: ProdCore::new(lines, slots, tracks, capacity),
                    map,
                }),
                RingSide::Consumer => ShmEnd::Consumer(ShmConsumer {
                    core: ConsCore::new(lines, slots, tracks, capacity),
                    map,
                }),
            }
        }
    }

    pub(super) fn create<T: Copy>(
        path: &Path,
        capacity: usize,
        page: PageSize,
        side: RingSide,
        layout_id: u64,
    ) -> Result<ShmEnd<T>, ShmErr> {
        if capacity < 2 || !capacity.is_power_of_two() {
            return Err(ShmErr::LayoutMismatch);
        }
        let len = file_len::<T>(capacity, page).ok_or(ShmErr::LayoutMismatch)?;
        let epoch = old_epoch(path);
        let name = path.file_name().ok_or(ShmErr::Io)?.to_string_lossy().into_owned();
        #[allow(clippy::disallowed_macros)] // allow-list: shm ring creation at startup
        let tmp = path.with_file_name(format!("{name}.tmp.{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        let ctmp = cpath(&tmp).ok_or(ShmErr::Io)?;
        // SAFETY: valid NUL-terminated path.
        let fd = unsafe {
            libc::open(
                ctmp.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
                0o640 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(ShmErr::Io);
        }
        let mapped = (|| {
            // SAFETY: `fd` is open for writing.
            if unsafe { libc::ftruncate(fd, len as libc::off_t) } != 0 {
                return Err(ShmErr::Io);
            }
            let st = fstat(fd)?;
            Ok((map_fd(fd, len)?, st))
        })();
        // SAFETY: `fd` was opened above and is closed once; the mapping outlives it.
        unsafe { libc::close(fd) };
        let (base, st) = match mapped {
            Ok(v) => v,
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        };
        let map = ShmMap {
            base,
            len,
            dev_ino: (st.st_dev, st.st_ino),
            path: path.to_path_buf(),
            side,
            creator: true,
            attached: true,
        };
        // SAFETY: the fresh file is zero-filled; line 0 is written before the file becomes
        // visible under its final name (rename below), so no other process reads it yet.
        unsafe {
            let h = base.cast::<ShmRingHdr>().as_ptr();
            (*h).magic = ShmRingHdr::MAGIC;
            (*h).version = ShmRingHdr::VERSION;
            (*h).layout_id = layout_id;
            (*h).elem_size = size_of::<T>() as u32;
            (*h).elem_align = align_of::<T>() as u32;
            (*h).capacity = capacity as u64;
            (*h).epoch = epoch;
            (*h).boot_id = boot_id();
            (*h).creator_pid = std::process::id();
        }
        let own = map.own();
        own.pid.store(std::process::id(), Ordering::Relaxed);
        own.start
            .store(proc_start(std::process::id()).unwrap_or(0), Ordering::Relaxed);
        // Pre-touch every page.
        let mut off = 0;
        while off < len {
            // SAFETY: `off < len` inside the live read-write mapping.
            unsafe {
                let p = base.as_ptr().add(off);
                core::ptr::write_volatile(p, core::ptr::read_volatile(p));
            }
            off += page.bytes().min(4096);
        }
        if std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return Err(ShmErr::Io);
        }
        // Release pairs with the attacher's Acquire load of `state`.
        map.hdr().state.store(ShmRingHdr::READY, Ordering::Release);
        Ok(ends::<T>(map, capacity as u64))
    }

    pub(super) fn attach<T: Copy>(path: &Path, side: RingSide, layout_id: u64) -> Result<ShmEnd<T>, ShmErr> {
        let c = cpath(path).ok_or(ShmErr::Io)?;
        // SAFETY: valid NUL-terminated path.
        let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if fd < 0 {
            return Err(ShmErr::Io);
        }
        let mapped = (|| {
            let st = fstat(fd)?;
            let len = st.st_size as usize;
            if len < SLOTS_OFF {
                return Err(ShmErr::Io);
            }
            Ok((map_fd(fd, len)?, st, len))
        })();
        // SAFETY: closed once; the mapping outlives the descriptor.
        unsafe { libc::close(fd) };
        let (base, st, len) = mapped?;
        let mut map = ShmMap {
            base,
            len,
            dev_ino: (st.st_dev, st.st_ino),
            path: path.to_path_buf(),
            side,
            creator: false,
            attached: false,
        };
        let check = || {
            let h = map.hdr();
            if h.magic != ShmRingHdr::MAGIC {
                return Err(ShmErr::BadMagic);
            }
            if h.version != ShmRingHdr::VERSION {
                return Err(ShmErr::VersionMismatch);
            }
            let cap = h.capacity;
            let fits = (cap as usize)
                .checked_mul(size_of::<T>())
                .and_then(|b| b.checked_add(SLOTS_OFF))
                .is_some_and(|need| need <= len);
            if h.layout_id != layout_id
                || h.elem_size as usize != size_of::<T>()
                || h.elem_align as usize != align_of::<T>()
                || cap < 2
                || !cap.is_power_of_two()
                || !fits
            {
                return Err(ShmErr::LayoutMismatch);
            }
            // Acquire pairs with the creator's Release store of `state`.
            match h.state.load(Ordering::Acquire) {
                ShmRingHdr::READY => {}
                ShmRingHdr::CLOSED => return Err(ShmErr::Closed),
                _ => return Err(ShmErr::NotReady),
            }
            claim(map.own())?;
            Ok(cap)
        };
        let cap = check()?;
        map.attached = true;
        Ok(ends::<T>(map, cap))
    }

    pub(super) fn replaced(map: &ShmMap) -> bool {
        match std::fs::metadata(&map.path) {
            Ok(m) => {
                use std::os::unix::fs::MetadataExt;
                (m.dev(), m.ino()) != map.dev_ino
            }
            Err(_) => true,
        }
    }

    pub(super) fn unmap(map: &ShmMap) {
        // SAFETY: (base, len) is exactly the mapping created for this end.
        unsafe { libc::munmap(map.base.as_ptr().cast(), map.len) };
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;

    pub(super) fn create<T: Copy>(
        _: &Path,
        _: usize,
        _: PageSize,
        _: RingSide,
        _: u64,
    ) -> Result<ShmEnd<T>, ShmErr> {
        Err(ShmErr::Io)
    }
    pub(super) fn attach<T: Copy>(_: &Path, _: RingSide, _: u64) -> Result<ShmEnd<T>, ShmErr> {
        Err(ShmErr::Io)
    }
    pub(super) fn replaced(_: &ShmMap) -> bool {
        true
    }
    pub(super) fn unmap(_: &ShmMap) {}
}
