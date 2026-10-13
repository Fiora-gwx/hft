//! The hft-testkit integration tests other than the allocation/benchmark ones, as one binary. It
//! installs `CountingAlloc` so `StrategyHarness` checks zero allocation (TK-028).

#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;

mod actors;
#[cfg(feature = "seq")]
mod cancel_filled;
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
/// Heavy DST migration proofs (TK-137/157/163/169/173: 50 conformance seeds × three real-engine
/// runs of 2 000 steps each): feature `dst-heavy`, outside the default local test binary; the CI
/// `heavy-tests` job runs every `heavy::` test and `cargo xtask ci` skips them
/// (docs/development.md).
#[cfg(feature = "dst-heavy")]
mod heavy {
    #[path = "../cancel_migration.rs"]
    mod cancel_migration;
    #[path = "../late_migration.rs"]
    mod late_migration;
    #[path = "../partial_migration.rs"]
    mod partial_migration;
    #[path = "../reconcile_migration.rs"]
    mod reconcile_migration;
    #[path = "../stale_migration.rs"]
    mod stale_migration;
}
#[cfg(feature = "seq")]
mod history;
mod invariants_map;
#[cfg(feature = "seq")]
mod late_recycle;
#[cfg(feature = "seq")]
mod ledger_case;
#[cfg(feature = "seq")]
mod partial_fills;
mod planted;
#[cfg(feature = "planted")]
mod planted_seq;
mod probes;
#[cfg(feature = "seq")]
mod reconcile_actor;
#[cfg(feature = "seq")]
mod reconcile_world;
mod replay_probe;
mod sdk_mock;
mod sdk_rd;
mod tmp;
mod truth;
#[cfg(feature = "seq")]
mod vacuity;
// The engine adapter's tests (feature `seq`, on in `--all-features` runs): enabling it here as a
// dev-dependency feature would put hft-sequencer into every dependent's dev closure.
#[cfg(feature = "seq")]
mod fee_cycle;
#[cfg(feature = "seq")]
mod fill_order;
#[cfg(feature = "seq")]
mod journal_config;
#[cfg(feature = "seq")]
mod journal_replay;
#[cfg(feature = "seq")]
mod model_journal;
#[cfg(feature = "seq")]
mod observations;
#[cfg(feature = "seq")]
mod seq_sut;
mod shrink;
#[cfg(feature = "seq")]
mod snapshot_checks;
#[cfg(feature = "seq")]
mod stale_gen;
mod suites;
mod support;
