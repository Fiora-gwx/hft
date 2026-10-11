//! File access: hot segments (shared mappings on hugetlbfs/tmpfs, or the in-memory backend),
//! cold copies (ordinary files), naming, directory modes and the in-process mapping registry.

#![allow(clippy::disallowed_types, clippy::disallowed_macros)] // allow-list: segment files, naming and mapping registry; called at open, roll preparation, persist and readers, not per append

use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use hft_rt::PageSize;

use crate::layout::{self, FsKind, MapSrc, SegMap};
use crate::oplog::{self, Op};

// ———————————————————————————— naming ————————————————————————————

/// `<segment_no:010>.jrn`.
pub(crate) fn hot_seg_name(no: u64) -> String {
    format!("{no:010}.jrn")
}

/// Private preparer file, never handed to the writer or enumerated by readers.
pub(crate) fn hot_staging_name(no: u64) -> String {
    format!("{no:010}.jrn.preparing")
}

/// Parses `<10 digits>.jrn`.
pub(crate) fn parse_hot_name(name: &str) -> Option<u64> {
    let d = name.strip_suffix(".jrn")?;
    (d.len() == 10 && d.bytes().all(|c| c.is_ascii_digit())).then(|| d.parse().ok())?
}

/// Parses `<10 digits>-<20 digits>.jrn` → `(segment_no, first_seq)`.
pub(crate) fn parse_cold_name(name: &str) -> Option<(u64, u64)> {
    let d = name.strip_suffix(".jrn")?;
    let (a, b) = d.split_once('-')?;
    if a.len() != 10 || b.len() != 20 || !a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((a.parse().ok()?, b.parse().ok()?))
}

/// UTC calendar date `yyyymmdd` of `ns` since the epoch.
pub(crate) fn utc_yyyymmdd(ns: i64) -> String {
    let days = ns.div_euclid(86_400_000_000_000);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}{m:02}{d:02}")
}

/// Cold copy path stem `<cold_jdir>/<yyyymmdd>/<no:010>-<first_seq:020>` (add `.jrn`/`.seal`/`.uploaded`).
pub(crate) fn cold_stem(cold_jdir: &Path, created_utc_ns: i64, no: u64, first_seq: u64) -> PathBuf {
    cold_jdir
        .join(utc_yyyymmdd(created_utc_ns))
        .join(format!("{no:010}-{first_seq:020}"))
}

pub(crate) fn with_ext(stem: &Path, ext: &str) -> PathBuf {
    let mut s = stem.as_os_str().to_os_string();
    s.push(".");
    s.push(ext);
    PathBuf::from(s)
}

// ———————————————————————————— hot segments ————————————————————————————

/// Whether `p` belongs to the in-memory backend.
pub(crate) fn is_mem(p: &Path) -> bool {
    layout::mem_is_under_root(p)
}

/// Checks that `dir` exists and is on hugetlbfs or tmpfs (the in-memory backend counts as tmpfs).
pub(crate) fn ram_fs(dir: &Path) -> io::Result<FsKind> {
    if is_mem(dir) {
        return Ok(FsKind::Tmpfs);
    }
    layout::fs_kind(dir)
}

pub(crate) fn ensure_hot_dir(dir: &Path) -> io::Result<()> {
    if is_mem(dir) {
        return Ok(());
    }
    std::fs::create_dir_all(dir)
}

fn inproc() -> MutexGuard<'static, HashMap<PathBuf, SegMap>> {
    static R: OnceLock<Mutex<HashMap<PathBuf, SegMap>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Creates (`O_EXCL`), sizes and maps a new hot segment, pre-faulted (and `mlock`ed for 4 KiB pages).
/// On failure no file is left behind.
pub(crate) fn hot_create(path: &Path, len: usize, page: PageSize) -> io::Result<SegMap> {
    let m = if is_mem(path) {
        layout::mem_create(path, len)?
    } else {
        let f = create_new_rw(path)?;
        let r = f
            .set_len(len as u64)
            .and_then(|()| layout::map_file(&f, len, true, true, page == PageSize::Normal));
        match r {
            Ok(m) => m,
            Err(e) => {
                drop(f);
                let _ = std::fs::remove_file(path);
                return Err(e);
            }
        }
    };
    oplog::record(Op::Map);
    inproc().insert(path.to_path_buf(), m);
    Ok(m)
}

