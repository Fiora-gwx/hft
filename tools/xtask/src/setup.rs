//! `cargo xtask setup` (tools/README.md "其他 xtask 子命令"): installs the pinned cargo plugins and the
//! fuzz nightly of `tools/xtask/toolchains.toml`, skipping what is already at the pinned version:
//!
//! ```toml
//! nightly = "nightly-2026-09-01"
//! [plugins]
//! cargo-deny = "0.20.2"
//! ```
//!
//! A plugin counts as installed when `cargo <name without cargo-> --version` prints the pinned version;
//! the nightly when `rustup toolchain list` lists it.

use std::path::Path;

use crate::exec::{Cmd, Exec};
use crate::{EXIT_ENV, EXIT_OK, EnvError};

pub const TOOLCHAINS: &str = "tools/xtask/toolchains.toml";

/// `(nightly, [(plugin, version)])`.
pub fn load(root: &Path) -> Result<(String, Vec<(String, String)>), EnvError> {
    let text = std::fs::read_to_string(root.join(TOOLCHAINS))
        .map_err(|e| EnvError(format!("read {TOOLCHAINS}: {e}")))?;
    let t: toml::Table = text
        .parse()
        .map_err(|e| EnvError(format!("parse {TOOLCHAINS}: {e}")))?;
    let nightly = t
        .get("nightly")
        .and_then(|v| v.as_str())
        .ok_or_else(|| EnvError(format!("{TOOLCHAINS}: missing `nightly`")))?
        .to_string();
    let plugins = t
        .get("plugins")
        .and_then(|v| v.as_table())
        .map(|p| {
            p.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    Ok((nightly, plugins))
}

fn has_version(out: &str, ver: &str) -> bool {
    out.split(|c: char| c.is_whitespace() || c == 'v')
        .any(|w| w == ver)
}

fn setup(root: &Path, exec: &mut dyn Exec) -> Result<(), EnvError> {
    let (nightly, plugins) = load(root)?;
    for (name, ver) in &plugins {
        let sub = name.strip_prefix("cargo-").unwrap_or(name);
        let o = exec.run(&Cmd::capture("cargo", &[sub, "--version"]))?;
        if o.ok() && has_version(&o.stdout, ver) {
            eprintln!("setup: {name} {ver} already installed");
            continue;
        }
        eprintln!("setup: installing {name} {ver}");
        let o = exec.run(&Cmd::new(
            "cargo",
            &["install", name, "--version", ver, "--locked"],
        ))?;
        if !o.ok() {
            return Err(EnvError(format!(
                "cargo install {name} {ver} failed (exit {})",
                o.code
            )));
        }
    }
    let o = exec.run(&Cmd::capture("rustup", &["toolchain", "list"]))?;
    if o.ok()
        && o.stdout
            .lines()
            .any(|l| l.trim_start().starts_with(nightly.as_str()))
    {
        eprintln!("setup: {nightly} already installed");
    } else {
        eprintln!("setup: installing {nightly}");
        let o = exec.run(&Cmd::new(
            "rustup",
            &["toolchain", "install", &nightly, "--profile", "minimal"],
        ))?;
        if !o.ok() {
            return Err(EnvError(format!(
                "rustup toolchain install {nightly} failed (exit {})",
                o.code
            )));
        }
    }
    Ok(())
}

/// Runs `setup` with `exec`.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec) -> i32 {
    if !args.is_empty() {
        eprintln!("setup: unexpected arguments {args:?}");
        return EXIT_ENV;
    }
    match setup(root, exec) {
        Ok(()) => {
            eprintln!("setup: -> PASS");
            EXIT_OK
        }
        Err(e) => {
            eprintln!("setup: error: {e}");
            EXIT_ENV
        }
    }
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    match crate::exec::from_env(root) {
        Ok(mut e) => run_with(root, args, e.as_mut()),
        Err(e) => {
            eprintln!("setup: error: {e}");
            EXIT_ENV
        }
    }
}
