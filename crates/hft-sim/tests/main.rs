//! hft-sim integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(clippy::too_many_arguments, clippy::type_complexity)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
mod book;
mod commands;
mod common;
mod config;
mod events;
mod fees;
mod funding;
mod glue;
mod maker;
mod margin;
mod prop_determinism;
mod prop_invariants;
mod prop_margin;
mod rules;
mod settlement;
mod spot;
mod taker;
mod time;
mod venue_model;
