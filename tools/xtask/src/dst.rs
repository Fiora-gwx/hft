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

/// Parsed arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Opts {
    pub seeds: Option<u64>,
    pub steps: Option<u64>,
    pub seed: Option<u64>,
    pub suite: Option<String>,
    pub case: Option<String>,
    pub regressions: bool,
    pub trace: bool,
    pub strategy: Option<String>,
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
            "--suite" => o.suite = Some(val(&mut it, a)?),
            "--case" => o.case = Some(val(&mut it, a)?),
            "--strategy" => o.strategy = Some(val(&mut it, a)?),
            "--regressions" => o.regressions = true,
            "--trace" => o.trace = true,
            x => return Err(EnvError(format!("unknown argument {x}"))),
        }
    }
    match o.seeds {
        None => return Err(EnvError("--seeds N is required".into())),
        Some(0) => return Err(EnvError("--seeds must be at least 1".into())),
        _ => {}
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

/// Case identities and their required suite, from the same compiled runner as the suite table.
pub fn parse_cases(list: &str) -> Vec<(String, Option<String>)> {
    list.lines()
        .filter_map(|l| {
            let mut words = l.strip_prefix("case ")?.split_whitespace();
            let id = words.next()?;
            if !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
                return None;
            }
            let suite = words.find_map(|w| w.strip_prefix("suite="))?;
            Some((id.to_owned(), (suite != "all").then(|| suite.to_owned())))
        })
        .collect()
}

fn fail(msg: impl std::fmt::Display) -> i32 {
    eprintln!("dst: error: {msg}");
    EXIT_ENV
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

/// Runs `dst` with `exec`; returns the exit code.
pub fn run_with(root: &Path, args: &[String], exec: &mut dyn Exec) -> i32 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            return fail(format!(
                "{e}\nusage: cargo xtask dst --seeds N [--steps M] [--seed S] [--suite X] [--case ID] [--regressions] [--trace] [--strategy K]"
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
    let list = match exec.run(&Cmd::capture(&bin, &["list"])) {
        Ok(r) if r.ok() => r.stdout,
        Ok(r) => return fail(format!("`{bin} list` exited {}", r.code)),
        Err(e) => return fail(e),
    };
    let suites = parse_suites(&list);
    let cases = parse_cases(&list);
    let case_suite = if let Some(id) = &o.case {
        let Some((_, suite)) = cases.iter().find(|(name, _)| name == id) else {
            return fail(format!("unknown case {id}"));
        };
        if let (Some(required), Some(chosen)) = (suite, &o.suite)
            && required != chosen
        {
            return fail(format!("case {id} requires suite {required}, not {chosen}"));
        }
        suite.clone()
    } else {
        None
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
        match case_suite.as_ref().or(o.suite.as_ref()) {
            Some(x) => x.clone(),
            None => suites[hft_testkit::seed::suite_index(s, suites.len())].clone(),
        }
    };
    let mut runs: Vec<(u64, String, &str)> = (0..n)
        .map(|i| start + i)
        .map(|s| (s, suite_of(s), "range"))
        .collect();
    runs.extend(
        regs.iter()
            .map(|(s, x)| (*s, case_suite.as_ref().unwrap_or(x).clone(), "regression")),
    );
    // Execute.
    let mut crash = root.join("target").join("dst");
    if let Some(id) = &o.case {
        crash = crash.join(id);
    }
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
        if let Some(id) = &o.case {
            c = c.arg("--case").arg(id.clone());
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
        if let Some(id) = &o.case {
            c = c.arg("--case").arg(id.clone());
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
    for (index, (s, x)) in regs.iter().enumerate() {
        let mut c = repro(*s, case_suite.as_ref().unwrap_or(x), o.trace);
        if o.case.is_some() {
            c = c.arg("--out").arg(
                crash
                    .join(format!("regression-{index}-{}.json", hex(*s)))
                    .display()
                    .to_string(),
            );
        }
        cmds.push(c);
    }
    if let Err(e) = std::fs::create_dir_all(&crash) {
        return fail(format!("create {crash_s}: {e}"));
    }
    let mut code = EXIT_OK;
    for c in &cmds {
        match exec.run(c) {
            Ok(r) if r.ok() => {}
            Ok(r) => {
                // Exit 1 already carries the runner's single reproducible failure line.
                if r.code != EXIT_FAIL {
                    eprintln!("dst: `{}` exited {}", c.line(), r.code);
                }
                code = code.max(if r.code == EXIT_ENV { EXIT_ENV } else { EXIT_FAIL });
            }
            Err(e) => return fail(e),
        }
    }
    let j = json!({
        "seeds": runs.iter().map(|(s, _, _)| hex(*s)).collect::<Vec<_>>(),
        "runs": runs.iter().enumerate().map(|(i, (s, x, src))| json!({"seed": hex(*s), "suite": x, "source": src,
            "case": o.case, "regression_suite": i.checked_sub(n as usize).and_then(|i| regs.get(i)).map(|(_, suite)| suite)})).collect::<Vec<_>>(),
        "steps": steps,
        "suite": o.suite,
        "case": o.case,
        "trace": o.trace,
        "strategy_set": "TestStrategies",
        "result": match code { EXIT_OK => "pass", EXIT_FAIL => "fail", _ => "error" },
    });
    if let Err(e) = write_json(root, "dst", &j) {
        return fail(e);
    }
    if let Some(id) = &o.case
        && let Err(e) = write_json(root, &format!("dst-{id}"), &j)
    {
        return fail(e);
    }
    eprintln!(
        "dst: {} seeds ({} regression), steps {steps}{} -> {}",
        runs.len(),
        regs.len(),
        o.case.as_ref().map_or(String::new(), |id| format!(", case {id}")),
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
