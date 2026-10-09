//! Shared helpers of the DST framework tests (toy engine runs).

use hft_testkit::dst::faults::{FaultCategory, FaultRates};
use hft_testkit::dst::{DstConfig, DstFailure, DstOutcome, RerunPolicy, ToyEngine, run};

/// A short W1 run on the toy engine with the given fault categories at `per_mille` and planted
/// defects.
pub fn cfg(
    seed: u64,
    steps: u64,
    cats: &[FaultCategory],
    per_mille: u32,
    planted: &[&'static str],
) -> DstConfig {
    let mut c = DstConfig::new(seed, "core");
    c.steps = steps;
    c.horizon_ns = 600 * 1_000_000_000;
    let mut r = FaultRates::off();
    for cat in cats {
        r.set(*cat, true, per_mille);
    }
    c.faults = r;
    c.planted = planted.to_vec();
    c.rerun = RerunPolicy::Never;
    c
}

/// First failing seed in `1..=max` of `make(seed)`.
pub fn first_failure(max: u64, make: impl Fn(u64) -> DstConfig) -> Option<(DstConfig, Box<DstFailure>)> {
    for s in 1..=max {
        let c = make(s);
        if let DstOutcome::Fail(f) = run::<ToyEngine>(&c) {
            return Some((c, f));
        }
    }
    None
}
