//! Segment file layout (`SegmentHeader`, `SegmentHeaderFixed`, `SealRecord`, `SchemaEntry`) and the
//! segment memory handle `SegMap` with every memory-level operation the crate needs: shared file
//! mappings (Linux), the heap backend (`mem-backend` and the offline writer), atomic access to the
//! writer and persist rows, and the `statfs`/`fallocate` system calls.
//!
//! This is the only module of the crate with `unsafe` (README "读者", JNL-103).

use std::alloc::Layout as AllocLayout;
#[allow(clippy::disallowed_types)] // allow-list: heap segment backend (tests, offline writer)
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[allow(clippy::disallowed_types)] // allow-list: heap segment backend (tests, offline writer)
use std::sync::{Mutex, MutexGuard, OnceLock};

use hft_types::VenueId;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Header size; records start here.
pub const HEADER_LEN: usize = 4096;
/// `SegmentHeader.magic`.
pub const SEGMENT_MAGIC: [u8; 8] = *b"HFTJRNL1";
/// `SegmentHeader.format_version` written by this crate.
pub const FORMAT_VERSION: u16 = 1;
/// `SegmentHeader.flags`: the previous segment's tail may have been lost (power loss).
pub const RECOVERY_GAP: u8 = 1;
/// `SealRecord.magic`.
pub const SEAL_MAGIC: [u8; 8] = *b"HFTSEAL1";
/// Bytes covered by `header_crc32c`.
pub const HEADER_CRC_LEN: usize = 3068;
/// Immutable region of the header (`prev_header_sha256` covers it).
pub const HEADER_FIXED_LEN: usize = 3072;
/// Writer row + persist row: excluded from cold/hot comparisons and content hashes.
pub const SHARED_ROWS: core::ops::Range<usize> = 3072..3200;

pub(crate) const OFF_COMMITTED: usize = 3072;
pub(crate) const OFF_LAST_SEQ: usize = 3080;
pub(crate) const OFF_STATE: usize = 3088;
pub(crate) const OFF_PERSISTED: usize = 3136;
pub(crate) const OFF_SYNCED: usize = 3144;
pub(crate) const OFF_RETIRED: usize = 3152;

/// One `SegmentHeader.schema[]` entry: a kind known to the writer binary (8 B).
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SchemaEntry {
    pub kind: u16,
    pub version: u16,
    /// Fixed: body size; Var/Multi: prefix size.
    pub size: u32,
}
const _: () = assert!(size_of::<SchemaEntry>() == 8 && align_of::<SchemaEntry>() == 4);

/// The immutable region `0..3072` of a segment header (zerocopy view; journal-format "SegmentHeader").
#[repr(C, align(8))]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SegmentHeaderFixed {
    pub magic: [u8; 8],
    pub format_version: u16,
    pub journal_kind: u8,
    pub venue: VenueId,
    pub header_len: u32,
    pub journal_name: [u8; 32],
    pub instance: [u8; 16],
    pub segment_no: u64,
    pub first_seq: u64,
    pub prev_last_seq: u64,
    pub created_utc_ns: i64,
    pub created_tsc: u64,
    pub segment_bytes: u64,
    pub writer_epoch: u32,
    pub roll_reason: u8,
    pub flags: u8,
    pub _pad0: u16,
    pub tsc_hz_milli: u64,
    pub calib_tsc: u64,
    pub calib_realtime_ns: i64,
    pub calib_phc_ns: i64,
    pub calib_err_ns: u32,
    pub clock_max_error_ns: u32,
    pub prev_header_sha256: [u8; 32],
    pub host_profile_sha256: [u8; 32],
    pub build_git: [u8; 20],
    pub _pad1: [u8; 4],
    pub n_schema: u16,
    pub n_config: u16,
    pub _pad2: u32,
    pub schema: [SchemaEntry; 128],
    pub config_digest: [u8; 32],
    pub _rsv: [u8; 1756],
    pub header_crc32c: u32,
}