/// Atomically publishes a fully prepared mapping without replacing an existing segment.
pub(crate) fn hot_publish(staging: &Path, path: &Path) -> io::Result<()> {
    if is_mem(staging) {
        layout::mem_rename_noreplace(staging, path)?;
    } else {
        layout::rename_noreplace(staging, path)?;
    }
    let mut maps = inproc();
    if let Some(map) = maps.remove(staging) {
        maps.insert(path.to_path_buf(), map);
    }
    Ok(())
}

fn create_new_rw(path: &Path) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o640);
    }
    o.open(path)
}

/// An open mapping of an existing segment plus its file (for `st_nlink` checks).
#[derive(Debug)]
pub(crate) struct HotOpen {
    pub map: SegMap,
    pub file: Option<File>,
}

/// Maps an existing segment file (whole file).
pub(crate) fn hot_open(path: &Path, writable: bool) -> io::Result<HotOpen> {
    if is_mem(path) {
        let map = layout::mem_open(path, true)?;
        oplog::record(Op::Map);
        return Ok(HotOpen { map, file: None });
    }
    let file = OpenOptions::new().read(true).write(writable).open(path)?;
    let len = file.metadata()?.len() as usize;
    let map = if cfg!(target_os = "linux") {
        layout::map_file(&file, len, writable, false, false)?
    } else {
        // Off Linux only the in-memory backend can hold hot segments; readers of files written by
        // the offline writer copy them.
        heap_copy(&file, len)?
    };
    oplog::record(Op::Map);
    if writable {
        inproc().insert(path.to_path_buf(), map);
    }
    Ok(HotOpen {
        map,
        file: Some(file),
    })
}

/// An existing in-process mapping of `path` (same-process readers; no new `mmap`).
pub(crate) fn hot_inproc(path: &Path) -> Option<SegMap> {
    if is_mem(path) {
        return layout::mem_open(path, false).ok();
    }
    inproc().get(path).copied()
}

/// Releases a mapping obtained from `hot_create`/`hot_open`/`cold_open` (munmap).
pub(crate) fn release(path: Option<&Path>, m: SegMap) {
    if let Some(p) = path {
        let mut r = inproc();
        if r.get(p).is_some_and(|x| x.base_addr() == m.base_addr()) {
            r.remove(p);
        }
    }
    if m.src != MapSrc::Mem(0) {
        oplog::record(Op::Unmap);
    }
    layout::release(m);
}

pub(crate) fn hot_unlink(path: &Path) -> io::Result<()> {
    oplog::record(Op::Unlink);
    if is_mem(path) {
        return layout::mem_unlink(path);
    }
    std::fs::remove_file(path)
}

/// Whether the file behind an open mapping still has a link.
pub(crate) fn still_linked(h: &HotOpen) -> bool {
    match h.map.src {
        MapSrc::Mem(id) if id != 0 => layout::mem_linked(id),
        _ => match &h.file {
            #[cfg(unix)]
            Some(f) => {
                use std::os::unix::fs::MetadataExt;
                f.metadata().map(|m| m.nlink() > 0).unwrap_or(false)
            }
            #[cfg(not(unix))]
            Some(_) => true,
            None => true,
        },
    }
}

/// Hot segment files `<10 digits>.jrn` directly in `dir`, by segment number.
pub(crate) fn hot_list(dir: &Path) -> BTreeMap<u64, PathBuf> {
    hot_list_with_suffix(dir, "")
}

/// Unpublished preparer files; even an empty file is owned by the preparer.
pub(crate) fn hot_staging_list(dir: &Path) -> BTreeMap<u64, PathBuf> {
    hot_list_with_suffix(dir, ".preparing")
}

fn hot_list_with_suffix(dir: &Path, suffix: &str) -> BTreeMap<u64, PathBuf> {
    let names: Vec<PathBuf> = if is_mem(dir) {
        layout::mem_list(dir)
    } else {
        match std::fs::read_dir(dir) {
            Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
            Err(_) => Vec::new(),
        }
    };
    names
        .into_iter()
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.strip_suffix(suffix)?;
            let n = parse_hot_name(name)?;
            Some((n, p))
        })
        .collect()
}

/// Whole file content (hot backend aware).
pub(crate) fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    if is_mem(path) {
        let m = layout::mem_open(path, false)?;
        return Ok(m.bytes(0, m.len()).to_vec());
    }
    std::fs::read(path)
}

