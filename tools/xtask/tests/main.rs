//! xtask integration tests as one binary: one module per test file (files, function names
//! and TESTS.md locations unchanged); files with process-wide effects are separate targets.
mod ci;
mod common;
mod decoupling_gate;
mod deps;
mod deps_ctl;
mod deps_latency_probe;
mod deps_registry;
mod deps_stratlib;
mod doc_check;
mod doc_check_perms;
mod dst;
mod exit_codes;
mod fixtures_cmd;
mod hot_check_stratlib;
mod journal_verify;
mod new_strategy;
mod purity;
mod replay_verify;
mod scaffold_gate;
mod setup;
mod test_trace;
