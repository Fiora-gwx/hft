//! `cargo xtask ci` (tools/TESTS.md TOOL-003–TOOL-011, TOOL-186). Every step
//! runs through the recording executor; the fixture holds a workspace whose components make every
//! step run.

use crate::common;

use common::{Fixture, Script};

const SCOPE: &str = "tools/xtask/scope.toml";

fn member(f: &Fixture, dir: &str, name: &str, tier: &str) {
    f.write(
        &format!("{dir}/Cargo.toml"),
        &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    );
    f.write(&format!("{dir}/src/lib.rs"), "");
    f.write(
        &format!("{dir}/README.md"),
        &format!(
            "---\ndescription: \"x\"\nkind: crate\ntier: {tier}\nscope: core\nstatus: impl\n---\n\n# {name}\n"
        ),
    );
}

/// Workspace `a` (pure, golden tests) + `hft-testkit` with the DST framework; scope.toml lists hft-amm.
fn ci_repo(tag: &str) -> Fixture {
    let f = Fixture::empty(tag);
    f.write(
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/a\", \"crates/hft-testkit\"]\n",
    );
    member(&f, "crates/a", "a", "pure");
    f.write("crates/a/tests/golden.rs", "");
    member(&f, "crates/hft-testkit", "hft-testkit", "dev");
    f.write("crates/hft-testkit/src/dst/mod.rs", "");
    f.write(SCOPE, "optional = [\"crates/hft-amm\"]\n");
    f
}

const STEPS: [&str; 13] = [
    "cargo fmt --all --check",
    "cargo clippy --profile ci --workspace --all-targets --all-features -- -D warnings",
    "cargo deny check licenses bans",
    "xtask deps",
    "xtask hot-check",
    "xtask fixtures check",
    "cargo test --profile ci --workspace --all-features --no-fail-fast",
    "cargo test --profile ci --workspace --all-features --test golden",
    "xtask dst --seeds 200 --regressions",
    "xtask dst --seeds 200 --regressions --case LED-081",
    "xtask dst --hashtrail check --first 50",
    "xtask gen-schema --check",
    "xtask doc-check",
];

#[test]
fn tool_003_stops_at_first_failure() {
    let f = ci_repo("ci-003");
    let s = Script::new(&f, &[("xtask deps", 1, "", "")], 0);
    let r = f.xtask_script(&["ci"], &s, &[]);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(s.calls(), STEPS[..4].to_vec());
    assert!(!s.calls().iter().any(|c| c.contains("hot-check")));
}

#[test]
fn tool_004_runs_twelve_steps_in_order() {
    let f = ci_repo("ci-004");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(s.calls(), STEPS.to_vec());
    let j = r.json.unwrap();
    assert_eq!(j["steps"].as_array().unwrap().len(), 12);
    assert_eq!(j["result"], "pass");
}

#[test]
fn tool_005_pure_only_selects_pure() {
    let f = Fixture::empty("ci-005");
    f.write(
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/p1\", \"crates/h1\", \"crates/c1\"]\n",
    );
    member(&f, "crates/p1", "p1", "pure");
    member(&f, "crates/h1", "h1", "hot");
    member(&f, "crates/c1", "c1", "cold");
    for (p, t) in [("py1", "pure"), ("py2", "cold")] {
        f.write(
            &format!("python/{p}/README.md"),
            &format!(
                "---\ndescription: \"x\"\nkind: python\ntier: {t}\nscope: core\nstatus: impl\n---\n\n# {p}\n"
            ),
        );
        f.write(
            &format!("python/{p}/tests/test_x.py"),
            "def test_x():\n    pass\n",
        );
    }
    f.write(SCOPE, "optional = []\n");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci", "--pure-only"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let calls = s.calls();
    let test = calls
        .iter()
        .find(|c| c.starts_with("cargo test --profile ci"))
        .expect("unit step");
    let ps: Vec<&str> = test
        .split(' ')
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| w[0] == "-p")
        .map(|w| w[1])
        .collect();
    assert_eq!(ps, vec!["p1"], "{test}");
    let py = calls
        .iter()
        .find(|c| c.contains("-m pytest"))
        .expect("python step");
    assert!(py.ends_with("python/py1/tests"), "{py}");
    for c in &calls {
        for bad in ["h1", "c1", "py2"] {
            assert!(
                !c.split(' ').any(|w| w == bad || w.contains(&format!("/{bad}/"))),
                "{c}"
            );
        }
    }
}

