//! The test strategies (README "5. 解耦门槛夹具") and their set `TestStrategies` (kind_ids in the
//! test range 0xFF00–0xFFFF): `probe-taker` 0xFF01, `probe-target` 0xFF02 and `probe-np` 0xFF03
//! (`probe-taker` code, `non_prod_only = true`). The strategy modules use only hft-strategy-sdk and
//! hft-strat-lib (checked by TK-036); this module is the only place that assembles them.

pub mod probe_taker;
pub mod probe_target;
pub mod replay_probe;
pub mod replay_warmup;

pub use probe_taker::{NpParams, ProbeNp, ProbeTaker, TakerParams};
pub use probe_target::{ProbeModel, ProbeTarget, TargetParams};

hft_strategy_sdk::strategy_set! {
    /// Only the test strategies; used by the DST and scenario suites and by engine-crate tests.
    pub struct TestStrategies {
        probe_taker::ProbeTaker,
        probe_target::ProbeTarget,
        probe_taker::ProbeNp,
    }
}

mod replay_set {
    use super::replay_probe;
    hft_strategy_sdk::strategy_set! {
        /// Full-file replay fixture, independent of DST's strategy set and HashTrail.
        pub struct ReplayStrategies { replay_probe::ReplayA }
    }
}
mod replay_path_v2_set {
    use super::replay_probe;
    hft_strategy_sdk::strategy_set! {
        /// Same sample layout and algorithm with an incompatible path version.
        pub struct ReplayPathV2Strategies { replay_probe::ReplayPathV2 }
    }
}
pub use replay_path_v2_set::ReplayPathV2Strategies;
pub use replay_set::ReplayStrategies;

mod replay_warmup_set {
    use super::replay_warmup;
    hft_strategy_sdk::strategy_set! {
        pub struct ReplayWarmupStrategies { replay_warmup::ReplayWarmup }
    }
}
pub use replay_warmup_set::ReplayWarmupStrategies;

/// The test kind_id range.
pub const TEST_KIND_IDS: core::ops::RangeInclusive<u16> = 0xFF00..=0xFFFF;
