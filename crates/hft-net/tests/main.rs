//! hft-net integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod arena;
mod config;
mod connector;
mod hot_syscalls;
mod layout;
mod loopback;
mod ready;
mod rx_path;
mod rx_prop;
mod support;
mod timestamps;
mod tx_path;
mod txts;
mod ws_rx;
mod ws_tx;
