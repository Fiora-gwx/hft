//! The hft-testkit integration tests other than the allocation/benchmark ones, as one binary. It
//! installs `CountingAlloc` so `StrategyHarness` checks zero allocation (TK-028).

#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;

mod actors;
mod clock;
mod deps;
mod dst_report;
mod dst_world;
mod faults;
mod fees;
mod r#gen;
mod hash;
mod invariants_map;
mod planted;
mod probes;
mod sdk_mock;
mod sdk_rd;
// The engine adapter's tests (feature `seq`, on in `--all-features` runs): enabling it here as a
// dev-dependency feature would put hft-sequencer into every dependent's dev closure.
#[cfg(feature = "seq")]
mod fee_cycle;
#[cfg(feature = "seq")]
mod journal_config;
#[cfg(feature = "seq")]
mod observations;
#[cfg(feature = "seq")]
mod seq_sut;
mod shrink;
mod suites;
mod support;