/// First `n` bytes (fewer when the file is shorter).
pub(crate) fn read_prefix(path: &Path, n: usize) -> io::Result<Vec<u8>> {
    if is_mem(path) {
        let m = layout::mem_open(path, false)?;
        return Ok(m.bytes(0, n.min(m.len())).to_vec());
    }
    let mut f = File::open(path)?;
    let mut b = vec![0u8; n];
    let mut got = 0;
    while got < n {
        let k = f.read(&mut b[got..])?;
        if k == 0 {
            break;
        }
        got += k;
    }
    b.truncate(got);
    Ok(b)
}

pub(crate) fn file_len(path: &Path) -> Option<u64> {
    if is_mem(path) {
        return layout::mem_len(path).map(|l| l as u64);
    }
    std::fs::metadata(path).ok().map(|m| m.len())
}

// ———————————————————————————— cold copies ————————————————————————————

/// A cold copy found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColdSeg {
    pub first_seq: u64,
    /// Path without extension.
    pub stem: PathBuf,
}

impl ColdSeg {
    pub fn jrn(&self) -> PathBuf {
        with_ext(&self.stem, "jrn")
    }
    pub fn seal(&self) -> PathBuf {
        with_ext(&self.stem, "seal")
    }
}

/// Cold copies `<cold_jdir>/<yyyymmdd>/<no>-<first>.jrn`, by segment number.
pub(crate) fn cold_list(cold_jdir: &Path) -> BTreeMap<u64, ColdSeg> {
    let mut out = BTreeMap::new();
    let Ok(days) = std::fs::read_dir(cold_jdir) else {
        return out;
    };
    for d in days.flatten() {
        let dp = d.path();
        if !dp.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&dp) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            let Some((no, first)) = p.file_name().and_then(|n| n.to_str()).and_then(parse_cold_name) else {
                continue;
            };
            out.insert(
                no,
                ColdSeg {
                    first_seq: first,
                    stem: p.with_extension(""),
                },
            );
        }
    }
    out
}

/// Creates the directories from `base` down to `dir` that do not exist, each with mode exactly
/// `0770` (`2770` when the parent carries setgid; README "冷副本目录").
pub(crate) fn mkdir_0770(base: &Path, dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if dir != base
        && let Some(parent) = dir.parent()
    {
        mkdir_0770(base, parent)?;
    }
    match make_dir(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(unix)]
fn make_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().mode(0o770).create(dir)?;
    let mode = std::fs::metadata(dir)?.permissions().mode();
    let want = (mode & 0o2000) | 0o770;
    if mode & 0o7777 != want {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(want))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir(dir)
}

/// `pwrite` (positional write; no seek state).
pub(crate) fn pwrite(f: &File, off: u64, b: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        f.write_all_at(b, off)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0usize;
        while done < b.len() {
            let n = f.seek_write(&b[done..], off + done as u64)?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::WriteZero));
            }
            done += n;
        }
        Ok(())
    }
}

/// `pread` of exactly `b.len()` bytes.
pub(crate) fn pread(f: &File, off: u64, b: &mut [u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        f.read_exact_at(b, off)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0usize;
        while done < b.len() {
            let n = f.seek_read(&mut b[done..], off + done as u64)?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            done += n;
        }
        Ok(())
    }
}

/// `fsync` of a directory (no-op off Unix).
pub(crate) fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

/// `fdatasync` of a file (counted in the operation log).
pub(crate) fn datasync(f: &File) -> io::Result<()> {
    oplog::record(Op::Sync);
    f.sync_data()
}

/// Maps a cold copy read-only (Linux) or copies it to the heap (elsewhere). Empty file → error.
pub(crate) fn cold_open(path: &Path) -> io::Result<HotOpen> {
    let file = File::open(path)?;
    let len = file.metadata()?.len() as usize;
    if len == 0 {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    let map = if cfg!(target_os = "linux") {
        layout::map_file(&file, len, false, false, false)?
    } else {
        heap_copy(&file, len)?
    };
    oplog::record(Op::Map);
    Ok(HotOpen {
        map,
        file: Some(file),
    })
}

fn heap_copy(file: &File, len: usize) -> io::Result<SegMap> {
    if len == 0 {
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    let m = layout::heap_alloc(len)?;
    let mut buf = vec![0u8; len];
    if let Err(e) = pread(file, 0, &mut buf) {
        layout::release(m);
        return Err(e);
    }
    m.write(0, &buf);
    Ok(m)
}
