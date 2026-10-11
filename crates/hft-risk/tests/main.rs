//! hft-risk integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::too_many_arguments,
    clippy::type_complexity
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod blob;
mod broken;
mod bucket;
mod chain;
mod chain_props;
mod classify;
mod common;
mod gates;
mod layout;
mod leverage;
mod margin;
mod mode_props;
mod pnl;
mod recon;
mod recon_gate;
mod snapshot;
mod storage;
