//! `cargo xtask dst` (tools/README.md "cargo xtask dst"): plans the seeds and their suites and
//! drives the `hft-dst` runner of hft-testkit (the same binary the verification bundle carries).
//!
//! - Start seed: `--seed S`, else the first 16 hex digits of the HEAD commit SHA.
//! - Suite of a seed: `--suite X`, else `hft_testkit::seed::suite_index` over the suites `hft-dst
//!   list` prints (lines `suite <name>`), sorted by name.
//! - `--regressions`: every seed of `fixtures/dst-seeds/<suite>.txt` (first token of each line;
//!   `#` lines skipped) in that suite.
//! - Runs: `hft-dst run --seeds A..B` for the range (`hft-dst repro` per seed with `--trace`), `hft-dst
//!   repro` per regression seed; crash directories under `target/dst/`. Nothing is written outside
//!   `target/` (no server ledger).
//! - `--hashtrail check|write`: `hft-dst hashtrail` per registered seed of
//!   `fixtures/dst/hashtrail.toml`, compared with / written into its `trail` values.
//! - JSON `target/xtask/dst.json`: `seeds` (16-digit hex, range then regressions), `runs`
//!   (`seed`, `suite`, `source`), `steps`, `strategy_set`, `result`.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

use crate::exec::{Cmd, Exec};
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};

/// Where the DST framework lives once it exists (crates/hft-testkit README §6).
pub const DST_DIR: &str = "crates/hft-testkit/src/dst";
pub const REGRESSIONS: &str = "fixtures/dst-seeds";
pub const HASHTRAIL: &str = "fixtures/dst/hashtrail.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trail {
    Check,
    Write,
}

/// Parsed arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opts {
    pub seeds: Option<u64>,
    pub steps: Option<u64>,
    pub seed: Option<u64>,
    pub suite: Option<String>,
    pub regressions: bool,
    pub trace: bool,
    pub strategy: Option<String>,
    pub hashtrail: Option<Trail>,
    pub first: Option<u64>,
}

/// `123` or `0x7b`.
pub fn parse_u64(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// `0x` + 16 lowercase hex digits.
pub fn hex(seed: u64) -> String {
    format!("{seed:#018x}")
}

/// Argument parsing and every combination rule of the README table (errors → exit 2).
pub fn parse(args: &[String]) -> Result<Opts, EnvError> {
    let mut o = Opts::default();
    let mut it = args.iter();
    let val = |it: &mut std::slice::Iter<String>, k: &str| -> Result<String, EnvError> {
        it.next()
            .cloned()
            .ok_or_else(|| EnvError(format!("{k} needs a value")))
    };
    let num = |s: String, k: &str| parse_u64(&s).ok_or_else(|| EnvError(format!("{k} {s}: not a u64")));
    while let Some(a) = it.next() {
        match a.as_str() {
            "--seeds" => o.seeds = Some(num(val(&mut it, a)?, a)?),
            "--steps" => o.steps = Some(num(val(&mut it, a)?, a)?),
            "--seed" => o.seed = Some(num(val(&mut it, a)?, a)?),
            "--first" => o.first = Some(num(val(&mut it, a)?, a)?),
            "--suite" => o.suite = Some(val(&mut it, a)?),
            "--strategy" => o.strategy = Some(val(&mut it, a)?),
            "--regressions" => o.regressions = true,
            "--trace" => o.trace = true,
            "--hashtrail" => {
                o.hashtrail = Some(match val(&mut it, a)?.as_str() {
                    "check" => Trail::Check,
                    "write" => Trail::Write,
                    x => return Err(EnvError(format!("--hashtrail {x}: expected check or write"))),
                })
            }
            x => return Err(EnvError(format!("unknown argument {x}"))),
        }
    }
    match o.hashtrail {
        Some(t) => {
            if o.seeds.is_some() || o.seed.is_some() || o.suite.is_some() || o.regressions {
                return Err(EnvError(
                    "--hashtrail runs the registered seeds: it cannot be combined with --seeds/--seed/--suite/--regressions".into(),
                ));
            }
            match (t, o.first) {
                (Trail::Write, Some(_)) => {
                    return Err(EnvError("--first goes only with --hashtrail check".into()));
                }
                (_, Some(0)) => return Err(EnvError("--first must be at least 1".into())),
                _ => {}
            }
        }
        None => {
            if o.first.is_some() {
                return Err(EnvError("--first goes only with --hashtrail check".into()));
            }
            match o.seeds {
                None => return Err(EnvError("--seeds N is required".into())),
                Some(0) => return Err(EnvError("--seeds must be at least 1".into())),
                _ => {}
            }
        }
    }
    Ok(o)
}

/// Default start seed: the first 16 hex digits of the HEAD commit.
pub fn seed_from_head(root: &Path) -> Result<u64, EnvError> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| EnvError(format!("run git: {e}")))?;
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || sha.len() < 16 {
        return Err(EnvError(format!(
            "no HEAD commit to derive the start seed from (give --seed): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    u64::from_str_radix(&sha[..16], 16).map_err(|e| EnvError(format!("HEAD {sha}: {e}")))
}

/// `(seed, suite)` of every regression file, files in name order.
pub fn regressions(root: &Path) -> Result<Vec<(u64, String)>, EnvError> {
    let dir = root.join(REGRESSIONS);
    let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "txt"))
            .collect(),
        Err(_) => return Ok(Vec::new()),
    };
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let suite = f
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = std::fs::read_to_string(&f).map_err(|e| EnvError(format!("read {}: {e}", f.display())))?;
        for (i, l) in text.lines().enumerate() {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let tok = l
                .split(|c: char| c.is_whitespace() || c == ',')
                .next()
                .unwrap_or("");
            let s = parse_u64(tok)
                .ok_or_else(|| EnvError(format!("{}:{}: `{tok}` is not a seed", f.display(), i + 1)))?;
            out.push((s, suite.clone()));
        }
    }
    Ok(out)
}

