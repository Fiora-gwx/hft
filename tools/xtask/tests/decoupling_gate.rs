//! `cargo xtask decoupling-gate` (tools/TESTS.md TOOL-012–TOOL-019). git and GitHub answers come from
//! the recording executor; deps.toml `[layers]` is the fixture `layers` version.

use crate::common;

use common::{Fixture, Run, Script};

const PULL: &str = "gh api -H Accept: application/vnd.github+json repos/o/r/pulls/17";
const REVIEWS: &str = "gh api -H Accept: application/vnd.github+json repos/o/r/pulls/17/reviews?per_page=100";
const LABELED: &str = r#"{"number":17,"labels":[{"name":"engine-change"}]}"#;
const APPROVED: &str = r#"[{"user":{"login":"owner1"},"state":"APPROVED"}]"#;

/// Fixture `layers`.
fn layers(tag: &str) -> Fixture {
    let f = Fixture::empty(tag);
    f.write(
        "tools/xtask/deps.toml",
        "[layers]\nengine = [\"crates/*\"]\napp = [\"apps/*\"]\nstrategy = [\"strategies/*\"]\ntool = [\"tools/*\"]\n",
    );
    f.write(".github/hft-admins.txt", "owner1\n");
    f
}

/// Runs the gate on `files` with GitHub answers `pull`/`reviews` (`None` = no `--pr`).
fn gate(
    f: &Fixture,
    files: &[&str],
    pr: Option<(&str, &str)>,
    extra: &[(&str, i32, &str, &str)],
) -> (Run, Script) {
    let diff = files.join("\n") + "\n";
    let mut rules: Vec<(&str, i32, &str, &str)> = extra.to_vec();
    rules.push(("git rev-parse", 0, "", ""));
    rules.push(("git diff --name-only B..H", 0, &diff, ""));
    if let Some((p, r)) = pr {
        rules.push((REVIEWS, 0, r, ""));
        rules.push((PULL, 0, p, ""));
    }
    let s = Script::new(f, &rules, 0);
    let summary = f.path("summary.md").display().to_string();
    let mut args = vec!["decoupling-gate", "--base", "B", "--head", "H"];
    if pr.is_some() {
        args.extend(["--pr", "17"]);
    }
    let r = f.xtask_script(
        &args,
        &s,
        &[
            ("GITHUB_TOKEN", Some("t")),
            ("GH_TOKEN", None),
            ("GITHUB_REPOSITORY", Some("o/r")),
            ("GITHUB_STEP_SUMMARY", Some(&summary)),
        ],
    );
    (r, s)
}

const MIXED: [&str; 2] = ["strategies/x/src/lib.rs", "crates/hft-oms/src/lib.rs"];

#[test]
fn tool_012_classify_diff() {
    let rows: [(&str, &[&str], i32); 9] = [
        ("TOOL-012.1", &["strategies/x/src/lib.rs"], 0),
        ("TOOL-012.2", &MIXED, 1),
        (
            "TOOL-012.3",
            &["strategies/registry/src/lib.rs", "apps/hft-core/src/main.rs"],
            1,
        ),
        (
            "TOOL-012.4",
            &[
                "strategies/x/pipeline/x_plugin/plugin.py",
                "apps/watchtower/src/main.rs",
            ],
            1,
        ),
        (
            "TOOL-012.5",
            &["strategies/x/src/lib.rs", "crates/hft-testkit/src/lib.rs"],
            1,
        ),
        ("TOOL-012.6", &["crates/hft-oms/src/lib.rs"], 0),
        (
            "TOOL-012.7",
            &["strategies/x/README.md", "tools/xtask/src/deps.rs"],
            0,
        ),
        ("TOOL-012.8", &["strategies/x/src/lib.rs", "docs/testing.md"], 0),
        (
            "TOOL-012.9",
            &["strategies/x/src/lib.rs", "python/pipeline/README.md"],
            0,
        ),
    ];
    for (row, files, code) in rows {
        let f = layers(&format!("dg-{}", row.replace('.', "-")));
        let (r, _) = gate(&f, files, None, &[]);
        assert_eq!(r.code, code, "{row}: {}", r.stderr);
        if row == "TOOL-012.2" {
            let j = r.json.unwrap();
            assert_eq!(
                j["strategies"],
                serde_json::json!(["strategies/x/src/lib.rs"]),
                "{row}"
            );
            assert_eq!(
                j["engine"],
                serde_json::json!(["crates/hft-oms/src/lib.rs"]),
                "{row}"
            );
        }
    }
}

