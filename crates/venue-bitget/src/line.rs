//! The two product lines share one implementation; `Line` carries their differences.

use hft_types::VenueId;

/// Product line.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Line {
    /// USDT-M perpetual (`usdt-futures`).
    Fut,
    /// Spot (`spot`).
    Spot,
}

impl Line {
    pub const fn venue(self) -> VenueId {
        match self {
            Line::Fut => VenueId::BITGET_UTA_USDT_FUT,
            Line::Spot => VenueId::BITGET_UTA_SPOT,
        }
    }

    /// WS `instType` of public subscriptions and the `category` of WS trade frames.
    pub const fn ws_type(self) -> &'static [u8] {
        match self {
            Line::Fut => b"usdt-futures",
            Line::Spot => b"spot",
        }
    }

    /// REST `category`.
    pub const fn rest_category(self) -> &'static [u8] {
        match self {
            Line::Fut => b"USDT-FUTURES",
            Line::Spot => b"SPOT",
        }
    }

    /// Whether a pushed `category`/`instType` names this line (either case). V2 spellings
    /// (`USDT-FUTURES`, `SPOT`) compare equal.
    pub fn is_category(self, s: &str) -> bool {
        s.as_bytes().eq_ignore_ascii_case(self.ws_type())
    }
}
