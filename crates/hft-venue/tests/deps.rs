//! VEN-341: dependency closure and source rules.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

struct Tree {
    names: BTreeSet<String>,
    features: BTreeMap<String, BTreeSet<String>>,
}

/// Resolve the selected package, not the workspace-wide feature union returned by metadata.
/// Other test crates enable hft-testkit's real sequencer adapter; hft-venue does not.
fn tree(edges: &str) -> Tree {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args([
            "tree",
            "--locked",
            "--color",
            "never",
            "--manifest-path",
            MANIFEST,
            "-p",
            "hft-venue",
            "--edges",
            edges,
            "--prefix",
            "none",
            "--format",
            "{p}|{f}",
        ])
        .output()
        .expect("cargo tree");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let output = String::from_utf8(out.stdout).unwrap();
    let mut names = BTreeSet::new();
    let mut features: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in output.lines() {
        let (package, enabled) = line.split_once('|').expect("cargo tree row");
        let name = package
            .split_whitespace()
            .next()
            .expect("package name")
            .to_string();
        names.insert(name.clone());
        features.entry(name).or_default().extend(
            enabled
                .trim_end_matches(" (*)")
                .split(',')
                .filter(|f| !f.is_empty())
                .map(str::to_string),
        );
    }
    assert!(
        names.remove("hft-venue"),
        "tree must include the selected package"
    );
    Tree { names, features }
}

fn src_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            src_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn ven_341_dependency_closure() {
    let normal = tree("normal,build");
    let all = tree("normal,build,dev");
    assert!(!all.names.iter().any(|n| n.starts_with("venue-")), "VEN-341.1");
    for n in ["hft-sequencer", "hft-oms", "hft-risk"] {
        assert!(!all.names.contains(n), "VEN-341.2: {n}");
    }
    for n in ["tokio", "serde_json"] {
        assert!(!normal.names.contains(n), "VEN-341.3: {n}");
    }
    let mut files = Vec::new();
    src_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    for f in &files {
        let s = std::fs::read_to_string(f).unwrap();
        assert!(!s.contains("libc::"), "VEN-341.4: {}", f.display());
    }
    assert!(normal.features["hft-journal"].contains("writer"), "VEN-341.5");
    let manifest = std::fs::read_to_string(MANIFEST).unwrap();
    let t: toml::Table = manifest.parse().unwrap();
    let j = &t["dependencies"]["hft-journal"];
    assert!(
        j["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f.as_str() == Some("writer")),
        "VEN-341.5 (normal dependency)"
    );
    let kill = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/kill.rs")).unwrap();
    assert!(
        kill.contains(
            "pub fn kill_admit(word: u32, mask: u32, acct_bits: u32, ix: u8, op: GwOp, oflags: u16) -> bool"
        ),
        "VEN-341.6"
    );
    assert!(!kill.contains("&self"), "VEN-341.6: pure function");
}
