// Crate-level lint block required at the top of every pure crate's lib.rs (docs/hot-path-rules.md
// "检查机制", C1, N3, B5). `cargo xtask hot-check` requires each lint below at this level or stronger
// (`forbid` ≥ `deny`); extra lints are allowed.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
