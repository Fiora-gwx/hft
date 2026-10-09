//! EXE-476: dependency closure (via `cargo tree`; no JSON parser among the allowed dev deps).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::process::Command;

/// Package names printed by `cargo tree -p hft-exec` with `args`.
fn tree(args: &[&str]) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "hft-exec",
            "--prefix",
            "none",
            "-f",
            "{p}",
            "--manifest-path",
        ])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .filter(|n| !n.is_empty() && !n.starts_with('['))
        .collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn exe_476_dependency_closure() {
    // .1 direct dependencies (normal + build), and dev dependencies.
    let direct = tree(&["-e", "normal,build", "--depth", "1"]);
    for n in &direct {
        assert!(
            [
                "hft-exec",
                "hft-types",
                "hft-refdata",
                "hft-event",
                "hft-strategy-sdk",
                "zerocopy"
            ]
            .contains(&n.as_str()),
            "EXE-476.1: {n}"
        );
    }
    let dev = tree(&["-e", "normal,build,dev", "--depth", "1"]);
    for n in &dev {
        assert!(
            [
                "hft-exec",
                "hft-types",
                "hft-refdata",
                "hft-event",
                "hft-strategy-sdk",
                "zerocopy",
                "hft-testkit",
                "proptest",
                "criterion"
            ]
            .contains(&n.as_str()),
            "EXE-476.1 (dev): {n}"
        );
    }
    // .2 / .3 production closure.
    let all = tree(&["-e", "normal,build"]);
    let banned = [
        "hft-oms",
        "hft-ledger",
        "hft-risk",
        "hft-sequencer",
        "hft-journal",
        "hft-rt",
        "hft-strat-lib",
        "hft-venue",
        "hft-strategy-registry",
    ];
    for n in &all {
        assert!(
            !banned.contains(&n.as_str()) && !n.starts_with("venue-") && !n.starts_with("strategy-"),
            "EXE-476.2: {n}"
        );
        assert_ne!(n, "tokio", "EXE-476.3");
    }
    assert!(all.iter().any(|n| n == "hft-strategy-sdk"));
}
