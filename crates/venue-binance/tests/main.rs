//! venue-binance integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(clippy::disallowed_types)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod caps;
mod cid;
mod classify;
mod codes;
mod encode;
mod endpoint;
mod md;
mod prop_encode;
mod reports;
mod req_id;
mod routes;
mod session;
mod subscribe;
mod support;
