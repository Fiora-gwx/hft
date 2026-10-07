//! `cargo xtask bench-gate [--crate <x>] [--path <p>] [--smoke | --reference] [--syscall-audit]
//! [--bench-dir <dir>] [--baseline-dir <dir>]` (tools/README.md "cargo xtask bench-gate";
//! thresholds, reference machine, regression rules and the output format: benchmarks/README.md).
//!
//! - `--smoke` (CI): builds every selected crate's benchmarks with the `ci` profile, runs them
//!   briefly and writes the numbers; fails only when a benchmark does not build or run.
//! - `--reference` (reference machine): `hostprobe verify --expected benchmarks/reference.toml`
//!   first (mismatch → exit 2), then runs the benchmarks (`--bench-dir`: the executables of the
//!   benchmark artifact; else `cargo bench --profile bench`) and judges them against
//!   `benches/budgets.toml` and the per-HostProfile baseline (`--baseline-dir`, default
//!   `target/bench-baseline/<host_profile_hash>/`).
//! - `--syscall-audit`: bpftrace counts syscalls per thread name while the run of
//!   `benchmarks/syscall-audit.toml` executes; compared with the hot-path-rules T7 allow table.
//!
//! Results: a benchmark reports `hft-bench: {json}` lines on stdout (HistBench), or, for Criterion
//! benchmarks, its `<target>/criterion/**/new/{benchmark,sample}.json` written during the run.
//! Test hooks: `XTASK_BENCH_PRIV=root|none` replaces the privilege check of `--syscall-audit`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::exec::{Cmd, Exec, from_env};
use crate::workspace::members;
use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, write_json};

/// Package of the path benchmarks (`benchmarks/`).
pub const PATH_PKG: &str = "hft-benchmarks";
/// Expected HostProfile of the reference machine.
pub const REFERENCE: &str = "benchmarks/reference.toml";
/// Syscall audit configuration.
pub const AUDIT_CFG: &str = "benchmarks/syscall-audit.toml";
/// Where `hostprobe verify` writes the reference machine's profile.
pub const HOSTPROFILE: &str = "target/bench/hostprofile.json";
/// Relative regression threshold (benchmarks/README.md 回归阈值): p99 > baseline × 1.10 fails.
const REL_NUM: f64 = 11.0;
const REL_DEN: f64 = 10.0;
/// Baseline window: the last N passing runs.
const BASELINE_RUNS: usize = 7;
/// Smoke-run Criterion arguments (short run; numbers are reported only).
const SMOKE_ARGS: &[&str] = &["--warm-up-time", "0.1", "--measurement-time", "0.2", "--noplot"];
/// Path-benchmark prefixes whose segment includes the kernel (benchmarks/README.md 回归阈值,
/// "含内核的分段"): judged on absolute thresholds only.
const KERNEL_PREFIXES: &[&str] = &[
    "kernel_rx",
    "gw-egress",
    "tick-to-trade",
    "journal-persist",
    "feed-ingest/poll-cadence",
    "gw-ingest/poll-cadence",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Smoke,
    Reference,
    AuditOnly,
}

#[derive(Debug)]
struct Opts {
    mode: Mode,
    audit: bool,
    krate: Option<String>,
    path: Option<String>,
    bench_dir: Option<PathBuf>,
    baseline_dir: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Opts, EnvError> {
    let (mut smoke, mut reference, mut audit) = (false, false, false);
    let (mut krate, mut path, mut bench_dir, mut baseline_dir) = (None, None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = |n: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| EnvError(format!("{n} needs a value")))
        };
        match a.as_str() {
            "--smoke" => smoke = true,
            "--reference" => reference = true,
            "--syscall-audit" => audit = true,
            "--crate" => krate = Some(val("--crate")?),
            "--path" => path = Some(val("--path")?),
            "--bench-dir" => bench_dir = Some(PathBuf::from(val("--bench-dir")?)),
            "--baseline-dir" => baseline_dir = Some(PathBuf::from(val("--baseline-dir")?)),
            x => return Err(EnvError(format!("unknown argument {x}"))),
        }
    }
    let mode = match (smoke, reference, audit) {
        (true, true, _) => return Err(EnvError("--smoke and --reference are exclusive".into())),
        (true, false, _) => Mode::Smoke,
        (false, true, _) => Mode::Reference,
        (false, false, true) => Mode::AuditOnly,
        (false, false, false) => {
            return Err(EnvError(
                "one of --smoke, --reference, --syscall-audit is required".into(),
            ));
        }
    };
    if audit && mode == Mode::Smoke {
        return Err(EnvError(
            "--syscall-audit runs only on the reference machine (not with --smoke)".into(),
        ));
    }
    Ok(Opts {
        mode,
        audit,
        krate,
        path,
        bench_dir,
        baseline_dir,
    })
}

