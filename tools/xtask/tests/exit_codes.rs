//! Exit-code and output conventions of every subcommand (tools/TESTS.md TOOL-001, TOOL-002).

use crate::common;

use common::Fixture;
use common::repos::{OMS_A, deps_clean, docs_clean, git, hot_clean, reverse_dep};

#[test]
fn tool_001_exit_code_convention() {
    // TOOL-001.1
    let f = deps_clean("ec-1");
    assert_eq!(f.xtask(&["deps"]).code, 0, "TOOL-001.1");
    // TOOL-001.2
    let f = reverse_dep("ec-2");
    assert_eq!(f.xtask(&["deps"]).code, 1, "TOOL-001.2");
    // TOOL-001.3
    let f = deps_clean("ec-3");
    f.remove("tools/xtask/deps.toml");
    assert_eq!(f.xtask(&["deps"]).code, 2, "TOOL-001.3");
    // TOOL-001.4: fixture `dyn-in-hot`.
    let f = hot_clean("ec-4");
    f.append(OMS_A, "\npub struct Cb {\n    pub f: Box<dyn Fn(u64)>,\n}\n");
    assert_eq!(f.xtask(&["hot-check"]).code, 1, "TOOL-001.4");
    // TOOL-001.5: fixture `bad-anchor`.
    let f = docs_clean("ec-5");
    f.append(
        "crates/hft-sequencer/README.md",
        "\n见 [OMS](../hft-oms/README.md#no-such-anchor)。\n",
    );
    assert_eq!(f.xtask(&["doc-check"]).code, 1, "TOOL-001.5");
    // TOOL-001.6
    let f = Fixture::empty("ec-6");
    let missing = f.path("nonexistent").display().to_string();
    assert_eq!(f.xtask(&["journal-verify", &missing]).code, 2, "TOOL-001.6");
    // TOOL-001.7: a real git repository (no recording executor).
    let f = deps_clean("ec-7");
    git(&f, &["init", "-q"]);
    git(&f, &["add", "-A"]);
    git(&f, &["commit", "-q", "-m", "init"]);
    let r = f.xtask(&[
        "decoupling-gate",
        "--base",
        "0000000000000000000000000000000000000000",
        "--head",
        "HEAD",
    ]);
    assert_eq!(r.code, 2, "TOOL-001.7: {}", r.stderr);
    // TOOL-001.8
    let f = Fixture::empty("ec-8");
    assert_eq!(f.xtask(&["test-trace"]).code, 2, "TOOL-001.8");
    // TOOL-001.9
    let f = Fixture::empty("ec-9");
    assert_eq!(
        f.xtask(&["gen-host", "--check", "--host", "nosuch"]).code,
        2,
        "TOOL-001.9"
    );
    // TOOL-001.10: cached copy whose SHA-256 differs from the pointer.
    let f = Fixture::empty("ec-10");
    f.write(
        "fixtures/replay/x.pointer.toml",
        "uri = \"s3://b/hft-fixtures/x/ba78.zst\"\n\
         sha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\nbytes = 3\nkind = \"seq-journal\"\n",
    );
    f.write("target/fixtures-cache/x", "abd");
    assert_eq!(
        f.xtask(&["replay-verify", "fixtures/replay/x.pointer.toml"]).code,
        2,
        "TOOL-001.10"
    );
}

#[test]
fn tool_002_json_and_stderr_summary() {
    let f = reverse_dep("ec-json");
    assert!(!f.path("target/xtask").exists());
    let r = f.xtask(&["deps"]);
    assert_eq!(r.code, 1);
    let j = r.json.expect("target/xtask/deps.json");
    let vs = j["violations"].as_array().unwrap();
    assert_eq!(vs.len(), 1, "{vs:?}");
    assert_eq!(vs[0]["from"], "hft-types");
    assert_eq!(vs[0]["to"], "hft-refdata");
    assert!(
        r.stderr
            .lines()
            .any(|l| l.contains("hft-types") && l.contains("hft-refdata")),
        "{}",
        r.stderr
    );
    assert!(r.stdout.is_empty(), "stdout: {}", r.stdout);
}
