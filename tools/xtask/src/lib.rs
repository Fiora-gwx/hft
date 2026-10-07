//! `cargo xtask`: repository gates and generators. Spec: `tools/README.md`.
//!
//! Exit-code convention for every subcommand (tools/README.md "使用本组件"): 0 pass, 1 check failed,
//! 2 environment or input error. Human summary goes to stderr; machine-readable JSON goes to
//! `<root>/target/xtask/<subcommand>.json`.

use std::fmt;
use std::path::{Path, PathBuf};

pub mod bench_gate;
pub mod ci;
pub mod code_scan;
pub mod decoupling_gate;
pub mod deps;
pub mod doc_check;
pub mod exec;
pub mod fixtures;
pub mod gen_host;
pub mod gen_schema;
pub mod hot_check;
pub mod journal_verify;
pub mod replay_verify;
pub mod setup;
pub mod spec;
pub mod test_trace;
pub mod workspace;

/// Check passed.
pub const EXIT_OK: i32 = 0;
/// Check failed (violation, inconsistency).
pub const EXIT_FAIL: i32 = 1;
/// Environment or input error (missing file, bad config, missing tool).
pub const EXIT_ENV: i32 = 2;

/// An environment or input error; maps to exit code 2.
#[derive(Debug)]
pub struct EnvError(pub String);

impl fmt::Display for EnvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EnvError {}

/// Path of `p` relative to `root`, with `/` separators.
pub fn rel_path(root: &Path, p: &Path) -> String {
    let r = p.strip_prefix(root).unwrap_or(p);
    r.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Write `value` as pretty JSON to `<root>/target/xtask/<name>.json`.
pub fn write_json(root: &Path, name: &str, value: &serde_json::Value) -> Result<PathBuf, EnvError> {
    let dir = root.join("target").join("xtask");
    std::fs::create_dir_all(&dir).map_err(|e| EnvError(format!("create {}: {e}", dir.display())))?;
    let path = dir.join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(value).map_err(|e| EnvError(format!("encode JSON: {e}")))?;
    std::fs::write(&path, text + "\n").map_err(|e| EnvError(format!("write {}: {e}", path.display())))?;
    Ok(path)
}