/// The whole 4 096 B segment header, including the atomic writer row and persist row. Not
/// `FromBytes`: readers take a reference to the mapped first page (journal-format "SegmentHeader").
#[repr(C, align(8))]
pub struct SegmentHeader {
    pub magic: [u8; 8],
    pub format_version: u16,
    pub journal_kind: u8,
    pub venue: VenueId,
    pub header_len: u32,
    pub journal_name: [u8; 32],
    pub instance: [u8; 16],
    pub segment_no: u64,
    pub first_seq: u64,
    pub prev_last_seq: u64,
    pub created_utc_ns: i64,
    pub created_tsc: u64,
    pub segment_bytes: u64,
    pub writer_epoch: u32,
    pub roll_reason: u8,
    pub flags: u8,
    pub _pad0: u16,
    pub tsc_hz_milli: u64,
    pub calib_tsc: u64,
    pub calib_realtime_ns: i64,
    pub calib_phc_ns: i64,
    pub calib_err_ns: u32,
    pub clock_max_error_ns: u32,
    pub prev_header_sha256: [u8; 32],
    pub host_profile_sha256: [u8; 32],
    pub build_git: [u8; 20],
    pub _pad1: [u8; 4],
    pub n_schema: u16,
    pub n_config: u16,
    pub _pad2: u32,
    pub schema: [SchemaEntry; 128],
    pub config_digest: [u8; 32],
    pub _rsv: [u8; 1756],
    pub header_crc32c: u32,
    // —— writer row ——
    pub committed_len: AtomicU64,
    pub last_seq: AtomicU64,
    pub state: AtomicU32,
    pub _w: [u8; 44],
    // —— persist row ——
    pub persisted_len: AtomicU64,
    pub synced_len: AtomicU64,
    pub retired: AtomicU32,
    pub _p: [u8; 44],
    pub _tail: [u8; 896],
}

impl core::fmt::Debug for SegmentHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SegmentHeader")
            .field("segment_no", &self.segment_no)
            .field("first_seq", &self.first_seq)
            .field("committed_len", &self.committed_len.load(Ordering::Relaxed))
            .field("state", &self.state.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

const _: () = {
    use core::mem::{align_of, offset_of, size_of};
    assert!(size_of::<SegmentHeader>() == 4096 && align_of::<SegmentHeader>() == 8);
    assert!(size_of::<SegmentHeaderFixed>() == HEADER_FIXED_LEN);
    assert!(offset_of!(SegmentHeader, schema) == 256);
    assert!(offset_of!(SegmentHeader, config_digest) == 1280);
    assert!(offset_of!(SegmentHeader, header_crc32c) == HEADER_CRC_LEN);
    assert!(offset_of!(SegmentHeaderFixed, header_crc32c) == HEADER_CRC_LEN);
    assert!(offset_of!(SegmentHeader, committed_len) == OFF_COMMITTED);
    assert!(offset_of!(SegmentHeader, last_seq) == OFF_LAST_SEQ);
    assert!(offset_of!(SegmentHeader, state) == OFF_STATE);
    assert!(offset_of!(SegmentHeader, persisted_len) == OFF_PERSISTED);
    assert!(offset_of!(SegmentHeader, synced_len) == OFF_SYNCED);
    assert!(offset_of!(SegmentHeader, retired) == OFF_RETIRED);
    assert!(offset_of!(SegmentHeader, _tail) == 3200);
};

/// `.seal` file content (128 B; journal-format "冷副本与热段").
#[repr(C, align(8))]
#[derive(Copy, Clone, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SealRecord {
    pub magic: [u8; 8],
    pub segment_no: u64,
    pub first_seq: u64,
    pub last_seq: u64,
    pub len: u64,
    pub sealed_utc_ns: i64,
    /// SHA-256 of the cold copy `[0, len)` with `3072..3200` zeroed.
    pub sha256: [u8; 32],
    pub reason: u8,
    pub flags: u8,
    pub _pad: [u8; 2],
    pub _rsv: [u8; 40],
    /// CRC32C of the first 124 bytes.
    pub crc32c: u32,
}
const _: () = assert!(core::mem::size_of::<SealRecord>() == 128);

impl SealRecord {
    /// CRC over the first 124 bytes.
    pub fn compute_crc(&self) -> u32 {
        crc32c::crc32c(self.as_bytes().get(..124).unwrap_or(&[]))
    }
}

