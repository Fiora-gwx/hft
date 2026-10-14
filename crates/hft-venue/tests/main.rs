//! hft-venue integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod bbo_live;
mod codes;
mod config;
mod conn_id;
mod deps;
mod feed_err;
mod feed_frame;
mod feed_poll;
mod feed_pool;
mod funding;
mod gw_conn;
mod gw_drain;
mod gw_loop;
mod gw_report;
mod gw_send;
mod heartbeat;
mod kill;
mod layout;
mod raw_modes;
mod ring_full;
mod seq;
mod session;
mod support;
