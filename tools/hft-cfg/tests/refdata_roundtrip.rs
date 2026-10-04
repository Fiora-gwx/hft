//! REF-079 (crates/hft-refdata/TESTS.md): refdata TOML → `RdbBuilder` → load → dump round trip.

mod common;
#[allow(dead_code, clippy::all)]
#[path = "../../../crates/hft-refdata/tests/common/f0.rs"]
mod f0;

use common::*;
use hft_refdata::{RefData, dump};

fn build(input: &std::path::Path, out: &std::path::Path) -> Vec<u8> {
    let r = run(&["build", "--kind", "refdata", "--in", p(input), "--out", p(out)]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    std::fs::read(out).unwrap()
}

#[test]
fn ref_079_toml_roundtrip() {
    let d = scratch("ref_079");
    let toml = repo().join("fixtures/refdata/f0.toml");
    let b1 = build(&toml, &d.join("b1.blob"));
    let b2 = build(&toml, &d.join("b2.blob"));
    let rd = RefData::load(&b1, None).unwrap();
    std::fs::write(d.join("dump.toml"), dump(&rd)).unwrap();
    let b3 = build(&d.join("dump.toml"), &d.join("b3.blob"));
    assert_eq!(b1, b2, "two builds of the same TOML");
    assert_eq!(b1, b3, "dump → build reproduces the blob");
    assert_eq!(b1, f0::f0(), "the TOML build equals the test builder's F0");
}