impl SegmentHeaderFixed {
    /// CRC over `0..3068`.
    pub fn compute_crc(&self) -> u32 {
        crc32c::crc32c(self.as_bytes().get(..HEADER_CRC_LEN).unwrap_or(&[]))
    }

    /// A zeroed header.
    pub fn zeroed() -> SegmentHeaderFixed {
        zerocopy::FromZeros::new_zeroed()
    }
}

// ———————————————————————————— segment memory ————————————————————————————

/// Where a `SegMap`'s memory comes from (decides how it is released).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) enum MapSrc {
    /// `mmap` of a file (Linux).
    Os,
    /// File of the in-memory backend (id in the store).
    Mem(u64),
    /// Private heap block (offline writer, cold copies off Linux).
    Heap,
}

/// A mapped segment (or heap block): base address and length. Plain data, `Copy`: whoever
/// created it releases it exactly once with `release` (the writer never does; README "旧段回收").
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct SegMap {
    base: NonNull<u8>,
    len: usize,
    pub(crate) src: MapSrc,
}

// SAFETY: a `SegMap` is an address range of shared memory; every access goes through the methods
// below (raw copies and atomics), which are valid from any thread.
unsafe impl Send for SegMap {}
// SAFETY: as above; `&SegMap` exposes no non-atomic interior mutability of its own.
unsafe impl Sync for SegMap {}

impl SegMap {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn base_addr(&self) -> usize {
        self.base.as_ptr() as usize
    }

    /// `[off, off + n)` as a slice; out of range → empty.
    ///
    /// The crate only borrows ranges no one writes concurrently (committed records, the header's
    /// immutable region after `Open`, or the writer's own region before publication).
    pub(crate) fn bytes(&self, off: usize, n: usize) -> &[u8] {
        match off.checked_add(n) {
            Some(end) if end <= self.len => {
                // SAFETY: `[off, end)` lies inside the live mapping (`end <= len`); the mapping stays
                // valid while the owner keeps this `SegMap` (released only by `release`), and the
                // range is not written while the borrow is used (see above).
                unsafe { core::slice::from_raw_parts(self.base.as_ptr().add(off), n) }
            }
            _ => &[],
        }
    }

    /// Copies `src` to `off` (out of range → nothing written).
    pub(crate) fn write(&self, off: usize, src: &[u8]) {
        if off.checked_add(src.len()).is_some_and(|e| e <= self.len) {
            // SAFETY: destination `[off, off + len)` is inside the writable mapping; `src` is a
            // separate borrow, so the ranges cannot overlap unless the caller passes a slice of this
            // same mapping, which the crate never does.
            unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), self.base.as_ptr().add(off), src.len()) };
        }
    }

    /// Zeroes `[off, off + n)` (clamped to the mapping).
    pub(crate) fn zero(&self, off: usize, n: usize) {
        let end = off.saturating_add(n).min(self.len);
        if off < end {
            // SAFETY: `[off, end)` is inside the writable mapping.
            unsafe { core::ptr::write_bytes(self.base.as_ptr().add(off), 0, end - off) };
        }
    }

    /// A mutable view of `[off, off + n)` for in-place writes by the single writer (`VarSlot`).
    #[allow(clippy::mut_from_ref)]
    pub(crate) fn bytes_mut(&self, off: usize, n: usize) -> &mut [u8] {
        match off.checked_add(n) {
            Some(end) if end <= self.len => {
                // SAFETY: inside the mapping; the range is beyond `committed_len` and owned by the
                // single writer, so no other reference to it exists while the `VarSlot` lives.
                unsafe { core::slice::from_raw_parts_mut(self.base.as_ptr().add(off), n) }
            }
            _ => &mut [],
        }
    }

    fn atom64(&self, off: usize) -> Option<&AtomicU64> {
        if !off.is_multiple_of(8) || off + 8 > self.len {
            return None;
        }
        // SAFETY: 8-byte aligned (base is page aligned, `off % 8 == 0`), inside the mapping, and
        // only ever accessed atomically by this crate (writer row / persist row).
        Some(unsafe { AtomicU64::from_ptr(self.base.as_ptr().add(off).cast::<u64>()) })
    }

    fn atom32(&self, off: usize) -> Option<&AtomicU32> {
        if !off.is_multiple_of(4) || off + 4 > self.len {
            return None;
        }
        // SAFETY: 4-byte aligned, inside the mapping, accessed atomically only.
        Some(unsafe { AtomicU32::from_ptr(self.base.as_ptr().add(off).cast::<u32>()) })
    }

    pub(crate) fn load64(&self, off: usize) -> u64 {
        self.atom64(off).map_or(0, |a| a.load(Ordering::Acquire))
    }

    pub(crate) fn store64(&self, off: usize, v: u64) {
        if let Some(a) = self.atom64(off) {
            a.store(v, Ordering::Release);
        }
    }

    pub(crate) fn load32(&self, off: usize) -> u32 {
        self.atom32(off).map_or(0, |a| a.load(Ordering::Acquire))
    }

    pub(crate) fn store32(&self, off: usize, v: u32) {
        if let Some(a) = self.atom32(off) {
            a.store(v, Ordering::Release);
        }
    }

    /// Copy of the immutable region (zeroed when the mapping is shorter than 3 072 B).
    pub(crate) fn fixed(&self) -> SegmentHeaderFixed {
        SegmentHeaderFixed::read_from_bytes(self.bytes(0, HEADER_FIXED_LEN))
            .unwrap_or_else(|_| SegmentHeaderFixed::zeroed())
    }

    pub(crate) fn write_fixed(&self, f: &SegmentHeaderFixed) {
        self.write(0, f.as_bytes());
    }
}

