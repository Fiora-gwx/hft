//! Explicit case selection must reach its own assertions and preserve its identity on repro.
use std::process::Command;

#[test]
fn tk_146_cli_foreign_case_keeps_settlement_evidence_on_run_and_repro() {
    let dir = std::env::temp_dir().join(format!("hft-dst-foreign-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
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
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn tk_143_cli_case_selection_and_repro_are_not_baseline_runs() {
    let dir = std::env::temp_dir().join(format!("hft-dst-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
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
    std::fs::remove_dir_all(&dir).unwrap();
}
