//! `hft-testkit`: shared test infrastructure. This build carries the M1 part of the README:
//! proptest generators (`gen`), manual clocks (`clock`), seed derivation for deterministic
//! proptest runners (`seed`) and the per-thread allocation counter (`alloc`). The golden runner,
//! SDK strategy stubs, DST framework, state-hash tools, `HistBench` and `hft-dst` arrive with
//! their milestones (README "源文件").
//!
//! Only `[dev-dependencies]`, `tools/xtask` and the verification bundle may depend on this crate.
//! Nothing here reads the system clock or a global random source: every generator is a pure
//! proptest strategy, and `seed::runner` builds a runner whose RNG is derived from a `u64` seed.

pub mod alloc;
pub mod clock;
pub mod r#gen;
pub mod seed;

pub use alloc::{AllocScope, CountingAlloc};
pub use clock::{ManualTsc, ManualUtc};
pub use r#gen::market::{
    GapPolicy, MarketStreamCfg, MdEvent, OwnedL2Snapshot, QuietPolicy, market_stream, market_stream_with,
};
pub use r#gen::perturb::{Delivery, Perturb, perturb};
