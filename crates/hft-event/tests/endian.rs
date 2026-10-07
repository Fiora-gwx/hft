//! Big-endian targets do not build (EVT-027; CI ubuntu with the powerpc64 target installed).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[cfg(target_os = "linux")]
#[test]
fn evt_027_big_endian_build_fails() {
    use std::process::Command;
    let check = |target: &str| {
        Command::new(env!("CARGO"))
            .args(["check", "-p", "hft-event", "--target", target, "--target-dir"])
            .arg(concat!(env!("CARGO_TARGET_TMPDIR"), "/endian"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            // CI sets CARGO_TERM_COLOR=always; colour codes would hide the leading "error".
            .env("CARGO_TERM_COLOR", "never")
            .output()
            .unwrap()
    };
    let be = check("powerpc64-unknown-linux-gnu");
    let err = String::from_utf8_lossy(&be.stderr);
    assert!(!be.status.success(), "big-endian check succeeded:\n{err}");
    let Some(first) = err.lines().position(|l| l.starts_with("error")) else {
        panic!("no line starting with \"error\" in stderr:\n{err}");
    };
    let block: Vec<&str> = err.lines().skip(first).take(6).collect();
    let block = block.join("\n");
    assert!(block.contains("little-endian only"), "{block}");
    assert!(
        block.contains("crates/hft-event/src/lib.rs") || block.contains("src/lib.rs"),
        "{block}"
    );
    let le = check("aarch64-unknown-linux-gnu");
    assert!(le.status.success(), "{}", String::from_utf8_lossy(&le.stderr));
}
