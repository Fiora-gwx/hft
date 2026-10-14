//! venue-bitget integration tests as one binary: one module per test file (function names follow
//! the TESTS.md case ids); files with process-wide effects are separate targets.
#![allow(clippy::disallowed_types)] // allow-list: test code, not a hot path
mod encode;
mod md;
mod reports;
mod session;
mod support;
