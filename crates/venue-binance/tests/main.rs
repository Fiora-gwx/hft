//! venue-binance integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(clippy::disallowed_types)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod caps;
mod cid;
mod classify;
mod codes;
mod demo;
mod encode;
mod endpoint;
mod funding;
mod md;
mod prop_encode;
mod reports;
mod req_id;
mod routes;
mod session;
mod subscribe;
mod support;
mod user_stream;

mod bbo;
#[path = "../fuzz/exercise.rs"]
mod fuzz_exercise;
mod readonly;
mod rest;
