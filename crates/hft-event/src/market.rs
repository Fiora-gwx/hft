//! Market bodies (0x01xx) and the common prefix `MarketHdr`.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{ConnId, InstId, Px, Qty, Rate, Side, VenueId};

use crate::EventKind;
use crate::codes;
use crate::macros::layout;
use crate::schema::FieldTy;

crate::event_body! {
    /// Common prefix of every market body (56 B). Evolves only by occupying `_rsv`.
    pub struct MarketHdr : plain, FromBytes {
        pub venue: VenueId,
        /// Connection that delivered the event (BBO connections: slot number).
        pub conn: ConnId,
        pub _pad: u16,
        pub inst: InstId,
        pub ex_ts_ns: i64,
        pub ex_send_ns: i64,
        pub hw_rx_ns: i64,
        pub rx_tsc: u64,
        /// Kernel software receive stamp; 0 = unavailable.
        pub sw_rx_ns: i64,
        /// Time since the connection's receive queue was last confirmed empty; 0 = no confirmation yet.
        pub rx_empty_gap_ns: u32,
        pub _rsv: u32,
    }
}
layout!(MarketHdr, 56, 8; venue @ 0, conn @ 1, _pad @ 2, inst @ 4, ex_ts_ns @ 8, ex_send_ns @ 16,
    hw_rx_ns @ 24, rx_tsc @ 32, sw_rx_ns @ 40, rx_empty_gap_ns @ 48, _rsv @ 52);

crate::event_body! {
    /// Best bid/offer (0x0101); `update_id` 0 = venue has no sequence.
    pub struct Bbo : fixed(EventKind::BBO, 1), FromBytes {
        pub h: MarketHdr,
        pub bid_px: Px,
        pub bid_qty: Qty,
        pub ask_px: Px,
        pub ask_qty: Qty,
        pub update_id: u64,
    }
}
layout!(Bbo, 96, 8; h @ 0, bid_px @ 56, bid_qty @ 64, ask_px @ 72, ask_qty @ 80, update_id @ 88);

crate::event_body! {
    /// One level of an incremental depth message (0x0102).
    pub struct L2Delta : fixed(EventKind::L2_DELTA, 1), TryFromBytes {
        pub h: MarketHdr,
        pub prev_id: u64,
        pub last_id: u64,
        pub px: Px,
        /// 0 = delete the level.
        pub qty: Qty,
        pub side: Side,
        pub dflags: u8 = FieldTy::Bits(&codes::L2_DFLAGS),
        pub idx: u16,
        pub _pad: u32,
    }
}
layout!(L2Delta, 96, 8; h @ 0, prev_id @ 56, last_id @ 64, px @ 72, qty @ 80, side @ 88, dflags @ 89,
    idx @ 90, _pad @ 92);

impl L2Delta {
    /// `dflags`: last level of the message.
    pub const LAST: u8 = 1;
}

crate::event_body! {
    /// Public trade (0x0104).
    pub struct Trade : fixed(EventKind::TRADE, 1), TryFromBytes {
        pub h: MarketHdr,
        pub px: Px,
        pub qty: Qty,
        pub trade_id: u64,
        pub taker_side: Side,
        pub _pad: [u8; 7],
    }
}
layout!(Trade, 88, 8; h @ 0, px @ 56, qty @ 64, trade_id @ 72, taker_side @ 80, _pad @ 81);

crate::event_body! {
    /// Funding rate (0x0105); `interval_s` = actual settlement period, 0 = unknown.
    pub struct Funding : fixed(EventKind::FUNDING, 1), FromBytes {
        pub h: MarketHdr,
        pub rate: Rate,
        pub next_funding_ns: i64,
        pub interval_s: u32,
        pub _pad: u32,
    }
}
layout!(Funding, 80, 8; h @ 0, rate @ 56, next_funding_ns @ 64, interval_s @ 72, _pad @ 76);

crate::event_body! {
    /// Mark price (0x0106).
    pub struct MarkPrice : fixed(EventKind::MARK_PRICE, 1), FromBytes {
        pub h: MarketHdr,
        pub px: Px,
    }
}
layout!(MarkPrice, 64, 8; h @ 0, px @ 56);

crate::event_body! {
    /// Index price (0x0107).
    pub struct IndexPrice : fixed(EventKind::INDEX_PRICE, 1), FromBytes {
        pub h: MarketHdr,
        pub px: Px,
    }
}
layout!(IndexPrice, 64, 8; h @ 0, px @ 56);

crate::event_body! {
    /// Public liquidation stream (0x0108; not this account's liquidation).
    pub struct Liquidation : fixed(EventKind::LIQUIDATION, 1), TryFromBytes {
        pub h: MarketHdr,
        pub px: Px,
        pub qty: Qty,
        pub side: Side,
        pub _pad: [u8; 7],
    }
}
layout!(Liquidation, 80, 8; h @ 0, px @ 56, qty @ 64, side @ 72, _pad @ 73);

crate::event_body! {
    /// `L2Batch` (0x0109, Var) prefix; followed by `[Level; n_bids + n_asks]` (bids first, qty 0 = delete).
    pub struct L2BatchHdr : var(EventKind::L2_BATCH, 1, 64_000), FromBytes {
        pub h: MarketHdr,
        pub prev_id: u64,
        pub last_id: u64,
        pub n_bids: u16,
        pub n_asks: u16,
        pub bflags: u16 = FieldTy::Bits(&codes::L2_BFLAGS),
        pub _pad: u16,
    }
}
layout!(L2BatchHdr, 80, 8; h @ 0, prev_id @ 56, last_id @ 64, n_bids @ 72, n_asks @ 74, bflags @ 76, _pad @ 78);

impl L2BatchHdr {
    /// `bflags`: first message after a snapshot.
    pub const SNAPSHOT_BRIDGE: u16 = 1;
}
