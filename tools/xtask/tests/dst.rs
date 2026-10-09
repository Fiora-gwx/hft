//! `cargo xtask dst` (tools/TESTS.md TOOL-047–TOOL-049, TOOL-051, TOOL-053). The fixture holds the
//! DST framework directory; the recording executor plays `cargo build` of `hft-dst` and the runner
//! (`hft-dst list` prints the suites).

mod common;

use common::repos::git;
use common::{Fixture, Script};

fn dst_repo(tag: &str) -> Fixture {
    let f = Fixture::empty(tag);
    f.write("crates/hft-testkit/src/dst/mod.rs", "");
    f.write(".gitignore", "target/\n");
    git(&f, &["init", "-q"]);
    git(&f, &["add", "-A"]);
    git(&f, &["commit", "-q", "-m", "A"]);
    f
}

fn runner(f: &Fixture) -> String {
    let exe = if cfg!(windows) { "hft-dst.exe" } else { "hft-dst" };
    f.path("target").join("ci").join(exe).display().to_string()
}

/// Recording executor: `hft-dst list` prints `suites`, `hft-dst run` exits `run_code`.
fn script(f: &Fixture, suites: &[&str], run_code: i32) -> Script {
    let list: String = suites.iter().map(|s| format!("suite {s} faults=…\n")).collect();
    let l = format!("{} list", runner(f));
    let r = format!("{} run", runner(f));
    Script::new(
        f,
        &[(l.as_str(), 0, list.as_str(), ""), (r.as_str(), run_code, "", "")],
        0,
    )
}

fn dst(f: &Fixture, s: &Script, args: &[&str]) -> common::Run {
    let t = f.path("target").display().to_string();
    let mut a = vec!["dst"];
    a.extend_from_slice(args);
    f.xtask_script(&a, s, &[("CARGO_TARGET_DIR", Some(&t))])
}

fn seeds(r: &common::Run) -> Vec<u64> {
    r.json.as_ref().unwrap()["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u64::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap())
        .collect()
}

#[test]
fn tool_047_bad_args() {
    let f = dst_repo("dst-047");
    let rows: [(&str, &[&str]); 5] = [
        ("TOOL-047.1", &[]),
        ("TOOL-047.2", &["--seeds", "1", "--suite", "nosuch"]),
        ("TOOL-047.3", &["--hashtrail", "check", "--seeds", "10"]),
        ("TOOL-047.4", &["--hashtrail", "check", "--suite", "core"]),
        ("TOOL-047.5", &["--hashtrail", "compare"]),
    ];
    for (id, args) in rows {
        let s = script(&f, &["core", "modes"], 0);
        let r = dst(&f, &s, args);
        assert_eq!(r.code, 2, "{id}: {}", r.stderr);
        assert!(
            !s.calls()
                .iter()
                .any(|c| c.contains(" run ") || c.contains(" repro ")),
            "{id}: no seed ran"
        );
    }
}

#[test]
fn tool_048_default_seed_from_sha() {
    let f = dst_repo("dst-048");
    let s = script(&f, &["core", "modes"], 0);
    let a1 = dst(&f, &s, &["--seeds", "3"]);
    let a2 = dst(&f, &s, &["--seeds", "3"]);
    assert_eq!(a1.code, 0, "{}", a1.stderr);
    let (x, y) = (seeds(&a1), seeds(&a2));
    assert_eq!(x, y);
    assert_eq!(x.len(), 3);
    assert_eq!((x[1], x[2]), (x[0] + 1, x[0] + 2), "consecutive");
    f.write("b.txt", "B\n");
    git(&f, &["add", "-A"]);
    git(&f, &["commit", "-q", "-m", "B"]);
    let b = dst(&f, &s, &["--seeds", "3"]);
    assert_eq!(b.code, 0, "{}", b.stderr);
    assert_ne!(seeds(&b)[0], x[0], "commit B derives another start");
}

#[test]
fn tool_049_single_seed() {
    let f = dst_repo("dst-049");
    let s = script(&f, &["core", "modes"], 0);
    let r = dst(
        &f,
        &s,
        &["--seeds", "1", "--seed", "0x9e3779b97f4a7c15", "--steps", "500"],
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
    let j = r.json.unwrap();
    assert_eq!(j["seeds"], serde_json::json!(["0x9e3779b97f4a7c15"]));
    assert_eq!(j["steps"], 500);
    let runs: Vec<String> = s.calls().into_iter().filter(|c| c.contains(" run ")).collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let want = format!(" run --seeds {0}..{0} --steps 500 ", 0x9e37_79b9_7f4a_7c15_u64);
    assert!(runs[0].contains(&want), "{}", runs[0]);
}

#[test]
fn tool_051_regressions_in_own_suite() {
    let f = dst_repo("dst-051");
    f.write(
        "fixtures/dst-seeds/core.txt",
        "7 2026-10-01 .agents/notes/x.md\n9 2026-10-02 #12\n",
    );
    f.write(
        "fixtures/dst-seeds/modes.txt",
        "# seed date link\n11 2026-10-03 #13\n",
    );
    let s = script(&f, &["core", "modes", "storage"], 0);
    let r = dst(&f, &s, &["--seeds", "2", "--seed", "100", "--regressions"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(seeds(&r), vec![100, 101, 7, 9, 11]);
    let j = r.json.unwrap();
    let runs = j["runs"].as_array().unwrap();
    let pair = |i: usize| {
        (
            runs[i]["suite"].as_str().unwrap(),
            runs[i]["source"].as_str().unwrap(),
        )
    };
    assert_eq!(pair(2), ("core", "regression"));
    assert_eq!(pair(3), ("core", "regression"));
    assert_eq!(pair(4), ("modes", "regression"));
    for i in 0..2 {
        let derived = ["core", "modes", "storage"][hft_testkit::seed::suite_index(100 + i as u64, 3)];
        assert_eq!(pair(i), (derived, "range"), "seed {}", 100 + i);
    }
    let calls = s.calls();
    for (seed, suite) in [(7u64, "core"), (9, "core"), (11, "modes")] {
        let want = format!(" repro --seed {seed:#018x} --suite {suite} ");
        assert!(calls.iter().any(|c| c.contains(&want)), "{want} not in {calls:?}");
    }
}

#[test]
fn tool_053_never_writes_ledger() {
    let f = dst_repo("dst-053");
    let home = Fixture::empty("dst-053-home");
    let before = files(&f.root);
    let s = script(&f, &["core"], 1);
    let h = home.root.display().to_string();
    let t = f.path("target").display().to_string();
    let r = f.xtask_script(
        &["dst", "--seeds", "5", "--seed", "1"],
        &s,
        &[
            ("CARGO_TARGET_DIR", Some(&t)),
            ("HOME", Some(&h)),
            ("HFT_VERIFY_STATE", None),
        ],
    );
    assert_eq!(r.code, 1, "a failing seed fails the run: {}", r.stderr);
    let after: Vec<String> = files(&f.root)
        .into_iter()
        .filter(|p| !p.starts_with("target/"))
        .collect();
    assert_eq!(after, before, "no new file outside target/");
    assert!(files(&home.root).is_empty(), "nothing under HOME");
    for p in files(&f.root) {
        assert!(!p.ends_with("ledger.json") && !p.contains("runs/"), "{p}");
    }
}

/// Files under `root` (relative, `/`-separated), `.git/` excluded.
fn files(root: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| xtask::rel_path(root, e.path()))
        .filter(|p| !p.starts_with(".git/"))
        .collect();
    v.sort();
    v
}
