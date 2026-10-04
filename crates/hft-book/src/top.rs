//! `Top`: best level of the `Bbo` stream, one cache line.

use hft_event::Bbo;
use hft_types::{Px, Qty, Seq, Tsc};

/// Best bid/ask from the `Bbo` stream (64 B). `rx_tsc` is the `MarketHdr.rx_tsc` of the last
/// applied `Bbo` (measurement and signals only, never quote availability).
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Top {
    pub bid_px: Px,
    pub bid_qty: Qty,
    pub ask_px: Px,
    pub ask_qty: Qty,
    pub update_id: u64,
    pub ex_ts_ns: i64,
    pub rx_tsc: Tsc,
    pub seq: u64,
}
const _: () = assert!(size_of::<Top>() == 64 && align_of::<Top>() == 64);
const _: () = assert!(core::mem::offset_of!(Top, bid_px) == 0);
const _: () = assert!(core::mem::offset_of!(Top, bid_qty) == 8);
const _: () = assert!(core::mem::offset_of!(Top, ask_px) == 16);
const _: () = assert!(core::mem::offset_of!(Top, ask_qty) == 24);
const _: () = assert!(core::mem::offset_of!(Top, update_id) == 32);
const _: () = assert!(core::mem::offset_of!(Top, ex_ts_ns) == 40);
const _: () = assert!(core::mem::offset_of!(Top, rx_tsc) == 48);
const _: () = assert!(core::mem::offset_of!(Top, seq) == 56);

/// Result bits of `Top::apply`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct TopOutcome(pub u8);

impl TopOutcome {
    pub const NONE: TopOutcome = TopOutcome(0);
    pub const BID_PX: TopOutcome = TopOutcome(1);
    pub const ASK_PX: TopOutcome = TopOutcome(2);
    pub const BID_QTY: TopOutcome = TopOutcome(4);
    pub const ASK_QTY: TopOutcome = TopOutcome(8);
    pub const DUPLICATE: TopOutcome = TopOutcome(16);
    pub const OLDER: TopOutcome = TopOutcome(32);
    pub const INVALID: TopOutcome = TopOutcome(64);

    #[inline]
    pub const fn contains(self, o: TopOutcome) -> bool {
        self.0 & o.0 == o.0
    }
    #[inline]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl core::ops::BitOr for TopOutcome {
    type Output = TopOutcome;
    #[inline]
    fn bitor(self, o: TopOutcome) -> TopOutcome {
        TopOutcome(self.0 | o.0)
    }
}

impl core::ops::BitOrAssign for TopOutcome {
    #[inline]
    fn bitor_assign(&mut self, o: TopOutcome) {
        self.0 |= o.0;
    }
}

impl Default for Top {
    fn default() -> Top {
        Top::ZERO
    }
}

impl Top {
    /// All fields 0 (no `Bbo` received yet).
    pub const ZERO: Top = Top {
        bid_px: Px(0),
        bid_qty: Qty(0),
        ask_px: Px(0),
        ask_qty: Qty(0),
        update_id: 0,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        seq: 0,
    };

    /// Applies one `Bbo` recorded at sequence `seq` (README "`Top::apply`").
    #[inline]
    pub fn apply(&mut self, b: &Bbo, seq: Seq) -> TopOutcome {
        if b.update_id != 0 && b.update_id <= self.update_id {
            return if b.update_id == self.update_id {
                TopOutcome::DUPLICATE
            } else {
                TopOutcome::OLDER
            };
        }
        if b.bid_px.0 > 0 && b.ask_px.0 > 0 && b.bid_px >= b.ask_px {
            return TopOutcome::INVALID;
        }
        let mut o = TopOutcome::NONE;
        if b.bid_px != self.bid_px {
            o |= TopOutcome::BID_PX;
        }
        if b.ask_px != self.ask_px {
            o |= TopOutcome::ASK_PX;
        }
        if b.bid_qty != self.bid_qty {
            o |= TopOutcome::BID_QTY;
        }
        if b.ask_qty != self.ask_qty {
            o |= TopOutcome::ASK_QTY;
        }
        self.bid_px = b.bid_px;
        self.bid_qty = b.bid_qty;
        self.ask_px = b.ask_px;
        self.ask_qty = b.ask_qty;
        self.update_id = b.update_id;
        self.ex_ts_ns = b.h.ex_ts_ns;
        self.rx_tsc = Tsc(b.h.rx_tsc);
        self.seq = seq.0;
        o
    }
}
