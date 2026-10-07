//! VEN-341: dependency closure and source rules.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const MANIFEST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

struct Meta {
    names: BTreeMap<String, String>,
    edges: BTreeMap<String, Vec<(String, bool)>>,
    features: BTreeMap<String, Vec<String>>,
    root: String,
}

fn meta() -> Meta {
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
    let mut edges = BTreeMap::new();
    let mut features = BTreeMap::new();
    for n in v["resolve"]["nodes"].as_array().unwrap() {
        let id = n["id"].as_str().unwrap().to_string();
        let mut to = Vec::new();
        for d in n["deps"].as_array().unwrap() {
            let kinds = d["dep_kinds"].as_array().unwrap();
            let normal = kinds
                .iter()
                .any(|k| (k["kind"].is_null() || k["kind"] == "build") && k["target"] != "cfg(loom)");
            to.push((d["pkg"].as_str().unwrap().to_string(), normal));
        }
        edges.insert(id.clone(), to);
        features.insert(
            id,
            n["features"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_str().unwrap().to_string())
                .collect(),
        );
    }
    let root = names
        .iter()
        .find(|(_, n)| *n == "hft-venue")
        .map(|(id, _)| id.clone())
        .unwrap();
    Meta {
        names,
        edges,
        features,
        root,
    }
}

/// Closure from the root: normal edges only, or normal + dev (dev edges only from the root).
fn closure(m: &Meta, with_dev: bool) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![m.root.clone()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        for (to, normal) in m.edges.get(&id).cloned().unwrap_or_default() {
            if normal || (with_dev && id == m.root) {
                stack.push(to);
            }
        }
    }
    seen.remove(&m.root);
    seen.iter().map(|id| m.names[id].clone()).collect()
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
    let m = meta();
    let normal = closure(&m, false);
    let all = closure(&m, true);
    assert!(!all.iter().any(|n| n.starts_with("venue-")), "VEN-341.1");
    for n in ["hft-sequencer", "hft-oms", "hft-risk"] {
        assert!(!all.contains(n), "VEN-341.2: {n}");
    }
    for n in ["tokio", "serde_json"] {
        assert!(!normal.contains(n), "VEN-341.3: {n}");
    }
    let mut files = Vec::new();
    src_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    for f in &files {
        let s = std::fs::read_to_string(f).unwrap();
        assert!(!s.contains("libc::"), "VEN-341.4: {}", f.display());
    }
    let jid = m
        .names
        .iter()
        .find(|(_, n)| *n == "hft-journal")
        .map(|(id, _)| id.clone())
        .unwrap();
    assert!(m.features[&jid].iter().any(|f| f == "writer"), "VEN-341.5");
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
