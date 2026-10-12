//! hft-event integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::large_enum_variant,
    clippy::type_complexity
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod bodies;
mod cmd;
mod codec;
mod common;
mod compile;
mod deps;
mod endian;
mod enums;
mod flags;
mod kinds;
mod layout;
mod measure;
mod notes;
mod prop_cmd;
mod prop_codec;
mod prop_spb1;
mod prop_var;
mod reconcile;
mod ring;
#[path = "common/spb.rs"]
mod spb;
mod spb1;
mod timer;
mod var;
mod version;
