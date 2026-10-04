//! `cargo xtask gen-host [--host <name>] [--check]` (tools/README.md "cargo xtask gen-host"): input
//! checks only until the generators (`deploy/gen/`) and the hft-core host-class tables exist; then it
//! exits 2 naming what is missing (tools/README.md 已知限制).

use std::path::Path;

use crate::{EXIT_ENV, EnvError};

fn parse(args: &[String]) -> Result<(Option<String>, bool), EnvError> {
    let (mut host, mut check) = (None, false);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--check" => check = true,
            "--host" => {
                host = Some(
                    it.next()
                        .cloned()
                        .ok_or_else(|| EnvError("--host needs a value".into()))?,
                )
            }
            x => return Err(EnvError(format!("unknown argument {x}"))),
        }
    }
    Ok((host, check))
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    let (host, _check) = match parse(args) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("gen-host: error: {e}");
            return EXIT_ENV;
        }
    };
    if let Some(h) = &host {
        let rel = format!("deploy/hosts/{h}.toml");
        if !root.join(&rel).is_file() {
            eprintln!("gen-host: error: host inventory {rel} does not exist");
            return EXIT_ENV;
        }
    }
    let missing: Vec<&str> = ["deploy/gen", "apps/hft-core/Cargo.toml"]
        .into_iter()
        .filter(|p| !root.join(p).exists())
        .collect();
    eprintln!(
        "gen-host: error: generator inputs not in the repository yet: {} (tools/README.md 已知限制)",
        if missing.is_empty() {
            "generator entry".to_string()
        } else {
            missing.join(", ")
        }
    );
    EXIT_ENV
}
