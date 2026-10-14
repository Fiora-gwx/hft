//! `hft-testkit`: shared test infrastructure. This build carries the M1 and M1.5 parts of the
//! README: proptest generators (`gen`), manual clocks (`clock`), seed derivation for deterministic
//! proptest runners (`seed`), the per-thread allocation counter (`alloc`) and the `HistBench`
//! driver (`bench`). The golden runner, SDK strategy stubs, DST framework, state-hash tools and
//! `hft-dst` arrive with their milestones (README "源文件").
//!
//! Only `[dev-dependencies]`, `tools/xtask` and the verification bundle may depend on this crate.
//! Nothing here reads a global random source: every generator is a pure proptest strategy, and
//! `seed::runner` builds a runner whose RNG is derived from a `u64` seed. Only `HistBench` reads
//! the TSC (and calibrates it once through hft-rt when no nominal frequency is known).

pub mod alloc;
pub mod bench;
pub mod clock;
pub mod dst;
pub mod r#gen;
pub mod golden;
pub mod hash;
pub mod scenarios;
#[cfg(feature = "sdk-mock")]
pub mod sdk_mock;
pub mod seed;
#[cfg(feature = "seq")]
pub mod sim_golden;
#[cfg(feature = "seq")]
pub use sim_golden::{GoldenDiff, SimStep, run_sim_case};
#[cfg(feature = "seq")]
pub mod seq_rig;
pub mod strategies;
pub mod tmp;

pub use alloc::{AllocScope, CountingAlloc};
pub use bench::{BenchMode, BenchOutput, HistBench};
pub use clock::{ManualTsc, ManualUtc};
pub use r#gen::market::{
    GapPolicy, MarketStreamCfg, MdEvent, OwnedL2Snapshot, QuietPolicy, market_stream, market_stream_with,
};
pub use r#gen::perturb::{Delivery, Perturb, perturb};
#[cfg(feature = "sdk-mock")]
pub use sdk_mock::{CtxCapture, MockViews, ModelHarness, StrategyHarness};
pub use strategies::TestStrategies;
