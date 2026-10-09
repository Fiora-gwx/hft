//! hft-types integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::type_complexity
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod clid;
mod codes;
mod compile_fail;
mod err;
mod fixed;
mod format;
mod funding;
mod hedge;
mod ids;
mod kill;
mod layout;
mod parse;
mod prop_clid;
mod prop_fixed;
mod prop_funding;
mod prop_hedge;
mod prop_ids;
mod prop_parse;
mod prop_total;
mod prop_u256;
mod reference;
mod u256;
