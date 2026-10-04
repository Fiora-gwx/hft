//! `cargo xtask <subcommand>` entry point. Spec: tools/README.md.

use std::path::PathBuf;
use std::process::ExitCode;

use xtask::{EXIT_ENV, EXIT_OK, deps, doc_check, gen_schema, test_trace};

const SUBCOMMANDS: &[(&str, &str)] = &[
    (
        "deps",
        "dependency direction and forbidden dependencies (tools/xtask/deps.toml + cargo metadata)",
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
        s += &format!("  {name:<12} {what}\n");
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
        Some(cmd) if !rest.is_empty() => {
            eprintln!("xtask {cmd}: unexpected arguments {rest:?}");
            EXIT_ENV
        }
        Some("test-trace") => test_trace::run(&root),
        Some("doc-check") => doc_check::run(&root),
        Some("deps") => deps::run(&root),
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
