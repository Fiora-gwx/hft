//! `cargo xtask replay-verify <journal-dir | fixtures/replay/<name>.pointer.toml> …` (tools/README.md
//! "cargo xtask replay-verify"). A pointer is
//! resolved to its cached copy `target/fixtures-cache/<name>` (written by `fixtures fetch`), whose SHA-256
//! must equal the pointer's `sha256` (missing or different → exit 2). Execution uses the same
//! locked CI profile as other verification tools; the application owns replay semantics.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::exec::{Cmd, Exec};
use crate::{EXIT_ENV, EnvError};

fn check_pointer(root: &Path, rel: &str) -> Result<PathBuf, EnvError> {
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
    Ok(root.join(cache))
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    let mut exec = match crate::exec::from_env(root) {
        Ok(exec) => exec,
        Err(e) => {
            eprintln!("replay-verify: {e}");
            return EXIT_ENV;
        }
    };
    run_with(root, args, exec.as_mut())
}

/// Same entry with an injected executor for process-contract tests.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec) -> i32 {
    let Some(input) = args.first() else {
        eprintln!("usage: cargo xtask replay-verify <journal-dir | fixtures/replay/<name>.pointer.toml> …");
        return EXIT_ENV;
    };
    let mut i = 1;
    while i < args.len() {
        if !matches!(args[i].as_str(), "--from-seq" | "--to-seq")
            || args
                .get(i + 1)
                .and_then(|s| s.parse::<u64>().ok())
                .is_none_or(|v| v == 0)
        {
            eprintln!("replay-verify: unsupported option or invalid value: {}", args[i]);
            return EXIT_ENV;
        }
        i += 2;
    }
    let res = if input.ends_with(".pointer.toml") {
        check_pointer(root, input)
    } else if root.join(input).is_dir() || root.join(input).is_file() {
        Ok(root.join(input))
    } else {
        Err(EnvError(format!(
            "{input} is neither a journal file/directory nor a pointer file"
        )))
    };
    let path = match res {
        Ok(path) => path,
        Err(e) => {
            eprintln!("replay-verify: error: {e}");
            return EXIT_ENV;
        }
    };
    let mut cmd = Cmd::new(
        "cargo",
        &[
            "run",
            "--locked",
            "--profile",
            "ci",
            "-p",
            "replayer",
            "--",
            "verify",
            "--journal",
        ],
    )
    .arg(path.display().to_string());
    cmd.args.extend_from_slice(&args[1..]);
    match exec.run(&cmd) {
        Ok(out) => match out.code {
            0 => 0,
            1 | 3 | 4 => 1,
            _ => EXIT_ENV,
        },
        Err(e) => {
            eprintln!("replay-verify: {e}");
            EXIT_ENV
        }
    }
}
