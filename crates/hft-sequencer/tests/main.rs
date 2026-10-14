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
mod checkpoint_file;
mod clock;
mod commands;
mod common;
mod config_blob;
mod conflate;
mod drain;
mod env;
mod exec_glue;
mod fanout;
mod funding;
mod hash;
mod intents;
mod keyframe;
mod lint;
mod liveness;
mod mem_journal;
mod mismatch;
mod mode;
mod notify_backlog;
mod oms_integ;
mod params;
mod poison;
mod preempt;
mod process;
mod prop_fee_only;
mod recon;
mod recover;
mod replay_bytes;
mod replay_config;
#[cfg(all(target_os = "linux", feature = "live", feature = "replay-io"))]
mod replay_file;
mod ring_full;
mod scenarios;
mod scenarios_exec;
mod scenarios_hedge;
mod scenarios_oms;
mod scenarios_risk;
mod sdk_contract;
mod settlement;
mod sim_route;
mod snapshot;
#[cfg(feature = "replay-io")]
mod snapshot_diff;
mod snapshot_rows;
mod storage;
mod strategies;
mod subs;
mod timer_recovery;
mod timers;
mod version;
mod viewer_confirm;
mod views;
