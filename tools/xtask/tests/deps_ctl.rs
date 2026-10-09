//! crates/hft-ctl-proto/TESTS.md CTP-173 and crates/hft-ctl/TESTS.md CTL-161: dependency closures
//! of the control-plane crates (on top of `cargo xtask deps`, which checks the declared graph).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;

struct Meta {
    /// id → name
    names: BTreeMap<String, String>,
    /// id → (dep id, normal edge)
    edges: BTreeMap<String, Vec<(String, bool)>>,
    /// name → id (workspace members)
    members: BTreeMap<String, String>,
}

fn meta() -> Meta {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let v: Value = xtask::deps::metadata(&root).expect("cargo metadata");
    let names: BTreeMap<String, String> = v["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_owned(),
                p["name"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let members = v["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            let id = id.as_str().unwrap().to_owned();
            (names[&id].clone(), id)
        })
        .collect();
    let mut edges = BTreeMap::new();
    for n in v["resolve"]["nodes"].as_array().unwrap() {
        let to = n["deps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                let normal = d["dep_kinds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|k| k["kind"].is_null() || k["kind"] == "build");
                (d["pkg"].as_str().unwrap().to_owned(), normal)
            })
            .collect();
        edges.insert(n["id"].as_str().unwrap().to_owned(), to);
    }
    Meta {
        names,
        edges,
        members,
    }
}

impl Meta {
    /// Names in the closure of `name`: its normal and dev dependencies, then normal edges below.
    fn closure(&self, name: &str) -> BTreeSet<String> {
        let start = &self.members[name];
        let mut seen = BTreeSet::new();
        let mut stack: Vec<String> = self.edges[start].iter().map(|(d, _)| d.clone()).collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            for (d, normal) in &self.edges[&id] {
                if *normal {
                    stack.push(d.clone());
                }
            }
        }
        seen.iter().map(|id| self.names[id].clone()).collect()
    }

    /// Direct normal dependencies of a member.
    fn direct(&self, name: &str) -> BTreeSet<String> {
        self.edges[&self.members[name]]
            .iter()
            .filter(|(_, n)| *n)
            .map(|(d, _)| self.names[d].clone())
            .collect()
    }
}

const ENGINE_L1: &[&str] = &["hft-oms", "hft-risk", "hft-ledger", "hft-exec", "hft-book"];

#[test]
fn ctp_173_ctl_proto_dep_closure() {
    let m = meta();
    let direct = m.direct("hft-ctl-proto");
    let internal: BTreeSet<&str> = direct
        .iter()
        .filter(|n| m.members.contains_key(*n))
        .map(String::as_str)
        .collect();
    assert_eq!(
        internal,
        BTreeSet::from(["hft-types", "hft-refdata", "hft-event"])
    );
    let external: BTreeSet<&str> = direct
        .iter()
        .filter(|n| !m.members.contains_key(*n))
        .map(String::as_str)
        .collect();
    let allowed = BTreeSet::from(["serde", "serde_json", "uuid", "sha2", "base64", "zerocopy"]);
    assert!(external.is_subset(&allowed), "{external:?}");
    let closure = m.closure("hft-ctl-proto");
    for bad in ["hft-strategy-sdk", "hft-strategy-registry", "tokio"]
        .iter()
        .chain(ENGINE_L1)
    {
        assert!(!closure.contains(*bad), "{bad} in the hft-ctl-proto closure");
    }
    assert!(
        !closure
            .iter()
            .any(|n| m.members.contains_key(n) && n.starts_with("strategy-"))
    );
}

#[test]
fn ctl_161_ctl_dep_closure() {
    let m = meta();
    let closure = m.closure("hft-ctl");
    for bad in ["tokio", "hft-strategy-registry"] {
        assert!(!closure.contains(bad), "{bad} in the hft-ctl closure");
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let strategies: BTreeSet<String> = std::fs::read_dir(root.join("strategies"))
        .map(|d| {
            d.filter_map(Result::ok)
                .filter_map(|e| std::fs::read_to_string(e.path().join("Cargo.toml")).ok())
                .filter_map(|t| t.parse::<toml::Table>().ok())
                .filter_map(|t| t.get("package")?.get("name")?.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    for s in &strategies {
        assert!(!closure.contains(s), "strategy {s} in the hft-ctl closure");
    }
    // No hot crate depends on hft-ctl.
    for hot in [
        "hft-sequencer",
        "hft-venue",
        "hft-rt",
        "hft-oms",
        "hft-risk",
        "hft-exec",
        "hft-book",
        "hft-ledger",
    ] {
        if m.members.contains_key(hot) {
            assert!(!m.closure(hot).contains("hft-ctl"), "{hot} depends on hft-ctl");
        }
    }
    // Source references into hft_sequencer are only ParsedConfig.
    for e in walkdir::WalkDir::new(root.join("crates/hft-ctl/src"))
        .into_iter()
        .filter_map(Result::ok)
    {
        if e.path().extension().is_some_and(|x| x == "rs") {
            let t = std::fs::read_to_string(e.path()).unwrap();
            for (i, _) in t.match_indices("hft_sequencer::") {
                let rest = &t[i + "hft_sequencer::".len()..];
                assert!(
                    rest.starts_with("ParsedConfig"),
                    "{}: hft_sequencer::{}",
                    e.path().display(),
                    &rest[..rest.len().min(30)]
                );
            }
        }
    }
}