/// Releases a mapping created by this module (munmap / heap free / backend unref).
pub(crate) fn release(m: SegMap) {
    match m.src {
        MapSrc::Os => os::unmap(m),
        MapSrc::Heap => heap_free(m),
        MapSrc::Mem(id) => mem_release(id),
    }
}

const HEAP_ALIGN: usize = 4096;

fn heap_layout(len: usize) -> Option<AllocLayout> {
    AllocLayout::from_size_align(len.max(HEAP_ALIGN), HEAP_ALIGN).ok()
}

/// A zeroed, page-aligned heap block of `len` bytes.
pub(crate) fn heap_alloc(len: usize) -> io::Result<SegMap> {
    Ok(SegMap {
        base: raw_alloc(len)?,
        len,
        src: MapSrc::Heap,
    })
}

fn raw_alloc(len: usize) -> io::Result<NonNull<u8>> {
    let lay = heap_layout(len).ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: `lay` has a non-zero size.
    let p = unsafe { std::alloc::alloc_zeroed(lay) };
    NonNull::new(p).ok_or_else(|| io::Error::from(io::ErrorKind::OutOfMemory))
}

fn raw_free(p: NonNull<u8>, len: usize) {
    if let Some(lay) = heap_layout(len) {
        // SAFETY: `p` came from `raw_alloc(len)` with the same layout and is freed exactly once.
        unsafe { std::alloc::dealloc(p.as_ptr(), lay) };
    }
}

fn heap_free(m: SegMap) {
    raw_free(m.base, m.len);
}

// ———————————————————————————— in-memory backend ————————————————————————————

struct MemFile {
    base: NonNull<u8>,
    /// Allocated bytes.
    cap: usize,
    /// Visible file length (`mem_truncate` may shrink it).
    len: usize,
    maps: usize,
    linked: bool,
}

#[derive(Default)]
#[allow(clippy::disallowed_types)] // allow-list: heap segment backend (tests, offline writer)
struct MemStore {
    roots: Vec<PathBuf>,
    files: BTreeMap<PathBuf, u64>,
    data: HashMap<u64, MemFile>,
    next: u64,
}

// SAFETY: the store owns its heap blocks; access is serialised by the mutex.
unsafe impl Send for MemStore {}

#[allow(clippy::disallowed_types)] // allow-list: heap segment backend (tests, offline writer)
fn mem() -> MutexGuard<'static, MemStore> {
    static S: OnceLock<Mutex<MemStore>> = OnceLock::new();
    let m = S.get_or_init(|| Mutex::new(MemStore::default()));
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn not_found() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
}

