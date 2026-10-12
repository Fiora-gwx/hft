//! The hft-testkit integration tests other than the allocation/benchmark ones, as one binary. It
//! installs `CountingAlloc` so `StrategyHarness` checks zero allocation (TK-028).

#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;

mod actors;
mod clock;
mod deps;
#[cfg(feature = "dst-bin")]
mod dst_cli;
mod dst_report;
mod dst_world;
#[cfg(feature = "seq")]
mod external_receipts;
mod faults;
mod fees;
#[cfg(feature = "seq")]
mod foreign_ledger;
mod r#gen;
mod hash;
mod invariants_map;
#[cfg(feature = "seq")]
mod ledger_case;
mod planted;
mod probes;
#[cfg(feature = "seq")]
mod reconcile_actor;
#[cfg(feature = "seq")]
mod reconcile_migration;
#[cfg(feature = "seq")]
mod reconcile_world;
mod replay_probe;
mod sdk_mock;
mod sdk_rd;
mod truth;
// The engine adapter's tests (feature `seq`, on in `--all-features` runs): enabling it here as a
// dev-dependency feature would put hft-sequencer into every dependent's dev closure.
#[cfg(feature = "seq")]
mod fee_cycle;
#[cfg(feature = "seq")]
mod fill_order;
#[cfg(feature = "seq")]
mod journal_config;
#[cfg(feature = "seq")]
mod observations;
#[cfg(feature = "seq")]
mod seq_sut;
mod shrink;
#[cfg(feature = "seq")]
mod snapshot_checks;
mod suites;
mod support;
