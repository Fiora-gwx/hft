//! hft-refdata integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod alloc;
mod assembly;
mod builder;
mod caps;
mod codes;
mod common;
mod dump;
mod engine;
mod exps;
mod fees;
mod hygiene;
mod ids;
mod integrity;
mod layout;
mod load;
mod params;
mod prop_engine;
mod prop_symbols;
mod query;
mod ranges;
mod rebind;
mod refs;
mod report;
mod schema;
mod steps;
mod strings;
mod structure;
mod symbols;
mod version;
