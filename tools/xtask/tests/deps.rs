//! `cargo xtask deps` (tools/TESTS.md TOOL-030, TOOL-031).
//!
//! Fixture `clean`: a workspace with one member per package the rules name (package directories as in the
//! real repo) plus stub external crates under `vendor/` (excluded from the workspace, path dependencies
//! only, so `cargo metadata` needs no registry). `tools/xtask/deps.toml` is the real config. The clean
//! graph leaves out edges that would make a row's added edge a cycle (cargo rejects cyclic graphs).

mod common;

use std::path::PathBuf;

use common::{Fixture, Run};

/// `(dir, package name, [features], [dependencies], [dev-dependencies])`; dependency entries are package
/// directories or `vendor/<crate>`; `name:feat` enables a feature.
type Pkg = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

const PKGS: &[Pkg] = &[
    ("crates/hft-types", "hft-types", &[], &[], &[]),
    ("crates/hft-refdata", "hft-refdata", &[], &[], &[]),
    (
        "crates/hft-event",
        "hft-event",
        &[],
        &["crates/hft-types", "crates/hft-refdata"],
        &[],
    ),
    (
        "crates/hft-strategy-sdk",
        "hft-strategy-sdk",
        &["testing"],
        &["crates/hft-types", "crates/hft-refdata", "crates/hft-event"],
        &[],
    ),
    (
        "crates/hft-strat-lib",
        "hft-strat-lib",
        &[],
        &["crates/hft-types", "crates/hft-strategy-sdk"],
        &[],
    ),
    (
        "crates/hft-venue-proto",
        "hft-venue-proto",
        &[],
        &["crates/hft-types", "crates/hft-refdata", "crates/hft-event"],
        &[],
    ),
    (
        "crates/venue-binance",
        "venue-binance",
        &[],
        &["crates/hft-types", "crates/hft-venue-proto", "vendor/sonic-rs"],
        &["vendor/serde_json"],
    ),
    (
        "crates/venue-okx",
        "venue-okx",
        &[],
        &["crates/hft-venue-proto"],
        &[],
    ),
    (
        "crates/venue-bitget",
        "venue-bitget",
        &[],
        &["crates/hft-venue-proto"],
        &[],
    ),
    ("crates/hft-book", "hft-book", &[], &["crates/hft-types"], &[]),
    (
        "crates/hft-oms",
        "hft-oms",
        &[],
        &["crates/hft-types", "crates/hft-strategy-sdk"],
        &[],
    ),
    (
        "crates/hft-risk",
        "hft-risk",
        &[],
        &["crates/hft-types", "crates/hft-strategy-sdk"],
        &[],
    ),
    (
        "crates/hft-ledger",
        "hft-ledger",
        &[],
        &["crates/hft-types", "crates/hft-strategy-sdk"],
        &[],
    ),
    (
        "crates/hft-exec",
        "hft-exec",
        &[],
        &["crates/hft-types", "crates/hft-strategy-sdk"],
        &[],
    ),
    ("crates/hft-rt", "hft-rt", &[], &["crates/hft-types"], &[]),
    (
        "crates/hft-journal",
        "hft-journal",
        &["writer"],
        &["crates/hft-types", "crates/hft-rt"],
        &[],
    ),
    ("crates/hft-net", "hft-net", &[], &["crates/hft-rt"], &[]),
    (
        "crates/hft-venue",
        "hft-venue",
        &[],
        &[
            "crates/hft-venue-proto",
            "crates/hft-net",
            "crates/hft-journal:writer",
        ],
        &[],
    ),
    (
        "crates/hft-sequencer",
        "hft-sequencer",
        &[],
        &[
            "crates/hft-book",
            "crates/hft-oms",
            "crates/hft-risk",
            "crates/hft-exec",
            "crates/hft-journal",
        ],
        &[],
    ),
    (
        "crates/hft-telemetry",
        "hft-telemetry",
        &[],
        &["crates/hft-journal", "crates/hft-ledger", "vendor/serde_json"],
        &[],
    ),
    ("crates/hft-sim", "hft-sim", &[], &["crates/hft-book"], &[]),
    (
        "crates/hft-py",
        "hft-py",
        &["testing"],
        &[
            "crates/hft-journal",
            "crates/hft-ledger",
            "vendor/pyo3",
            "vendor/alloy-primitives",
        ],
        &[],
    ),
    (
        "crates/hft-testkit",
        "hft-testkit",
        &[],
        &["crates/hft-sim", "crates/hft-strat-lib"],
        &[],
    ),
    (
        "strategies/x",
        "x-strategy",
        &[],
        &["crates/hft-strategy-sdk", "crates/hft-strat-lib"],
        &[],
    ),
    (
        "strategies/registry",
        "hft-strategy-registry",
        &[],
        &["crates/hft-strategy-sdk", "strategies/x"],
        &["crates/hft-testkit", "crates/hft-oms"],
    ),
    (
        "apps/hft-core",
        "hft-core",
        &[],
        &[
            "crates/hft-venue",
            "crates/venue-binance",
            "crates/hft-sequencer",
            "crates/hft-journal:writer",
            "strategies/registry",
        ],
        &["crates/hft-testkit"],
    ),
    (
        "apps/replayer",
        "replayer",
        &[],
        &[
            "crates/hft-sequencer",
            "crates/hft-telemetry",
            "strategies/registry",
        ],
        &[],
    ),
    (
        "apps/watchtower",
        "watchtower",
        &[],
        &["crates/hft-telemetry", "vendor/tokio"],
        &[],
    ),
    (
        "apps/chain-core",
        "chain-core",
        &[],
        &["crates/hft-journal:writer", "vendor/tokio"],
        &[],
    ),
    (
        "tools/xtask",
        "xtask",
        &[],
        &["crates/hft-event", "vendor/serde_json"],
        &[],
    ),
    (
        "tools/hft-cfg",
        "hft-cfg",
        &[],
        &["crates/hft-strategy-sdk", "vendor/serde_json"],
        &[],
    ),
    ("tools/hft-verify", "hft-verify", &[], &["vendor/serde_json"], &[]),
    ("tools/hostprobe", "hostprobe", &[], &["vendor/serde"], &[]),
    (
        "tools/hft-secrets",
        "hft-secrets",
        &[],
        &[
            "vendor/aws-sdk-secretsmanager",
            "vendor/aws-sdk-kms",
            "vendor/tokio",
        ],
        &[],
    ),
];

