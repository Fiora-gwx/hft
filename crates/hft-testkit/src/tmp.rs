//! Test-owned temporary and evidence directories (README "11. 临时目录"). A [`TmpDir`] removes its
//! directory, with everything below it, when it drops: at the end of the test that owns it, also
//! when that test panics. An evidence directory ([`TmpDir::evidence`]) is kept when its owner drops
//! it while panicking, so a failing test leaves its files for the CI artifact upload. Setting
//! `HFT_TEST_KEEP_TMP=1` keeps every directory (debugging; CI jobs that upload evidence from
//! successful runs set it for that step).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Environment variable: `1` keeps every test directory.
pub const KEEP_TMP: &str = "HFT_TEST_KEEP_TMP";

/// A directory removed on drop (see the module documentation).
#[derive(Debug)]
pub struct TmpDir {
    path: PathBuf,
    keep_on_panic: bool,
    disarmed: bool,
}

static NEXT: AtomicU64 = AtomicU64::new(0);

impl TmpDir {
    /// Creates `path` (and its parents) and owns it; removed on every drop.
    pub fn new(path: impl Into<PathBuf>) -> std::io::Result<TmpDir> {
        let path = path.into();
        std::fs::create_dir_all(&path)?;
        Ok(TmpDir {
            path,
            keep_on_panic: false,
            disarmed: false,
        })
    }
    /// A fresh directory `<system temp>/<prefix>-<pid>-<n>` (`n` counts per process).
    pub fn in_temp(prefix: &str) -> std::io::Result<TmpDir> {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        TmpDir::new(std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id())))
    }
    /// Creates and owns an evidence directory: removed when its owner drops it normally, kept when
    /// the owner drops it during a panic.
    pub fn evidence(path: impl Into<PathBuf>) -> std::io::Result<TmpDir> {
        let mut d = TmpDir::new(path)?;
        d.keep_on_panic = true;
        Ok(d)
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Gives up ownership: the directory stays (a failure the caller caught and reports).
    pub fn keep(mut self) -> PathBuf {
        self.disarmed = true;
        self.path.clone()
    }
    /// Whether the drop keeps the directory now (environment or panicking evidence).
    pub fn kept(&self) -> bool {
        self.disarmed || keep_all() || self.keep_on_panic && std::thread::panicking()
    }
}

/// `HFT_TEST_KEEP_TMP=1`.
pub fn keep_all() -> bool {
    std::env::var(KEEP_TMP).is_ok_and(|v| v == "1")
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        if self.kept() {
            if !self.disarmed {
                eprintln!("kept {}", self.path.display());
            }
            return;
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A per-process file name `<stem>-<pid>.<ext>` under `dir`: concurrent test processes of one
/// checkout write their evidence files side by side instead of over each other.
pub fn per_process_file(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    dir.join(format!("{stem}-{}.{ext}", std::process::id()))
}