/// Rows of a Markdown table under the heading `heading` (cells trimmed, header and rule skipped).
fn table_rows(text: &str, heading: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut inside = false;
    let mut seen_rule = false;
    for line in text.lines() {
        if line.starts_with('#') {
            inside = line.trim_start_matches('#').trim() == heading;
            seen_rule = false;
            continue;
        }
        if !inside {
            continue;
        }
        let l = line.trim();
        if !l.starts_with('|') {
            if seen_rule && !l.is_empty() {
                inside = false;
            }
            continue;
        }
        if l.contains("---") && !seen_rule {
            seen_rule = true;
            continue;
        }
        if !seen_rule {
            continue;
        }
        out.push(
            l.trim_matches('|')
                .split('|')
                .map(|c| c.trim().to_string())
                .collect(),
        );
    }
    out
}

/// Names in backticks of a cell.
fn ticks(cell: &str) -> Vec<String> {
    cell.split('`')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_string())
        .collect()
}

/// benchmarks/README.md 基准名登记: name → component.
pub fn registry(root: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(root.join("benchmarks/README.md")).unwrap_or_default();
    let mut out = BTreeMap::new();
    for r in table_rows(&text, "基准名登记") {
        if r.len() >= 2
            && let Some(n) = ticks(&r[0]).into_iter().next()
        {
            out.insert(n, r[1].clone());
        }
    }
    out
}

