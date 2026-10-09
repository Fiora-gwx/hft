//! `cargo xtask new-strategy` (tools/TESTS.md TOOL-069–TOOL-071) on the `strat_repo` fixture; the
//! unit-test and `deps` runs go through the recording executor.

use crate::common;

use common::Script;
use common::repos::git;
use common::strat_repo::{status, strat_repo};

fn deps_of(toml: &str) -> Vec<String> {
    let t: toml::Table = toml.parse().unwrap();
    t["dependencies"].as_table().unwrap().keys().cloned().collect()
}

#[test]
fn tool_069_skeleton_builds() {
    let f = strat_repo("ns-069");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["new-strategy", "foo", "--kind-id", "5"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    for p in ["Cargo.toml", "src/lib.rs", "tests/scenario.rs", "README.md"] {
        assert!(f.path("strategies/foo").join(p).is_file(), "missing {p}");
    }
    assert!(!f.path("strategies/foo/src/model").exists(), "no Model skeleton");
    assert!(!f.path("strategies/foo/pipeline").exists(), "no pipeline/");
    assert_eq!(
        deps_of(&f.read("strategies/foo/Cargo.toml")),
        vec!["hft-strat-lib".to_string(), "hft-strategy-sdk".to_string()]
    );
    // Changed files: the crate, the workspace manifest, the registry list, its dependency and one
    // ledger row.
    let mut changed = status(&f, &[]);
    changed.retain(|p| !p.starts_with("strategies/foo/"));
    changed.sort();
    assert_eq!(
        changed,
        vec![
            "Cargo.toml",
            "strategies/registry/Cargo.toml",
            "strategies/registry/README.md",
            "strategies/registry/src/lib.rs"
        ]
    );
    for (p, want) in [
        ("strategies/registry/src/lib.rs", "1\t0\t"),
        ("strategies/registry/README.md", "1\t0\t"),
        ("strategies/registry/Cargo.toml", "1\t0\t"),
        ("Cargo.toml", "1\t0\t"),
    ] {
        let n = git(&f, &["diff", "--numstat", "--", p]);
        assert!(n.starts_with(want), "{p}: one added line, got {n}");
    }
    let lib = f.read("strategies/registry/src/lib.rs");
    let items: Vec<&str> = lib.lines().filter(|l| l.contains("::")).collect();
    assert_eq!(
        items.last().unwrap().trim(),
        "foo::Foo,",
        "last strategy_set! entry"
    );
    assert!(
        f.read("strategies/registry/README.md")
            .contains("| 5 | [foo](../foo/README.md) |")
    );
    assert_eq!(s.calls(), vec!["cargo test -p foo", "xtask deps"]);
}

#[test]
fn tool_070_model_and_pipeline() {
    let f = strat_repo("ns-070");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(
        &[
            "new-strategy",
            "foo",
            "--kind-id",
            "0x0005",
            "--with-model",
            "--with-pipeline",
        ],
        &s,
        &[],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let model = f.read("strategies/foo/src/model/mod.rs");
    assert!(
        model.contains("mem_bytes") && model.contains("snapshot_bytes"),
        "{model}"
    );
    assert!(f.path("strategies/foo/pipeline/pyproject.toml").is_file());
    assert!(
        f.path("strategies/foo/pipeline/src/hft_plugin_foo/__init__.py")
            .is_file()
    );
    let changed = status(&f, &[]);
    for p in &changed {
        for bad in ["crates/", "apps/", "tools/hft-cfg/", "python/"] {
            assert!(!p.starts_with(bad), "engine-side change {p}");
        }
    }
}

#[test]
fn tool_071_conflicts() {
    let f = strat_repo("ns-071");
    f.write("strategies/bar/README.md", "bar\n");
    git(&f, &["add", "-A"]);
    git(&f, &["commit", "-q", "-m", "bar"]);
    let rows: [(&str, &[&str], i32, &str); 5] = [
        ("TOOL-071.1", &["foo", "--kind-id", "3"], 1, "is 5"),
        (
            "TOOL-071.2",
            &["bar", "--kind-id", "5"],
            1,
            "strategies/bar/ already exists",
        ),
        ("TOOL-071.3", &["foo"], 2, "--kind-id"),
        ("TOOL-071.4", &["foo", "--kind-id", "4"], 1, "is 5"),
        ("TOOL-071.5", &["foo", "--kind-id", "6"], 1, "is 5"),
    ];
    for (id, args, want, msg) in rows {
        let s = Script::new(&f, &[], 0);
        let mut a = vec!["new-strategy"];
        a.extend_from_slice(args);
        let r = f.xtask_script(&a, &s, &[]);
        assert_eq!(r.code, want, "{id}: {}", r.stderr);
        assert!(r.stderr.contains(msg), "{id}: `{msg}` not in {}", r.stderr);
        assert_eq!(status(&f, &[]), Vec::<String>::new(), "{id}: nothing written");
        assert!(s.calls().is_empty(), "{id}: nothing run");
    }
}