#[test]
fn tool_013_admin_exemption() {
    let f = layers("dg-013");
    let (r, _) = gate(&f, &MIXED, Some((LABELED, APPROVED)), &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let j = r.json.unwrap();
    assert_eq!(
        j["exemption"],
        serde_json::json!({"pr": 17, "approver": "owner1", "engine": ["crates/hft-oms"], "apps": []})
    );
    let s = f.read("summary.md");
    for w in ["17", "owner1", "hft-oms"] {
        assert!(s.contains(w), "summary lacks {w}: {s}");
    }
}

#[test]
fn tool_014_label_without_review_fails() {
    let f = layers("dg-014");
    let (r, _) = gate(&f, &MIXED, Some((LABELED, "[]")), &[]);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.json.unwrap().get("exemption").is_none());
    let s = f.read("summary.md");
    assert!(!s.contains("豁免") && !s.contains("exemption"), "{s}");
}

#[test]
fn tool_015_non_admin_approval_fails() {
    let f = layers("dg-015");
    let rev = r#"[{"user":{"login":"dev2user"},"state":"APPROVED"}]"#;
    let (r, _) = gate(&f, &MIXED, Some((LABELED, rev)), &[]);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(
        r.stderr
            .lines()
            .any(|l| l.contains("dev2user") && l.contains("不在 Admin 名单")),
        "{}",
        r.stderr
    );
    assert!(r.json.unwrap().get("exemption").is_none());
}

