//! hft-oms integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
#![allow(
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::too_many_arguments,
    clippy::type_complexity
)] // allow-list: test code, not a hot path (crate level, as in the former per-file binaries)
#[cfg(feature = "testkit")]
mod audit;
mod cid;
mod codes;
mod common;
mod config;
mod config_blob;
mod external;
mod flags;
mod fsm;
mod fsm_table;
mod inflight;
mod layout;
mod link;
mod link_props;
mod matching;
mod model;
mod net;
mod notes;
mod out;
mod out_props;
mod price;
mod prop_fsm;
mod quiesce;
mod send;
mod settlement;
mod sim_reset;
mod simul;
mod smoke;
mod snapshot;
mod snapshot_props;
mod sweep;
mod table;
mod timers;
mod tomb;
mod unknown;
mod writeoff;
