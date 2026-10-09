//! hft-sequencer tests (one binary; each module is one TESTS.md group).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod advice;
mod advisor;
mod advisor_lag;
mod boot;
mod clock;
mod commands;
mod common;
mod conflate;
mod drain;
mod env;
mod exec_glue;
mod fanout;
mod funding;
mod intents;
mod keyframe;
mod liveness;
mod mismatch;
mod mode;
mod params;
mod poison;
mod preempt;
mod process;
mod recon;
mod recover;
mod ring_full;
mod sim_route;
mod snapshot;
mod snapshot_rows;
mod storage;
mod strategies;
mod subs;
mod timers;
mod version;
mod viewer_confirm;
mod views;
