//! hft-venue integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;
mod acct_fetch;
mod acct_sched;
mod bbo_live;
mod codes;
mod config;
mod conn_id;
mod deps;
mod feed_err;
mod feed_frame;
mod feed_poll;
mod feed_pool;
mod feed_routes;
mod funding;
mod gw_conn;
mod gw_drain;
mod gw_hold;
mod gw_loop;
mod gw_report;
mod gw_send;
mod heartbeat;
mod idtable;
mod kill;
mod layout;
mod rate;
mod raw_modes;
mod ring_full;
mod seq;
mod session;
mod snap_pool;
mod snapshot;
mod support;

mod private;

mod continuation;
mod gw_rest;

mod final_round;
