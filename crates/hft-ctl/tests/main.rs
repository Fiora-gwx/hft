//! hft-ctl tests (one binary; each module is one TESTS.md group).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod audit;
mod auth;
mod client;
mod common;
mod config;
mod e2e;
mod expiry;
mod generic;
mod order;
mod passthrough;
mod principals;
mod prop;
mod rate;
mod result;
mod ring;
mod schema;
mod set_params;
mod task;
mod validate;
