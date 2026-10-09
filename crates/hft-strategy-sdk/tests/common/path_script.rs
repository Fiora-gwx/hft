//! Fixed event scripts: the SDK-077 path-golden script (3 groups, 200 one-second samples each,
//! crossing 3 UTC 60 s boundaries, a model tick every second) and helpers.

use hft_strategy_sdk::testing::ev;
use hft_strategy_sdk::testing::{MockViews, ModelHarness};
use hft_strategy_sdk::{GIX_NONE, MODEL_TICK, Model};
use hft_types::{GROUP_NONE, Px, Qty, Tsc};

use super::{I1, model};

/// Deterministic mid of group `gix` at second `sec`.
pub fn mid_at(gix: u16, sec: u64) -> i64 {
    6_500_000 + i64::from(gix + 1) * 100 * ((sec as i64 * 7) % 13) + sec as i64
}

/// One second of the script: a Bbo per group (gix < n), then a model tick, at TSC `sec` × 10^9.
pub fn script_second<M: Model>(m: &mut M, v: &mut MockViews, h: &mut ModelHarness, n: u16, sec: u64) {
    v.at_tsc(Tsc(sec * 1_000_000_000));
    for gix in 0..n {
        let mid = mid_at(gix, sec);
        v.bbo(
            I1,
            (Px(mid - 50), Qty(100_000_000)),
            (Px(mid + 50), Qty(100_000_000)),
        );
        model(h, m, &ev::bbo(I1, gix, v.bbo_of(I1)), v);
    }
    model(h, m, &ev::timer(MODEL_TICK, GROUP_NONE, GIX_NONE), v);
}

/// The SDK-077 script over seconds 1..=200 (UTC0 is 30 s into a minute: boundaries at 30, 90, 150).
pub fn path_script<M: Model>(m: &mut M, v: &mut MockViews, h: &mut ModelHarness) {
    for sec in 1..=200u64 {
        script_second(m, v, h, 3, sec);
    }
}
