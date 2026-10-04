//! Declares the `loom` cfg used by the loom model tests (`RUSTFLAGS="--cfg loom"`).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: build script, not a hot path
fn main() {
    println!("cargo::rustc-check-cfg=cfg(loom)");
}
