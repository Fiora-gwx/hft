//! hft-sequencer tests (one binary; each module is one TESTS.md group).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
mod params;
mod preempt;
mod process;
mod boot;
mod clock;
mod intents;
mod keyframe;
mod conflate;
mod env;
mod exec_glue;
mod funding;
mod liveness;
mod mode;
mod recon;
mod recover;
mod ring_full;
mod sim_route;
mod snapshot;
mod subs;
mod timers;
mod strategies;
mod views;
mod advisor;
mod advisor_lag;
mod advice;
mod snapshot_rows;
