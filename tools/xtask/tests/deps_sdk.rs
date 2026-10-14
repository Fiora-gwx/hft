//! hft-strategy-sdk dependency direction and `strategy_set!` call sites (crates/hft-strategy-sdk/TESTS.md
//! SDK-001, SDK-002). Fixture `clean`: common::deps_repo.

use crate::common;

use common::Fixture;
use common::deps_repo::{D, DEV, apply, clean, violations};

/// `(row, [(dir, section, dependency entry)], expected exit, a violation must contain)`.
type Row = (
    &'static str,
    &'static [(&'static str, &'static str, &'static str)],
    i32,
    &'static str,
);

const SDK: &str = "crates/hft-strategy-sdk";
const X: &str = "strategies/x";

const ROWS: &[Row] = &[
    ("SDK-001.1", &[], 0, ""),
    (
        "SDK-001.2",
        &[(SDK, D, "crates/hft-oms")],
        1,
        "crates/hft-strategy-sdk → crates/hft-oms",
    ),
    (
        "SDK-001.3",
        &[(SDK, DEV, "crates/hft-sequencer")],
        1,
        "crates/hft-strategy-sdk → crates/hft-sequencer` ([dev-dependencies])",
    ),
    (
        "SDK-001.4",
        &[(SDK, D, "crates/hft-journal")],
        1,
        "crates/hft-strategy-sdk → crates/hft-journal",
    ),
    ("SDK-001.5", &[(SDK, D, "vendor/tokio")], 1, "`tokio`"),
    (
        "SDK-001.6",
        &[(X, D, "crates/hft-types")],
        1,
        "strategies/x → crates/hft-types",
    ),
    (
        "SDK-001.7",
        &[(X, D, "crates/hft-oms")],
        1,
        "strategies/x → crates/hft-oms",
    ),
    (
        "SDK-001.8",
        &[(X, D, "strategies/latency-probe")],
        1,
        "strategies/x → strategies/latency-probe",
    ),
    (
        "SDK-001.9",
        &[("crates/hft-sequencer", DEV, "strategies/registry")],
        1,
        "crates/hft-sequencer → strategies/registry",
    ),
    (
        "SDK-001.10",
        &[("crates/hft-testkit", D, X)],
        1,
        "crates/hft-testkit → strategies/x",
    ),
    (
        "SDK-001.11",
        &[("apps/hft-core", D, X)],
        1,
        "apps/hft-core → strategies/x",
    ),
    // strategies/x depends on exactly hft-strategy-sdk and hft-strat-lib in the clean fixture.
    ("SDK-001.12", &[], 0, ""),
];

#[test]
fn sdk_001_dependency_direction() {
    let mut fails = Vec::new();
    for (row, edits, want, needle) in ROWS {
        let f = clean(&row.to_lowercase().replace('.', "-"));
        if *row == "SDK-001.2" {
            // hft-oms depends on the SDK in the clean graph; cargo rejects the cycle the row's edge
            // would close, so that edge is taken out first.
            f.replace(
                "crates/hft-oms/Cargo.toml",
                "hft-strategy-sdk = { path = \"../../crates/hft-strategy-sdk\" }\n",
                "",
            );
        }
        if *row == "SDK-001.12" {
            let m: toml::Table = f.read("strategies/x/Cargo.toml").parse().unwrap();
            let deps: Vec<&String> = m["dependencies"].as_table().unwrap().keys().collect();
            assert_eq!(deps, ["hft-strat-lib", "hft-strategy-sdk"], "{row}");
        }
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
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

/// One `strategy_set!` invocation listing a test-range strategy, on line 3 of the written file.
const CALL: &str = "//! fixture\n\nhft_strategy_sdk::strategy_set! {\n    pub struct TestSet {\n        probe::Probe, // kind_id 0xFF10\n    }\n}\n";

/// The clean fixture plus the two invocations the repository has (registry, hft-testkit).
fn with_sites(tag: &str) -> Fixture {
    let f = clean(tag);
    f.write("strategies/registry/src/lib.rs", CALL);
    f.write("crates/hft-testkit/src/strategies/mod.rs", CALL);
    f
}

#[test]
fn sdk_002_strategy_set_call_sites() {
    let rows: [(&str, Option<&str>, i32); 6] = [
        ("SDK-002.1", None, 0),
        ("SDK-002.2", None, 0),
        (
            "SDK-002.3",
            Some("crates/hft-strategy-sdk/tests/set_dispatch.rs"),
            0,
        ),
        ("SDK-002.4", Some("apps/hft-core/src/main.rs"), 1),
        ("SDK-002.5", Some("crates/hft-sequencer/tests/x.rs"), 1),
        ("SDK-002.6", Some("strategies/x/src/lib.rs"), 1),
    ];
    let mut fails = Vec::new();
    for (row, file, want) in rows {
        let f = with_sites(&row.to_lowercase().replace('.', "-"));
        if let Some(file) = file {
            f.write(file, CALL);
        }
        let run = f.xtask(&["deps"]);
        let vs = violations(&run);
        if run.code != want {
            fails.push(format!(
                "{row}: exit {} (want {want}); {vs:?}; {}",
                run.code, run.stderr
            ));
            continue;
        }
        if want == 1 {
            let at = format!("{}:3", file.unwrap());
            if !vs.iter().any(|v| v.starts_with("[rule 13]") && v.contains(&at)) {
                fails.push(format!("{row}: no rule-13 violation names {at}: {vs:?}"));
            }
        }
    }
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}
