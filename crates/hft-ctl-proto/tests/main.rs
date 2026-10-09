//! hft-ctl-proto tests (one binary; each module is one TESTS.md group, golden.rs is its own target).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod accounts;
mod common;
mod confirm;
mod consts;
mod frame;
mod hash;
mod normalize;
mod perm_matrix;
mod perms;
mod prop;
mod response;
mod schema;