fn target_dir(root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(t) => {
            let t = PathBuf::from(t);
            if t.is_absolute() { t } else { root.join(t) }
        }
        None => root.join("target"),
    }
}

/// The `hft-dst` binary of the `ci` profile.
pub fn runner_bin(root: &Path) -> PathBuf {
    let name = if cfg!(windows) { "hft-dst.exe" } else { "hft-dst" };
    target_dir(root).join("ci").join(name)
}

/// Suites printed by `hft-dst list` (lines `suite <name> …`), sorted by name.
pub fn parse_suites(list: &str) -> Vec<String> {
    let mut v: Vec<String> = list
        .lines()
        .filter_map(|l| l.strip_prefix("suite "))
        .filter_map(|r| r.split_whitespace().next())
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

fn fail(msg: impl std::fmt::Display) -> i32 {
    eprintln!("dst: error: {msg}");
    EXIT_ENV
}

/// One registered conformance seed of `fixtures/dst/hashtrail.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailSeed {
    pub seed: u64,
    pub suite: String,
    pub steps: u64,
    pub trail: String,
}

/// The `[[seed]]` tables of the HashTrail golden file, in registration (file) order.
pub fn read_trails(text: &str) -> Result<Vec<TrailSeed>, EnvError> {
    let t: toml::Table = text.parse().map_err(|e| EnvError(format!("{HASHTRAIL}: {e}")))?;
    let arr = t
        .get("seed")
        .and_then(|v| v.as_array())
        .ok_or_else(|| EnvError(format!("{HASHTRAIL}: no [[seed]] tables")))?;
    arr.iter()
        .enumerate()
        .map(|(i, v)| {
            let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
            let bad = || {
                EnvError(format!(
                    "{HASHTRAIL}: [[seed]] #{}: needs seed, suite, steps, trail",
                    i + 1
                ))
            };
            Ok(TrailSeed {
                seed: s("seed").as_deref().and_then(parse_u64).ok_or_else(bad)?,
                suite: s("suite").ok_or_else(bad)?,
                steps: v
                    .get("steps")
                    .and_then(|x| x.as_integer())
                    .and_then(|x| u64::try_from(x).ok())
                    .ok_or_else(bad)?,
                trail: s("trail").ok_or_else(bad)?,
            })
        })
        .collect()
}

/// `text` with the `trail` value of the i-th `[[seed]]` table replaced by `trails[i]` (all else
/// byte-identical).
pub fn rewrite_trails(text: &str, trails: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut ix: Option<usize> = None;
    for line in text.split_inclusive('\n') {
        let t = line.trim_start();
        if t.starts_with("[[seed]]") {
            ix = Some(ix.map_or(0, |i| i + 1));
        } else if t.starts_with('[') {
            ix = None;
        }
        if let Some(i) = ix
            && t.starts_with("trail")
            && t[5..].trim_start().starts_with('=')
            && let Some(new) = trails.get(i)
        {
            let indent = &line[..line.len() - t.len()];
            let nl = if line.ends_with('\n') { "\n" } else { "" };
            out.push_str(&format!("{indent}trail = \"{new}\"{nl}"));
            continue;
        }
        out.push_str(line);
    }
    out
}

/// `trail <hex> points=<n>` → `<hex>`.
pub fn parse_trail_line(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("trail "))
        .and_then(|r| r.split_whitespace().next())
        .map(str::to_string)
}

