//! Memory: `HugeRegion` (anonymous or file mapping, optional huge pages), `Arena`, `lock_all`.

use std::path::Path;
use std::ptr::NonNull;

use crate::RtErr;

/// Page size of a mapping. `Normal` = 4 KiB (tmpfs hot segments).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PageSize {
    Huge1G,
    Huge2M,
    Normal,
}

impl PageSize {
    /// Bytes per page.
    pub const fn bytes(self) -> usize {
        match self {
            PageSize::Huge1G => 1 << 30,
            PageSize::Huge2M => 2 << 20,
            PageSize::Normal => 4096,
        }
    }

    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn round_up(self, len: usize) -> Option<usize> {
        let pg = self.bytes();
        let len = len.max(1);
        len.checked_add(pg - 1).map(|v| v & !(pg - 1))
    }
}

/// How `HugeRegion::file` opens its file.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// Already exists → `Os(EEXIST)`.
    CreateNew,
    /// Missing → `Os(ENOENT)`; shorter than `len` → `NoMem`.
    OpenExisting,
}

/// A read-write memory mapping (unmapped on drop).
#[derive(Debug)]
pub struct HugeRegion {
    ptr: NonNull<u8>,
    len: usize,
    page: PageSize,
}

// SAFETY: the region is plain memory owned by this value; moving it between threads is fine.
unsafe impl Send for HugeRegion {}
// SAFETY: `&HugeRegion` only exposes the address and length; access to the bytes is `unsafe`
// for the caller.
unsafe impl Sync for HugeRegion {}

impl HugeRegion {
    /// Anonymous private mapping; `len` rounded up to whole pages; huge pages short → `NoMem`.
    pub fn anon(len: usize, page: PageSize, numa: Option<u8>) -> Result<HugeRegion, RtErr> {
        imp::anon(len, page, numa)
    }

    /// Shared file mapping (hugetlbfs or `/dev/shm`).
    pub fn file(path: &Path, len: usize, page: PageSize, mode: OpenMode) -> Result<HugeRegion, RtErr> {
        imp::file(path, len, page, mode)
    }

    /// Reads one byte per page and writes it back (new regions stay zero, contents unchanged).
    pub fn prefault(&self) {
        let pg = self.page.bytes();
        let mut off = 0;
        while off < self.len {
            // SAFETY: `off < len`, so the byte lies inside the live read-write mapping.
            unsafe {
                let p = self.ptr.as_ptr().add(off);
                let v = core::ptr::read_volatile(p);
                core::ptr::write_volatile(p, v);
            }
            off += pg;
        }
    }

    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Mapped length (whole pages).
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn page(&self) -> PageSize {
        self.page
    }

    /// Gives up ownership: the mapping is never unmapped (`'static` users: `Arena`, rings).
    pub(crate) fn leak(self) -> (NonNull<u8>, usize) {
        let me = core::mem::ManuallyDrop::new(self);
        (me.ptr, me.len)
    }
}

impl Drop for HugeRegion {
    fn drop(&mut self) {
        imp::unmap(self.ptr, self.len);
    }
}

