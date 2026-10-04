//! TOOL-142: input and environment errors exit 2.

mod common;

use common::strat::*;
use common::*;

#[test]
fn tool_142_input_errors() {
    let env = Env::new("tool_142");
    let (_, blob) = env.ok("a", &base_toml(), None);
    let toml = env.dir.join("a.toml");
    let missing = env.dir.join("missing");
    std::fs::write(env.dir.join("bad.toml"), "[strategy\ninstance = ").unwrap();
    let strategy = |input: &str, refdata: &str| {
        vec![
            "build".to_string(),
            "--kind".into(),
            "strategy".into(),
            "--schemas".into(),
            p(&env.schemas).into(),
            "--refdata".into(),
            refdata.into(),
            "--in".into(),
            input.into(),
            "--out".into(),
            p(&env.dir.join("x.blob")).into(),
        ]
    };
    let rows: Vec<(&str, Vec<String>)> = vec![
        ("TOOL-142.1", strategy(p(&missing), p(&env.refdata))),
        ("TOOL-142.2", strategy(p(&toml), p(&missing))),
        (
            "TOOL-142.3",
            [
                "send",
                "--kind",
                "engine",
                "--blob",
                p(&blob),
                "--account",
                "pc-live",
                "--socket",
                p(&missing),
            ]
            .map(String::from)
            .to_vec(),
        ),
        (
            "TOOL-142.4",
            [
                "send",
                "--kind",
                "strategy",
                "--blob",
                p(&blob),
                "--account",
                "pc-live",
                "--socket",
                p(&missing),
            ]
            .map(String::from)
            .to_vec(),
        ),
        (
            "TOOL-142.5",
            strategy(p(&env.dir.join("bad.toml")), p(&env.refdata)),
        ),
    ];
    for (id, args) in rows {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let r = run(&refs);
        assert_eq!(r.code, 2, "{id}: {}", r.stderr);
        assert!(!env.dir.join("x.blob").exists(), "{id}: no blob");
    }
}
