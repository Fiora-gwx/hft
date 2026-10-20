//! `BinaryMakerParams` (README "参数") and its group-shape `check`.

use hft_strategy_sdk::{GroupTable, ParamError, RefView};

hft_strategy_sdk::define_params! {
    pub struct BinaryMakerParams(schema_id = 7, version = 1, fields = BinaryMakerFields) {
        /// false: no quotes (resting quotes are cancelled)
        enabled: Bool [default = 0, manual_only, relax = Up],
        /// window length in seconds; the slot pair of the window starting at S is (S / window_s) mod pairs
        window_s: Count [min = 60, max = 86_400, default = 300, relax = None],
        /// shares per quote
        clip_shares: Count [min = 1, max = 100_000, default = 10, relax = Up],
        /// net shares (Up minus Down of the current window) beyond which the long side stops quoting
        inv_max_shares: Count [min = 1, max = 1_000_000, default = 100, relax = Up],
        /// lowest bid price quoted, as a fraction of one (1e-9)
        p_min: Rate [min = 0, max = 1_000_000_000, default = 100_000_000, relax = Down],
        /// highest bid price quoted, as a fraction of one (1e-9)
        p_max: Rate [min = 0, max = 1_000_000_000, default = 900_000_000, relax = Up],
        /// minimum implied volatility per square-root second to quote (1e-9; 0.5 bp = 50_000)
        sigma_min: Rate [min = 0, max = 1_000_000_000, default = 50_000, relax = Down],
        /// implied over reference realized volatility (1e-9; 1.39 = 1_390_000_000)
        sigma_scale: Rate [min = 1, max = 10_000_000_000, default = 1_390_000_000, relax = None],
        /// minimum remaining standard deviation to quote, implied sigma times sqrt(seconds left) (1e-9)
        min_rem_sd: Rate [min = 0, max = 1_000_000_000, default = 100_000, relax = Down],
        /// reference move since the last one-second sample, in sigmas, that pulls every quote (1e-9)
        move_k: Rate [min = 1, max = 100_000_000_000, default = 1_000_000_000, relax = Up],
        /// quiet time after such a move
        pause_ms: DurationMs [min = 0, max = 600_000, default = 1_000, relax = Down],
        /// no quotes this close to the window end
        stop_before_ms: DurationMs [min = 0, max = 3_600_000, default = 60_000, relax = Down],
        /// one-second returns in the realized volatility
        vol_window_s: Count [min = 60, max = 7_200, default = 1_800, relax = None],
        /// one-second returns required before quoting
        vol_min_samples: Count [min = 2, max = 7_200, default = 300, relax = Down],
        /// minimum engine time between two quote changes of one leg
        requote_ms: DurationMs [min = 0, max = 60_000, default = 200, relax = Down],
        /// resting limit of every quote (OMS ages it out)
        max_rest_ms: DurationMs [min = 1_000, max = 3_600_000, default = 360_000, relax = Up],
    }
}

/// `ParamError::GroupShape` codes.
pub mod shape {
    /// Not an odd number of groups, at least three (pairs of slots plus the reference).
    pub const COUNT: u16 = 1;
    /// A group does not have exactly one leg.
    pub const LEGS: u16 = 2;
    /// A slot leg is not on the venue of the first slot, or the reference leg is.
    pub const VENUE: u16 = 3;
    /// A leg's inst is not in refdata.
    pub const INST: u16 = 4;
}

/// `ParamError::Invalid` codes.
pub mod invalid {
    /// `p_min > p_max`.
    pub const PRICE_BAND: u16 = 1;
}

impl BinaryMakerParams {
    /// Groups: `2 × pairs` single-leg slot groups on one venue (Up, Down of pair 0, then pair 1, …)
    /// and a last single-leg reference group on another venue.
    pub fn check(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError> {
        let n = groups.len();
        let d = self.defaults();
        if d.p_min > d.p_max {
            return Err(ParamError::Invalid {
                field_ix: 4,
                code: invalid::PRICE_BAND,
            });
        }
        if n < 3 || n.is_multiple_of(2) {
            return Err(ParamError::GroupShape {
                gix: 0,
                code: shape::COUNT,
            });
        }
        let mut slot_venue = None;
        for (gix, g) in groups.iter() {
            let err = |code| ParamError::GroupShape { gix, code };
            if g.n_legs != 1 {
                return Err(err(shape::LEGS));
            }
            let l = g.legs[0];
            if rd.try_inst(l.inst).is_none() {
                return Err(err(shape::INST));
            }
            let reference = gix + 1 == n;
            let v = *slot_venue.get_or_insert(l.venue);
            if (l.venue == v) == reference {
                return Err(err(shape::VENUE));
            }
        }
        Ok(())
    }
}