#[test]
fn tool_016_approval_without_label_fails() {
    let f = layers("dg-016");
    let (r, _) = gate(&f, &MIXED, Some((r#"{"number":17,"labels":[]}"#, APPROVED)), &[]);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.json.unwrap().get("exemption").is_none());
}

#[test]
fn tool_017_comment_review_fails() {
    let f = layers("dg-017");
    let rev = r#"[{"user":{"login":"owner1"},"state":"COMMENTED"}]"#;
    let (r, _) = gate(&f, &MIXED, Some((LABELED, rev)), &[]);
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.json.unwrap().get("exemption").is_none());
}

#[test]
fn tool_018_reviews_unreadable() {
    // TOOL-018.1: no token.
    let f = layers("dg-018-1");
    let diff = MIXED.join("\n");
    let s = Script::new(&f, &[("git rev-parse", 0, "", ""), ("git diff", 0, &diff, "")], 0);
    let r = f.xtask_script(
        &["decoupling-gate", "--base", "B", "--head", "H", "--pr", "17"],
        &s,
        &[
            ("GITHUB_TOKEN", None),
            ("GH_TOKEN", None),
            ("GITHUB_REPOSITORY", Some("o/r")),
        ],
    );
    assert_eq!(r.code, 2, "TOOL-018.1: {}", r.stderr);
    assert!(r.stderr.contains("GITHUB_TOKEN"), "TOOL-018.1: {}", r.stderr);
    // TOOL-018.2 / 018.3: API errors.
    for (row, msg) in [
        ("TOOL-018.2", "gh: Server Error (HTTP 500)"),
        ("TOOL-018.3", "gh: Not Found (HTTP 404)"),
    ] {
        let f = layers(&format!("dg-{}", row.replace('.', "-")));
        let (r, _) = gate(&f, &MIXED, Some((LABELED, APPROVED)), &[(REVIEWS, 1, "", msg)]);
        assert_eq!(r.code, 2, "{row}: {}", r.stderr);
    }
    // TOOL-018.4: --head does not exist.
    let f = layers("dg-018-4");
    let (r, _) = gate(
        &f,
        &MIXED,
        None,
        &[("git rev-parse --verify --quiet H^{commit}", 128, "", "")],
    );
    assert_eq!(r.code, 2, "TOOL-018.4: {}", r.stderr);
}

#[test]
fn tool_019_never_builds() {
    let f = layers("dg-019");
    let (r, s) = gate(&f, &MIXED, Some((LABELED, APPROVED)), &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let progs = s.programs();
    assert!(!progs.is_empty());
    assert!(progs.iter().all(|p| p == "git" || p == "gh"), "{progs:?}");
    assert!(!s.calls().iter().any(|c| c.starts_with("cargo build")
        || c.starts_with("cargo test")
        || c.starts_with("cargo check")));
}

// ------------------------------------------------------------------------ SDK-006–SDK-009
// crates/hft-strategy-sdk/TESTS.md: PRs without label and reviews (`--pr n`, empty review list).

/// Runs the gate on `files` as PR `pr` with no label and no reviews; also returns the step summary.
fn gate_pr(tag: &str, files: &[&str], pr: u32) -> (Run, String) {
    let f = layers(tag);
    let diff = files.join("\n") + "\n";
    let pull = format!("gh api -H Accept: application/vnd.github+json repos/o/r/pulls/{pr}");
    let reviews = format!("{pull}/reviews?per_page=100");
    let pull_json = format!("{{\"number\":{pr},\"labels\":[]}}");
    let s = Script::new(
        &f,
        &[
            ("git rev-parse", 0, "", ""),
            ("git diff --name-only B..H", 0, &diff, ""),
            (&reviews, 0, "[]", ""),
            (&pull, 0, &pull_json, ""),
        ],
        0,
    );
    let summary = f.path("summary.md").display().to_string();
    let pr = pr.to_string();
    let r = f.xtask_script(
        &["decoupling-gate", "--base", "B", "--head", "H", "--pr", &pr],
        &s,
        &[
            ("GITHUB_TOKEN", Some("t")),
            ("GH_TOKEN", None),
            ("GITHUB_REPOSITORY", Some("o/r")),
            ("GITHUB_STEP_SUMMARY", Some(&summary)),
        ],
    );
    (r, std::fs::read_to_string(&summary).unwrap_or_default())
}

#[test]
fn sdk_006_sdk_and_strategy_same_pr_rejected() {
    let sdk = "crates/hft-strategy-sdk/src/ctx.rs";
    let strat = "strategies/latency-probe/src/fast/mod.rs";
    let (r, _) = gate_pr("dg-sdk-006", &[sdk, strat], 1);
    assert_eq!(r.code, 1, "{}", r.stderr);
    for f in [sdk, strat] {
        assert!(
            r.stderr.lines().any(|l| l.trim() == f),
            "{f} not listed:\n{}",
            r.stderr
        );
    }
    let j = r.json.unwrap();
    assert_eq!(j["engine"], serde_json::json!([sdk]));
    assert_eq!(j["strategies"], serde_json::json!([strat]));
    assert!(j.get("exemption").is_none());
}

#[test]
fn sdk_007_strategy_only_pr_passes() {
    let mut files: Vec<String> = (0..12).map(|i| format!("strategies/foo/src/m{i}.rs")).collect();
    files.extend(
        [
            "strategies/registry/src/lib.rs",
            "strategies/registry/Cargo.toml",
            "fixtures/strategy-schemas/7-foo.schema.json",
            "fixtures/strategy-schemas/index.json",
        ]
        .map(str::to_string),
    );
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let (r, summary) = gate_pr("dg-sdk-007", &files, 2);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(r.json.unwrap().get("exemption").is_none());
    assert!(
        !summary.contains("豁免") && !summary.contains("exemption"),
        "{summary}"
    );
}

#[test]
fn sdk_008_sdk_only_pr_passes() {
    let (r, _) = gate_pr(
        "dg-sdk-008",
        &[
            "crates/hft-strategy-sdk/src/views.rs",
            "crates/hft-strategy-sdk/README.md",
            "crates/hft-strategy-sdk/TESTS.md",
        ],
        3,
    );
    assert_eq!(r.code, 0, "{}", r.stderr);
}

#[test]
fn sdk_009_capacity_change_with_strategy_rejected() {
    let (r, _) = gate_pr(
        "dg-sdk-009",
        &["strategies/foo/src/params.rs", "apps/hft-core/README.md"],
        4,
    );
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(
        r.stderr.lines().any(|l| l.trim() == "apps/hft-core/README.md"),
        "{}",
        r.stderr
    );
    assert_eq!(
        r.json.unwrap()["apps"],
        serde_json::json!(["apps/hft-core/README.md"])
    );
}
