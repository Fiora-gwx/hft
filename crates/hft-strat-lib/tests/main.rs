//! hft-strat-lib integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
mod book;
mod common;
mod confirm;
mod cost;
mod dirty;
mod expiring;
mod funding;
mod impact;
mod layout;
mod props;
mod restart;
mod rf;
mod spread;
mod throttle;
mod trigger;
