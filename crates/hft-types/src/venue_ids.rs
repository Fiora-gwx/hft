//! `VenueId` constant table (protocol level; numbers are never reused, new venues append).

use crate::types::{MarketKind, VenueId};

impl VenueId {
    pub const BINANCE_USDM: VenueId = VenueId(1);
    pub const BINANCE_SPOT: VenueId = VenueId(2);
    pub const GATE_USDT_FUT: VenueId = VenueId(3);
    pub const GATE_SPOT: VenueId = VenueId(4);
    pub const OKX_SWAP: VenueId = VenueId(5);
    pub const OKX_SPOT: VenueId = VenueId(6);
    // 7: reserved (BINANCE_COINM), never assigned.
    pub const BYBIT_LINEAR: VenueId = VenueId(8);
    pub const BYBIT_SPOT: VenueId = VenueId(9);
    pub const BITGET_UTA_USDT_FUT: VenueId = VenueId(10);
    pub const BITGET_UTA_SPOT: VenueId = VenueId(11);
    pub const KUCOIN_FUT: VenueId = VenueId(12);
    pub const KUCOIN_SPOT: VenueId = VenueId(13);
    pub const HYPERLIQUID_PERP: VenueId = VenueId(14);
    /// Polymarket CLOB outcome tokens (one spot-like instrument per token).
    pub const POLYMARKET_CLOB: VenueId = VenueId(15);
    pub const CHAIN_BSC: VenueId = VenueId(32);
    pub const CHAIN_SOLANA: VenueId = VenueId(33);

    /// Market segment of the venue; chain venues, reserved and unassigned numbers → `None`.
    #[inline]
    pub const fn market(self) -> Option<MarketKind> {
        match self.0 {
            1 | 3 | 5 | 8 | 10 | 12 | 14 => Some(MarketKind::LinearPerp),
            2 | 4 | 6 | 9 | 11 | 13 | 15 => Some(MarketKind::Spot),
            _ => None,
        }
    }

    /// Constant name (e.g. `"BINANCE_USDM"`); reserved and unassigned numbers → `None`.
    #[inline]
    pub const fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            1 => "BINANCE_USDM",
            2 => "BINANCE_SPOT",
            3 => "GATE_USDT_FUT",
            4 => "GATE_SPOT",
            5 => "OKX_SWAP",
            6 => "OKX_SPOT",
            8 => "BYBIT_LINEAR",
            9 => "BYBIT_SPOT",
            10 => "BITGET_UTA_USDT_FUT",
            11 => "BITGET_UTA_SPOT",
            12 => "KUCOIN_FUT",
            13 => "KUCOIN_SPOT",
            14 => "HYPERLIQUID_PERP",
            15 => "POLYMARKET_CLOB",
            32 => "CHAIN_BSC",
            33 => "CHAIN_SOLANA",
            _ => return None,
        })
    }
}
