//! Test hooks (feature `mem-backend`): the heap segment backend, the per-thread operation log and
//! raw byte access to segment files of either backend.

use std::io;
use std::path::{Path, PathBuf};

pub use crate::oplog::Op;

/// Makes `root` and everything below it live in the heap backend (any platform).
pub fn mem_root(root: &Path) {
    crate::layout::mem_register_root(root);
}

/// Frees every heap-backend file below `root`, mapped or not. Tear-down only: drop every writer,
/// reader, preparer, persister and retirer of those files first.
pub fn mem_purge(root: &Path) {
    crate::layout::mem_purge(root);
}

/// Whether `p` is in the heap backend.
pub fn is_mem(p: &Path) -> bool {
    crate::fs::is_mem(p)
}

/// Starts recording backend operations of the calling thread.
pub fn oplog_start() {
    crate::oplog::start();
}

/// Stops recording and returns what the calling thread did.
pub fn oplog_take() -> Vec<Op> {
    crate::oplog::take()
}

/// Whole content of a segment file (heap backend or disk).
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    crate::fs::read_file(path)
}

/// Owned copy of the current writer's raw published prefix, including its unchanged header.
/// Test-only, on the writer's owning thread: never publishes, seals, renumbers or modifies it.
/// An offline writer keeps this prefix in memory until sealing; None means no segment yet.
#[cfg(feature = "writer")]
pub fn published_bytes(writer: &crate::JournalWriter) -> Option<Vec<u8>> {
    let map = writer.cur?;
    let len = usize::try_from(map.load64(crate::layout::OFF_COMMITTED)).ok()?;
    if len > map.len() {
        return None;
    }
    Some(map.bytes(0, len).to_vec())
}

/// Overwrites bytes of a segment file in place (heap backend: through the shared memory).
pub fn poke(path: &Path, off: usize, bytes: &[u8]) -> io::Result<()> {
    if crate::fs::is_mem(path) {
        let m = crate::layout::mem_open(path, false)?;
        if off + bytes.len() > m.len() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        m.write(off, bytes);
        return Ok(());
    }
    let f = std::fs::OpenOptions::new().write(true).open(path)?;
    crate::fs::pwrite(&f, off as u64, bytes)
}

/// Shrinks a segment file.
pub fn truncate(path: &Path, len: usize) -> io::Result<()> {
    if crate::fs::is_mem(path) {
        return crate::layout::mem_truncate(path, len);
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .set_len(len as u64)
}

/// Deletes a segment file.
pub fn unlink(path: &Path) -> io::Result<()> {
    if crate::fs::is_mem(path) {
        return crate::layout::mem_unlink(path);
    }
    std::fs::remove_file(path)
}

/// Whether a segment file exists.
pub fn exists(path: &Path) -> bool {
    crate::fs::file_len(path).is_some()
}

/// Segment files `<10 digits>.jrn` in a hot journal directory, ascending.
pub fn hot_files(dir: &Path) -> Vec<PathBuf> {
    crate::fs::hot_list(dir).into_values().collect()
}

/// Cold copy stem (no extension) of a segment header: `<cold_jdir>/<yyyymmdd>/<no>-<first_seq>`.
pub fn cold_stem(cold_jdir: &Path, created_utc_ns: i64, no: u64, first_seq: u64) -> PathBuf {
    crate::fs::cold_stem(cold_jdir, created_utc_ns, no, first_seq)
}

/// Writes a new file into the heap backend (or disk), e.g. a foreign file in a hot directory.
pub fn create(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if crate::fs::is_mem(path) {
        let m = crate::layout::mem_create(path, bytes.len().max(1))?;
        m.write(0, bytes);
        crate::layout::release(m);
        return Ok(());
    }
    std::fs::write(path, bytes)
}
