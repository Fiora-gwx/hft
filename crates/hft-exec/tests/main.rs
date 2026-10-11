//! hft-exec integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod accept;
mod advance;
mod band;
mod broken;
mod cap;
mod cell;
mod class;
mod common;
mod config;
mod config_blob;
mod decision;
mod deps;
mod detail;
mod drain;
mod entry;
mod external;
mod fix;
mod flatten;
mod force;
mod fsm;
mod gate;
mod hold;
mod layout;
mod lint;
mod lots;
mod modes;
mod output;
mod pair;
mod params;
mod price;
mod props;
mod quote;
mod recovery_gate;
mod residual;
mod resolve;
mod restore;
mod simul;
mod snapshot;
mod taker;
mod timers;