/// Stub external crates: `(name, [dependencies])`.
const VENDOR: &[(&str, &[&str])] = &[
    ("tokio", &["mio"]),
    ("async-std", &[]),
    ("mio", &[]),
    ("futures-executor", &[]),
    ("tracing-subscriber", &[]),
    ("serde", &[]),
    ("serde_json", &["serde"]),
    ("reqwest", &["hyper", "tokio"]),
    ("hyper", &[]),
    ("pyo3", &[]),
    ("rust_decimal", &[]),
    ("bigdecimal", &["num-bigint"]),
    ("num-bigint", &[]),
    ("sonic-rs", &[]),
    ("memchr", &[]),
    ("zerocopy", &[]),
    ("ed25519-dalek", &[]),
    ("hmac", &[]),
    ("sha2", &[]),
    ("base64", &[]),
    ("alloy-primitives", &[]),
    ("revm", &["alloy-primitives"]),
    ("aws-sdk-secretsmanager", &[]),
    ("aws-sdk-kms", &[]),
    ("x", &["tokio"]),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// `name = { path = "../../<dir>", features = [...] }` for a dependency entry `dir[:feat]`.
fn dep_line(entry: &str) -> String {
    let (dir, feat) = entry.split_once(':').map_or((entry, None), |(d, f)| (d, Some(f)));
    let name = PKGS
        .iter()
        .find(|p| p.0 == dir)
        .map(|p| p.1)
        .unwrap_or_else(|| dir.strip_prefix("vendor/").unwrap());
    match feat {
        Some(f) => format!("{name} = {{ path = \"../../{dir}\", features = [\"{f}\"] }}\n"),
        None => format!("{name} = {{ path = \"../../{dir}\" }}\n"),
    }
}

fn manifest(name: &str, features: &[&str], deps: &[&str], dev: &[&str]) -> String {
    let mut s =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\n");
    for f in features {
        s += &format!("{f} = []\n");
    }
    s += "\n[dependencies]\n";
    for d in deps {
        s += &dep_line(d);
    }
    s += "\n[dev-dependencies]\n";
    for d in dev {
        s += &dep_line(d);
    }
    s
}

/// The `clean` fixture.
fn clean(tag: &str) -> Fixture {
    let f = Fixture::empty(&format!("deps-{tag}"));
    let members: Vec<String> = PKGS.iter().map(|p| format!("    \"{}\",\n", p.0)).collect();
    f.write(
        "Cargo.toml",
        &format!(
            "[workspace]\nresolver = \"2\"\nexclude = [\"vendor\"]\nmembers = [\n{}]\n",
            members.concat()
        ),
    );
    for (dir, name, feats, deps, dev) in PKGS {
        f.write(&format!("{dir}/Cargo.toml"), &manifest(name, feats, deps, dev));
        f.write(&format!("{dir}/src/lib.rs"), "");
    }
    for (name, deps) in VENDOR {
        let deps: Vec<String> = deps.iter().map(|d| format!("vendor/{d}")).collect();
        let deps: Vec<&str> = deps.iter().map(String::as_str).collect();
        let m = manifest(name, &[], &deps, &[]);
        f.write(
            &format!("vendor/{name}/Cargo.toml"),
            &m.replace("../../vendor/", "../"),
        );
        f.write(&format!("vendor/{name}/src/lib.rs"), "");
    }
    let cfg = std::fs::read_to_string(repo_root().join(xtask::deps::CONFIG)).unwrap();
    f.write(xtask::deps::CONFIG, &cfg);
    f
}

/// Insert `line` right after the `[section]` header of `<dir>/Cargo.toml`.
fn add(f: &Fixture, dir: &str, section: &str, line: &str) {
    let p = format!("{dir}/Cargo.toml");
    f.replace(&p, &format!("[{section}]\n"), &format!("[{section}]\n{line}"));
}

fn violations(run: &Run) -> Vec<String> {
    run.json
        .as_ref()
        .and_then(|j| j["violations"].as_array())
        .map(|a| {
            a.iter()
                .map(|v| {
                    format!(
                        "[rule {}] {}: {}",
                        v["rule"],
                        v["package"].as_str().unwrap_or(""),
                        v["detail"].as_str().unwrap_or("")
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn tool_030_clean_passes() {
    let f = clean("tool-030");
    let run = f.xtask(&["deps"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(violations(&run).is_empty(), "{:?}", violations(&run));
    assert_eq!(run.json.as_ref().unwrap()["members"], PKGS.len());
}

/// `(row, [(dir, section, dependency entry or raw line)], expected exit, output must contain)`.
type Row = (
    &'static str,
    &'static [(&'static str, &'static str, &'static str)],
    i32,
    &'static str,
);

const D: &str = "dependencies";
const DEV: &str = "dev-dependencies";

const ROWS: &[Row] = &[
    (
        "TOOL-031.1",
        &[("crates/hft-book", D, "crates/hft-oms")],
        1,
        "crates/hft-book → crates/hft-oms",
    ),
    (
        "TOOL-031.2",
        &[("crates/hft-types", D, "crates/hft-refdata")],
        1,
        "crates/hft-types → crates/hft-refdata",
    ),
    (
        "TOOL-031.3",
        &[("crates/hft-oms", D, "vendor/x")],
        1,
        "crates/hft-oms → x → tokio",
    ),
    (
        "TOOL-031.4",
        &[("crates/hft-oms", D, "vendor/async-std")],
        1,
        "`async-std`",
    ),
    ("TOOL-031.5", &[("crates/hft-oms", D, "vendor/mio")], 1, "`mio`"),
    (
        "TOOL-031.6",
        &[("crates/hft-oms", D, "vendor/futures-executor")],
        1,
        "`futures-executor`",
    ),
    (
        "TOOL-031.7",
        &[("crates/hft-oms", D, "vendor/tracing-subscriber")],
        1,
        "`tracing-subscriber`",
    ),
    (
        "TOOL-031.8",
        &[("crates/hft-oms", D, "vendor/serde_json")],
        1,
        "`serde_json`",
    ),
    (
        "TOOL-031.9",
        &[("crates/hft-oms", D, "vendor/reqwest")],
        1,
        "`reqwest`",
    ),
    (
        "TOOL-031.10",
        &[("crates/hft-oms", D, "vendor/hyper")],
        1,
        "`hyper`",
    ),
    (
        "TOOL-031.11",
        &[("crates/hft-oms", D, "vendor/pyo3")],
        1,
        "`pyo3`",
    ),
    (
        "TOOL-031.12",
        &[("crates/hft-oms", D, "vendor/rust_decimal")],
        1,
        "`rust_decimal`",
    ),
    (
        "TOOL-031.13",
        &[("crates/hft-oms", D, "vendor/bigdecimal")],
        1,
        "`bigdecimal`",
    ),
    (
        "TOOL-031.14",
        &[("crates/hft-oms", D, "vendor/num-bigint")],
        1,
        "`num-bigint`",
    ),
    (
        "TOOL-031.15",
        &[("crates/hft-types", D, "vendor/serde_json")],
        1,
        "pure package closure contains `serde_json`",
    ),
    (
        "TOOL-031.16",
        &[("crates/venue-binance", D, "crates/hft-net")],
        1,
        "crates/venue-binance → crates/hft-net",
    ),
    (
        "TOOL-031.17",
        &[("crates/venue-binance", D, "vendor/serde")],
        1,
        "crates/venue-binance → serde",
    ),
    (
        "TOOL-031.18",
        &[
            ("crates/venue-binance", D, "vendor/memchr"),
            ("crates/venue-binance", D, "vendor/zerocopy"),
            ("crates/venue-binance", D, "vendor/ed25519-dalek"),
            ("crates/venue-binance", D, "vendor/hmac"),
            ("crates/venue-binance", D, "vendor/sha2"),
            ("crates/venue-binance", D, "vendor/base64"),
        ],
        0,
        "",
    ),
    (
        "TOOL-031.19",
        &[("crates/hft-oms", D, "vendor/alloy-primitives")],
        1,
        "chain crate",
    ),
    ("TOOL-031.20", &[("apps/chain-core", D, "vendor/revm")], 0, ""),
    (
        "TOOL-031.21",
        &[("crates/hft-oms", D, "crates/hft-testkit")],
        1,
        "dev-tier package outside [dev-dependencies]",
    ),
    (
        "TOOL-031.22",
        &[("crates/hft-oms", DEV, "crates/hft-testkit")],
        0,
        "",
    ),
    ("TOOL-031.23", &[("tools/xtask", D, "crates/hft-testkit")], 0, ""),
    (
        "TOOL-031.24",
        &[("crates/hft-venue", D, "crates/venue-okx")],
        1,
        "hft-venue depends on a venue crate",
    ),
    (
        "TOOL-031.25",
        &[("crates/hft-exec", D, "crates/hft-rt")],
        1,
        "closure contains crates/hft-rt",
    ),
    (
        "TOOL-031.26",
        &[("crates/venue-bitget", D, "crates/hft-rt")],
        1,
        "closure contains crates/hft-rt",
    ),
    (
        "TOOL-031.27",
        &[("strategies/x", D, "crates/hft-rt")],
        1,
        "closure contains crates/hft-rt",
    ),
    (
        "TOOL-031.28",
        &[("apps/watchtower", D, "crates/hft-journal:writer")],
        1,
        "enables `hft-journal/writer`",
    ),
    (
        "TOOL-031.29",
        &[("apps/replayer", D, "crates/hft-journal:writer")],
        0,
        "",
    ),
    (
        "TOOL-031.30",
        &[("crates/hft-oms", DEV, "strategies/x")],
        1,
        "only strategies/registry depends on strategy crates",
    ),
    (
        "TOOL-031.31",
        &[("strategies/x", D, "crates/hft-oms")],
        1,
        "strategies/x → crates/hft-oms",
    ),
    (
        "TOOL-031.32",
        &[("strategies/x", D, "crates/hft-types")],
        1,
        "strategies/x → crates/hft-types",
    ),
    (
        "TOOL-031.33",
        &[("crates/hft-strategy-sdk", D, "crates/hft-book")],
        1,
        "crates/hft-strategy-sdk → crates/hft-book",
    ),
    (
        "TOOL-031.34",
        &[("crates/hft-strat-lib", D, "crates/hft-oms")],
        1,
        "crates/hft-strat-lib → crates/hft-oms",
    ),
    (
        "TOOL-031.35",
        &[("crates/hft-testkit", DEV, "strategies/registry")],
        1,
        "crates/hft-testkit → strategies/registry",
    ),
    (
        "TOOL-031.36",
        &[("tools/hft-cfg", D, "strategies/x")],
        1,
        "tools/hft-cfg → strategies/x",
    ),
    (
        "TOOL-031.37",
        &[("crates/hft-py", DEV, "strategies/x")],
        1,
        "crates/hft-py → strategies/x",
    ),
    (
        "TOOL-031.38",
        &[("apps/watchtower", D, "strategies/registry")],
        1,
        "apps/watchtower → strategies/registry",
    ),
    (
        "TOOL-031.39",
        &[("tools/hft-verify", D, "crates/hft-types")],
        1,
        "hft crate or network library",
    ),
    (
        "TOOL-031.40",
        &[("tools/hft-verify", D, "vendor/reqwest")],
        1,
        "tools/hft-verify → reqwest",
    ),
    (
        "TOOL-031.41",
        &[("tools/hft-verify", D, "vendor/tokio")],
        1,
        "tools/hft-verify → tokio",
    ),
    (
        "TOOL-031.42",
        &[("tools/xtask", D, "strategies/x")],
        1,
        "tools/xtask → strategies/x",
    ),
    (
        "TOOL-031.43",
        &[("tools/hostprobe", D, "crates/hft-rt")],
        1,
        "tools/hostprobe → crates/hft-rt",
    ),
    (
        "TOOL-031.44",
        &[("tools/hft-cfg", D, "crates/hft-rt")],
        1,
        "tools/hft-cfg → crates/hft-rt",
    ),
    (
        "TOOL-031.45",
        &[("crates/hft-oms", D, "tools/hft-secrets")],
        1,
        "AWS secrets SDK",
    ),
    (
        "TOOL-031.46",
        &[("crates/hft-oms", D, "crates/hft-telemetry")],
        1,
        "hot package depends on a cold package",
    ),
    (
        "TOOL-031.47",
        &[("strategies/x", "features", "!sdk-testing")],
        1,
        "enables the `testing` feature in [dependencies]",
    ),
    (
        "TOOL-031.48",
        &[("strategies/x", DEV, "crates/hft-strategy-sdk:testing")],
        0,
        "",
    ),
    (
        "TOOL-031.49",
        &[("crates/hft-py", "features", "!default-testing")],
        1,
        "default features enable `testing`",
    ),
];

fn apply(f: &Fixture, dir: &str, section: &str, entry: &str) {
    match entry {
        // The [dependencies] line of strategies/x on the SDK gains `features = ["testing"]`.
        "!sdk-testing" => f.replace(
            &format!("{dir}/Cargo.toml"),
            "hft-strategy-sdk = { path = \"../../crates/hft-strategy-sdk\" }\nhft-strat-lib",
            "hft-strategy-sdk = { path = \"../../crates/hft-strategy-sdk\", features = [\"testing\"] }\nhft-strat-lib",
        ),
        "!default-testing" => {
            add(f, dir, section, "default = [\"testing\"]\n");
            f
        }
        _ => {
            add(f, dir, section, &dep_line(entry));
            f
        }
    };
}

#[test]
fn tool_031_rule_counterexamples() {
    std::thread::scope(|sc| {
        let handles: Vec<_> = ROWS
            .chunks(8)
            .map(|chunk| {
                sc.spawn(move || {
                    let mut fails = Vec::new();
                    for (row, edits, want, needle) in chunk {
                        let f = clean(&row.to_lowercase().replace('.', "-"));
                        for (dir, section, entry) in *edits {
                            apply(&f, dir, section, entry);
                        }
                        let run = f.xtask(&["deps"]);
                        let vs = violations(&run);
                        if run.code != *want {
                            fails.push(format!(
                                "{row}: exit {} (want {want}); {vs:?}; {}",
                                run.code, run.stderr
                            ));
                        } else if !needle.is_empty() && !vs.iter().any(|v| v.contains(needle)) {
                            fails.push(format!("{row}: no violation mentions '{needle}': {vs:?}"));
                        }
                    }
                    fails
                })
            })
            .collect();
        let fails: Vec<String> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
        assert!(fails.is_empty(), "{}", fails.join("\n"));
    });
}
