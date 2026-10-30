//! `PmTakerProbeParams` (README "参数") and its group-shape `check`.

use hft_strategy_sdk::{GroupTable, ParamError, RefView};

hft_strategy_sdk::define_params! {
    pub struct PmTakerProbeParams(schema_id = 8, version = 2, fields = PmTakerProbeFields) {
        /// a change from 0 to 1 arms one round (a spend buy, then the sell-back); nothing else starts one
        enabled: Bool [default = 0, manual_only, relax = Up],
        /// quote the buy spends at most (Notional, 1e-6 of the quote asset); hard cap 2
        spend_usdc: Notional [min = 1, max = 2_000_000, default = 1_000_000, relax = Up],
        /// after the buy ends, sell the group leg's whole position (rounded down to the lot) at the best bid
        sell_back: Bool [default = 1, relax = None],
        /// wait this long after the buy ends before the sell-back: an on-chain venue credits bought tokens
        /// only once the trade is mined (about 2 s); a sell before that is refused (balance 0)
        sell_delay_ms: DurationMs [since = 2, min = 0, max = 60_000, default = 0, relax = None],
        /// a refused sell-back is sent again after 1 s, at most this many times
        sell_retries: Count [since = 2, min = 0, max = 20, default = 0, relax = None],
    }
}

/// `ParamError::GroupShape` codes.
pub mod shape {
    /// The group does not have exactly one leg.
    pub const LEGS: u16 = 1;
    /// More than one group.
    pub const GROUPS: u16 = 2;
}

impl PmTakerProbeParams {
    /// Group shape: exactly one group with exactly one leg (the inst probed).
    pub fn check(&self, groups: &GroupTable, _rd: &RefView<'_>) -> Result<(), ParamError> {
        for (gix, g) in groups.iter() {
            if gix > 0 {
                return Err(ParamError::GroupShape {
                    gix,
                    code: shape::GROUPS,
                });
            }
            if g.n_legs != 1 {
                return Err(ParamError::GroupShape {
                    gix,
                    code: shape::LEGS,
                });
            }
        }
        Ok(())
    }
}