/// Makes `root` (and everything below it) a directory of the in-memory backend.
pub(crate) fn mem_register_root(root: &Path) {
    let mut s = mem();
    if !s.roots.iter().any(|r| r == root) {
        s.roots.push(root.to_path_buf());
    }
}

pub(crate) fn mem_is_under_root(p: &Path) -> bool {
    let s = mem();
    s.roots.iter().any(|r| p.starts_with(r))
}

/// Creates a zeroed file of `len` bytes and maps it (`O_EXCL` semantics).
pub(crate) fn mem_create(path: &Path, len: usize) -> io::Result<SegMap> {
    let mut s = mem();
    if s.files.contains_key(path) {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    let base = raw_alloc(len)?;
    s.next += 1;
    let id = s.next;
    s.files.insert(path.to_path_buf(), id);
    s.data.insert(
        id,
        MemFile {
            base,
            cap: len,
            len,
            maps: 1,
            linked: true,
        },
    );
    Ok(SegMap {
        base,
        len,
        src: MapSrc::Mem(id),
    })
}

/// Maps an existing file (`counted` = counts as a mapping that must be released).
pub(crate) fn mem_open(path: &Path, counted: bool) -> io::Result<SegMap> {
    let mut s = mem();
    let id = *s.files.get(path).ok_or_else(not_found)?;
    let f = s.data.get_mut(&id).ok_or_else(not_found)?;
    if counted {
        f.maps += 1;
    }
    Ok(SegMap {
        base: f.base,
        len: f.len,
        src: if counted { MapSrc::Mem(id) } else { MapSrc::Mem(0) },
    })
}

pub(crate) fn mem_unlink(path: &Path) -> io::Result<()> {
    let mut s = mem();
    let id = s.files.remove(path).ok_or_else(not_found)?;
    let free = match s.data.get_mut(&id) {
        Some(f) => {
            f.linked = false;
            f.maps == 0
        }
        None => false,
    };
    if free && let Some(f) = s.data.remove(&id) {
        raw_free(f.base, f.cap);
    }
    Ok(())
}

fn mem_release(id: u64) {
    if id == 0 {
        return;
    }
    let mut s = mem();
    let free = match s.data.get_mut(&id) {
        Some(f) => {
            f.maps = f.maps.saturating_sub(1);
            f.maps == 0 && !f.linked
        }
        None => false,
    };
    if free && let Some(f) = s.data.remove(&id) {
        raw_free(f.base, f.cap);
    }
}

/// Frees every file below `root` regardless of mappings (tests/benches tear-down only: no mapping
/// of those files may be used afterwards).
pub(crate) fn mem_purge(root: &Path) {
    let mut s = mem();
    let gone: Vec<(PathBuf, u64)> = s
        .files
        .iter()
        .filter(|(p, _)| p.starts_with(root))
        .map(|(p, id)| (p.clone(), *id))
        .collect();
    for (p, id) in gone {
        s.files.remove(&p);
        if let Some(f) = s.data.remove(&id) {
            raw_free(f.base, f.cap);
        }
    }
}

/// Whether the file behind a mapping is still linked (`st_nlink > 0`).
pub(crate) fn mem_linked(id: u64) -> bool {
    let s = mem();
    s.data.get(&id).is_some_and(|f| f.linked)
}

pub(crate) fn mem_len(path: &Path) -> Option<usize> {
    let s = mem();
    let id = s.files.get(path)?;
    s.data.get(id).map(|f| f.len)
}

/// Shrinks the visible length of a file (tests: "截短").
pub(crate) fn mem_truncate(path: &Path, len: usize) -> io::Result<()> {
    let mut s = mem();
    let id = *s.files.get(path).ok_or_else(not_found)?;
    let f = s.data.get_mut(&id).ok_or_else(not_found)?;
    f.len = len.min(f.cap);
    Ok(())
}

/// Files directly inside `dir`.
pub(crate) fn mem_list(dir: &Path) -> Vec<PathBuf> {
    let s = mem();
    s.files
        .keys()
        .filter(|p| p.parent() == Some(dir))
        .cloned()
        .collect()
}

/// Every file below `dir` (any depth).
pub(crate) fn mem_list_all(dir: &Path) -> Vec<PathBuf> {
    let s = mem();
    s.files.keys().filter(|p| p.starts_with(dir)).cloned().collect()
}

// ———————————————————————————— OS mappings and calls ————————————————————————————

/// Kind of file system behind a directory (`statfs` magic).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) enum FsKind {
    Hugetlbfs { page: usize },
    Tmpfs,
    Other,
}

