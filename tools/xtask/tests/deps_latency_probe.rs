//! Machine checks of the latency-probe strategy crate (strategies/latency-probe/TESTS.md LPR-064):
//! dependency direction on the `deps` clean fixture (common::deps_repo, `strategies/latency-probe`
//! depends on the SDK), the strategy-README exchange-name rule on `docs-clean` (its strategy README is
//! `strategies/fooarb`), the strategy fast-part float rule on the hot-check clean tree (strategy `x`)
//! and clippy's clock ban with the hot `clippy.toml` on a standalone strategy crate.

mod common;

use std::process::Command;

use common::Fixture;
use common::deps_repo::{D, apply, clean, violations};
use common::repos::{copy_real, docs_clean, hot_clean};

const LP: &str = "strategies/latency-probe";

#[test]
fn lpr_064_machine_checks() {
    // .1: the clean trees pass every check.
    let f = clean("lpr-064-1");
    let run = f.xtask(&["deps"]);
    assert_eq!(run.code, 0, "LPR-064.1 deps: {:?}", violations(&run));
    let f = docs_clean("lpr-064-1d");
    let run = f.xtask(&["doc-check"]);
    assert_eq!(run.code, 0, "LPR-064.1 doc-check: {}", run.stderr);
    let f = hot_clean("lpr-064-1h");
    let run = f.xtask(&["hot-check"]);
    assert_eq!(run.code, 0, "LPR-064.1 hot-check: {}", run.stderr);

    // .2–.4: dependency direction.
    let rows = [
        (
            "LPR-064.2",
            "crates/hft-oms",
            "strategies/latency-probe → crates/hft-oms",
        ),
        (
            "LPR-064.3",
            "strategies/x",
            "strategies/latency-probe → strategies/x",
        ),
        (
            "LPR-064.4",
            "crates/hft-types",
            "strategies/latency-probe → crates/hft-types",
        ),
    ];
    for (row, dep, needle) in rows {
        let f = clean(&row.to_lowercase().replace('.', "-"));
        apply(&f, LP, D, dep);
        let run = f.xtask(&["deps"]);
        let vs = violations(&run);
        assert_eq!(run.code, 1, "{row}: {vs:?}");
        assert!(vs.iter().any(|v| v.contains(needle)), "{row}: {vs:?}");
    }

    // .5: an exchange name in a strategy README.
    let f = docs_clean("lpr-064-5");
    f.append("strategies/fooarb/README.md", "\n本策略在 Binance 上运行。\n");
    let run = f.xtask(&["doc-check"]);
    assert_eq!(
        run.code, 1,
        "LPR-064.5: {}
{}",
        run.stdout, run.stderr
    );
    assert!(
        run.stderr.contains("VENUENAME") || run.stdout.contains("VENUENAME"),
        "LPR-064.5: {}",
        run.stderr
    );

    // .6: f64 in the fast part.
    let f = hot_clean("lpr-064-6");
    f.write(
        "strategies/x/src/fast/quote.rs",
        "pub fn q() {\n    let x: f64 = 0.0;\n    let _ = x;\n}\n",
    );
    let run = f.xtask(&["hot-check"]);
    assert_eq!(run.code, 1, "LPR-064.6: {}", run.stderr);
    assert!(
        run.stderr.contains("[S1] strategies/x/src/fast/quote.rs"),
        "LPR-064.6: {}",
        run.stderr
    );

    // .7: the system clock in the fast part fails clippy (hot clippy.toml, `cargo xtask ci`'s clippy step).
    let f = Fixture::empty("lpr-064-7");
    f.write(
        "Cargo.toml",
        "[package]\nname = \"latency-probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    copy_real(&f, "tools/lint/clippy-hot.toml");
    f.write("clippy.toml", &f.read("tools/lint/clippy-hot.toml"));
    f.write("src/lib.rs", "pub mod fast;\n");
    f.write("src/fast/mod.rs", "pub mod pair;\n");
    f.write(
        "src/fast/pair.rs",
        "pub fn tick() -> u128 {\n    std::time::Instant::now().elapsed().as_nanos()\n}\n",
    );
    let out = Command::new(env!("CARGO"))
        .args(["clippy", "--quiet", "--offline", "--", "-D", "warnings"])
        .current_dir(&f.root)
        .env("CARGO_TARGET_DIR", f.path("target"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "LPR-064.7: clippy passed");
    assert!(
        err.contains("disallowed_methods") || err.contains("disallowed-methods"),
        "LPR-064.7: {err}"
    );
}
