//! hft-sequencer tests (one binary; each module is one TESTS.md group).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
mod params;
mod boot;
mod recover;
mod strategies;
mod views;
mod advisor_lag;
mod snapshot_rows;
