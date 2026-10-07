//! NET-295: the normal dependency closure holds none of the forbidden crates.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

/// Forbidden names (README "依赖"): async runtimes, OpenSSL, JSON values, tracing, and the
/// crates above or beside hft-net.
fn forbidden(name: &str) -> bool {
    matches!(
        name,
        "tokio" | "mio" | "openssl" | "native-tls" | "serde_json" | "tracing" | "hft-venue" | "hft-journal"
    ) || name.starts_with("venue-")
}

/// The check: forbidden members of a closure, sorted.
fn violations(closure: &BTreeSet<String>) -> Vec<String> {
    closure.iter().filter(|n| forbidden(n)).cloned().collect()
}

/// Names in hft-net's normal (and build) dependency closure, from `cargo metadata`.
fn closure() -> BTreeSet<String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = std::process::Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--manifest-path", MANIFEST])
        .output()
        .expect("cargo metadata");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: BTreeMap<String, String> = v["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_string(),
                p["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let nodes = v["resolve"]["nodes"].as_array().unwrap();
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in nodes {
        let id = n["id"].as_str().unwrap().to_string();
        let mut to = Vec::new();
        for d in n["deps"].as_array().unwrap() {
            // Normal and build edges; `cfg(loom)` edges are test-only (tools/xtask/deps.toml `test_cfgs`).
            let normal = d["dep_kinds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| (k["kind"].is_null() || k["kind"] == "build") && k["target"] != "cfg(loom)");
            if normal {
                to.push(d["pkg"].as_str().unwrap().to_string());
            }
        }
        edges.insert(id, to);
    }
    let root = names
        .iter()
        .find(|(_, n)| *n == "hft-net")
        .map(|(id, _)| id.clone())
        .unwrap();
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.clone()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        stack.extend(edges.get(&id).cloned().unwrap_or_default());
    }
    seen.remove(&root);
    seen.iter().map(|id| names[id].clone()).collect()
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
fn net_295_forbidden_deps() {
    let cl = closure();
    let v = violations(&cl);
    for (row, names) in [
        ("NET-295.1", &["tokio", "mio"][..]),
        ("NET-295.2", &["openssl", "native-tls"]),
        ("NET-295.3", &["serde_json", "tracing"]),
        ("NET-295.4", &["hft-venue", "hft-journal"]),
    ] {
        for n in names {
            assert!(!cl.contains(*n), "{row}: {n} in the closure");
        }
    }
    assert!(!cl.iter().any(|n| n.starts_with("venue-")), "NET-295.4");
    assert!(v.is_empty(), "{v:?}");
    // NET-295.5: hft-event is in the closure and the sources use only its ConnRole.
    assert!(cl.contains("hft-event"), "NET-295.5");
    let mut files = Vec::new();
    src_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    for f in files {
        let s = std::fs::read_to_string(&f).unwrap();
        for (k, _) in s.match_indices("hft_event::") {
            assert!(
                s[k..].starts_with("hft_event::ConnRole"),
                "NET-295.5: {} uses hft_event beyond ConnRole",
                f.display()
            );
        }
    }
    // NET-295.6: a copy of the manifest with hft-journal added, through the same check.
    let text = std::fs::read_to_string(MANIFEST).unwrap();
    let copy = text.replace(
        "[dependencies]\n",
        "[dependencies]\nhft-journal.workspace = true\n",
    );
    let t: toml::Table = copy.parse().unwrap();
    let mut cl2 = cl.clone();
    for k in t["dependencies"].as_table().unwrap().keys() {
        cl2.insert(k.clone());
    }
    assert_eq!(violations(&cl2), vec!["hft-journal".to_string()], "NET-295.6");
}
