//! The `clean` fixture of `cargo xtask deps` (tools/TESTS.md TOOL-030, TOOL-031; also SLB-133,
//! REG-015, LPR-064): a workspace with one member per package the rules name (package directories as in
//! the real repo) plus stub external crates under `vendor/` (excluded from the workspace, path
//! dependencies only, so `cargo metadata` needs no registry). `tools/xtask/deps.toml` is the real config.
//! The clean graph leaves out edges that would make a row's added edge a cycle (cargo rejects cyclic
//! graphs).
#![allow(dead_code)]

use std::path::PathBuf;

use super::{Fixture, Run};

/// `(dir, package name, [features], [dependencies], [dev-dependencies])`; dependency entries are package
/// directories or `vendor/<crate>`; `name:feat` enables a feature.
pub type Pkg = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);

pub const PKGS: &[Pkg] = &[
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
        "strategies/latency-probe",
        "latency-probe",
        &[],
        &["crates/hft-strategy-sdk"],
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
pub const VENDOR: &[(&str, &[&str])] = &[
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
    ("simd-json", &[]),
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
pub fn dep_line(entry: &str) -> String {
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

pub fn manifest(name: &str, features: &[&str], deps: &[&str], dev: &[&str]) -> String {
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
pub fn clean(tag: &str) -> Fixture {
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
pub fn add(f: &Fixture, dir: &str, section: &str, line: &str) {
    let p = format!("{dir}/Cargo.toml");
    f.replace(&p, &format!("[{section}]\n"), &format!("[{section}]\n{line}"));
}

pub fn violations(run: &Run) -> Vec<String> {
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

pub fn apply(f: &Fixture, dir: &str, section: &str, entry: &str) {
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

pub const D: &str = "dependencies";
pub const DEV: &str = "dev-dependencies";
