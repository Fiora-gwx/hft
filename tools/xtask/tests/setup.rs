//! `cargo xtask setup` (tools/TESTS.md TOOL-085).

mod common;

use common::{Fixture, Script};

#[test]
fn tool_085_pinned_and_idempotent() {
    let f = Fixture::empty("setup");
    f.write(
        "tools/xtask/toolchains.toml",
        "nightly = \"nightly-2026-09-01\"\n\n[plugins]\ncargo-deny = \"0.16.1\"\ncargo-fuzz = \"0.12.0\"\n",
    );
    // First run: nothing installed yet.
    let s1 = Script::new(
        &f,
        &[
            ("cargo deny --version", 101, "", "error: no such command: `deny`"),
            ("cargo fuzz --version", 101, "", "error: no such command: `fuzz`"),
            (
                "rustup toolchain list",
                0,
                "stable-x86_64-unknown-linux-gnu (default)\n",
                "",
            ),
        ],
        0,
    );
    let r = f.xtask_script(&["setup"], &s1, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let calls = s1.calls();
    for want in [
        "cargo install cargo-deny --version 0.16.1 --locked",
        "cargo install cargo-fuzz --version 0.12.0 --locked",
        "rustup toolchain install nightly-2026-09-01 --profile minimal",
    ] {
        assert!(calls.iter().any(|c| c == want), "missing {want}: {calls:?}");
    }
    // Second run: versions match.
    let s2 = Script::new(
        &f,
        &[
            ("cargo deny --version", 0, "cargo-deny 0.16.1\n", ""),
            ("cargo fuzz --version", 0, "cargo-fuzz 0.12.0\n", ""),
            (
                "rustup toolchain list",
                0,
                "stable-x86_64-unknown-linux-gnu (default)\nnightly-2026-09-01-x86_64-unknown-linux-gnu\n",
                "",
            ),
        ],
        0,
    );
    let r = f.xtask_script(&["setup"], &s2, &[]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let calls = s2.calls();
    assert!(!calls.iter().any(|c| c.starts_with("cargo install")), "{calls:?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("rustup toolchain install")),
        "{calls:?}"
    );
}
