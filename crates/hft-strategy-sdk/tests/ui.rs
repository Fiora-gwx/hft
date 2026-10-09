//! Compile-time checks (SDK-057, SDK-105, SDK-114, SDK-174, SDK-187) through `trybuild`.
//! Cases: `tests/ui/<group>_<row>_{pass,fail}.rs`, expected errors in the `.stderr` next to them
//! (`TRYBUILD=overwrite` regenerates them). trybuild serializes its builds, so the five tests run
//! one cargo build at a time.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

/// Runs every `tests/ui/{prefix}*` case: `_pass.rs` must compile, `_fail.rs` must not.
fn cases(prefix: &str) -> usize {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ui");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && n.ends_with(".rs"))
        .collect();
    names.sort();
    let t = trybuild::TestCases::new();
    for n in &names {
        let p = format!("tests/ui/{n}");
        if n.ends_with("_pass.rs") {
            t.pass(p);
        } else {
            t.compile_fail(p);
        }
    }
    names.len()
}

#[test]
fn sdk_057_non_exhaustive_needs_wildcard() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/event_no_wildcard.rs");
    t.compile_fail("tests/ui/exec_kind_no_wildcard.rs");
}

#[test]
fn sdk_105_schema_const_checks() {
    assert_eq!(cases("params_105_"), 19);
}

#[test]
fn sdk_114_descriptor_const_checks() {
    assert_eq!(cases("desc_114_"), 23);
}

#[test]
fn sdk_174_payload_const_checks() {
    assert_eq!(cases("payload_174_"), 4);
}

#[test]
fn sdk_187_set_uniqueness() {
    assert_eq!(cases("set_187_"), 4);
}
