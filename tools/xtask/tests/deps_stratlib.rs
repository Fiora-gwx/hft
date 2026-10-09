//! hft-strat-lib dependency direction (crates/hft-strat-lib/TESTS.md SLB-133). Fixture `clean`:
//! common::deps_repo (`strategies/latency-probe` there depends on the SDK only, so row .6 adds no cycle).

use crate::common;

use common::deps_repo::{D, DEV, apply, clean, violations};

/// `(row, [(dir, section, dependency entry)], expected exit, a violation must contain)`.
type Row = (
    &'static str,
    &'static [(&'static str, &'static str, &'static str)],
    i32,
    &'static str,
);

const SL: &str = "crates/hft-strat-lib";

const ROWS: &[Row] = &[
    ("SLB-133.1", &[], 0, ""),
    (
        "SLB-133.2",
        &[(SL, D, "crates/hft-oms")],
        1,
        "crates/hft-strat-lib → crates/hft-oms",
    ),
    (
        "SLB-133.3",
        &[(SL, D, "crates/hft-sequencer")],
        1,
        "crates/hft-strat-lib → crates/hft-sequencer",
    ),
    (
        "SLB-133.4",
        &[(SL, D, "crates/hft-journal")],
        1,
        "crates/hft-strat-lib → crates/hft-journal",
    ),
    (
        "SLB-133.5",
        &[(SL, D, "crates/hft-rt")],
        1,
        "crates/hft-strat-lib → crates/hft-rt",
    ),
    (
        "SLB-133.6",
        &[(SL, D, "strategies/latency-probe")],
        1,
        "crates/hft-strat-lib → strategies/latency-probe",
    ),
    (
        "SLB-133.7",
        &[(SL, DEV, "crates/hft-book")],
        1,
        "crates/hft-strat-lib → crates/hft-book",
    ),
    ("SLB-133.8", &[(SL, D, "vendor/tokio")], 1, "`tokio`"),
    (
        "SLB-133.9",
        &[("crates/hft-sequencer", D, SL)],
        1,
        "crates/hft-sequencer → crates/hft-strat-lib",
    ),
    // strategies/x already depends on hft-strat-lib and hft-strategy-sdk in the clean fixture.
    ("SLB-133.10", &[], 0, ""),
];

#[test]
fn slb_133_dependency_direction() {
    let mut fails = Vec::new();
    for (row, edits, want, needle) in ROWS {
        let f = clean(&row.to_lowercase().replace('.', "-"));
        if *row == "SLB-133.10" {
            let m = f.read("strategies/x/Cargo.toml");
            assert!(
                m.contains("hft-strat-lib") && m.contains("hft-strategy-sdk"),
                "{row}: {m}"
            );
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