fn build_runner(root: &Path, exec: &mut dyn Exec) -> Result<String, i32> {
    let build = Cmd::new(
        "cargo",
        &[
            "build",
            "--profile",
            "ci",
            "-p",
            "hft-testkit",
            "--features",
            "dst-bin",
            "--bin",
            "hft-dst",
        ],
    );
    match exec.run(&build) {
        Ok(r) if r.ok() => Ok(runner_bin(root).display().to_string()),
        Ok(r) => Err(fail(format!("`{}` exited {}", build.line(), r.code))),
        Err(e) => Err(fail(e)),
    }
}

/// `--hashtrail check|write [--first K]`: `hft-dst hashtrail --seed S --suite X --steps N` per
/// registered seed (stdout `trail <xxh3_128 hex> points=<n>`); `check` stops at the first digest
/// that differs from the file (exit 1), `write` rewrites the `trail` values (seed set unchanged).
fn hashtrail(root: &Path, o: &Opts, exec: &mut dyn Exec) -> i32 {
    let path = root.join(HASHTRAIL);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return fail(format!("{HASHTRAIL}: {e}")),
    };
    let seeds = match read_trails(&text) {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    let n = match o.first {
        Some(k) if k as usize > seeds.len() => {
            return fail(format!(
                "--first {k}: {HASHTRAIL} registers {} seeds",
                seeds.len()
            ));
        }
        Some(k) => k as usize,
        None => seeds.len(),
    };
    let bin = match build_runner(root, exec) {
        Ok(b) => b,
        Err(c) => return c,
    };
    let write = o.hashtrail == Some(Trail::Write);
    let mut got = Vec::with_capacity(n);
    let mut code = EXIT_OK;
    let mut first_diff = serde_json::Value::Null;
    for t in &seeds[..n] {
        let c = Cmd::capture(
            &bin,
            &[
                "hashtrail",
                "--seed",
                &hex(t.seed),
                "--suite",
                &t.suite,
                "--steps",
                &t.steps.to_string(),
            ],
        );
        let r = match exec.run(&c) {
            Ok(r) => r,
            Err(e) => return fail(e),
        };
        eprint!("{}", r.stderr);
        if !r.ok() {
            eprintln!("dst: `{}` exited {}", c.line(), r.code);
            code = if r.code == EXIT_ENV { EXIT_ENV } else { EXIT_FAIL };
            break;
        }
        let Some(trail) = parse_trail_line(&r.stdout) else {
            return fail(format!("`{}` printed no `trail <hex>` line", c.line()));
        };
        if !write && trail != t.trail {
            eprintln!(
                "dst: HashTrail differs: seed={} suite={} steps={} expected={} got={}",
                hex(t.seed),
                t.suite,
                t.steps,
                t.trail,
                trail
            );
            first_diff = json!({"seed": hex(t.seed), "suite": t.suite, "expected": t.trail, "got": trail});
            code = EXIT_FAIL;
            break;
        }
        got.push(trail);
    }
    if write
        && code == EXIT_OK
        && let Err(e) = std::fs::write(&path, rewrite_trails(&text, &got))
    {
        return fail(format!("write {HASHTRAIL}: {e}"));
    }
    let j = json!({
        "hashtrail": if write { "write" } else { "check" },
        "seeds": seeds[..n].iter().map(|t| hex(t.seed)).collect::<Vec<_>>(),
        "first_divergence": first_diff,
        "result": match code { EXIT_OK => "pass", EXIT_FAIL => "fail", _ => "error" },
    });
    if let Err(e) = write_json(root, "dst", &j) {
        return fail(e);
    }
    eprintln!(
        "dst: --hashtrail {}: {n} seeds -> {}",
        if write { "write" } else { "check" },
        if code == EXIT_OK { "PASS" } else { "FAIL" }
    );
    code
}

