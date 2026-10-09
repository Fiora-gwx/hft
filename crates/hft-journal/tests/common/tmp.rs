//! Test-owned temporary directories (`/dev/shm/hftj-*`, `<temp>/hftj-*`): each one is removed when
//! the test that created it ends, also when it panics. The guard sits in a per-thread list and drops
//! with the test's thread (libtest runs every test on its own thread and joins it), so the directory
//! outlives every writer, reader, preparer and child process of the test, including state a
//! simulated crash leaves behind for a later `open` in the same test. `HFT_TEST_KEEP_TMP=1` keeps
//! the directories for debugging.
#![allow(dead_code)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// Environment variable: `1` keeps test temporary directories.
pub const KEEP_TMP: &str = "HFT_TEST_KEEP_TMP";

/// Removes its directory (with everything below it) on drop unless `HFT_TEST_KEEP_TMP=1`; errors
/// are ignored.
pub struct TmpDir(PathBuf);

impl TmpDir {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        if std::env::var(KEEP_TMP).is_ok_and(|v| v == "1") {
            eprintln!("{KEEP_TMP}=1: kept {}", self.0.display());
            return;
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

thread_local! {
    static OWNED: RefCell<Vec<TmpDir>> = const { RefCell::new(Vec::new()) };
}

/// Ties `dir` to the calling test: removed when the test's thread ends. Returns `dir`.
pub fn owned(dir: PathBuf) -> PathBuf {
    OWNED.with(|o| o.borrow_mut().push(TmpDir(dir.clone())));
    dir
}
