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

/// `[features]` table body of a Cargo.toml.
fn features(toml: &str) -> String {
    let mut out = String::new();
    let mut on = false;
    for l in toml.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            on = t == "[features]";
        } else if on && !t.starts_with('#') {
            out.push_str(t);
            out.push('\n');
        }
    }
    out
}

/// The value list of feature `name` in a `[features]` body (`name = [...]`, one line).
fn feature<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    body.lines().find_map(|l| {
        l.split_once('=')
            .filter(|(k, _)| k.trim() == name)
            .map(|(_, v)| v.trim())
    })
}

/// TK-178: the planted defects of the engine crates exist only behind their `planted` features,
/// which only hft-testkit's own `planted` feature enables; no other manifest (in particular no
/// production binary's dependency path, `seq` or `dst-bin`) reaches them.
#[test]
fn tk_178_planted_defects_never_in_production_builds() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let engine = ["hft-oms", "hft-risk", "hft-exec", "hft-ledger", "hft-sequencer"];
    // TK-178.1: each engine crate declares `planted`; the sequencer's only forwards to the others.
    for c in engine {
        let t = std::fs::read_to_string(root.join("crates").join(c).join("Cargo.toml")).unwrap();
        let f = features(&t);
        let v = feature(&f, "planted").unwrap_or_else(|| panic!("TK-178.1: {c} has no planted feature"));
        let want = if c == "hft-sequencer" {
            r#"["hft-oms/planted", "hft-risk/planted", "hft-exec/planted", "hft-ledger/planted"]"#
        } else {
            "[]"
        };
        assert_eq!(v, want, "TK-178.1: {c}");
        let lib = std::fs::read_to_string(root.join("crates").join(c).join("src/lib.rs")).unwrap();
        assert!(
            lib.contains("#[cfg(feature = \"planted\")]\npub mod planted;"),
            "TK-178.1: {c} compiles its planted switch only with the feature"
        );
    }
    // TK-178.2: hft-testkit's `planted` is separate from `seq` and `dst-bin` (the verification
    // bundle's hft-dst) and from the crate's own dev-dependency features.
    let me = include_str!("../Cargo.toml");
    let f = features(me);
    assert_eq!(
        feature(&f, "planted"),
        Some(r#"["seq", "hft-sequencer/planted"]"#),
        "TK-178.2"
    );
    for name in ["seq", "dst-bin", "sdk-mock"] {
        assert!(
            !feature(&f, name).unwrap_or("").contains("planted"),
            "TK-178.2: `{name}` enables planted defects"
        );
    }
    // TK-178.3: no other manifest of the workspace mentions a planted feature.
    let mut stack = vec![
        root.join("crates"),
        root.join("apps"),
        root.join("strategies"),
        root.join("tools"),
    ];
    let mut seen = 0;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if !matches!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("target" | "src" | "tests" | "fixtures")
                ) {
                    stack.push(p);
                }
                continue;
            }
            if p.file_name().is_none_or(|n| n != "Cargo.toml") {
                continue;
            }
            seen += 1;
            let dir = p
                .parent()
                .and_then(|d| d.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if engine.contains(&dir) || dir == "hft-testkit" {
                continue;
            }
            let t = std::fs::read_to_string(&p).unwrap();
            assert!(
                !t.contains("planted"),
                "TK-178.3: {} mentions planted",
                p.display()
            );
        }
    }
    assert!(seen > 10, "TK-178.3: manifests found {seen}");
    // TK-178.4: the adapter's list is exactly the engine crates' defect table.
    #[cfg(feature = "planted")]
    {
        let mut a: Vec<&str> = hft_testkit::dst::seq::PLANTED.iter().map(|(n, _)| *n).collect();
        let mut b: Vec<&str> = hft_sequencer::planted::DEFECTS
            .iter()
            .map(|(n, _, _)| *n)
            .collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "TK-178.4");
    }
}
