//! Explicit case selection must reach its own assertions and preserve its identity on repro.
use std::process::Command;

#[test]
fn tk_146_cli_foreign_case_keeps_settlement_evidence_on_run_and_repro() {
    let tmp = hft_testkit::tmp::TmpDir::in_temp("hft-dst-foreign-cli").unwrap();
    let dir = tmp.path().to_path_buf();
    let call = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_hft-dst"))
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap()
    };
    let list = call(&["list"]);
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).contains("case LED-080 suite=core"));
    let failed = call(&[
        "run",
        "--case",
        "LED-080",
        "--seeds",
        "0..0",
        "--steps",
        "1",
        "--out",
        "short.json",
        "--crash-dir",
        "short",
    ]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("invariant=LED-080"));
    assert!(
        std::fs::read_to_string(dir.join("short/0000000000000000/repro.txt"))
            .unwrap()
            .contains("--case LED-080")
    );
    let pass = call(&[
        "run",
        "--case",
        "LED-080",
        "--seeds",
        "0..0",
        "--out",
        "pass.json",
        "--crash-dir",
        "ok",
    ]);
    assert!(pass.status.success(), "{}", String::from_utf8_lossy(&pass.stderr));
    let body = std::fs::read_to_string(dir.join("pass.json")).unwrap();
    for field in ["exchange_receipts", "foreign_orders", "ledger_inputs"] {
        assert!(body.contains(&format!("\"{field}\":[")) && !body.contains(&format!("\"{field}\":[]")));
    }
    assert!(body.contains("\"truth_unskipped\":true"));
    assert!(body.contains("\"journal_verification\":{\"through_seq\":"));
    assert!(!body.contains("\"inputs\":0") && !body.contains("\"outputs\":0"));
    let repro = call(&[
        "repro",
        "--case",
        "LED-080",
        "--seed",
        "0",
        "--out",
        "again.json",
        "--crash-dir",
        "again",
    ]);
    assert!(
        repro.status.success(),
        "{}",
        String::from_utf8_lossy(&repro.stderr)
    );
    let single = std::fs::read_to_string(dir.join("again.json")).unwrap();
    assert!(
        body.contains(&format!("\"runs\":[{single}]")),
        "run/repro evidence differs"
    );
}

#[test]
fn tk_143_cli_case_selection_and_repro_are_not_baseline_runs() {
    let tmp = hft_testkit::tmp::TmpDir::in_temp("hft-dst-cli").unwrap();
    let dir = tmp.path().to_path_buf();
    let bin = env!("CARGO_BIN_EXE_hft-dst");
    let call = |args: &[&str]| Command::new(bin).args(args).current_dir(&dir).output().unwrap();
    let list = call(&["list"]);
    assert!(list.status.success());
    assert!(String::from_utf8_lossy(&list.stdout).contains("case LED-081 suite=core"));
    for args in [
        vec!["run", "--case", "missing"],
        vec!["run", "--case", "LED-081", "--suite", "storage"],
        vec!["hashtrail", "--case", "LED-081", "--seed", "0"],
        vec!["run", "--case", "LED-081", "--seeds", "0..4096"],
    ] {
        assert_eq!(call(&args).status.code(), Some(2), "{args:?}");
    }
    let failed = call(&[
        "run",
        "--case",
        "LED-081",
        "--seeds",
        "0..0",
        "--steps",
        "1",
        "--out",
        "fail.json",
        "--crash-dir",
        "crashes",
    ]);
    assert_eq!(failed.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(
        stderr.contains("invariant=LED-081") && stderr.contains("--case LED-081"),
        "{stderr}"
    );
    let repro = std::fs::read_to_string(dir.join("crashes/0000000000000000/repro.txt")).unwrap();
    assert!(repro.contains("--case LED-081"));
    let again = call(&[
        "repro",
        "--case",
        "LED-081",
        "--suite",
        "core",
        "--seed",
        "0",
        "--steps",
        "1",
        "--crash-dir",
        "again",
    ]);
    assert_eq!(again.status.code(), Some(1));
    assert_eq!(failed.stderr, again.stderr);
    let passed = call(&[
        "run",
        "--case",
        "LED-081",
        "--seeds",
        "2..2",
        "--out",
        "pass.json",
        "--crash-dir",
        "ok",
    ]);
    assert!(
        passed.status.success(),
        "{}",
        String::from_utf8_lossy(&passed.stderr)
    );
    let body = std::fs::read_to_string(dir.join("pass.json")).unwrap();
    assert!(body.contains("\"case\":\"LED-081\"") && body.contains("\"seeds_run\":1"));
    assert!(body.contains("\"offset_notes\":1") && body.contains("\"truth_unskipped\":true"));
    let again = call(&[
        "repro",
        "--case",
        "LED-081",
        "--seed",
        "2",
        "--out",
        "again.json",
        "--crash-dir",
        "again-ok",
    ]);
    assert!(again.status.success());
    let single = std::fs::read_to_string(dir.join("again.json")).unwrap();
    assert!(
        body.contains(&format!("\"runs\":[{single}]")),
        "run/repro evidence differs"
    );
}

/// TK-180: `hft-dst run` reports the items each checker compared, summed over its seeds.
#[test]
fn tk_180_cli_reports_compared_items_per_invariant() {
    use hft_testkit::dst::invariants::CHECKERS;
    use hft_testkit::scenarios::{DstEngine, suites};
    let tmp = hft_testkit::tmp::TmpDir::in_temp("hft-dst-compared").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hft-dst"))
        .args([
            "run",
            "--seeds",
            "3..4",
            "--suite",
            "core",
            "--steps",
            "300",
            "--out",
            "r.json",
            "--crash-dir",
            "c",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let body = std::fs::read_to_string(tmp.path().join("r.json")).unwrap();
    let suite = suites().into_iter().find(|s| s.name == "core").unwrap();
    let mut want = [0u64; 32];
    for seed in 3..=4 {
        let mut c = suite.config(seed);
        c.steps = 300;
        let r = hft_testkit::dst::run_record::<DstEngine>(&c);
        for (w, n) in want.iter_mut().zip(r.compared) {
            *w += n;
        }
    }
    let fields: Vec<String> = CHECKERS
        .iter()
        .map(|k| format!("\"{}\":{}", k.id.code(), want[usize::from(k.id.0)]))
        .collect();
    assert!(
        body.contains(&format!("\"compared\":{{{}}}", fields.join(","))),
        "TK-180: {body}"
    );
    assert!(want[16] > 0 && want[1] > 0, "TK-180: the sample compared nothing");
}