#[test]
fn tool_006_fast_skips_dst_and_golden() {
    let f = ci_repo("ci-006");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci", "--fast"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let want: Vec<&str> = STEPS
        .iter()
        .copied()
        .filter(|c| !c.contains("--test golden") && !c.starts_with("xtask dst"))
        .collect();
    assert_eq!(want.len(), 9);
    assert_eq!(s.calls(), want);
}

#[test]
fn tool_186_no_doc_check_skips_doc_check() {
    let f = ci_repo("ci-186");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci", "--no-doc-check"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(s.calls(), STEPS[..12].to_vec());
    assert!(r.stderr.contains("doc-check: skipped"), "{}", r.stderr);
}

#[test]
fn tool_007_never_runs_advisories() {
    let f = ci_repo("ci-007");
    f.write("Cargo.lock", "# changed lock\nversion = 4\n");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let deny: Vec<String> = s
        .calls()
        .into_iter()
        .filter(|c| c.starts_with("cargo deny"))
        .collect();
    assert_eq!(deny, vec!["cargo deny check licenses bans".to_string()]);
    assert!(!s.calls().iter().any(|c| c.contains("advisories")));
}

#[test]
fn tool_008_no_path_triggered_steps() {
    let f = ci_repo("ci-008");
    f.write("crates/hft-oms/src/lib.rs", "// changed\n");
    f.write("crates/venue-okx/src/decode.rs", "// changed\n");
    f.write("benchmarks/README.md", "# changed\n");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    for c in s.calls() {
        for bad in [
            "scaffold-gate",
            "fuzz",
            "replay-verify",
            "bench-gate",
            "decoupling-gate",
        ] {
            assert!(!c.split(' ').any(|w| w == bad), "{c}");
        }
    }
}

#[test]
fn tool_010_optional_absent_passes() {
    let f = ci_repo("ci-010");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        !r.stderr
            .lines()
            .any(|l| l.contains("hft-amm") && l.to_lowercase().contains("error")),
        "{}",
        r.stderr
    );
    let j = r.json.unwrap();
    for st in j["steps"].as_array().unwrap() {
        assert_ne!(st["status"], "fail", "{st}");
    }
    let test = s
        .calls()
        .into_iter()
        .find(|c| c.starts_with("cargo test --profile ci --workspace"))
        .unwrap();
    assert!(!test.contains("hft-amm"), "{test}");
}

#[test]
fn tool_011_optional_present_included() {
    let f = ci_repo("ci-011");
    f.write(
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/a\", \"crates/hft-testkit\", \"crates/hft-amm\"]\n",
    );
    member(&f, "crates/hft-amm", "hft-amm", "pure");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let test = s
        .calls()
        .into_iter()
        .find(|c| c.starts_with("cargo test --profile ci"))
        .unwrap();
    assert!(
        test.split(' ').any(|w| w == "--workspace") && !test.contains("--exclude"),
        "{test}"
    );
    let j = r.json.unwrap();
    assert_eq!(j["optional"][0]["in_workspace"], true);
}

#[test]
fn tool_009_dst_smoke_args() {
    // ci passes no start seed: `dst` derives it from the HEAD commit itself.
    let f = ci_repo("ci-009");
    let s = Script::new(&f, &[], 0);
    let r = f.xtask_script(&["ci"], &s, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let calls = s.calls();
    let i = calls
        .iter()
        .position(|c| c.starts_with("xtask dst --seeds"))
        .expect("dst smoke step");
    assert_eq!(calls[i], "xtask dst --seeds 200 --regressions");
    assert!(!calls[i].contains("--seed "), "the smoke start is commit-derived");
    assert_eq!(calls[i + 1], "xtask dst --hashtrail check --first 50");
}
