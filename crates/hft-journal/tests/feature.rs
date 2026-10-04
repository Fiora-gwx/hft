//! The `writer` feature gate (JNL-102) and `DurableMeta` without it (JNL-122): a throw-away
//! project depending on `hft-journal` with default features, built by a nested `cargo`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::path::{Path, PathBuf};
use std::process::Command;

fn cargo() -> Command {
    Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Workspace `target/` (shared by the nested builds through their own sub-directory).
fn target_root() -> PathBuf {
    manifest_dir()
        .join("..")
        .join("..")
        .join("target")
        .join("jnl-feature")
}

const BINS: &[(&str, &str)] = &[
    (
        "use_writer",
        "fn main() { let _ = std::mem::size_of::<hft_journal::JournalWriter>(); }",
    ),
    (
        "use_persister",
        "fn main() { let _ = std::mem::size_of::<hft_journal::Persister>(); }",
    ),
    (
        "use_preparer",
        "fn main() { let _ = std::mem::size_of::<hft_journal::SegmentPreparer>(); }",
    ),
    (
        "use_retirer",
        "fn main() { let _ = std::mem::size_of::<hft_journal::HotRetirer>(); }",
    ),
    (
        "use_reader",
        r#"fn main() {
    let _ = std::mem::size_of::<hft_journal::JournalReader>();
    let _ = hft_journal::verify;
    let _ = hft_journal::cold_retire;
    let _ = std::mem::size_of::<hft_journal::DurableMeta>();
}"#,
    ),
    (
        "meta_run",
        r#"fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let m = hft_journal::DurableMeta::open(&root, &hft_journal::InstanceName::Prod).unwrap();
    m.write_sync("engine_epoch", 42).unwrap();
    assert_eq!(m.read("engine_epoch").unwrap(), Some(42));
}"#,
    ),
];

/// Writes the fixture project once per test process.
fn fixture() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(make_fixture).clone()
}

fn make_fixture() -> PathBuf {
    let dir = target_root().join(format!("fixture-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src").join("bin")).unwrap();
    let dep = manifest_dir().to_string_lossy().replace('\\', "/");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"jnl-feature-fixture\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n\
             [dependencies]\nhft-journal = {{ path = \"{dep}\" }}\n\n[workspace]\n"
        ),
    )
    .unwrap();
    for (name, src) in BINS {
        std::fs::write(dir.join("src").join("bin").join(format!("{name}.rs")), src).unwrap();
    }
    let lock = manifest_dir().join("..").join("..").join("Cargo.lock");
    if lock.exists() {
        std::fs::copy(lock, dir.join("Cargo.lock")).unwrap();
    }
    dir
}

fn check_bin(dir: &Path, bin: &str) -> bool {
    cargo()
        .current_dir(dir)
        .args(["check", "--quiet", "--bin", bin])
        .env("CARGO_TARGET_DIR", target_root().join("t"))
        .env_remove("RUSTFLAGS")
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[test]
fn jnl_102_writer_feature_gate() {
    let dir = fixture();
    assert!(!check_bin(&dir, "use_writer"), "JNL-102.1");
    for b in ["use_persister", "use_preparer", "use_retirer"] {
        assert!(!check_bin(&dir, b), "JNL-102.2 {b}");
    }
    assert!(check_bin(&dir, "use_reader"), "JNL-102.3");
    // Row 4: without `writer`, the examples build skips `testwriter` (required-features) and exits 0.
    let t = target_root().join("ex");
    let st = cargo()
        .args(["build", "--quiet", "--examples", "--manifest-path"])
        .arg(manifest_dir().join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", &t)
        .status()
        .unwrap();
    assert!(st.success(), "JNL-102.4");
    let ex = t.join("debug").join("examples");
    let built = std::fs::read_dir(&ex)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("testwriter"))
        })
        .unwrap_or(false);
    assert!(!built, "JNL-102.4: testwriter must not be built");
}

#[test]
fn jnl_122_durable_meta_without_writer() {
    let dir = fixture();
    let root = target_root().join(format!("meta-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let st = cargo()
        .current_dir(&dir)
        .args(["run", "--quiet", "--bin", "meta_run", "--"])
        .arg(&root)
        .env("CARGO_TARGET_DIR", target_root().join("t"))
        .env_remove("RUSTFLAGS")
        .status()
        .unwrap();
    assert!(st.success());
    let want = format!("42\ncrc32c:{:08x}\n", crc32c::crc32c(b"42"));
    let got = std::fs::read_to_string(root.join("prod").join("meta").join("engine_epoch")).unwrap();
    assert_eq!(got, want);
}
