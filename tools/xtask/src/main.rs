//! `cargo xtask <subcommand>` entry point. Spec: tools/README.md.

use std::path::PathBuf;
use std::process::ExitCode;

use xtask::{
    EXIT_ENV, EXIT_OK, bench_gate, ci, decoupling_gate, deps, doc_check, fixtures, gen_host, gen_schema,
    hot_check, journal_verify, replay_verify, setup, test_trace,
};

const SUBCOMMANDS: &[(&str, &str)] = &[
    (
        "ci",
        "[--pure-only] [--fast] the CI quality gate steps in order (tools/README.md \"cargo xtask ci\")",
    ),
    (
        "decoupling-gate",
        "--base <sha> --head <sha> [--pr <n>] strategy/engine co-change check (git + GitHub only)",
    ),
    (
        "deps",
        "dependency direction and forbidden dependencies (tools/xtask/deps.toml + cargo metadata)",
    ),
    (
        "hot-check",
        "hot-path source and configuration checks (docs/hot-path-rules.md)",
    ),
    (
        "fixtures",
        "check | scrub [<path>…] sensitive-data scan and scrubbing of fixtures/",
    ),
    (
        "journal-verify",
        "<dir> [--from-segment N] [--json] journal segment verification",
    ),
    (
        "setup",
        "install the pinned cargo plugins and fuzz nightly (tools/xtask/toolchains.toml)",
    ),
    (
        "gen-host",
        "[--host <name>] [--check] host configuration generator (input checks only for now)",
    ),
    (
        "bench-gate",
        "[--crate <x>] [--path <p>] [--smoke | --reference] [--syscall-audit] benchmark smoke and reference gates (benchmarks/README.md)",
    ),
    (
        "replay-verify",
        "<journal-dir | pointer> verification replay (input checks only for now)",
    ),
    (
        "doc-check",
        "documentation gates (native checks + scripts listed in tools/xtask/doc-check.toml)",
    ),
    (
        "gen-schema",
        "[--check] event schema outputs + append-only lock check (crates/hft-event README 实现说明)",
    ),
    (
        "test-trace",
        "test specs (TESTS.md, docs/tests) <-> test functions, scenarios, benches (docs/tests §6)",
    ),
    ("help", "list subcommands"),
];

fn usage() -> String {
    let mut s = String::from("usage: cargo xtask [--root <repo>] <subcommand>\n\nsubcommands:\n");
    for (name, what) in SUBCOMMANDS {
        s += &format!("  {name:<16} {what}\n");
    }
    s += "\nexit codes: 0 pass, 1 check failed, 2 environment or input error\n";
    s
}

fn main() -> ExitCode {
    // Repo root: two levels above this crate, unless --root is given.
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let mut args = std::env::args().skip(1).peekable();
    let mut sub = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => match args.next() {
                Some(r) => root = PathBuf::from(r),
                None => {
                    eprintln!("xtask: --root needs a value\n\n{}", usage());
                    return ExitCode::from(EXIT_ENV as u8);
                }
            },
            _ => {
                sub = Some(a);
                break;
            }
        }
    }
    let rest: Vec<String> = args.collect();
    let code = match sub.as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print!("{}", usage());
            EXIT_OK
        }
        Some("gen-schema") => gen_schema::run(&root, &rest),
        Some("ci") => ci::run(&root, &rest),
        Some("decoupling-gate") => decoupling_gate::run(&root, &rest),
        Some("fixtures") => fixtures::run(&root, &rest),
        Some("journal-verify") => journal_verify::run(&root, &rest),
        Some("setup") => setup::run(&root, &rest),
        Some("gen-host") => gen_host::run(&root, &rest),
        Some("replay-verify") => replay_verify::run(&root, &rest),
        Some("bench-gate") => bench_gate::run(&root, &rest),
        Some(cmd) if !rest.is_empty() => {
            eprintln!("xtask {cmd}: unexpected arguments {rest:?}");
            EXIT_ENV
        }
        Some("test-trace") => test_trace::run(&root),
        Some("doc-check") => doc_check::run(&root),
        Some("deps") => deps::run(&root),
        Some("hot-check") => hot_check::run(&root),
        Some(other) => {
            eprintln!(
                "xtask: unknown or not yet implemented subcommand '{other}'\n\n{}",
                usage()
            );
            EXIT_ENV
        }
    };
    ExitCode::from(code as u8)
}
