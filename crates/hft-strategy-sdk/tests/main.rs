//! hft-strategy-sdk integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::type_complexity
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod advice;
mod checkpoint;
mod cmd;
mod common;
mod ctx;
mod determinism;
mod group;
mod group_reset;
mod harness;
mod layout;
mod minor_change;
mod mock_views;
mod model_math;
mod params_codec;
mod params_decode;
mod params_golden;
mod params_macro;
mod params_text;
mod prelude;
mod probe_fast;
mod quote;
mod refview;
mod schema_json;
mod set_cfg;
mod set_dispatch;
mod snapshot;
mod time;
mod ui;
mod warmup;
