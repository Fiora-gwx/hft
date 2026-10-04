//! Declares the `loom` cfg used by the loom model tests (`RUSTFLAGS="--cfg loom"`).
fn main() {
    println!("cargo::rustc-check-cfg=cfg(loom)");
}
