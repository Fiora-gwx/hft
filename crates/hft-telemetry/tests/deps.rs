//! TEL-081: dependency direction (`cargo xtask deps` on a fixture workspace with the real
//! `tools/xtask/deps.toml`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// `(dir, package name, [dependencies])` of the fixture `clean`: the packages TEL-081 touches.
const PKGS: &[(&str, &str, &[&str])] = &[
    ("crates/hft-types", "hft-types", &[]),
    ("crates/hft-refdata", "hft-refdata", &["crates/hft-types"]),
    (
        "crates/hft-event",
        "hft-event",
        &["crates/hft-types", "crates/hft-refdata"],
    ),
    ("crates/hft-rt", "hft-rt", &["crates/hft-types"]),
    (
        "crates/hft-journal",
        "hft-journal",
        &["crates/hft-event", "crates/hft-rt"],
    ),
    (
        "crates/hft-strategy-sdk",
        "hft-strategy-sdk",
        &["crates/hft-event"],
    ),
    ("crates/hft-ledger", "hft-ledger", &["crates/hft-strategy-sdk"]),
    ("crates/hft-oms", "hft-oms", &["crates/hft-strategy-sdk"]),
    (
        "crates/hft-telemetry",
        "hft-telemetry",
        &["crates/hft-event", "crates/hft-journal", "crates/hft-ledger"],
    ),
    ("strategies/x", "x-strategy", &["crates/hft-strategy-sdk"]),
    ("strategies/registry", "hft-strategy-registry", &["strategies/x"]),
    (
        "apps/hft-core",
        "hft-core",
        &["crates/hft-oms", "strategies/registry"],
    ),
    ("apps/watchtower", "watchtower", &["crates/hft-journal"]),
];

static N: AtomicU32 = AtomicU32::new(0);

fn name_of(dir: &str) -> &'static str {
    PKGS.iter().find(|p| p.0 == dir).unwrap().1
}

fn manifest(name: &str, deps: &[&str]) -> String {
    let mut s =
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n");
    for d in deps {
        s += &format!("{} = {{ path = \"../../{d}\" }}\n", name_of(d));
    }
    s
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

/// The `clean` fixture plus `extra = (from dir, to dir)`.
fn fixture(extra: Option<(&str, &str)>) -> PathBuf {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("tel-deps-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let members: String = PKGS.iter().map(|p| format!("    \"{}\",\n", p.0)).collect();
    write(
        &root,
        "Cargo.toml",
        &format!("[workspace]\nresolver = \"2\"\nmembers = [\n{members}]\n"),
    );
    for (dir, name, deps) in PKGS {
        let mut deps: Vec<&str> = deps.to_vec();
        if let Some((from, to)) = extra
            && from == *dir
        {
            deps.push(to);
        }
        write(&root, &format!("{dir}/Cargo.toml"), &manifest(name, &deps));
        write(&root, &format!("{dir}/src/lib.rs"), "");
    }
    let real = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/xtask/deps.toml");
    write(
        &root,
        "tools/xtask/deps.toml",
        &std::fs::read_to_string(real).unwrap(),
    );
    root
}

#[test]
fn tel_081_dependency_rules() {
    let clean = fixture(None);
    assert_eq!(xtask::deps::run(&clean), 0, "fixture clean passes");
    for (row, from, to, want) in [
        ("TEL-081.1", "crates/hft-oms", "crates/hft-telemetry", 1),
        ("TEL-081.2", "apps/hft-core", "crates/hft-telemetry", 1),
        ("TEL-081.3", "crates/hft-telemetry", "strategies/x", 1),
        ("TEL-081.4", "crates/hft-telemetry", "strategies/registry", 1),
        ("TEL-081.5", "apps/watchtower", "crates/hft-telemetry", 0),
    ] {
        let root = fixture(Some((from, to)));
        assert_eq!(xtask::deps::run(&root), want, "{row}: {from} → {to}");
        let _ = std::fs::remove_dir_all(&root);
    }
    let _ = std::fs::remove_dir_all(&clean);
}