/// `mlockall(MCL_CURRENT | MCL_FUTURE)`; failure → `Os(errno)`.
pub fn lock_all() -> Result<(), RtErr> {
    imp::lock_all()
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::sys::{cpath, errno};

    const MAP_HUGE_SHIFT: i32 = 26;

    fn huge_flags(page: PageSize) -> i32 {
        match page {
            PageSize::Normal => 0,
            PageSize::Huge2M => libc::MAP_HUGETLB | (21 << MAP_HUGE_SHIFT),
            PageSize::Huge1G => libc::MAP_HUGETLB | (30 << MAP_HUGE_SHIFT),
        }
    }

    pub(super) fn anon(len: usize, page: PageSize, numa: Option<u8>) -> Result<HugeRegion, RtErr> {
        let len = page.round_up(len).ok_or(RtErr::NoMem)?;
        let flags = libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | huge_flags(page);
        // SAFETY: anonymous mapping request with no address hint; the kernel validates arguments.
        let p = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                flags,
                -1,
                0,
            )
        };
        if p == libc::MAP_FAILED {
            let e = errno();
            return Err(if page != PageSize::Normal || e == libc::ENOMEM {
                RtErr::NoMem
            } else {
                RtErr::Os(e)
            });
        }
        let ptr = NonNull::new(p.cast::<u8>()).ok_or(RtErr::NoMem)?;
        let region = HugeRegion { ptr, len, page };
        if let Some(node) = numa {
            let mask: u64 = 1u64 << (node % 64);
            const MPOL_BIND: i64 = 2;
            // SAFETY: mbind on the mapping just created; `mask` outlives the call; maxnode = 64 bits.
            let r = unsafe {
                libc::syscall(
                    libc::SYS_mbind,
                    p,
                    len,
                    MPOL_BIND,
                    &mask as *const u64,
                    64usize,
                    0u32,
                )
            };
            if r != 0 {
                return Err(RtErr::Os(errno()));
            }
        }
        Ok(region)
    }

    pub(super) fn file(path: &Path, len: usize, page: PageSize, mode: OpenMode) -> Result<HugeRegion, RtErr> {
        let map_len = page.round_up(len).ok_or(RtErr::NoMem)?;
        let c = cpath(path).ok_or(RtErr::Os(libc::EINVAL))?;
        let flags = match mode {
            OpenMode::CreateNew => libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
            OpenMode::OpenExisting => libc::O_RDWR | libc::O_CLOEXEC,
        };
        // SAFETY: `c` is a valid NUL-terminated path.
        let fd = unsafe { libc::open(c.as_ptr(), flags, 0o640 as libc::c_uint) };
        if fd < 0 {
            return Err(RtErr::Os(errno()));
        }
        let res = map_fd(fd, len, map_len, page, mode);
        // SAFETY: `fd` was opened above and is closed exactly once; the mapping stays valid.
        unsafe { libc::close(fd) };
        res
    }

    fn map_fd(
        fd: i32,
        len: usize,
        map_len: usize,
        page: PageSize,
        mode: OpenMode,
    ) -> Result<HugeRegion, RtErr> {
        match mode {
            OpenMode::CreateNew => {
                // SAFETY: `fd` is an open, writable file.
                if unsafe { libc::ftruncate(fd, map_len as libc::off_t) } != 0 {
                    return Err(RtErr::Os(errno()));
                }
            }
            OpenMode::OpenExisting => {
                // SAFETY: zeroed `stat` is a valid out-buffer for fstat.
                let mut st: libc::stat = unsafe { core::mem::zeroed() };
                // SAFETY: `fd` is open; `st` is writable.
                if unsafe { libc::fstat(fd, &mut st) } != 0 {
                    return Err(RtErr::Os(errno()));
                }
                if (st.st_size as u64) < len as u64 {
                    return Err(RtErr::NoMem);
                }
            }
        }
        // SAFETY: shared mapping of an open file; the kernel validates the arguments.
        let p = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if p == libc::MAP_FAILED {
            let e = errno();
            return Err(if e == libc::ENOMEM {
                RtErr::NoMem
            } else {
                RtErr::Os(e)
            });
        }
        let ptr = NonNull::new(p.cast::<u8>()).ok_or(RtErr::NoMem)?;
        Ok(HugeRegion {
            ptr,
            len: map_len,
            page,
        })
    }

    pub(super) fn unmap(ptr: NonNull<u8>, len: usize) {
        // SAFETY: (ptr, len) is exactly a mapping created by this module and not yet unmapped.
        unsafe { libc::munmap(ptr.as_ptr().cast(), len) };
    }

    pub(super) fn lock_all() -> Result<(), RtErr> {
        // SAFETY: mlockall takes only flags.
        if unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) } != 0 {
            return Err(RtErr::Os(errno()));
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;
    use crate::sys::ENOSYS;

    pub(super) fn anon(_: usize, _: PageSize, _: Option<u8>) -> Result<HugeRegion, RtErr> {
        Err(RtErr::Os(ENOSYS))
    }
    pub(super) fn file(_: &Path, _: usize, _: PageSize, _: OpenMode) -> Result<HugeRegion, RtErr> {
        Err(RtErr::Os(ENOSYS))
    }
    pub(super) fn unmap(_: NonNull<u8>, _: usize) {}
    pub(super) fn lock_all() -> Result<(), RtErr> {
        Err(RtErr::Os(ENOSYS))
    }
}

/// Startup bump allocator over a `HugeRegion`; never frees (the region is leaked: `'static`).
#[derive(Debug)]
pub struct Arena {
    base: NonNull<u8>,
    len: usize,
    off: usize,
}

// SAFETY: the arena exclusively owns its leaked region; handing it to another thread is fine.
unsafe impl Send for Arena {}

impl Arena {
    pub fn new(region: HugeRegion) -> Arena {
        let (base, len) = region.leak();
        Arena { base, len, off: 0 }
    }

    /// `n` copies of `init`, aligned to `align_of::<T>()`; not enough space → `ArenaFull`.
    pub fn alloc_slice<T: Copy>(&mut self, n: usize, init: T) -> Result<&'static mut [T], RtErr> {
        let align = align_of::<T>();
        let addr = self.base.as_ptr() as usize;
        let cur = addr.checked_add(self.off).ok_or(RtErr::ArenaFull)?;
        let start = cur.checked_add(align - 1).ok_or(RtErr::ArenaFull)? & !(align - 1);
        let start_off = start - addr;
        let bytes = n.checked_mul(size_of::<T>()).ok_or(RtErr::ArenaFull)?;
        let end = start_off.checked_add(bytes).ok_or(RtErr::ArenaFull)?;
        if end > self.len {
            return Err(RtErr::ArenaFull);
        }
        // SAFETY: `start_off ≤ end ≤ len`, so the pointer stays inside the leaked region.
        let p = unsafe { self.base.as_ptr().add(start_off) }.cast::<T>();
        for i in 0..n {
            // SAFETY: `p + i` lies within [start_off, end) of the region, aligned for T.
            unsafe { p.add(i).write(init) };
        }
        self.off = end;
        // SAFETY: the range is initialized, aligned, inside a never-freed mapping and handed out
        // exactly once (the bump offset moves past it).
        Ok(unsafe { core::slice::from_raw_parts_mut(p, n) })
    }
}