/// Strategy benchmark entries every registered strategy crate must provide
/// (crates/hft-strategy-sdk/README.md 延迟与资源契约, names `strategies/<kind>/…`).
fn strategy_entries(root: &Path) -> Result<Vec<String>, EnvError> {
    let p = root.join("crates/hft-strategy-sdk/README.md");
    let text = std::fs::read_to_string(&p).map_err(|e| EnvError(format!("read {}: {e}", p.display())))?;
    let mut out = Vec::new();
    for line in text.lines().filter(|l| l.trim_start().starts_with('|')) {
        for t in ticks(line) {
            if t.starts_with("strategies/<kind>/") && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    Ok(out)
}

/// Registered strategy crates (strategies/registry/README.md kind_id table) that exist: directory names.
fn registered_strategies(root: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(root.join("strategies/registry/README.md")).unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines().filter(|l| l.trim_start().starts_with('|')) {
        if line.contains("退役") {
            continue;
        }
        if let Some(i) = line.find("](../") {
            let rest = &line[i + 5..];
            if let Some(dir) = rest.split('/').next()
                && rest[dir.len()..].starts_with("/README.md")
                && root.join("strategies").join(dir).join("Cargo.toml").is_file()
                && !out.iter().any(|d| d == dir)
            {
                out.push(dir.to_string());
            }
        }
    }
    out
}

/// String literals of the `.rs` files under `dir`.
fn literals(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for e in walkdir::WalkDir::new(dir).into_iter().flatten() {
        if e.path().extension().is_some_and(|x| x == "rs")
            && let Ok(t) = std::fs::read_to_string(e.path())
        {
            for (i, part) in t.split('"').enumerate() {
                if i % 2 == 1 {
                    out.insert(part.to_string());
                }
            }
        }
    }
    out
}

/// Missing strategy entries: `(strategy dir, benchmark name)`.
fn missing_strategy_entries(root: &Path) -> Result<Vec<(String, String)>, EnvError> {
    let strategies = registered_strategies(root);
    if strategies.is_empty() {
        return Ok(Vec::new());
    }
    let entries = strategy_entries(root)?;
    let mut out = Vec::new();
    for s in strategies {
        let lits = literals(&root.join("strategies").join(&s).join("benches"));
        for e in &entries {
            let a = e.replace("<kind>", &s);
            let b = e.replace("<kind>", &s.replace('-', "_"));
            if !lits.contains(&a) && !lits.contains(&b) {
                out.push((s.clone(), a));
            }
        }
    }
    Ok(out)
}

/// One benchmark package: `(dir, package name)`.
#[derive(Debug, Clone)]
struct Pkg {
    dir: String,
    name: String,
}

fn bench_packages(root: &Path, o: &Opts) -> Result<Vec<Pkg>, EnvError> {
    let mut out = Vec::new();
    for m in members(root)? {
        let b = root.join(&m.dir).join("benches");
        let has = std::fs::read_dir(&b)
            .map(|rd| {
                rd.flatten()
                    .any(|e| e.path().extension().is_some_and(|x| x == "rs"))
            })
            .unwrap_or(false);
        if has {
            out.push(Pkg {
                dir: m.dir,
                name: m.name,
            });
        }
    }
    if let Some(c) = &o.krate {
        out.retain(|p| &p.name == c);
        if out.is_empty() {
            return Err(EnvError(format!(
                "--crate {c}: no workspace member with that name has benches/"
            )));
        }
    }
    if o.path.is_some() {
        out.retain(|p| p.name == PATH_PKG);
        if out.is_empty() {
            return Err(EnvError(format!(
                "--path needs the {PATH_PKG} package (benchmarks/Cargo.toml)"
            )));
        }
    }
    Ok(out)
}

/// Benchmark target names of the member at `dir`: `[[bench]]` names plus, unless
/// `autobenches = false`, the auto-discovered `benches/<name>.rs` and `benches/<name>/main.rs`.
fn bench_targets(root: &Path, dir: &str) -> BTreeSet<String> {
    let d = root.join(dir);
    let mut out = BTreeSet::new();
    let manifest: toml::Table = std::fs::read_to_string(d.join("Cargo.toml"))
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    for b in manifest
        .get("bench")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        if let Some(n) = b.get("name").and_then(|v| v.as_str()) {
            out.insert(n.to_string());
        }
    }
    let auto = manifest
        .get("package")
        .and_then(|p| p.get("autobenches"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    if auto {
        for e in std::fs::read_dir(d.join("benches"))
            .into_iter()
            .flatten()
            .flatten()
        {
            let p = e.path();
            if p.is_file() && p.extension().is_some_and(|x| x == "rs") {
                if let Some(s) = p.file_stem() {
                    out.insert(s.to_string_lossy().into_owned());
                }
            } else if p.join("main.rs").is_file() {
                out.insert(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    out
}

/// Executables of the benchmark targets of the member at `dir` among the `compiler-artifact`
/// messages of a `--message-format=json…` build (in output order, without duplicates).
fn bench_executables(stdout: &str, dir: &str) -> Vec<String> {
    let want = format!("/{}/Cargo.toml", dir.trim_matches('/'));
    let mut out: Vec<String> = Vec::new();
    for l in stdout.lines() {
        let Ok(v) = serde_json::from_str::<Value>(l) else {
            continue;
        };
        if v["reason"] != "compiler-artifact" {
            continue;
        }
        let bench = v["target"]["kind"]
            .as_array()
            .is_some_and(|k| k.iter().any(|x| x == "bench"));
        let ours = v["manifest_path"]
            .as_str()
            .is_some_and(|m| m.replace('\\', "/").ends_with(&want));
        if let (true, true, Some(x)) = (bench, ours, v["executable"].as_str())
            && !out.iter().any(|o| o == x)
        {
            out.push(x.to_string());
        }
    }
    out
}

/// Measured numbers of one benchmark.
#[derive(Debug, Clone, Default)]
struct Res {
    name: String,
    n: u64,
    warmup: u64,
    p: [Option<f64>; 6], // p50 p90 p99 p999 max (index 0..5), unused slot 5
    allocs: Option<u64>,
}

fn pct(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let k = ((q * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[k - 1]
}

fn parse_lines(stdout: &str) -> Vec<Res> {
    let mut out = Vec::new();
    for l in stdout.lines() {
        let Some(j) = l.trim().strip_prefix("hft-bench:") else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(j.trim()) else {
            continue;
        };
        let f = |k: &str| v.get(k).and_then(Value::as_f64);
        out.push(Res {
            name: v.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
            n: v.get("n").and_then(Value::as_u64).unwrap_or(0),
            warmup: v.get("warmup").and_then(Value::as_u64).unwrap_or(0),
            p: [
                f("p50_ns"),
                f("p90_ns"),
                f("p99_ns"),
                f("p999_ns"),
                f("max_ns"),
                None,
            ],
            allocs: v.get("allocs_after_warmup").and_then(Value::as_u64),
        });
    }
    out
}

/// Criterion results written after `since` under `<target>/criterion`: per-sample mean times.
fn criterion_results(target: &Path, since: SystemTime) -> Vec<Res> {
    let mut out = Vec::new();
    let dir = target.join("criterion");
    for e in walkdir::WalkDir::new(&dir).into_iter().flatten() {
        if e.file_name() != "benchmark.json" || !e.path().parent().is_some_and(|p| p.ends_with("new")) {
            continue;
        }
        let fresh = e
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .is_some_and(|t| t >= since);
        if !fresh {
            continue;
        }
        let read = |p: PathBuf| {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        };
        let (Some(b), Some(s)) = (
            read(e.path().to_path_buf()),
            read(e.path().with_file_name("sample.json")),
        ) else {
            continue;
        };
        let iters: Vec<f64> = s["iters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect();
        let times: Vec<f64> = s["times"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .collect();
        let mut per: Vec<f64> = iters
            .iter()
            .zip(&times)
            .filter(|(i, _)| **i > 0.0)
            .map(|(i, t)| t / i)
            .collect();
        per.iter_mut().for_each(|x| *x = x.round());
        per.sort_by(|a, b| a.total_cmp(b));
        if per.is_empty() {
            continue;
        }
        let name = b["full_id"].as_str().unwrap_or("").to_string();
        out.push(Res {
            name,
            n: iters.iter().sum::<f64>() as u64,
            warmup: 0,
            p: [
                Some(pct(&per, 0.5)),
                Some(pct(&per, 0.9)),
                Some(pct(&per, 0.99)),
                Some(pct(&per, 0.999)),
                per.last().copied(),
                None,
            ],
            allocs: None,
        });
    }
    out
}

fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"))
}

fn git_sha(root: &Path) -> Option<String> {
    let head = std::fs::read_to_string(root.join(".git/HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: ") {
        Some(r) => std::fs::read_to_string(root.join(".git").join(r))
            .ok()
            .map(|s| s.trim().to_string()),
        None => Some(head.to_string()),
    }
}

/// Budget table of one benchmark.
#[derive(Debug, Clone, Default)]
struct Budget {
    p50: Option<f64>,
    p99: Option<f64>,
    any: Option<f64>,
    gate: Option<String>,
    allocs_zero: bool,
}

impl Budget {
    fn has_values(&self) -> bool {
        self.p50.is_some() || self.p99.is_some() || self.any.is_some()
    }
}

fn budgets(path: &Path) -> Result<BTreeMap<String, Budget>, EnvError> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(BTreeMap::new());
    };
    let t: toml::Table = text
        .parse()
        .map_err(|e| EnvError(format!("{}: {e}", path.display())))?;
    let num =
        |v: Option<&toml::Value>| v.and_then(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)));
    Ok(t.iter()
        .filter_map(|(k, v)| v.as_table().map(|b| (k, b)))
        .map(|(k, b)| {
            (
                k.clone(),
                Budget {
                    p50: num(b.get("p50_ns")),
                    p99: num(b.get("p99_ns")),
                    any: num(b.get("budget_ns")),
                    gate: b.get("gate").and_then(|g| g.as_str()).map(str::to_string),
                    allocs_zero: b.get("allocs").and_then(|a| a.as_integer()) == Some(0),
                },
            )
        })
        .collect())
}

fn safe(name: &str) -> String {
    name.replace('/', "__")
}

fn baseline_file(dir: &Path, hash: &str, name: &str) -> PathBuf {
    dir.join(hash).join(format!("{}.jsonl", safe(name)))
}

/// Median p99 of the last [`BASELINE_RUNS`] passing runs of this HostProfile series.
fn baseline(dir: &Path, hash: &str, name: &str) -> Option<f64> {
    let text = std::fs::read_to_string(baseline_file(dir, hash, name)).ok()?;
    let mut v: Vec<f64> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|x| x.get("p99_ns").and_then(Value::as_f64))
        .collect();
    let n = v.len();
    if n == 0 {
        return None;
    }
    v = v.split_off(n.saturating_sub(BASELINE_RUNS));
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len();
    Some(if m % 2 == 1 {
        v[m / 2]
    } else {
        (v[m / 2 - 1] + v[m / 2]) / 2.0
    })
}

fn opt(x: Option<f64>) -> Value {
    match x {
        None => Value::Null,
        Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => json!(f as i64),
        Some(f) => json!(f),
    }
}

/// Syscall numbers by name for the build architecture (aarch64 generic table or x86_64).
pub fn syscall_nr(name: &str) -> Option<u32> {
    syscall_table().iter().find(|(n, _)| *n == name).map(|(_, x)| *x)
}

fn syscall_table() -> &'static [(&'static str, u32)] {
    if cfg!(target_arch = "aarch64") {
        &[
            ("epoll_pwait", 22),
            ("futex", 98),
            ("openat", 56),
            ("read", 63),
            ("write", 64),
            ("writev", 66),
            ("sendto", 206),
            ("recvfrom", 207),
            ("sendmsg", 211),
            ("recvmsg", 212),
            ("nanosleep", 101),
            ("clock_nanosleep", 115),
            ("sched_yield", 124),
            ("setsockopt", 208),
            ("mmap", 222),
            ("munmap", 215),
        ]
    } else {
        &[
            ("read", 0),
            ("write", 1),
            ("mmap", 9),
            ("munmap", 11),
            ("writev", 20),
            ("sched_yield", 24),
            ("nanosleep", 35),
            ("sendto", 44),
            ("recvfrom", 45),
            ("sendmsg", 46),
            ("recvmsg", 47),
            ("setsockopt", 54),
            ("futex", 202),
            ("clock_nanosleep", 230),
            ("epoll_wait", 232),
            ("openat", 257),
            ("epoll_pwait", 281),
        ]
    }
}

fn syscall_name(nr: u32) -> String {
    syscall_table()
        .iter()
        .find(|(_, x)| *x == nr)
        .map(|(n, _)| n.to_string())
        .unwrap_or_else(|| format!("syscall_{nr}"))
}

/// hot-path-rules T7 allow table by role; park-state syscalls are added for non-`Spin` threads.
fn allowed(role: &str, spin: bool) -> BTreeSet<&'static str> {
    let mut s: BTreeSet<&str> = match role {
        "sequencer" => BTreeSet::new(),
        "gw" => ["recvmsg", "recvfrom", "sendmsg", "sendto", "writev"].into(),
        "feed" => ["recvmsg", "recvfrom", "sendmsg", "sendto", "epoll_pwait"].into(),
        _ => BTreeSet::new(),
    };
    if !spin {
        s.extend(["futex", "epoll_wait", "epoll_pwait", "write"]);
    }
    s
}

fn privileged() -> bool {
    match std::env::var("XTASK_BENCH_PRIV").as_deref() {
        Ok("root") => return true,
        Ok("none") => return false,
        _ => {}
    }
    let Ok(st) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    let field = |k: &str| {
        st.lines()
            .find_map(|l| l.strip_prefix(k))
            .map(str::trim)
            .unwrap_or("")
    };
    if field("Uid:").split_whitespace().nth(1) == Some("0") {
        return true;
    }
    let cap = u64::from_str_radix(field("CapEff:"), 16).unwrap_or(0);
    cap & (1 << 38) != 0 && cap & (1 << 39) != 0 // CAP_PERFMON, CAP_BPF
}

/// Runs the syscall audit; `Ok(violations)` as `(thread, syscall, count)`.
fn audit(root: &Path, ex: &mut dyn Exec) -> Result<Vec<(String, String, u64)>, EnvError> {
    let p = root.join(AUDIT_CFG);
    let text = std::fs::read_to_string(&p).map_err(|e| EnvError(format!("{AUDIT_CFG}: {e}")))?;
    let t: toml::Table = text.parse().map_err(|e| EnvError(format!("{AUDIT_CFG}: {e}")))?;
    let run: Vec<String> = t
        .get("run")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .filter(|v: &Vec<String>| !v.is_empty())
        .ok_or_else(|| EnvError(format!("{AUDIT_CFG}: `run` (command line array) is required")))?;
    let threads_rel = t.get("threads").and_then(|v| v.as_str()).ok_or_else(|| {
        EnvError(format!(
            "{AUDIT_CFG}: `threads` (file with [[threads]]) is required"
        ))
    })?;
    let tt: toml::Table = std::fs::read_to_string(root.join(threads_rel))
        .map_err(|e| EnvError(format!("{threads_rel}: {e}")))?
        .parse()
        .map_err(|e| EnvError(format!("{threads_rel}: {e}")))?;
    // Thread name → (role, spin); roles from the tasks (not from name prefixes).
    let mut roles: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for th in tt.get("threads").and_then(|v| v.as_array()).into_iter().flatten() {
        let name = th.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tasks: Vec<&str> = th
            .get("tasks")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str())
            .collect();
        let role = if tasks.contains(&"sequencer") {
            "sequencer"
        } else if tasks.iter().any(|t| t.starts_with("gw:")) {
            "gw"
        } else if tasks.iter().any(|t| t.starts_with("feed")) {
            "feed"
        } else {
            "warm"
        };
        let spin = th.get("idle").and_then(|v| v.as_str()) == Some("Spin");
        roles.insert(name, (role.to_string(), spin));
    }
    let script = "tracepoint:raw_syscalls:sys_enter /pid == cpid/ { @[comm, args->id] = count(); }";
    let out = ex.run(&Cmd::capture("bpftrace", &["-e", script, "-c", &run.join(" ")]))?;
    if !out.ok() {
        return Err(EnvError(format!(
            "bpftrace exited {}: {}",
            out.code,
            out.stderr.trim()
        )));
    }
    let mut v = Vec::new();
    for l in out.stdout.lines() {
        let Some(rest) = l.trim().strip_prefix("@[") else {
            continue;
        };
        let Some((key, cnt)) = rest.split_once("]:") else {
            continue;
        };
        let Some((comm, nr)) = key.rsplit_once(',') else {
            continue;
        };
        let (comm, nr) = (
            comm.trim().to_string(),
            nr.trim().parse::<u32>().unwrap_or(u32::MAX),
        );
        let count: u64 = cnt.trim().parse().unwrap_or(0);
        let Some((role, spin)) = roles.get(&comm) else {
            continue;
        };
        if role == "warm" {
            continue;
        }
        let name = syscall_name(nr);
        if !allowed(role, *spin).contains(name.as_str()) {
            v.push((comm, name, count));
        }
    }
    Ok(v)
}

/// Entry point.
pub fn run(root: &Path, args: &[String]) -> i32 {
    let o = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("bench-gate: error: {e}");
            return EXIT_ENV;
        }
    };
    match run_inner(root, &o) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("bench-gate: error: {e}");
            EXIT_ENV
        }
    }
}

