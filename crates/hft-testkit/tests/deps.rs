//! TK-036: the test strategies use only the SDK and hft-strat-lib; hft-testkit never depends on the
//! registry or any strategy crate; engine crates use hft-testkit only as a dev-dependency.

/// Crate paths a probe module refers to (`hft_*` identifiers in `use` lines and paths).
fn crates_used(src: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for tok in src.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if tok.starts_with("hft_") && !v.iter().any(|x| x == tok) {
            v.push(tok.to_string());
        }
    }
    v
}

fn violations(src: &str) -> Vec<String> {
    crates_used(src)
        .into_iter()
        .filter(|c| c != "hft_strategy_sdk" && c != "hft_strat_lib")
        .collect()
}

/// `[dependencies]`-like section bodies of a Cargo.toml (not dev).
fn normal_deps(toml: &str) -> String {
    let mut out = String::new();
    let mut on = false;
    for l in toml.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            on = t == "[dependencies]"
                || t == "[build-dependencies]"
                || t.starts_with("[target.") && !t.contains("dev-dependencies");
        } else if on {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
}

#[test]
fn tk_036_dependency_rules() {
    let taker = include_str!("../src/strategies/probe_taker.rs");
    let target = include_str!("../src/strategies/probe_target.rs");
    let replay = include_str!("../src/strategies/replay_probe.rs");
    assert!(
        violations(replay).is_empty(),
        "TK-036.1 replay-a: {:?}",
        violations(replay)
    );
    // TK-036.1
    assert!(
        violations(taker).is_empty(),
        "TK-036.1 probe-taker: {:?}",
        violations(taker)
    );
    assert!(
        violations(target).is_empty(),
        "TK-036.1 probe-target: {:?}",
        violations(target)
    );
    // TK-036.2: a planted edge to an engine crate is caught.
    let planted = format!("{taker}\nuse hft_oms::Oms;\n");
    assert_eq!(violations(&planted), vec!["hft_oms".to_string()], "TK-036.2");
    let me = include_str!("../Cargo.toml");
    // TK-036.3
    assert!(!me.contains("hft-strategy-registry"), "TK-036.3");
    // TK-036.4
    assert!(!me.contains("strategies/"), "TK-036.4");
    // TK-036.5 / .6: engine crates list hft-testkit only in [dev-dependencies].
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for e in std::fs::read_dir(&root).expect("crates dir").flatten() {
        let p = e.path().join("Cargo.toml");
        let Ok(t) = std::fs::read_to_string(&p) else {
            continue;
        };
        if e.file_name() == "hft-testkit" {
            continue;
        }
        assert!(
            !normal_deps(&t).contains("hft-testkit"),
            "TK-036.6: {} depends on hft-testkit outside dev",
            p.display()
        );
    }
    let rules = include_str!("../../../tools/xtask/deps.toml");
    let line = rules
        .lines()
        .find(|l| l.starts_with("no_strategy_closure"))
        .expect("rule 9 list");
    assert!(
        line.contains("crates/hft-testkit"),
        "TK-036.5: deps rule 9 covers hft-testkit"
    );
}
