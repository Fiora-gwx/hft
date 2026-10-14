//! `cargo xtask bench-gate` (tools/TESTS.md TOOL-033–TOOL-038, TOOL-185). Benchmarks run through the
//! recording executor: a scripted `cargo bench …` answers with `hft-bench: {json}` result lines.

mod common;

use common::{Fixture, Script};
use serde_json::{Value, json};

const SDK_ENTRIES: [&str; 7] = [
    "tick_miss",
    "tick_fire",
    "slow_update",
    "advice",
    "target_set",
    "exec_update",
    "risk_event",
];

fn member(f: &Fixture, dir: &str, name: &str) {
    f.write(
        &format!("{dir}/Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    );
    f.write(&format!("{dir}/src/lib.rs"), "");
    f.write(&format!("{dir}/benches/b.rs"), "fn main() {}\n");
}

/// Workspace with the given `(dir, package)` members, the registry rows `(name, component)` and
/// `benchmarks/reference.toml`.
fn repo(tag: &str, members: &[(&str, &str)], registered: &[(&str, &str)]) -> Fixture {
    let f = Fixture::empty(tag);
    let list: Vec<String> = members.iter().map(|(d, _)| format!("\"{d}\"")).collect();
    f.write(
        "Cargo.toml",
        &format!("[workspace]\nresolver = \"2\"\nmembers = [{}]\n", list.join(", ")),
    );
    for (d, n) in members {
        member(&f, d, n);
    }
    let mut reg = String::from(
        "# benchmarks\n\n### 基准名登记\n\n| 基准名 | 组件 | 预算出处 | 用例 |\n|---|---|---|---|\n",
    );
    for (n, c) in registered {
        reg += &format!("| `{n}` | {c} | x | — |\n");
    }
    f.write("benchmarks/README.md", &reg);
    f.write("benchmarks/reference.toml", "host_class = \"c16\"\n");
    f
}

fn line(name: &str, p99: u64) -> String {
    format!(
        "hft-bench: {}\n",
        json!({"name": name, "n": 10000, "warmup": 1000, "p50_ns": p99 / 2, "p90_ns": p99 * 3 / 4, "p99_ns": p99, "p999_ns": p99, "max_ns": p99, "allocs_after_warmup": 0})
    )
}

fn read(f: &Fixture, rel: &str) -> Value {
    serde_json::from_str(&f.read(rel)).unwrap()
}

/// Smoke build: one workspace-wide `cargo bench --no-run` (shared feature resolution).
const BUILD: &str = "cargo bench --profile ci --workspace --all-features --no-run";
/// Smoke run of crate `a`'s benchmark executable.
const SMOKE_A: &str = "/bench/a-b --bench";

/// `compiler-artifact` lines of the smoke build for `(member dir, executable)`.
fn built(list: &[(&str, &str)]) -> String {
    list.iter()
        .map(|(dir, exe)| {
            format!(
                "{}
",
                json!({"reason": "compiler-artifact", "manifest_path": format!("/repo/{dir}/Cargo.toml"),
                       "target": {"kind": ["bench"], "name": "b"}, "executable": exe})
            )
        })
        .collect()
}
const REF_A: &str = "cargo bench --profile bench --all-features -p a --bench * --";

#[test]
fn tool_033_smoke_never_blocks() {
    let f = repo("bg-033", &[("crates/a", "a")], &[("a/x", "a")]);
    f.write(
        "crates/a/benches/budgets.toml",
        "[\"a/x\"]\np50_ns = 50\np99_ns = 100\n",
    );
    let ba = built(&[("crates/a", "/bench/a-b")]);
    let s = Script::new(
        &f,
        &[(BUILD, 0, &ba, ""), (SMOKE_A, 0, &line("a/x", 1000), "")],
        0,
    );
    let r = f.xtask_script(&["bench-gate", "--smoke"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let j = read(&f, "target/bench/a/a/x.json");
    assert_eq!(j["dataset"]["mode"], "smoke");
    assert_eq!(j["gate"]["kind"], "smoke");
    assert_eq!(j["p99_ns"], 1000);
    for k in ["budget_p50_ns", "budget_p99_ns", "baseline_p99_ns"] {
        assert_eq!(j["gate"][k], Value::Null, "{k}");
    }
    let calls = s.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.contains("--no-run") && c.contains("--profile ci")),
        "{calls:?}"
    );
    for c in &calls {
        assert!(
            !c.contains("--profile bench") && !c.contains("release-hot"),
            "{c}"
        );
    }
}

#[test]
fn tool_034_smoke_build_failure() {
    let f = repo("bg-034", &[("crates/a", "a")], &[]);
    let s = Script::new(&f, &[(BUILD, 101, "", "error[E0425]")], 0);
    let r = f.xtask_script(&["bench-gate", "--smoke"], &s, &[]);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.stderr.contains("crates/a"), "{}", r.stderr);
}

#[test]
fn tool_035_reference_hostprobe_first() {
    let f = repo("bg-035", &[("crates/a", "a")], &[]);
    let s = Script::new(
        &f,
        &[(
            "hostprobe verify",
            1,
            "",
            "cpu.isolated expected=2-15 actual=2-14 required\n",
        )],
        0,
    );
    let r = f.xtask_script(&["bench-gate", "--reference"], &s, &[]);
    assert_eq!(r.code, 2, "{}", r.stderr);
    let calls = s.calls();
    assert_eq!(
        calls[0],
        "hostprobe verify --expected benchmarks/reference.toml --out target/bench/hostprofile.json"
    );
    assert!(!calls.iter().any(|c| c.starts_with("cargo bench")), "{calls:?}");
}

/// Reference fixture: crate `a` (budget `a/x` p99 500 ns, zero allocations) and the path package
/// with `seq-decide` and `gw-egress`; HostProfile hash `hash`; baseline p99 history per name.
fn ref_repo(tag: &str, hash: &str, history: &[(&str, &str, &[u64])], path_absolute: bool) -> Fixture {
    let f = repo(
        tag,
        &[("crates/a", "a"), ("benchmarks", "hft-benchmarks")],
        &[
            ("a/x", "a"),
            ("seq-decide", "benchmarks"),
            ("gw-egress", "benchmarks"),
        ],
    );
    f.write(
        "crates/a/benches/budgets.toml",
        "[\"a/x\"]\np99_ns = 500\nallocs = 0\n",
    );
    f.write(
        "benchmarks/budgets.toml",
        "[seq-decide]\np50_ns = 5000\np99_ns = 15000\n\n[gw-egress]\np50_ns = 5000\np99_ns = 15000\n",
    );
    f.write(
        "benchmarks/reference.toml",
        &format!("host_class = \"c16\"\npath_absolute = {path_absolute}\n"),
    );
    f.write(
        "target/bench/hostprofile.json",
        &json!({"host_profile_hash": hash}).to_string(),
    );
    for (h, name, runs) in history {
        let text: String = runs
            .iter()
            .map(|p| format!("{}\n", json!({"p99_ns": p})))
            .collect();
        f.write(
            &format!("target/bench-baseline/{h}/{}.jsonl", name.replace('/', "__")),
            &text,
        );
    }
    f
}

fn run_ref(f: &Fixture, a: &str, path: &str) -> common::Run {
    let s = Script::new(
        f,
        &[
            (REF_A, 0, a, ""),
            (
                "cargo bench --profile bench --all-features -p hft-benchmarks --bench * --",
                0,
                path,
                "",
            ),
        ],
        0,
    );
    f.xtask_script(&["bench-gate", "--reference"], &s, &[])
}

#[test]
fn tool_036_reference_verdicts() {
    let seven: &[u64] = &[1000; 7];
    // TOOL-036.1 / .2: relative threshold on seq-decide (baseline 1000).
    for (row, p99, code) in [("TOOL-036.1", 1100, 0), ("TOOL-036.2", 1101, 1)] {
        let f = ref_repo("bg-036", "H1", &[("H1", "seq-decide", seven)], false);
        let r = run_ref(&f, &line("a/x", 400), &line("seq-decide", p99));
        assert_eq!(r.code, code, "{row}: {}", r.stderr);
        let j = read(&f, "target/bench/seq-decide/default.json");
        assert_eq!(j["gate"]["baseline_p99_ns"], 1000, "{row}");
        if code == 1 {
            assert_eq!(j["gate"]["kind"], "relative", "{row}");
            assert_eq!(j["gate"]["verdict"], "fail", "{row}");
        }
    }
    // TOOL-036.3 / .4: single-crate upper limit (500 ns).
    for (row, p99, code) in [("TOOL-036.3", 500, 0), ("TOOL-036.4", 501, 1)] {
        let f = ref_repo("bg-036", "H1", &[], false);
        let r = run_ref(&f, &line("a/x", p99), &line("seq-decide", 1000));
        assert_eq!(r.code, code, "{row}: {}", r.stderr);
        let j = read(&f, "target/bench/a/a/x.json");
        assert_eq!(j["gate"]["kind"], "absolute", "{row}");
        assert_eq!(j["dataset"]["mode"], "reference", "{row}");
    }
    // TOOL-036.5: a kernel segment 1.5x its baseline but within the absolute threshold.
    let f = ref_repo("bg-036", "H1", &[("H1", "gw-egress", seven)], true);
    let r = run_ref(
        &f,
        &line("a/x", 400),
        &format!("{}{}", line("gw-egress", 1500), line("seq-decide", 1000)),
    );
    assert_eq!(r.code, 0, "TOOL-036.5: {}", r.stderr);
    assert_eq!(
        read(&f, "target/bench/gw-egress/default.json")["gate"]["verdict"],
        "pass"
    );
    // TOOL-036.6: allocations after warm-up on a zero-allocation benchmark.
    let f = ref_repo("bg-036", "H1", &[], false);
    let alloc = format!(
        "hft-bench: {}\n",
        json!({"name": "a/x", "n": 10, "p50_ns": 100, "p99_ns": 200, "allocs_after_warmup": 1})
    );
    let r = run_ref(&f, &alloc, &line("seq-decide", 1000));
    assert_eq!(r.code, 1, "TOOL-036.6: {}", r.stderr);
    // TOOL-036.7: three passing runs -> their median.
    let f = ref_repo("bg-036", "H1", &[("H1", "seq-decide", &[900, 2000, 1000])], false);
    let r = run_ref(&f, &line("a/x", 400), &line("seq-decide", 1100));
    assert_eq!(r.code, 0, "TOOL-036.7: {}", r.stderr);
    assert_eq!(
        read(&f, "target/bench/seq-decide/default.json")["gate"]["baseline_p99_ns"],
        1000
    );
    // TOOL-036.8: a new HostProfile hash starts a new series; the old one is not compared.
    let f = ref_repo("bg-036", "H2", &[("H1", "seq-decide", seven)], false);
    let r = run_ref(&f, &line("a/x", 400), &line("seq-decide", 5000));
    assert_eq!(r.code, 0, "TOOL-036.8: {}", r.stderr);
    assert_eq!(
        read(&f, "target/bench/seq-decide/default.json")["gate"]["baseline_p99_ns"],
        Value::Null
    );
    assert!(
        f.read("target/bench-baseline/H2/seq-decide.jsonl")
            .contains("5000")
    );
    let r = run_ref(&f, &line("a/x", 400), &line("seq-decide", 5600));
    assert_eq!(r.code, 1, "TOOL-036.8 second run vs the new series: {}", r.stderr);
    assert_eq!(
        read(&f, "target/bench/seq-decide/default.json")["gate"]["baseline_p99_ns"],
        5000
    );
}

fn audit_repo(tag: &str) -> Fixture {
    let f = repo(tag, &[("crates/a", "a")], &[]);
    f.write(
        "benchmarks/syscall-audit.toml",
        "run = [\"hft-core\", \"--config\", \"loopback.toml\"]\nthreads = \"benchmarks/threads.toml\"\n",
    );
    f.write(
        "benchmarks/threads.toml",
        "[[threads]]\nname = \"seq\"\ncore = 2\nidle = \"Spin\"\ntasks = [\"sequencer\"]\n\n\
         [[threads]]\nname = \"gw-bn\"\ncore = 3\nidle = \"Spin\"\ntasks = [\"gw:binance-usdm\"]\n\n\
         [[threads]]\nname = \"warm\"\ncore = 0\nidle = { Park = { park_us = 300 } }\ntasks = [\"ctl-gateway\", \"conn-builder\"]\n",
    );
    f
}

fn bt(entries: &[(&str, &str, u64)]) -> String {
    entries
        .iter()
        .map(|(t, s, c)| format!("@[{t}, {}]: {c}\n", xtask::bench_gate::syscall_nr(s).unwrap()))
        .collect()
}

#[test]
fn tool_037_syscall_audit() {
    // TOOL-037.1: no root, no CAP_BPF + CAP_PERFMON.
    let f = audit_repo("bg-037");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(
        &["bench-gate", "--syscall-audit"],
        &s,
        &[("XTASK_BENCH_PRIV", Some("none"))],
    );
    assert_eq!(r.code, 2, "TOOL-037.1: {}", r.stderr);
    assert!(!s.programs().iter().any(|p| p == "bpftrace"), "TOOL-037.1");
    let rows: Vec<(&str, String, i32)> = vec![
        (
            "TOOL-037.2",
            bt(&[("gw-bn", "recvmsg", 10), ("gw-bn", "writev", 5)]),
            0,
        ),
        (
            "TOOL-037.3",
            bt(&[("seq", "futex", 1), ("gw-bn", "recvmsg", 10)]),
            1,
        ),
        ("TOOL-037.4", bt(&[("warm", "openat", 3)]), 0),
    ];
    for (row, out, code) in rows {
        let f = audit_repo("bg-037");
        let s = Script::new(&f, &[("bpftrace", 0, &out, "")], 0);
        let r = f.xtask_script(
            &["bench-gate", "--syscall-audit"],
            &s,
            &[("XTASK_BENCH_PRIV", Some("root"))],
        );
        assert_eq!(r.code, code, "{row}: {}", r.stderr);
        let j = r.json.unwrap();
        if row == "TOOL-037.3" {
            assert_eq!(
                j["syscalls_outside_allowlist"],
                json!([["seq", "futex", 1]]),
                "{row}"
            );
        } else {
            assert_eq!(j["syscalls_outside_allowlist"], json!([]), "{row}");
        }
        // TOOL-037.5
        assert!(s.programs().iter().any(|p| p == "bpftrace"), "{row}");
        assert!(!s.programs().iter().any(|p| p == "strace"), "TOOL-037.5 ({row})");
    }
}

#[test]
fn tool_038_crate_filter() {
    let f = repo(
        "bg-038",
        &[("crates/hft-book", "hft-book"), ("crates/hft-oms", "hft-oms")],
        &[],
    );
    // Both members have a benchmark target `b`: the build reports both, only hft-book's runs.
    let bb = built(&[
        ("crates/hft-book", "/bench/hft-book-b"),
        ("crates/hft-oms", "/bench/hft-oms-b"),
    ]);
    let s = Script::new(
        &f,
        &[
            (BUILD, 0, &bb, ""),
            ("/bench/hft-book-b --bench", 0, &line("book_best", 10), ""),
            ("/bench/hft-oms-b --bench", 0, &line("oms/place", 10), ""),
        ],
        0,
    );
    let r = f.xtask_script(&["bench-gate", "--smoke", "--crate", "hft-book"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let calls = s.calls();
    let builds: Vec<&String> = calls.iter().filter(|c| c.starts_with("cargo bench")).collect();
    assert!(!builds.is_empty());
    assert!(
        builds
            .iter()
            .all(|c| c.contains("--workspace") && !c.contains(" -p ")),
        "{builds:?}"
    );
    let runs: Vec<&String> = calls.iter().filter(|c| c.starts_with("/bench/")).collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert!(runs[0].starts_with("/bench/hft-book-b --bench"), "{runs:?}");
    assert!(f.path("target/bench/hft-book/book_best.json").is_file());
    assert!(!f.path("target/bench/hft-oms").exists());
}

/// Crate `a` (`a/x` absolute, `a/y` report; both p99 100 ns) and the registered strategy `s`
/// whose benches/ names the SDK entries `entries`.
fn strat_repo(tag: &str, registered: &[(&str, &str)], entries: &[&str]) -> Fixture {
    let f = repo(tag, &[("crates/a", "a")], registered);
    f.write(
        "crates/a/benches/budgets.toml",
        "[\"a/x\"]\np99_ns = 100\ngate = \"absolute\"\n\n[\"a/y\"]\np99_ns = 100\ngate = \"report\"\n",
    );
    let mut sdk = String::from(
        "# hft-strategy-sdk\n\n## 延迟与资源契约\n\n| 入口 | p50 | p99 | 基准名 |\n|---|---|---|---|\n",
    );
    for e in SDK_ENTRIES {
        sdk += &format!("| x | ≤ 1 ns | ≤ 2 ns | `strategies/<kind>/{e}` |\n");
    }
    sdk += "| y | ≤ 1 µs | ≤ 5 µs | `model/<kind>/sample` |\n";
    f.write("crates/hft-strategy-sdk/README.md", &sdk);
    f.write(
        "strategies/registry/README.md",
        "| kind_id | crate | scope | Cargo feature |\n|---|---|---|---|\n| 1 | [s](../s/README.md) | core | 默认 |\n",
    );
    f.write(
        "strategies/s/Cargo.toml",
        "[package]\nname = \"s\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let body: String = entries
        .iter()
        .map(|e| format!("    c.bench_function(\"strategies/s/{e}\", |b| b.iter(|| 0));\n"))
        .collect();
    f.write(
        "strategies/s/benches/s.rs",
        &format!("fn f(c: &mut C) {{\n{body}}}\n"),
    );
    f
}

#[test]
fn tool_185_budget_kinds() {
    let all = SDK_ENTRIES.to_vec();
    let both = format!("{}{}", line("a/x", 50), line("a/y", 50));
    // TOOL-185.1
    let f = strat_repo("bg-185", &[("a/x", "a"), ("a/y", "a"), ("a/z", "a")], &all);
    let s = Script::new(&f, &[(REF_A, 0, &both, "")], 0);
    let r = f.xtask_script(&["bench-gate", "--reference"], &s, &[]);
    assert_eq!(r.code, 1, "TOOL-185.1: {}", r.stderr);
    assert!(r.stderr.contains("`a/z`"), "TOOL-185.1: {}", r.stderr);
    assert_eq!(r.json.unwrap()["missing_budgets"], json!(["a/z"]));
    // TOOL-185.2 / .3
    let reg = [("a/x", "a"), ("a/y", "a")];
    for (row, out, code) in [
        (
            "TOOL-185.2",
            format!("{}{}", line("a/x", 50), line("a/y", 200)),
            0,
        ),
        (
            "TOOL-185.3",
            format!("{}{}", line("a/x", 200), line("a/y", 50)),
            1,
        ),
    ] {
        let f = strat_repo("bg-185", &reg, &all);
        let s = Script::new(&f, &[(REF_A, 0, &out, "")], 0);
        let r = f.xtask_script(&["bench-gate", "--reference"], &s, &[]);
        assert_eq!(r.code, code, "{row}: {}", r.stderr);
        if row == "TOOL-185.2" {
            let j = read(&f, "target/bench/a/a/y.json");
            assert_eq!(j["gate"]["kind"], "report");
            assert_eq!(j["gate"]["verdict"], "exceeded");
        }
    }
    // TOOL-185.4
    let f = strat_repo("bg-185", &reg, &all);
    let ba = built(&[("crates/a", "/bench/a-b")]);
    let s = Script::new(
        &f,
        &[
            (BUILD, 0, &ba, ""),
            (
                SMOKE_A,
                0,
                &format!("{}{}", line("a/x", 200), line("a/y", 50)),
                "",
            ),
        ],
        0,
    );
    assert_eq!(
        f.xtask_script(&["bench-gate", "--smoke"], &s, &[]).code,
        0,
        "TOOL-185.4"
    );
    // TOOL-185.5
    let some: Vec<&str> = all.iter().copied().filter(|e| *e != "advice").collect();
    let f = strat_repo("bg-185", &reg, &some);
    let s = Script::new(&f, &[(BUILD, 0, &ba, ""), (SMOKE_A, 0, &both, "")], 0);
    let r = f.xtask_script(&["bench-gate", "--smoke"], &s, &[]);
    assert_eq!(r.code, 1, "TOOL-185.5: {}", r.stderr);
    assert!(
        r.stderr.contains("strategies/s/advice"),
        "TOOL-185.5: {}",
        r.stderr
    );
    assert_eq!(
        r.json.unwrap()["missing_strategy_entries"],
        json!(["strategies/s/advice"])
    );
    // An absent optional interface is explicit and reported, never a fake timing sample.
    f.write(
        "strategies/s/benches/not-applicable.toml",
        "advice = 'No Advice payload or subscription'\n",
    );
    let r = f.xtask_script(&["bench-gate", "--smoke"], &s, &[]);
    assert_eq!(r.code, 0, "TOOL-185.5: {}", r.stderr);
    assert_eq!(
        r.json.unwrap()["strategy_not_applicable"]["strategies/s/advice"],
        "No Advice payload or subscription"
    );
    // Critical hooks, empty reasons, typos and entries with real benchmarks cannot be excluded.
    for declaration in [
        "tick_fire = 'required'",
        "advice = ''",
        "advise = 'typo'",
        "target_set = 'already measured'",
    ] {
        f.write("strategies/s/benches/not-applicable.toml", declaration);
        let r = f.xtask_script(&["bench-gate", "--smoke"], &s, &[]);
        assert_ne!(r.code, 0, "TOOL-185.5: {declaration}");
        assert!(r.stderr.contains("invalid not-applicable"), "{}", r.stderr);
    }
    // TOOL-185.6
    let f = strat_repo("bg-185", &reg, &all);
    let s = Script::new(&f, &[(BUILD, 0, &ba, ""), (SMOKE_A, 0, &both, "")], 0);
    let r = f.xtask_script(&["bench-gate", "--smoke"], &s, &[]);
    assert_eq!(r.code, 0, "TOOL-185.6: {}", r.stderr);
}
