//! Shared test helpers: run the CLI in-process, scratch directories, fixture paths.
#![allow(dead_code)]

pub mod schemas;
pub mod strat;

use std::path::{Path, PathBuf};

/// Exit code, stdout and stderr of one `hft-cfg` invocation.
pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(args: &[&str]) -> Out {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut o, mut e) = (Vec::new(), Vec::new());
    let code = hft_cfg::run(&args, &mut o, &mut e);
    Out {
        code,
        stdout: String::from_utf8_lossy(&o).into_owned(),
        stderr: String::from_utf8_lossy(&e).into_owned(),
    }
}

/// A fresh, empty scratch directory under the integration-test tmp dir.
pub fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hft-cfg").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn p(x: &Path) -> &str {
    x.to_str().unwrap()
}
