//! Dependency closure (EVT-123): only hft-types and hft-refdata among workspace crates (normal,
//! build and dev edges of hft-event); no serde / SBE / SDK / L1+ crates in the normal + build closure.

use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::process::Command;

#[test]
fn evt_123_dependency_closure() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let m: Value = serde_json::from_slice(&out.stdout).unwrap();
    let name: HashMap<&str, &str> = m["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let ws: BTreeSet<&str> = m["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| name[v.as_str().unwrap()])
        .collect();
    let nodes: HashMap<&str, &Value> = m["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n))
        .collect();
    let root = *name.iter().find(|(_, n)| **n == "hft-event").unwrap().0;
    // deps of `id` through edges of the given kinds (null = normal).
    let edges = |id: &str, kinds: &[Option<&str>]| -> Vec<String> {
        nodes[id]["deps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| {
                d["dep_kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| kinds.contains(&k["kind"].as_str()))
            })
            .map(|d| d["pkg"].as_str().unwrap().to_string())
            .collect()
    };
    let closure = |start: Vec<String>| {
        let mut seen = BTreeSet::new();
        let mut stack = start;
        while let Some(id) = stack.pop() {
            if seen.insert(id.clone()) {
                stack.extend(edges(&id, &[None, Some("build")]));
            }
        }
        seen
    };
    let prod = closure(edges(root, &[None, Some("build")]));
    let all = closure(edges(root, &[None, Some("build"), Some("dev")]));
    let ws_in: BTreeSet<&str> = all
        .iter()
        .map(|id| name[id.as_str()])
        .filter(|n| ws.contains(n))
        .collect();
    assert_eq!(ws_in, BTreeSet::from(["hft-refdata", "hft-types"]));
    let forbidden = [
        "hft-strategy-sdk",
        "hft-strategy-registry",
        "hft-book",
        "hft-oms",
        "hft-risk",
        "hft-ledger",
        "hft-exec",
        "hft-sequencer",
        "hft-rt",
        "hft-journal",
        "hft-net",
        "hft-venue",
        "hft-venue-proto",
    ];
    for id in &all {
        let n = name[id.as_str()];
        assert!(!forbidden.contains(&n), "{n} in the closure");
        let path = m["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id.as_str())
            .unwrap()["manifest_path"]
            .as_str()
            .unwrap()
            .replace('\\', "/");
        assert!(!path.contains("/strategies/"), "{n} is a strategy crate");
    }
    for id in &prod {
        let n = name[id.as_str()];
        assert!(
            !n.starts_with("serde") && !n.contains("sbe"),
            "{n} in the normal/build closure"
        );
    }
}
