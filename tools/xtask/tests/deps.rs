//! `cargo xtask deps` (tools/TESTS.md TOOL-030, TOOL-031). Fixture `clean`: common::deps_repo.

use crate::common;

use common::deps_repo::{D, DEV, PKGS, apply, clean, violations};

#[test]
fn tool_030_clean_passes() {
    let f = clean("tool-030");
    let run = f.xtask(&["deps"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(violations(&run).is_empty(), "{:?}", violations(&run));
    assert_eq!(run.json.as_ref().unwrap()["members"], PKGS.len());
}

/// `(row, [(dir, section, dependency entry or raw line)], expected exit, output must contain)`.
type Row = (
    &'static str,
    &'static [(&'static str, &'static str, &'static str)],
    i32,
    &'static str,
);

const ROWS: &[Row] = &[
    (
        "TOOL-031.1",
        &[("crates/hft-book", D, "crates/hft-oms")],
        1,
        "crates/hft-book → crates/hft-oms",
    ),
    (
        "TOOL-031.2",
        &[("crates/hft-types", D, "crates/hft-refdata")],
        1,
        "crates/hft-types → crates/hft-refdata",
    ),
    (
        "TOOL-031.3",
        &[("crates/hft-oms", D, "vendor/x")],
        1,
        "crates/hft-oms → x → tokio",
    ),
    (
        "TOOL-031.4",
        &[("crates/hft-oms", D, "vendor/async-std")],
        1,
        "`async-std`",
    ),
    ("TOOL-031.5", &[("crates/hft-oms", D, "vendor/mio")], 1, "`mio`"),
    (
        "TOOL-031.6",
        &[("crates/hft-oms", D, "vendor/futures-executor")],
        1,
        "`futures-executor`",
    ),
    (
        "TOOL-031.7",
        &[("crates/hft-oms", D, "vendor/tracing-subscriber")],
        1,
        "`tracing-subscriber`",
    ),
    (
        "TOOL-031.8",
        &[("crates/hft-oms", D, "vendor/serde_json")],
        1,
        "`serde_json`",
    ),
    (
        "TOOL-031.9",
        &[("crates/hft-oms", D, "vendor/reqwest")],
        1,
        "`reqwest`",
    ),
    (
        "TOOL-031.10",
        &[("crates/hft-oms", D, "vendor/hyper")],
        1,
        "`hyper`",
    ),
    (
        "TOOL-031.11",
        &[("crates/hft-oms", D, "vendor/pyo3")],
        1,
        "`pyo3`",
    ),
    (
        "TOOL-031.12",
        &[("crates/hft-oms", D, "vendor/rust_decimal")],
        1,
        "`rust_decimal`",
    ),
    (
        "TOOL-031.13",
        &[("crates/hft-oms", D, "vendor/bigdecimal")],
        1,
        "`bigdecimal`",
    ),
    (
        "TOOL-031.14",
        &[("crates/hft-oms", D, "vendor/num-bigint")],
        1,
        "`num-bigint`",
    ),
    (
        "TOOL-031.15",
        &[("crates/hft-types", D, "vendor/serde_json")],
        1,
        "pure package closure contains `serde_json`",
    ),
    (
        "TOOL-031.16",
        &[("crates/venue-binance", D, "crates/hft-net")],
        1,
        "crates/venue-binance → crates/hft-net",
    ),
    (
        "TOOL-031.17",
        &[("crates/venue-binance", D, "vendor/simd-json")],
        1,
        "crates/venue-binance → simd-json",
    ),
    (
        "TOOL-031.18",
        &[
            ("crates/venue-binance", D, "vendor/serde"),
            ("crates/venue-binance", D, "vendor/memchr"),
            ("crates/venue-binance", D, "vendor/zerocopy"),
            ("crates/venue-binance", D, "vendor/ed25519-dalek"),
            ("crates/venue-binance", D, "vendor/hmac"),
            ("crates/venue-binance", D, "vendor/sha2"),
            ("crates/venue-binance", D, "vendor/base64"),
        ],
        0,
        "",
    ),
    (
        "TOOL-031.19",
        &[("crates/hft-oms", D, "vendor/alloy-primitives")],
        1,
        "chain crate",
    ),
    ("TOOL-031.20", &[("apps/chain-core", D, "vendor/revm")], 0, ""),
    (
        "TOOL-031.21",
        &[("crates/hft-oms", D, "crates/hft-testkit")],
        1,
        "dev-tier package outside [dev-dependencies]",
    ),
    (
        "TOOL-031.22",
        &[("crates/hft-oms", DEV, "crates/hft-testkit")],
        0,
        "",
    ),
    ("TOOL-031.23", &[("tools/xtask", D, "crates/hft-testkit")], 0, ""),
    (
        "TOOL-031.24",
        &[("crates/hft-venue", D, "crates/venue-okx")],
        1,
        "hft-venue depends on a venue crate",
    ),
    (
        "TOOL-031.25",
        &[("crates/hft-exec", D, "crates/hft-rt")],
        1,
        "closure contains crates/hft-rt",
    ),
    (
        "TOOL-031.26",
        &[("crates/venue-bitget", D, "crates/hft-rt")],
        1,
        "closure contains crates/hft-rt",
    ),
    (
        "TOOL-031.27",
        &[("strategies/x", D, "crates/hft-rt")],
        1,
        "closure contains crates/hft-rt",
    ),
    (
        "TOOL-031.28",
        &[("apps/watchtower", D, "crates/hft-journal:writer")],
        1,
        "enables `hft-journal/writer`",
    ),
    (
        "TOOL-031.29",
        &[("apps/replayer", D, "crates/hft-journal:writer")],
        0,
        "",
    ),
    (
        "TOOL-031.30",
        &[("crates/hft-oms", DEV, "strategies/x")],
        1,
        "only strategies/registry depends on strategy crates",
    ),
    (
        "TOOL-031.31",
        &[("strategies/x", D, "crates/hft-oms")],
        1,
        "strategies/x → crates/hft-oms",
    ),
    (
        "TOOL-031.32",
        &[("strategies/x", D, "crates/hft-types")],
        1,
        "strategies/x → crates/hft-types",
    ),
    (
        "TOOL-031.33",
        &[("crates/hft-strategy-sdk", D, "crates/hft-book")],
        1,
        "crates/hft-strategy-sdk → crates/hft-book",
    ),
    (
        "TOOL-031.34",
        &[("crates/hft-strat-lib", D, "crates/hft-oms")],
        1,
        "crates/hft-strat-lib → crates/hft-oms",
    ),
    (
        "TOOL-031.35",
        &[("crates/hft-testkit", DEV, "strategies/registry")],
        1,
        "crates/hft-testkit → strategies/registry",
    ),
    (
        "TOOL-031.36",
        &[("tools/hft-cfg", D, "strategies/x")],
        1,
        "tools/hft-cfg → strategies/x",
    ),
    (
        "TOOL-031.37",
        &[("crates/hft-py", DEV, "strategies/x")],
        1,
        "crates/hft-py → strategies/x",
    ),
    (
        "TOOL-031.38",
        &[("apps/watchtower", D, "strategies/registry")],
        1,
        "apps/watchtower → strategies/registry",
    ),
    (
        "TOOL-031.39",
        &[("tools/hft-verify", D, "crates/hft-types")],
        1,
        "hft crate or network library",
    ),
    (
        "TOOL-031.40",
        &[("tools/hft-verify", D, "vendor/reqwest")],
        1,
        "tools/hft-verify → reqwest",
    ),
    (
        "TOOL-031.41",
        &[("tools/hft-verify", D, "vendor/tokio")],
        1,
        "tools/hft-verify → tokio",
    ),
    (
        "TOOL-031.42",
        &[("tools/xtask", D, "strategies/x")],
        1,
        "tools/xtask → strategies/x",
    ),
    (
        "TOOL-031.43",
        &[("tools/hostprobe", D, "crates/hft-rt")],
        1,
        "tools/hostprobe → crates/hft-rt",
    ),
    (
        "TOOL-031.44",
        &[("tools/hft-cfg", D, "crates/hft-rt")],
        1,
        "tools/hft-cfg → crates/hft-rt",
    ),
    (
        "TOOL-031.45",
        &[("crates/hft-oms", D, "tools/hft-secrets")],
        1,
        "AWS secrets SDK",
    ),
    (
        "TOOL-031.46",
        &[("crates/hft-oms", D, "crates/hft-telemetry")],
        1,
        "hot package depends on a cold package",
    ),
    (
        "TOOL-031.47",
        &[("strategies/x", "features", "!sdk-testing")],
        1,
        "enables the `testing` feature in [dependencies]",
    ),
    (
        "TOOL-031.48",
        &[("strategies/x", DEV, "crates/hft-strategy-sdk:testing")],
        0,
        "",
    ),
    (
        "TOOL-031.49",
        &[("crates/hft-py", "features", "!default-testing")],
        1,
        "default features enable `testing`",
    ),
];

