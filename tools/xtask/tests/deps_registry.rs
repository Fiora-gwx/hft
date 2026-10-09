//! hft-strategy-registry dependency edges (strategies/registry/TESTS.md REG-015). Fixture `clean`:
//! common::deps_repo.

use crate::common;

use common::deps_repo::{D, DEV, apply, clean, violations};

/// `(row, [(dir, section, dependency entry)], expected exit, a violation must contain)`.
type Row = (
    &'static str,
    &'static [(&'static str, &'static str, &'static str)],
    i32,
    &'static str,
);

const REG: &str = "strategies/registry";

const ROWS: &[Row] = &[
    ("REG-015.1", &[], 0, ""),
    (
        "REG-015.2",
        &[("apps/watchtower", D, REG)],
        1,
        "apps/watchtower → strategies/registry",
    ),
    ("REG-015.3", &[("tools/hft-cfg", D, REG)], 1, "tools/hft-cfg"),
    ("REG-015.4", &[("crates/hft-py", D, REG)], 1, "crates/hft-py"),
    (
        "REG-015.5",
        &[("crates/hft-testkit", DEV, REG)],
        1,
        "crates/hft-testkit",
    ),
    (
        "REG-015.6",
        &[(REG, D, "crates/hft-sequencer")],
        1,
        "strategies/registry → crates/hft-sequencer",
    ),
    ("REG-015.7", &[(REG, DEV, "crates/hft-sequencer")], 0, ""),
    // apps/replayer already depends on the registry in the clean fixture.
    ("REG-015.8", &[], 0, ""),
    (
        "REG-015.9",
        &[("apps/hft-core", D, "strategies/latency-probe")],
        1,
        "apps/hft-core",
    ),
];

#[test]
fn reg_015_registry_edges() {
    let mut fails = Vec::new();
    for (row, edits, want, needle) in ROWS {
        let f = clean(&row.to_lowercase().replace('.', "-"));
        if *row == "REG-015.7" {
            let m = f.read("strategies/registry/Cargo.toml");
            assert!(m.contains("hft-testkit"), "{row}: {m}");
        }
        if *row == "REG-015.8" {
            let m = f.read("apps/replayer/Cargo.toml");
            assert!(m.contains("hft-strategy-registry"), "{row}: {m}");
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
