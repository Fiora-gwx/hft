//! hft-rt integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod bytes;
mod compile_fail;
mod config;
mod host;
mod layout;
mod prop_bytes;
mod prop_time;
mod traits;