#[test]
fn tool_031_rule_counterexamples() {
    std::thread::scope(|sc| {
        let handles: Vec<_> = ROWS
            .chunks(8)
            .map(|chunk| {
                sc.spawn(move || {
                    let mut fails = Vec::new();
                    for (row, edits, want, needle) in chunk {
                        let f = clean(&row.to_lowercase().replace('.', "-"));
                        for (dir, section, entry) in *edits {
                            apply(&f, dir, section, entry);
                        }
                        let run = f.xtask(&["deps"]);
                        let vs = violations(&run);
                        if run.code != *want {
                            fails.push(format!(
                                "{row}: exit {} (want {want}); {vs:?}; {}",
                                run.code, run.stderr
                            ));
                        } else if !needle.is_empty() && !vs.iter().any(|v| v.contains(needle)) {
                            fails.push(format!("{row}: no violation mentions '{needle}': {vs:?}"));
                        }
                    }
                    fails
                })
            })
            .collect();
        let fails: Vec<String> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        assert!(fails.is_empty(), "{}", fails.join("\n"));
    });
}

/// crates/hft-py/TESTS.md PY-141: forbidden dependencies of hft-py fail `cargo xtask deps`.
#[test]
fn py_141_forbidden_deps() {
    const PY: &str = "crates/hft-py";
    let rows: &[(&str, &str, i32, &str)] = &[
        ("PY-141.1", "crates/hft-journal:writer", 1, "hft-journal/writer"),
        (
            "PY-141.2",
            "crates/hft-sequencer",
            1,
            "crates/hft-py → crates/hft-sequencer",
        ),
        ("PY-141.3", "crates/hft-sim", 1, "crates/hft-py → crates/hft-sim"),
        ("PY-141.4", "crates/hft-net", 1, "crates/hft-py → crates/hft-net"),
        (
            "PY-141.5",
            "crates/hft-venue",
            1,
            "crates/hft-py → crates/hft-venue",
        ),
        ("PY-141.6", "strategies/x", 1, "crates/hft-py → strategies/x"),
        (
            "PY-141.7",
            "strategies/registry",
            1,
            "crates/hft-py → strategies/registry",
        ),
        ("PY-141.8", "", 0, ""),
    ];
    let mut fails = Vec::new();
    for (row, entry, want, needle) in rows {
        let f = clean(&row.to_lowercase().replace('.', "-"));
        if entry.ends_with(":writer") {
            // hft-py already depends on hft-journal in the clean fixture: the line gains the feature.
            f.replace(
                &format!("{PY}/Cargo.toml"),
                "hft-journal = { path = \"../../crates/hft-journal\" }",
                "hft-journal = { path = \"../../crates/hft-journal\", features = [\"writer\"] }",
            );
        } else if !entry.is_empty() {
            apply(&f, PY, D, entry);
        }
        let run = f.xtask(&["deps"]);
        let vs = violations(&run);
        if run.code != *want {
            fails.push(format!(
                "{row}: exit {} (want {want}); {vs:?}; {}",
                run.code, run.stderr
            ));
        } else if !needle.is_empty() && !vs.iter().any(|v| v.contains(needle)) {
            fails.push(format!("{row}: no violation mentions '{needle}': {vs:?}"));
        }
    }
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

/// crates/hft-py/TESTS.md PY-142: the feature closure of hft-py (with and without `testing`, for
/// the aarch64 target) never enables hft-journal's `writer`.
#[test]
fn py_142_no_writer_feature() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for extra in [&[][..], &["--features", "testing"][..]] {
        let out = std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .current_dir(&root)
            .args([
                "tree",
                "-p",
                "hft-py",
                "-e",
                "features",
                "--target",
                "aarch64-unknown-linux-gnu",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let t = String::from_utf8_lossy(&out.stdout);
        assert!(
            !t.contains("hft-journal feature \"writer\""),
            "{extra:?}: writer enabled\n{t}"
        );
        assert!(
            t.contains("hft-journal feature \"default\""),
            "{extra:?}: no hft-journal default features\n{t}"
        );
    }
}
