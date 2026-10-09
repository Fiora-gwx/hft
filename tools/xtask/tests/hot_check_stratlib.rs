//! Forbidden constructs in hft-strat-lib (crates/hft-strat-lib/TESTS.md SLB-134). Fixture: the hot-check
//! `clean` tree (common::repos::hot_clean) plus a pure `crates/hft-strat-lib` with the planned source
//! files; the clippy row builds a standalone copy of that crate with the real pure `clippy.toml`.

mod common;

use std::process::Command;

use common::Fixture;
use common::repos::{copy_real, hot_clean};

const LIB: &str = "//! strat-lib fixture\n\n#![forbid(unsafe_code)]\n#![deny(\n    clippy::float_arithmetic,\n    clippy::unwrap_used,\n    clippy::expect_used,\n    clippy::panic,\n    clippy::unreachable,\n    clippy::todo,\n    clippy::unimplemented\n)]\n\npub mod funding;\npub mod impact;\npub mod model;\npub mod spread;\npub mod throttle;\n";

const FILES: &[(&str, &str)] = &[
    (
        "src/spread.rs",
        "pub fn buy(a: i64, b: i64) -> i64 {\n    a - b\n}\n",
    ),
    (
        "src/throttle.rs",
        "pub fn can(now: i64) -> bool {\n    now > 0\n}\n",
    ),
    (
        "src/impact.rs",
        "pub trait BookRead {\n    fn best(&self) -> i64;\n}\n\npub fn vwap<B: BookRead>(b: &B) -> i64 {\n    b.best()\n}\n",
    ),
    ("src/funding.rs", "pub fn daily(r: i64) -> i64 {\n    r * 3\n}\n"),
    ("src/model/mod.rs", "pub mod leadlag;\npub mod pct;\n"),
    ("src/model/leadlag.rs", "pub fn est(n: u32) -> u32 {\n    n\n}\n"),
    ("src/model/pct.rs", "pub fn value(n: u32) -> u32 {\n    n\n}\n"),
];

/// Writes the strat-lib crate under `dir` (`crates/hft-strat-lib` in a repo fixture, `.` standalone).
fn stratlib(f: &Fixture, dir: &str) {
    let p = |rel: &str| {
        if dir.is_empty() {
            rel.to_string()
        } else {
            format!("{dir}/{rel}")
        }
    };
    f.write(
        &p("Cargo.toml"),
        "[package]\nname = \"hft-strat-lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    copy_real(f, "tools/lint/clippy-pure.toml");
    f.write(&p("clippy.toml"), &f.read("tools/lint/clippy-pure.toml"));
    f.write(&p("src/lib.rs"), LIB);
    for (rel, text) in FILES {
        f.write(&p(rel), text);
    }
}

/// `(row, file, code appended inside a new function, expected hot-check exit, stderr must contain)`.
type Row = (&'static str, &'static str, &'static str, i32, &'static str);

const HOT_ROWS: &[Row] = &[
    (
        "SLB-134.1",
        "src/spread.rs",
        "let x: f64 = 0.0;\n    let _ = x;",
        1,
        "[N3] crates/hft-strat-lib/src/spread.rs:",
    ),
    (
        "SLB-134.2",
        "src/model/leadlag.rs",
        "let x: f64 = 0.0;\n    let _ = x;",
        0,
        "",
    ),
    (
        "SLB-134.3",
        "src/throttle.rs",
        "let _ = std::time::Instant::now();",
        1,
        "[T4] crates/hft-strat-lib/src/throttle.rs:",
    ),
    (
        "SLB-134.6",
        "src/funding.rs",
        "let _ = std::fs::read(\"x\");",
        1,
        "[pure] crates/hft-strat-lib/src/funding.rs:",
    ),
];

#[test]
fn slb_134_forbidden_constructs() {
    for (row, file, code, want, needle) in HOT_ROWS {
        let f = hot_clean(&format!("slb134-{}", row.replace('.', "-")));
        stratlib(&f, "crates/hft-strat-lib");
        f.append(
            &format!("crates/hft-strat-lib/{file}"),
            &format!("\npub fn probe() {{\n    {code}\n}}\n"),
        );
        let r = f.xtask(&["hot-check"]);
        assert_eq!(r.code, *want, "{row}: stderr:\n{}", r.stderr);
        assert!(
            r.stderr.contains(needle),
            "{row}: expected {needle:?} in:\n{}",
            r.stderr
        );
    }

    // SLB-134.5: a trait object parameter in the fast part.
    let f = hot_clean("slb134-5");
    stratlib(&f, "crates/hft-strat-lib");
    f.append(
        "crates/hft-strat-lib/src/impact.rs",
        "\npub fn vwap_dyn(b: &dyn BookRead) -> i64 {\n    b.best()\n}\n",
    );
    let r = f.xtask(&["hot-check"]);
    assert_eq!(r.code, 1, "SLB-134.5: stderr:\n{}", r.stderr);
    assert!(
        r.stderr.contains("[N6] crates/hft-strat-lib/src/impact.rs:"),
        "SLB-134.5:\n{}",
        r.stderr
    );

    // SLB-134.4: clippy with the pure clippy.toml rejects iterating a HashMap in src/model/pct.rs
    // (`cargo xtask ci` runs this clippy step with -D warnings).
    let f = Fixture::empty("slb134-4");
    stratlib(&f, "");
    f.append(
        "src/model/pct.rs",
        "\npub fn hist() -> usize {\n    let m: std::collections::HashMap<u8, u8> = Default::default();\n    m.iter().count()\n}\n",
    );
    let out = Command::new(env!("CARGO"))
        .args(["clippy", "--quiet", "--offline", "--", "-D", "warnings"])
        .current_dir(&f.root)
        .env("CARGO_TARGET_DIR", f.path("target"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "SLB-134.4: clippy passed:\n{err}");
    assert!(
        err.contains("disallowed_types") || err.contains("disallowed-types"),
        "SLB-134.4:\n{err}"
    );
}
