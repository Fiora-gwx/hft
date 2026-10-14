//! `cargo xtask gen-host [--host <name>] [--check]` (tools/README.md "cargo xtask gen-host"): input
//! delegates to the deterministic deployment generator. Unsupported host classes fail closed.

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
        if h.is_empty()
            || !h
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            eprintln!("gen-host: error: invalid host name");
            return EXIT_ENV;
        }
        let rel = format!("deploy/hosts/{h}.toml");
        if !root.join(&rel).is_file() {
            eprintln!("gen-host: error: host inventory {rel} does not exist");
            return EXIT_ENV;
        }
    }
    let script = root.join("deploy/gen/generate.py");
    let Some(python) = crate::doc_check::find_python().filter(|_| script.is_file()) else {
        eprintln!("gen-host: error: deployment generator or Python 3 is missing");
        return EXIT_ENV;
    };
    match std::process::Command::new(python)
        .arg(script)
        .args(args)
        .current_dir(root)
        .status()
    {
        Ok(status) => status.code().unwrap_or(crate::EXIT_FAIL),
        Err(error) => {
            eprintln!("gen-host: error: {error}");
            EXIT_ENV
        }
    }
}
