//! `cargo xtask replay-verify <journal-dir | fixtures/replay/<name>.pointer.toml> …` (tools/README.md
//! "cargo xtask replay-verify"): input checks only until `apps/replayer` is in the workspace. A pointer is
//! resolved to its cached copy `target/fixtures-cache/<name>` (written by `fixtures fetch`), whose SHA-256
//! must equal the pointer's `sha256` (missing or different → exit 2). Then, without the replayer, it
//! exits 2 (tools/README.md 已知限制).

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::{EXIT_ENV, EnvError};

fn check_pointer(root: &Path, rel: &str) -> Result<(), EnvError> {
    let text = std::fs::read_to_string(root.join(rel)).map_err(|e| EnvError(format!("read {rel}: {e}")))?;
    let t: toml::Table = text.parse().map_err(|e| EnvError(format!("parse {rel}: {e}")))?;
    let want = t
        .get("sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| EnvError(format!("{rel}: missing sha256")))?
        .to_ascii_lowercase();
    let name = Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(".pointer.toml"))
        .ok_or_else(|| EnvError(format!("{rel}: not a <name>.pointer.toml")))?;
    let cache = format!("target/fixtures-cache/{name}");
    let b = std::fs::read(root.join(&cache))
        .map_err(|e| EnvError(format!("{cache}: {e} (run `cargo xtask fixtures fetch` first)")))?;
    let got: String = Sha256::digest(&b).iter().map(|x| format!("{x:02x}")).collect();
    if got != want {
        return Err(EnvError(format!("{cache}: SHA-256 {got} != pointer {want}")));
    }
    Ok(())
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    let Some(input) = args.first() else {
        eprintln!("usage: cargo xtask replay-verify <journal-dir | fixtures/replay/<name>.pointer.toml> …");
        return EXIT_ENV;
    };
    let res = if input.ends_with(".pointer.toml") {
        check_pointer(root, input)
    } else if Path::new(input).is_dir() || root.join(input).is_dir() {
        Ok(())
    } else {
        Err(EnvError(format!(
            "{input} is neither a journal directory nor a pointer file"
        )))
    };
    if let Err(e) = res {
        eprintln!("replay-verify: error: {e}");
        return EXIT_ENV;
    }
    eprintln!("replay-verify: error: apps/replayer is not in the workspace yet (tools/README.md 已知限制)");
    EXIT_ENV
}
