//! TOOL-176: `decoupling-gate` time on a 2,000-file diff with an Admin exemption (tools/README.md
//! "延迟与资源契约": ≤ 1 min; it never builds). The recording executor answers git and GitHub.

use criterion::{Criterion, criterion_group, criterion_main};
use xtask::decoupling_gate::{GateEnv, run_with};
use xtask::exec::{Output, Rule, ScriptExec};

fn rule(prefix: &str, stdout: String) -> Rule {
    Rule {
        prefix: prefix.into(),
        out: Output {
            code: 0,
            stdout,
            stderr: String::new(),
        },
    }
}

fn tool_176_decoupling_gate_time(c: &mut Criterion) {
    let root = std::env::temp_dir().join(format!("xtask-bench-dg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("tools/xtask")).unwrap();
    std::fs::create_dir_all(root.join(".github")).unwrap();
    std::fs::write(
        root.join("tools/xtask/deps.toml"),
        "[layers]\nengine = [\"crates/*\"]\napp = [\"apps/*\"]\nstrategy = [\"strategies/*\"]\ntool = [\"tools/*\"]\n",
    )
    .unwrap();
    std::fs::write(root.join(".github/hft-admins.txt"), "owner1\n").unwrap();
    let mut diff = String::new();
    for i in 0..1000 {
        diff += &format!(
            "crates/c{}/src/m{i}.rs\nstrategies/s{}/src/m{i}.rs\n",
            i % 40,
            i % 7
        );
    }
    let rules = vec![
        rule("git rev-parse", String::new()),
        rule("git diff", diff),
        rule(
            "gh api -H Accept: application/vnd.github+json repos/o/r/pulls/17/reviews?per_page=100",
            r#"[{"user":{"login":"owner1"},"state":"APPROVED"}]"#.into(),
        ),
        rule("gh api", r#"{"labels":[{"name":"engine-change"}]}"#.into()),
    ];
    let env = GateEnv {
        token: Some("t".into()),
        repo: Some("o/r".into()),
        step_summary: None,
    };
    let args: Vec<String> = ["--base", "B", "--head", "H", "--pr", "17"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    c.bench_function("xtask/decoupling_gate", |b| {
        b.iter(|| {
            let mut ex = ScriptExec::new(rules.clone());
            assert_eq!(run_with(&root, &args, &mut ex, &env), 0);
        })
    });
    let _ = std::fs::remove_dir_all(&root);
}

criterion_group!(benches, tool_176_decoupling_gate_time);
criterion_main!(benches);
