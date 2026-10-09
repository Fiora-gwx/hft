//! crates/hft-ctl-proto/TESTS.md CTP-174: the crate's clippy configuration forbids I/O, threads,
//! clocks and the environment, and the configuration is effective (a copy with a clock read fails).
//! The crate itself is checked against it by the workspace clippy run.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn clippy(dir: &std::path::Path) -> (bool, String) {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["clippy", "--quiet", "-j", "1", "--", "-D", "warnings"])
        .current_dir(dir)
        .env_remove("RUSTFLAGS")
        .output()
        .expect("cargo clippy");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn ctp_174_ctl_proto_pure() {
    let cfg = fs::read_to_string(repo().join("crates/hft-ctl-proto/clippy.toml")).unwrap();
    let t: toml::Table = cfg.parse().unwrap();
    let paths: Vec<String> = ["disallowed-methods", "disallowed-types"]
        .iter()
        .flat_map(|k| t[*k].as_array().unwrap().iter())
        .map(|e| e["path"].as_str().unwrap().to_owned())
        .collect();
    for m in [
        "std::fs::",
        "std::net::",
        "std::time::",
        "std::thread::",
        "std::env::",
    ] {
        assert!(
            paths.iter().any(|p| p.starts_with(m)),
            "{m} not in the disallowed lists"
        );
    }
    // A crate with the same configuration: clean source passes, a clock read fails.
    let d = std::env::temp_dir().join(format!("xtask-ctp174-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("Cargo.toml"),
        "[package]\nname = \"ctp174\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n",
    )
    .unwrap();
    fs::write(d.join("clippy.toml"), &cfg).unwrap();
    fs::write(d.join("src/lib.rs"), "pub fn f(x: u8) -> u8 { x }\n").unwrap();
    let (ok, err) = clippy(&d);
    assert!(ok, "{err}");
    fs::write(
        d.join("src/lib.rs"),
        "pub fn f(x: u8) -> u8 { let _ = std::time::SystemTime::now(); x }\n",
    )
    .unwrap();
    let (ok, err) = clippy(&d);
    assert!(!ok);
    assert!(err.contains("disallowed"), "{err}");
    let _ = fs::remove_dir_all(&d);
}
