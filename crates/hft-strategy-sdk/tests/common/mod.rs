//! Shared fixtures of the SDK tests (TESTS.md "共用夹具").
#![allow(dead_code, unused_imports)]

pub mod alloc;
pub mod echo;
pub mod events;
pub mod model;
pub mod params;
pub mod path_script;
pub mod rd;
pub mod target;

pub use echo::*;
pub use events::*;
pub use model::*;
pub use params::*;
pub use path_script::*;
pub use rd::*;
pub use target::*;

use hft_strategy_sdk::testing::ev::Ev;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, ModelHarness, Recorded};
use hft_strategy_sdk::{EngineClockAnchor, Model, Now, Strategy, StrategyInstance};
use hft_types::{GroupId, Tsc};

/// UTC of TSC 0 in `clocked_views` (2025-10-01 00:00:30 UTC: 30 s into a minute).
pub const UTC0: i64 = 1_759_276_830_000_000_000;
/// Engine time of TSC 0 in `clocked_views`.
pub const ENS0: i64 = 1_000_000_000_000;

/// GT3 `MockViews` on a 1 GHz run: TSC 0 = engine time `ENS0` = UTC `UTC0`, `boot_seq = 1`.
pub fn clocked_views() -> MockViews {
    let mut v = gt3_views();
    let clk = EngineClockAnchor {
        base_engine_ns: ENS0,
        base_tsc: Tsc(0),
        tsc_hz_milli: 1_000_000_000_000,
        utc_tsc: Tsc(0),
        utc_ns: UTC0,
    };
    v.at(Now::new(Tsc(0), 1, 1, true, clk));
    v
}

/// The test instance S3 on A1 of kind `kind_id`.
pub fn s3(kind_id: u16) -> StrategyInstance {
    StrategyInstance {
        strategy: S3,
        account: A1,
        kind_id,
    }
}

/// Delivers `ev` to the strategy through the harness.
pub fn fast<S: Strategy>(h: &mut CtxHarness, s: &mut S, ev: &Ev, v: &MockViews) -> Recorded {
    h.run(|ctx| s.on_event(&ev.as_ref(), v, ctx))
}

/// Delivers `ev` to the Model through the harness; returns its Advice.
pub fn model<M: Model>(
    h: &mut ModelHarness,
    m: &mut M,
    ev: &Ev,
    v: &MockViews,
) -> Vec<(GroupId, u16, Vec<u8>)> {
    h.run(|out| m.on_event(&ev.as_ref(), v, out))
}
