// Crate-level lint block required at the top of every hot crate's lib.rs (docs/hot-path-rules.md
// "检查机制", C1, B5). `cargo xtask hot-check` requires each lint below at this level or stronger
// (`forbid` ≥ `deny`); extra lints are allowed. The four crates allowed to use `unsafe` (B5) replace
// `forbid(unsafe_code)` with attrs-unsafe.rs; the crates listed in N3 add `clippy::float_arithmetic`.
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
