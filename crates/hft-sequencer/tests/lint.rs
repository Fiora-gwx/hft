//! SEQ-018: strategies reach this crate only through static dispatch (TESTS.md "策略集"、"依赖").
//! Sources are scanned as text; direct dependencies come from `cargo tree` (no TOML parser among the
//! dev dependencies) and the allowed sets from `tools/xtask/deps.toml`, whose rule 1 (every direct
//! edge in the package's allowed set) and rule 2 (`hot_pure_forbidden` never in a hot closure) make
//! `cargo xtask deps` fail when one of the rows' dependencies is added.

use std::path::{Path, PathBuf};
use std::process::Command;

const PKG: &str = "crates/hft-sequencer";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `(repo-relative path, line number, line)` of every `src/**/*.rs` line containing one of `needles`.
fn scan(needles: &[&str]) -> Vec<(String, usize, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    files.sort();
    let mut hits = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for (i, l) in text.lines().enumerate() {
            if needles.iter().any(|n| l.contains(n)) {
                let rel = f.strip_prefix(&src).unwrap().display().to_string();
                hits.push((rel, i + 1, l.trim().to_string()));
            }
        }
    }
    hits
}

/// Direct dependencies (package names) of hft-sequencer over the edge kinds `edges`.
fn direct(edges: &str) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "hft-sequencer",
            "--prefix",
            "none",
            "-f",
            "{p}",
            "--depth",
            "1",
            "-e",
            edges,
            "--manifest-path",
        ])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let mut v: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .filter(|n| !n.is_empty() && !n.starts_with('[') && n != "hft-sequencer")
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Package names of every `strategies/*/Cargo.toml` (the registry included).
fn strategy_packages() -> Vec<String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(root().join("strategies")).unwrap().flatten() {
        let Ok(text) = std::fs::read_to_string(e.path().join("Cargo.toml")) else {
            continue;
        };
        if let Some(n) = text.lines().find_map(|l| l.trim().strip_prefix("name")).map(|v| {
            v.trim_start()
                .trim_start_matches('=')
                .trim()
                .trim_matches('"')
                .to_string()
        }) {
            v.push(n);
        }
    }
    v
}

/// The body of `[pkg."<pkg>"]` (or of `[<table>]`) in deps.toml, up to the next table header.
fn table<'a>(deps_toml: &'a str, header: &str) -> &'a str {
    let start = deps_toml
        .find(header)
        .unwrap_or_else(|| panic!("deps.toml: no {header}"));
    let body = &deps_toml[start + header.len()..];
    let end = body
        .match_indices("\n[")
        .map(|(i, _)| i)
        .next()
        .unwrap_or(body.len());
    &body[..end]
}

/// The quoted strings of the array `key = [ … ]` in a table body (empty when absent).
fn array(body: &str, key: &str) -> Vec<String> {
    let Some(at) = body.lines().position(|l| {
        let t = l.trim_start();
        t.starts_with(key) && t[key.len()..].trim_start().starts_with('=')
    }) else {
        return Vec::new();
    };
    let rest: String = body.lines().skip(at).collect::<Vec<_>>().join("\n");
    let open = rest.find('[').unwrap();
    let close = open + rest[open..].find(']').unwrap();
    rest[open + 1..close]
        .split(',')
        .map(|s| {
            s.split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// `tier` of a deps.toml package entry (`None` = no entry).
fn tier(deps_toml: &str, pkg: &str) -> Option<String> {
    let header = format!("[pkg.\"{pkg}\"]");
    deps_toml.contains(&header).then(|| {
        let body = table(deps_toml, &header);
        body.lines()
            .find_map(|l| l.trim().strip_prefix("tier"))
            .map(|v| {
                v.trim_start()
                    .trim_start_matches('=')
                    .trim()
                    .trim_matches('"')
                    .to_string()
            })
            .unwrap_or_default()
    })
}

#[test]
fn seq_018_static_dispatch_and_deps() {
    let deps_toml = std::fs::read_to_string(root().join("tools/xtask/deps.toml")).unwrap();
    let own = table(&deps_toml, &format!("[pkg.\"{PKG}\"]"));
    let allowed: Vec<String> = array(own, "deps").into_iter().chain(array(own, "dev")).collect();
    assert_eq!(tier(&deps_toml, PKG).as_deref(), Some("hot"));
    let all = direct("normal,build,dev");
    let prod = direct("normal,build");

    // .1: no trait objects.
    let dyns = scan(&["dyn "]);
    assert!(dyns.is_empty(), "SEQ-018.1: `dyn ` in src: {dyns:?}");

    // .2: no registry, no strategy crate; adding one breaks rule 1 (not in the allowed set; the
    // registry and every `strategies/*` crate are hot, not dev tier).
    let reg = "strategies/registry";
    let strategy_pkgs = strategy_packages();
    assert!(
        strategy_pkgs.iter().any(|n| n == "hft-strategy-registry"),
        "{strategy_pkgs:?}"
    );
    for d in &all {
        assert!(
            !strategy_pkgs.contains(d),
            "SEQ-018.2: depends on strategy package {d}"
        );
    }
    for p in [reg, "strategies/*"] {
        assert!(!allowed.iter().any(|a| a == p), "SEQ-018.2: deps.toml allows {p}");
        assert_eq!(tier(&deps_toml, p).as_deref(), Some("hot"), "SEQ-018.2: {p}");
    }

    // .3: none of these; adding one breaks rule 1 (internal packages outside the allowed set) or
    // rule 2 (tokio in a hot closure).
    let internal = [
        "crates/hft-strat-lib",
        "crates/hft-sim",
        "crates/hft-net",
        "crates/hft-venue",
    ];
    for d in &all {
        let banned = internal.iter().any(|p| p.rsplit('/').next() == Some(d.as_str()))
            || d.starts_with("venue-")
            || d == "tokio";
        assert!(!banned, "SEQ-018.3: depends on {d}");
    }
    for p in internal {
        assert!(!allowed.iter().any(|a| a == p), "SEQ-018.3: deps.toml allows {p}");
        assert_ne!(
            tier(&deps_toml, p).as_deref(),
            Some("dev"),
            "SEQ-018.3: {p} is dev tier"
        );
    }
    assert!(
        !allowed.iter().any(|a| a.starts_with("crates/venue-")),
        "SEQ-018.3: deps.toml allows a venue-* crate"
    );
    let forbidden = array(table(&deps_toml, "[rules]"), "hot_pure_forbidden");
    assert!(forbidden.iter().any(|f| f == "tokio"), "SEQ-018.3: {forbidden:?}");

    // .4: no direct dependency on a cold-tier package (normal, build or dev).
    for d in &all {
        for dir in ["crates", "apps", "tools"] {
            if let Some(t) = tier(&deps_toml, &format!("{dir}/{d}")) {
                assert_ne!(t, "cold", "SEQ-018.4: {d} is cold tier");
            }
        }
    }
    // Every internal production dependency is one deps.toml allows.
    for d in &prod {
        if d.starts_with("hft-") {
            assert!(
                allowed.iter().any(|a| a == &format!("crates/{d}")),
                "SEQ-018: {d} not allowed by deps.toml"
            );
        }
    }

    // .5: no threads, no file system or network I/O of its own.
    let io = scan(&["std::thread::spawn", "thread::spawn(", "std::fs", "std::net"]);
    assert!(io.is_empty(), "SEQ-018.5: {io:?}");
}