/// Runs `dst` with `exec`; returns the exit code.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec) -> i32 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            return fail(format!(
                "{e}\nusage: cargo xtask dst --seeds N [--steps M] [--seed S] [--suite X] [--regressions] [--trace] [--strategy K]\n       cargo xtask dst --hashtrail check|write [--first K] [--strategy K]"
            ));
        }
    };
    if !root.join(DST_DIR).is_dir() {
        return fail(format!(
            "the DST framework ({DST_DIR}/) is not in the workspace yet"
        ));
    }
    if let Some(k) = &o.strategy {
        return fail(format!(
            "--strategy {k}: strategies/registry/tests/{}_dst.rs is not in the repository",
            k.replace('-', "_")
        ));
    }
    if o.hashtrail.is_some() {
        if !root.join(HASHTRAIL).is_file() {
            return fail(format!("{HASHTRAIL} not found"));
        }
        return hashtrail(root, &o, exec);
    }
    let steps = o.steps.unwrap_or(hft_testkit::seed::DST_DEFAULT_STEPS);
    let start = match o.seed {
        Some(s) => s,
        None => match seed_from_head(root) {
            Ok(s) => s,
            Err(e) => return fail(e),
        },
    };
    let n = o.seeds.unwrap_or(1);
    let Some(end) = start.checked_add(n - 1) else {
        return fail(format!("seed range {}..+{n} overflows u64", hex(start)));
    };
    // Build the runner and read its suites.
    let bin = match build_runner(root, exec) {
        Ok(b) => b,
        Err(c) => return c,
    };
    let suites = match exec.run(&Cmd::capture(&bin, &["list"])) {
        Ok(r) if r.ok() => parse_suites(&r.stdout),
        Ok(r) => return fail(format!("`{bin} list` exited {}", r.code)),
        Err(e) => return fail(e),
    };
    if suites.is_empty() {
        return fail(format!("`{bin} list` printed no suite"));
    }
    if let Some(x) = &o.suite
        && !suites.contains(x)
    {
        return fail(format!(
            "--suite {x}: no such suite (have: {})",
            suites.join(", ")
        ));
    }
    let regs = if o.regressions {
        match regressions(root) {
            Ok(r) => r,
            Err(e) => return fail(e),
        }
    } else {
        Vec::new()
    };
    if let Some((s, x)) = regs.iter().find(|(_, x)| !suites.contains(x)) {
        return fail(format!(
            "{REGRESSIONS}/{x}.txt: seed {} names a suite that hft-dst does not have",
            hex(*s)
        ));
    }
    // Plan.
    let suite_of = |s: u64| -> String {
        match &o.suite {
            Some(x) => x.clone(),
            None => suites[hft_testkit::seed::suite_index(s, suites.len())].clone(),
        }
    };
    let mut runs: Vec<(u64, String, &str)> = (0..n)
        .map(|i| start + i)
        .map(|s| (s, suite_of(s), "range"))
        .collect();
    runs.extend(regs.iter().map(|(s, x)| (*s, x.clone(), "regression")));
    // Execute.
    let crash = root.join("target").join("dst");
    let crash_s = crash.display().to_string();
    let steps_s = steps.to_string();
    let mut cmds: Vec<Cmd> = Vec::new();
    let repro = |s: u64, x: &str, trace: bool| {
        let mut c = Cmd::new(
            &bin,
            &["repro", "--seed", &hex(s), "--suite", x, "--steps", &steps_s],
        );
        if trace {
            c = c.arg("--trace");
        }
        c.arg("--crash-dir").arg(crash_s.clone())
    };
    if o.trace {
        for (s, x, src) in &runs {
            if *src == "range" {
                cmds.push(repro(*s, x, true));
            }
        }
    } else {
        let mut c = Cmd::new(&bin, &["run", "--seeds", &format!("{start}..{end}")]);
        if let Some(x) = &o.suite {
            c = c.arg("--suite").arg(x.clone());
        }
        cmds.push(
            c.arg("--steps")
                .arg(steps_s.clone())
                .arg("--out")
                .arg(crash.join("run.json").display().to_string())
                .arg("--crash-dir")
                .arg(crash_s.clone()),
        );
    }
    for (s, x) in &regs {
        cmds.push(repro(*s, x, o.trace));
    }
    if let Err(e) = std::fs::create_dir_all(&crash) {
        return fail(format!("create {crash_s}: {e}"));
    }
    let mut code = EXIT_OK;
    for c in &cmds {
        match exec.run(c) {
            Ok(r) if r.ok() => {}
            Ok(r) => {
                eprintln!("dst: `{}` exited {}", c.line(), r.code);
                code = code.max(if r.code == EXIT_ENV { EXIT_ENV } else { EXIT_FAIL });
            }
            Err(e) => return fail(e),
        }
    }
    let j = json!({
        "seeds": runs.iter().map(|(s, _, _)| hex(*s)).collect::<Vec<_>>(),
        "runs": runs.iter().map(|(s, x, src)| json!({"seed": hex(*s), "suite": x, "source": src})).collect::<Vec<_>>(),
        "steps": steps,
        "suite": o.suite,
        "trace": o.trace,
        "strategy_set": "TestStrategies",
        "result": match code { EXIT_OK => "pass", EXIT_FAIL => "fail", _ => "error" },
    });
    if let Err(e) = write_json(root, "dst", &j) {
        return fail(e);
    }
    eprintln!(
        "dst: {} seeds ({} regression), steps {steps} -> {}",
        runs.len(),
        regs.len(),
        if code == EXIT_OK { "PASS" } else { "FAIL" }
    );
    code
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    match crate::exec::from_env(root) {
        Ok(mut e) => run_with(root, args, e.as_mut()),
        Err(e) => fail(e),
    }
}
