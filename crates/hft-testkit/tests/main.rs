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
mod r#gen;
mod hash;
mod invariants_map;
mod planted;
mod probes;
mod sdk_mock;
mod sdk_rd;
mod seq_sut;
mod shrink;
mod suites;
mod support;
