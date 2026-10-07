//! `hostprobe`: reads the host parameters actually in effect (kernel, CPU, memory, NICs, IRQs,
//! network stack, kernel threads, resctrl, clock, occupancy, resource guards, IMDS lockdown) and
//! writes the HostProfile JSON; `verify` compares it with an expected profile; `osnoise` wraps
//! `rtla osnoise`. Specification: tools/README.md "hostprobe".
//!
//! Exit codes: 0 ok (verify: every `required` item matches); 1 a `required` item differs or is
//! unreadable; 2 input or environment error (expected profile missing or invalid, `--out` not
//! writable, `rtla` missing). `--root <dir>` reads a fixture root instead of `/` ([`host`]).
#![deny(unsafe_code, clippy::undocumented_unsafe_blocks)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod collect;
pub mod cpus;
pub mod host;
pub mod json;
pub mod osnoise;
pub mod verify;

use std::path::Path;

use serde_json::Value;

use collect::Cfg;
use host::Host;

/// Exit code: success.
pub const EXIT_OK: i32 = 0;
/// Exit code: a required item differs or is unreadable.
pub const EXIT_FAIL: i32 = 1;
/// Exit code: input or environment error.
pub const EXIT_ENV: i32 = 2;

const USAGE: &str = "usage: hostprobe [--root <dir>] collect --out <file>\n       hostprobe [--root <dir>] verify --expected <profile.toml> --out <file>\n       hostprobe [--root <dir>] osnoise --cpus <list> --duration <d> [--out <file>]";

/// Joins `rel` to the directory of the host path `file` (`/a/b/x.toml` + `../g` → `/a/g`).
fn host_join(file: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = file.split('/').filter(|p| !p.is_empty()).collect();
    parts.pop();
    for p in rel.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x),
        }
    }
    format!("/{}", parts.join("/"))
}

/// Collection inputs of this host: the expected profile named by `/etc/hft/host-local.toml`
/// (`manifest`) under `/opt/hft/current-core/deploy/tuning/`, else the defaults.
pub fn host_cfg(h: &mut dyn Host) -> Cfg {
    let Ok(local) = h.read("/etc/hft/host-local.toml") else {
        return Cfg::default();
    };
    let Some(m) = local
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| t.get("manifest").and_then(|v| v.as_str()).map(str::to_string))
    else {
        return Cfg::default();
    };
    let path = format!("/opt/hft/current-core/deploy/tuning/{m}.toml");
    let Ok(text) = h.read(&path) else {
        return Cfg::default();
    };
    match verify::parse(&text, |rel| h.read(&host_join(&path, rel))) {
        Ok(e) => e.cfg,
        Err(_) => Cfg::default(),
    }
}

fn write_out(path: &str, v: &Value) -> Result<(), String> {
    std::fs::write(path, json::pretty(v)).map_err(|e| format!("write {path}: {e}"))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Runs one subcommand on `h`; returns the exit code. Human messages go to stderr.
pub fn run_with(h: &mut dyn Host, args: &[String]) -> i32 {
    let Some(sub) = args.first() else {
        eprintln!("{USAGE}");
        return EXIT_ENV;
    };
    let rest = &args[1..];
    match sub.as_str() {
        "collect" => {
            let Some(out) = flag(rest, "--out") else {
                eprintln!("hostprobe collect: --out is required\n{USAGE}");
                return EXIT_ENV;
            };
            let cfg = host_cfg(h);
            let p = collect::collect(h, &cfg);
            match write_out(out, &p) {
                Ok(()) => EXIT_OK,
                Err(e) => {
                    eprintln!("hostprobe collect: {e}");
                    EXIT_ENV
                }
            }
        }
        "verify" => {
            let (Some(exp), Some(out)) = (flag(rest, "--expected"), flag(rest, "--out")) else {
                eprintln!("hostprobe verify: --expected and --out are required\n{USAGE}");
                return EXIT_ENV;
            };
            let e = match verify::load(Path::new(exp)) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("hostprobe verify: expected profile: {e}");
                    return EXIT_ENV;
                }
            };
            let mut p = collect::collect(h, &e.cfg);
            let mism = verify::compare(&p, &e.items);
            let mut fail = false;
            let mut items = Vec::new();
            for m in &mism {
                let level = if m.required { "required" } else { "advisory" };
                fail |= m.required;
                eprintln!(
                    "{} expected={} actual={} {level}",
                    m.key,
                    verify::show(&m.expected),
                    verify::show(&m.actual)
                );
                items.push(serde_json::json!({"key": m.key, "expected": m.expected, "actual": m.actual, "level": level}));
            }
            p["verify"] = Value::Array(items);
            if let Err(e) = write_out(out, &p) {
                eprintln!("hostprobe verify: {e}");
                return EXIT_ENV;
            }
            eprintln!(
                "hostprobe verify: {} item(s) checked, {} required and {} advisory mismatch(es); pending_reboot = {}",
                e.items.len(),
                mism.iter().filter(|m| m.required).count(),
                mism.iter().filter(|m| !m.required).count(),
                p["pending_reboot"]
            );
            if fail { EXIT_FAIL } else { EXIT_OK }
        }
        "osnoise" => {
            let (Some(c), Some(d)) = (flag(rest, "--cpus"), flag(rest, "--duration")) else {
                eprintln!("hostprobe osnoise: --cpus and --duration are required\n{USAGE}");
                return EXIT_ENV;
            };
            match osnoise::run(h, c, d) {
                Ok(v) => {
                    print!("{}", json::pretty(&v));
                    if let Some(out) = flag(rest, "--out")
                        && let Err(e) = write_out(out, &v)
                    {
                        eprintln!("hostprobe osnoise: {e}");
                        return EXIT_ENV;
                    }
                    EXIT_OK
                }
                Err(e) => {
                    eprintln!("hostprobe osnoise: {e}");
                    EXIT_ENV
                }
            }
        }
        other => {
            eprintln!("hostprobe: unknown subcommand {other}\n{USAGE}");
            EXIT_ENV
        }
    }
}

/// Entry point: `--root <dir>` selects a fixture root, otherwise the real host (Linux only).
pub fn run(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("--root") {
        let Some(root) = args.get(1) else {
            eprintln!("{USAGE}");
            return EXIT_ENV;
        };
        let mut h = host::FixtureHost::new(Path::new(root));
        return run_with(&mut h, &args[2..]);
    }
    match real_host() {
        Some(mut h) => run_with(h.as_mut(), args),
        None => {
            eprintln!("hostprobe: reading a real host is only supported on Linux (use --root <fixture>)");
            EXIT_ENV
        }
    }
}

#[cfg(target_os = "linux")]
fn real_host() -> Option<Box<dyn Host>> {
    Some(Box::new(sys::SysHost::new()))
}

#[cfg(not(target_os = "linux"))]
fn real_host() -> Option<Box<dyn Host>> {
    None
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub mod sys;
