//! hft-journal integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod coldperm;
#[allow(dead_code)] // shared fixtures: each test module uses a part
mod common;
mod config;
mod framing;
mod held;
mod layout;
mod naming;
mod offline;
mod persist;
mod prepare;
mod prop_powerloss;
mod reader;
mod retain;
mod retire;
mod roll;
mod tmpfs;
mod verify;
mod writer;
