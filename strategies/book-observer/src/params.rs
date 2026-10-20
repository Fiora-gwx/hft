//! `BookObserverParams` (README "参数") and its group-shape `check`.

use hft_strategy_sdk::{GroupTable, ParamError, RefView};

hft_strategy_sdk::define_params! {
    pub struct BookObserverParams(schema_id = 6, version = 1, fields = BookObserverFields) {
        /// also subscribe to the trades of every leg (books are always subscribed)
        trades: Bool [default = 1, relax = Up],
    }
}

/// `ParamError::GroupShape` codes.
pub mod shape {
    /// A group has no leg.
    pub const LEGS: u16 = 1;
}

impl BookObserverParams {
    /// Group shape: at least one leg (any venue; the observer never trades).
    pub fn check(&self, groups: &GroupTable, _rd: &RefView<'_>) -> Result<(), ParamError> {
        for (gix, g) in groups.iter() {
            if g.n_legs == 0 {
                return Err(ParamError::GroupShape {
                    gix,
                    code: shape::LEGS,
                });
            }
        }
        Ok(())
    }
}
