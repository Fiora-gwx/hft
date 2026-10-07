//! The two product lines share one implementation; `Line` carries their differences.

use hft_types::VenueId;

/// Product line.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Line {
    /// USDT-M perpetual (`fstream`, `ws-fapi`).
    Usdm,
    /// Spot (`stream`, `ws-api`).
    Spot,
}

impl Line {
    pub const fn venue(self) -> VenueId {
        match self {
            Line::Usdm => VenueId::BINANCE_USDM,
            Line::Spot => VenueId::BINANCE_SPOT,
        }
    }
}
