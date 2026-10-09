//! hft-book integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::type_complexity
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod batch;
mod codes;
mod common;
mod delta;
mod export;
mod layout;
mod model;
mod overflow;
mod params;
mod prop_batch;
mod prop_dedup;
mod prop_model;
mod prop_recenter;
mod prop_restore;
mod prop_topn;
mod recenter;
mod seqgen;
mod set;
mod snapshot;
mod snapshot_rows;
mod state;
mod top;
mod topn;