fn run_inner(root: &Path, o: &Opts) -> Result<i32, EnvError> {
    if o.audit && !privileged() {
        return Err(EnvError(
            "--syscall-audit needs root or CAP_BPF + CAP_PERFMON (bpftrace); not started".into(),
        ));
    }
    let mut ex = from_env(root)?;
    let mut hash: Option<String> = None;
    let mut host_class: Option<String> = None;
    let mut path_absolute = false;
    if o.mode != Mode::Smoke {
        let out = ex.run(&Cmd::capture(
            "hostprobe",
            &["verify", "--expected", REFERENCE, "--out", HOSTPROFILE],
        ))?;
        eprint!("{}", out.stderr);
        if !out.ok() {
            return Err(EnvError(format!(
                "this host does not match {REFERENCE} (hostprobe verify exited {}); not a reference run",
                out.code
            )));
        }
        hash = std::fs::read_to_string(root.join(HOSTPROFILE))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["host_profile_hash"].as_str().map(str::to_string));
        if let Ok(t) = std::fs::read_to_string(root.join(REFERENCE))
            && let Ok(t) = t.parse::<toml::Table>()
        {
            host_class = t.get("host_class").and_then(|v| v.as_str()).map(str::to_string);
            path_absolute = t.get("path_absolute").and_then(|v| v.as_bool()) == Some(true);
        }
    }
    let mut failures: Vec<String> = Vec::new();
    let mut summary = json!({"mode": match o.mode { Mode::Smoke => "smoke", Mode::Reference => "reference", Mode::AuditOnly => "syscall-audit" }});
    if o.mode == Mode::AuditOnly {
        let v = audit(root, ex.as_mut())?;
        return Ok(finish_audit(root, summary, v, failures));
    }
    // Strategy benchmark entries (both modes).
    let missing = missing_strategy_entries(root)?;
    for (s, n) in &missing {
        eprintln!("bench-gate: strategies/{s}/benches/ lacks the SDK entry `{n}`");
        failures.push(format!("missing strategy entry {n}"));
    }
    summary["missing_strategy_entries"] = json!(missing.iter().map(|(_, n)| n).collect::<Vec<_>>());
    let pkgs = bench_packages(root, o)?;
    let reg = registry(root);
    let reference = o.mode == Mode::Reference;
    let hash_s = hash.clone().unwrap_or_else(|| "unknown".into());
    let base_dir = o
        .baseline_dir
        .clone()
        .unwrap_or_else(|| root.join("target/bench-baseline"));
    let target = target_dir(root);
    let mut results_json = Vec::new();
    let mut missing_budgets = Vec::new();
    for pkg in &pkgs {
        let bpath = if pkg.name == PATH_PKG {
            root.join("benchmarks/budgets.toml")
        } else {
            root.join(&pkg.dir).join("benches/budgets.toml")
        };
        let bud = budgets(&bpath)?;
        if reference {
            // Every registered benchmark of this crate needs a budget (path benchmarks only once
            // their absolute thresholds are in force).
            let comp = if pkg.name == PATH_PKG {
                "benchmarks"
            } else {
                pkg.name.as_str()
            };
            if pkg.name != PATH_PKG || path_absolute {
                for (n, c) in &reg {
                    if c == comp
                        && o.path.as_ref().is_none_or(|p| n.starts_with(p.as_str()))
                        && !bud
                            .get(n)
                            .is_some_and(|b| b.has_values() || b.gate.as_deref() == Some("report"))
                    {
                        eprintln!(
                            "bench-gate: `{n}` ({comp}) has no budget in {}",
                            crate::rel_path(root, &bpath)
                        );
                        missing_budgets.push(n.clone());
                        failures.push(format!("{n}: no budget"));
                    }
                }
            }
        }
        // Build (smoke) and run.
        let since = SystemTime::now();
        let out = if reference {
            match &o.bench_dir {
                Some(d) => {
                    let mut all = crate::exec::Output::default();
                    let mut exes: Vec<PathBuf> = std::fs::read_dir(d)
                        .map_err(|e| EnvError(format!("--bench-dir {}: {e}", d.display())))?
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| p.is_file())
                        .collect();
                    exes.sort();
                    for x in exes {
                        let r = ex.run(&Cmd::capture(&x.display().to_string(), &["--bench", "--noplot"]))?;
                        all.stdout += &r.stdout;
                        if !r.ok() {
                            all.code = r.code;
                            all.stderr += &r.stderr;
                        }
                    }
                    all
                }
                None => {
                    let mut c = Cmd::capture(
                        "cargo",
                        &[
                            "bench",
                            "--profile",
                            "bench",
                            "--all-features",
                            "-p",
                            &pkg.name,
                            "--bench",
                            "*",
                            "--",
                        ],
                    );
                    if let Some(p) = &o.path {
                        c = c.arg(p.clone());
                    }
                    ex.run(&c.arg("--noplot"))?
                }
            }
        } else {
            // Built with the workspace's feature resolution (`--workspace --all-features`, as the
            // unit/property step), so the `ci`-profile libraries of that step are reused; a
            // per-package `-p` resolves features differently and rebuilds dependencies.
            let mut c = Cmd::capture(
                "cargo",
                &[
                    "bench",
                    "--profile",
                    "ci",
                    "--workspace",
                    "--all-features",
                    "--no-run",
                    "--message-format=json-render-diagnostics",
                ],
            );
            for n in bench_targets(root, &pkg.dir) {
                c = c.arg("--bench").arg(n);
            }
            let b = ex.run(&c)?;
            let exes = bench_executables(&b.stdout, &pkg.dir);
            if !b.ok() || exes.is_empty() {
                eprint!("{}", b.stderr);
                eprintln!(
                    "bench-gate: benchmarks of {} ({}) do not build (exit {})",
                    pkg.dir, pkg.name, b.code
                );
                failures.push(format!("{}: build failed", pkg.dir));
                continue;
            }
            // Run the package's own executables (another member may have a benchmark target of
            // the same name), as `cargo bench` would: `--bench` plus the arguments.
            let mut all = crate::exec::Output::default();
            for x in exes {
                let mut c = Cmd::capture(&x, &["--bench"]);
                if let Some(p) = &o.path {
                    c = c.arg(p.clone());
                }
                for a in SMOKE_ARGS {
                    c = c.arg(*a);
                }
                let r = ex.run(&c)?;
                all.stdout += &r.stdout;
                if !r.ok() {
                    all.code = r.code;
                    all.stderr += &r.stderr;
                }
            }
            all
        };
        if !out.ok() {
            eprintln!(
                "bench-gate: benchmarks of {} failed to run (exit {}): {}",
                pkg.dir,
                out.code,
                out.stderr.trim()
            );
            failures.push(format!("{}: run failed", pkg.dir));
            continue;
        }
        let mut res = parse_lines(&out.stdout);
        let known: BTreeSet<String> = res.iter().map(|r| r.name.clone()).collect();
        res.extend(
            criterion_results(&target, since)
                .into_iter()
                .filter(|r| !known.contains(&r.name)),
        );
        if let Some(p) = &o.path {
            res.retain(|r| r.name.starts_with(p.as_str()));
        }
        for r in res {
            let (path, case) = if pkg.name == PATH_PKG {
                match r.name.split_once('/') {
                    Some((a, b)) => (a.to_string(), b.to_string()),
                    None => (r.name.clone(), "default".to_string()),
                }
            } else {
                (pkg.name.clone(), r.name.clone())
            };
            let b = bud.get(&r.name).cloned().unwrap_or_default();
            let mut gate = json!({"kind": "smoke", "budget_p50_ns": null, "budget_p99_ns": null, "baseline_p99_ns": null, "verdict": "pass"});
            if reference {
                let kernel = KERNEL_PREFIXES.iter().any(|k| r.name.starts_with(k));
                let journal = pkg.name == "hft-journal";
                let kind = match b.gate.as_deref() {
                    Some(k) => k.to_string(),
                    None if pkg.name == PATH_PKG && !path_absolute => "report".into(),
                    None if b.has_values() => "absolute".into(),
                    None => "report".into(),
                };
                let (p50, p99) = (r.p[0], r.p[2]);
                let over = |v: Option<f64>, lim: Option<f64>| matches!((v, lim), (Some(a), Some(l)) if a > l);
                let breach = over(p50, b.p50) || over(p99, b.p99) || over(p50, b.any);
                let base = if kernel || journal {
                    None
                } else {
                    baseline(&base_dir, &hash_s, &r.name)
                };
                let rel_breach = matches!((p99, base), (Some(a), Some(bl)) if a * REL_DEN > bl * REL_NUM);
                let alloc_breach = b.allocs_zero && r.allocs.is_some_and(|a| a > 0);
                let (kind_out, verdict) = if kind == "absolute" && breach {
                    ("absolute".to_string(), "fail")
                } else if rel_breach {
                    ("relative".to_string(), "fail")
                } else if alloc_breach {
                    (kind.clone(), "fail")
                } else if kind == "report" && breach {
                    ("report".to_string(), "exceeded")
                } else if kind == "absolute" || kind == "report" {
                    (kind.clone(), "pass")
                } else if base.is_some() {
                    ("relative".to_string(), "pass")
                } else {
                    (kind.clone(), "pass")
                };
                if verdict == "fail" {
                    eprintln!(
                        "bench-gate: {} {kind_out} gate failed (p50 {:?} p99 {:?} allocs {:?}; budget p50 {:?} p99 {:?} any {:?}; baseline p99 {:?})",
                        r.name, p50, p99, r.allocs, b.p50, b.p99, b.any, base
                    );
                    failures.push(format!("{}: {kind_out}", r.name));
                } else if let Some(a) = p99 {
                    let f = baseline_file(&base_dir, &hash_s, &r.name);
                    if let Some(d) = f.parent() {
                        std::fs::create_dir_all(d).map_err(|e| EnvError(format!("{}: {e}", d.display())))?;
                    }
                    let mut prev = std::fs::read_to_string(&f).unwrap_or_default();
                    prev += &format!("{}\n", json!({"p99_ns": a, "git_sha": git_sha(root)}));
                    std::fs::write(&f, prev).map_err(|e| EnvError(format!("{}: {e}", f.display())))?;
                }
                gate = json!({
                    "kind": kind_out,
                    "budget_p50_ns": opt(b.p50.or(b.any)),
                    "budget_p99_ns": opt(b.p99),
                    "baseline_p99_ns": opt(base),
                    "verdict": verdict,
                });
            }
            let doc = json!({
                "name": r.name, "path": path, "case": case,
                "dataset": {
                    "host_profile_hash": hash, "host_class": host_class, "git_sha": git_sha(root),
                    "target_cpu": if reference { json!("neoverse-v1") } else { Value::Null },
                    "ts_source": "SW", "mode": if reference { "reference" } else { "smoke" },
                },
                "n": r.n, "warmup": r.warmup,
                "p50_ns": opt(r.p[0]), "p90_ns": opt(r.p[1]), "p99_ns": opt(r.p[2]), "p999_ns": opt(r.p[3]), "max_ns": opt(r.p[4]),
                "allocs_after_warmup": r.allocs, "syscalls_outside_allowlist": null,
                "gate": gate,
            });
            let file = root.join("target/bench").join(&path).join(format!("{case}.json"));
            if let Some(d) = file.parent() {
                std::fs::create_dir_all(d).map_err(|e| EnvError(format!("{}: {e}", d.display())))?;
            }
            std::fs::write(
                &file,
                serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n",
            )
            .map_err(|e| EnvError(format!("{}: {e}", file.display())))?;
            eprintln!(
                "bench-gate: {:<40} p50 {:>10} p99 {:>10} {}",
                r.name,
                opt(r.p[0]),
                opt(r.p[2]),
                doc["gate"]["verdict"].as_str().unwrap_or("")
            );
            results_json.push(doc);
        }
    }
    summary["results"] = Value::Array(results_json);
    summary["missing_budgets"] = json!(missing_budgets);
    if o.audit {
        let v = audit(root, ex.as_mut())?;
        return Ok(finish_audit(root, summary, v, failures));
    }
    summary["failures"] = json!(failures);
    write_json(root, "bench-gate", &summary)?;
    eprintln!("bench-gate: {} failure(s)", failures.len());
    Ok(if failures.is_empty() { EXIT_OK } else { EXIT_FAIL })
}

fn finish_audit(
    root: &Path,
    mut summary: Value,
    v: Vec<(String, String, u64)>,
    mut failures: Vec<String>,
) -> i32 {
    for (t, s, c) in &v {
        eprintln!("bench-gate: syscall audit: thread {t} made {c} `{s}` call(s) outside the T7 allow table");
        failures.push(format!("{t}: {s}"));
    }
    summary["syscalls_outside_allowlist"] =
        Value::Array(v.iter().map(|(t, s, c)| json!([t, s, c])).collect());
    summary["failures"] = json!(failures);
    if let Err(e) = write_json(root, "bench-gate", &summary) {
        eprintln!("bench-gate: error: {e}");
        return EXIT_ENV;
    }
    if failures.is_empty() { EXIT_OK } else { EXIT_FAIL }
}
