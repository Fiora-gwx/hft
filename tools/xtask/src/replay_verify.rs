//! `cargo xtask replay-verify <journal-dir | fixtures/replay/<name>.pointer.toml> …` (tools/README.md
//! "cargo xtask replay-verify"). Single objects and files-v1 sets share acquisition's manifest
//! parser and revalidate cache bytes before executing. Bundle checks are closed, immutable input
//! descriptions; they cannot select an arbitrary program or weaken the matrix through CLI overrides.
//! Execution uses the locked CI profile; the application owns replay semantics.

use std::path::Path;

use serde_json::{Value, json};

use crate::fixtures::{bundle, fetch, pointer};
use pointer::{Content, StrategySet};

use crate::exec::{Cmd, Exec};
use crate::{EXIT_ENV, EnvError};

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
        if args[i] == "--record-decide-hist" {
            i += 1;
            continue;
        }
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
        let pointer = match pointer::read(&root.join(input)) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("replay-verify: {e}");
                return EXIT_ENV;
            }
        };
        let cache = match bundle::cache_root(root) {
            Ok(p) => p.join(&pointer.name),
            Err(e) => {
                eprintln!("replay-verify: {e}");
                return EXIT_ENV;
            }
        };
        match pointer.content {
            Content::Files(b) => {
                if args.len() != 1 {
                    eprintln!("replay-verify: bundle checks cannot be overridden by CLI options");
                    return EXIT_ENV;
                }
                match bundle::verified(&cache, &b) {
                    Ok(true) => return run_bundle(&pointer.name, &cache, &b, exec),
                    Ok(false) => Err(EnvError("bundle cache missing or differs from manifest".into())),
                    Err(e) => Err(e),
                }
            }
            Content::Object(p) => match fetch::matches(&cache, &p) {
                Ok(true) => Ok(cache),
                Ok(false) => Err(EnvError("cached SHA-256 or bytes differ from pointer".into())),
                Err(e) => Err(e),
            },
        }
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

fn run_bundle(name: &str, cache: &Path, b: &pointer::Bundle, exec: &mut dyn Exec) -> i32 {
    let mut checks = Vec::new();
    let mut code = 0;
    for check in &b.checks {
        let mut cmd = Cmd::capture("cargo", &["run", "--locked", "--profile", "ci", "-p", "replayer"]);
        if b.strategy_set == StrategySet::TestkitReplayV1 {
            cmd = cmd.arg("--example").arg("replay_probe");
        }
        cmd = cmd
            .arg("--")
            .arg("verify")
            .arg("--journal")
            .arg(cache.join(&b.journal).display().to_string())
            .arg("--advice")
            .arg(&check.advice);
        if let Some(path) = &check.checkpoint {
            cmd = cmd
                .arg("--model-checkpoint")
                .arg(cache.join(path).display().to_string());
        }
        for (option, bound) in [("--from-seq", check.from_seq), ("--to-seq", check.to_seq)] {
            if let Some(n) = bound {
                cmd = cmd.arg(option).arg(n.to_string());
            }
        }
        let out = match exec.run(&cmd) {
            Ok(out) => out,
            Err(e) => {
                checks.push(json!({"name": check.name, "exit_code": null, "status": 2,
                    "report": null, "stderr": "", "error": e.to_string()}));
                code = EXIT_ENV;
                break;
            }
        };
        let report = serde_json::from_str::<Value>(&out.stdout).ok();
        let mut status = match out.code {
            0 => 0,
            1 | 3 | 4 => 1,
            _ => EXIT_ENV,
        };
        // A successful subprocess without an actual replay report is not fixture acceptance.
        let valid = report.as_ref().is_some_and(|r| {
            let s = &r["summary"];
            s["verify"] == "pass"
                && s["mismatches"] == 0
                && ["outputs_compared", "snapshots_checked"]
                    .iter()
                    .all(|k| s[*k].as_u64().is_some_and(|n| n > 0))
                && (check.advice != "recompute"
                    || (s["recompute"] == "pass"
                        && s["model_advice_compared"].as_u64().is_some_and(|n| n > 0)))
        });
        let error = if (out.code == 0 && !valid)
            || (out.code == 1 && !report.as_ref().is_some_and(Value::is_object))
        {
            status = EXIT_ENV;
            Some("missing or inconsistent replay report")
        } else {
            None
        };
        checks.push(
            json!({"name": check.name, "exit_code": out.code, "status": status,
            "report": report, "stderr": out.stderr, "error": error}),
        );
        code = code.max(status);
        if status == EXIT_ENV {
            break;
        }
    }
    println!(
        "{}",
        json!({"bundle": name, "exit_code": code, "checks_planned": b.checks.len(), "checks": checks})
    );
    code
}