#[cfg_attr(not(feature = "writer"), allow(unused_imports))]
pub(crate) use os::{fallocate, fs_kind, map_file};

#[cfg(target_os = "linux")]
mod os {
    use std::os::fd::AsRawFd;

    use super::*;

    const HUGETLBFS_MAGIC: i64 = 0x9584_58f6;
    const TMPFS_MAGIC: i64 = 0x0102_1994;

    pub(crate) fn fs_kind(p: &Path) -> io::Result<FsKind> {
        let c = std::ffi::CString::new(p.as_os_str().as_encoded_bytes())
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: a zeroed `statfs` is a valid out-buffer.
        let mut st: libc::statfs = unsafe { core::mem::zeroed() };
        // SAFETY: `c` is NUL-terminated; `st` is writable.
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return Err(io::Error::last_os_error());
        }
        #[allow(clippy::unnecessary_cast)]
        let magic = st.f_type as i64;
        Ok(if magic == HUGETLBFS_MAGIC {
            FsKind::Hugetlbfs {
                page: st.f_bsize as usize,
            }
        } else if magic == TMPFS_MAGIC {
            FsKind::Tmpfs
        } else {
            FsKind::Other
        })
    }

    /// Maps `len` bytes of `file` (`MAP_SHARED`); `populate` adds `MAP_POPULATE`, `lock` adds `mlock`.
    pub(crate) fn map_file(
        file: &File,
        len: usize,
        writable: bool,
        populate: bool,
        lock: bool,
    ) -> io::Result<SegMap> {
        if len == 0 {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let prot = if writable {
            libc::PROT_READ | libc::PROT_WRITE
        } else {
            libc::PROT_READ
        };
        let flags = libc::MAP_SHARED | if populate { libc::MAP_POPULATE } else { 0 };
        // SAFETY: shared mapping of an open file descriptor; the kernel validates the arguments.
        let p = unsafe { libc::mmap(core::ptr::null_mut(), len, prot, flags, file.as_raw_fd(), 0) };
        if p == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let base = NonNull::new(p.cast::<u8>()).ok_or_else(|| io::Error::from(io::ErrorKind::OutOfMemory))?;
        let m = SegMap {
            base,
            len,
            src: MapSrc::Os,
        };
        // SAFETY: `(p, len)` is the mapping just created.
        if lock && unsafe { libc::mlock(p, len) } != 0 {
            let e = io::Error::last_os_error();
            unmap(m);
            return Err(e);
        }
        Ok(m)
    }

    pub(crate) fn unmap(m: SegMap) {
        // SAFETY: `(base, len)` is exactly a mapping created by `map_file` and released once.
        unsafe { libc::munmap(m.base.as_ptr().cast(), m.len) };
    }

    /// Reserves `len` bytes of disk for `file` (`posix_fallocate`; falls back to `set_len`).
    pub(crate) fn fallocate(file: &File, len: u64) -> io::Result<()> {
        // SAFETY: `fd` is an open, writable descriptor.
        let r = unsafe { libc::posix_fallocate(file.as_raw_fd(), 0, len as libc::off_t) };
        if r == 0 {
            return Ok(());
        }
        if r == libc::EOPNOTSUPP || r == libc::EINVAL {
            return file.set_len(len);
        }
        Err(io::Error::from_raw_os_error(r))
    }
}

#[cfg(not(target_os = "linux"))]
mod os {
    use super::*;

    pub(crate) fn fs_kind(p: &Path) -> io::Result<FsKind> {
        std::fs::metadata(p)?;
        Ok(FsKind::Other)
    }

    pub(crate) fn map_file(_: &File, _: usize, _: bool, _: bool, _: bool) -> io::Result<SegMap> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub(crate) fn unmap(_: SegMap) {}

    pub(crate) fn fallocate(file: &File, len: u64) -> io::Result<()> {
        file.set_len(len)
    }
}
