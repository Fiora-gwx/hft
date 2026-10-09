//! `MarketLast` (section 10) and BBO connection liveness (section 26): per-inst last trade,
//! funding, mark, index, BBO member slots and flags; per-(venue, slot) `ConnLive`; the by-value
//! `BboView` projection (README "连接活性", "只读视图").

use hft_book::{BookSet, BookState, Top};
use hft_strategy_sdk::{BboView, FundingView, RxTsSrc};
use hft_types::{InstId, MAX_BBO_CONN_SLOTS, Px, Qty, Rate, Tsc, VenueId};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub const SECTION_MARKET_LAST: u16 = 10;
pub const MARKET_LAST_VERSION: u16 = 6;
pub const SECTION_CONN_LIVE: u16 = 26;
pub const CONN_LIVE_VERSION: u16 = 2;

/// `bbo_flags` bits.
pub const BBO_RESYNC: u8 = 1;
pub const BBO_SW_TS: u8 = 2;

/// Section 10 row (112 B, v6; key inst).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct MarketLastRow {
    pub inst: InstId,
    pub interval_s: u32,
    pub trade_px: Px,
    pub trade_qty: Qty,
    pub trade_ts_ns: i64,
    pub funding_rate: Rate,
    pub next_funding_ns: i64,
    pub mark_px: Px,
    pub index_px: Px,
    pub mark_ts_ns: i64,
    pub settled_cut_ns: i64,
    pub bbo_member_mask: u8,
    pub bbo_flags: u8,
    pub _rsv: [u8; 6],
    pub bbo_hw_rx_ns: i64,
    pub mark_ens: i64,
    pub prev_interval_s: u32,
    pub _pad2: u32,
}
const _: () = assert!(core::mem::size_of::<MarketLastRow>() == 112);
const _: () = assert!(core::mem::offset_of!(MarketLastRow, funding_rate) == 32);
const _: () = assert!(core::mem::offset_of!(MarketLastRow, mark_px) == 48);
const _: () = assert!(core::mem::offset_of!(MarketLastRow, settled_cut_ns) == 72);
const _: () = assert!(core::mem::offset_of!(MarketLastRow, bbo_member_mask) == 80);
const _: () = assert!(core::mem::offset_of!(MarketLastRow, bbo_hw_rx_ns) == 88);
const _: () = assert!(core::mem::offset_of!(MarketLastRow, mark_ens) == 96);
const _: () = assert!(core::mem::offset_of!(MarketLastRow, prev_interval_s) == 104);
/// Size of a v5 row (no `prev_interval_s`).
pub const MARKET_LAST_V5_SIZE: usize = 104;

impl MarketLastRow {
    /// Whether the row carries anything worth snapshotting.
    pub fn is_empty(&self) -> bool {
        let mut z = *self;
        z.inst = InstId(0);
        z == MarketLastRow::default()
    }
}

/// Section 26 row (16 B, v2; key (venue, slot)).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct ConnLiveRow {
    pub venue: VenueId,
    pub slot: u8,
    /// UP=1 RESYNC=2.
    pub flags: u8,
    pub _pad: [u8; 5],
    pub live_tsc: Tsc,
}
const _: () = assert!(core::mem::size_of::<ConnLiveRow>() == 16);
const _: () = assert!(core::mem::offset_of!(ConnLiveRow, flags) == 2);
const _: () = assert!(core::mem::offset_of!(ConnLiveRow, live_tsc) == 8);

pub const CONN_UP: u8 = 1;
pub const CONN_RESYNC: u8 = 2;

const SLOTS: usize = MAX_BBO_CONN_SLOTS as usize;

/// One (venue, slot) liveness entry.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct ConnLive {
    pub up: bool,
    pub resync: bool,
    pub live_tsc: u64,
    /// Used in this run (only these go into section 26).
    pub used: bool,
}

/// Per-inst values that are not in the snapshot (this run only).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct MemOnly {
    /// `MarketHdr.rx_tsc` of the last applied `Funding` and its seq.
    pub funding_rx_tsc: u64,
    pub funding_seq: u64,
}

/// `MarketLast` rows (dense by `InstId`) plus BBO connection liveness.
#[derive(Clone, Debug)]
pub struct Market {
    pub rows: Vec<MarketLastRow>,
    pub mem: Vec<MemOnly>,
    /// `[venue][slot]`.
    pub conns: Vec<[ConnLive; SLOTS]>,
    /// `max(boot_seq, last snapshot at_seq)`: receive times at or before it are invisible.
    pub rx_from_seq: u64,
    /// Precomputed funding views (by inst) for `Views::funding`.
    pub funding: Vec<FundingView>,
    /// Inst → venue (refdata does not change during a run).
    pub inst_venue: Vec<VenueId>,
}

impl Market {
    pub fn new(rd: &hft_refdata::RefData) -> Market {
        let n_insts = rd.inst_count();
        let inst_venue = (0..=n_insts).map(|i| rd.inst(InstId(i as u32)).venue).collect();
        Market {
            inst_venue,
            rows: vec![MarketLastRow::default(); n_insts + 1],
            mem: vec![MemOnly::default(); n_insts + 1],
            conns: vec![[ConnLive::default(); SLOTS]; 256],
            rx_from_seq: 0,
            funding: vec![FundingView::EMPTY; n_insts + 1],
        }
    }
    #[inline]
    pub fn row(&self, i: InstId) -> Option<&MarketLastRow> {
        self.rows.get(i.0 as usize)
    }
    #[inline]
    pub fn row_mut(&mut self, i: InstId) -> Option<&mut MarketLastRow> {
        self.rows.get_mut(i.0 as usize)
    }
    #[inline]
    pub fn conn(&self, v: VenueId, slot: u8) -> Option<&ConnLive> {
        self.conns.get(usize::from(v.0))?.get(usize::from(slot))
    }
    #[inline]
    pub fn conn_mut(&mut self, v: VenueId, slot: u8) -> Option<&mut ConnLive> {
        self.conns.get_mut(usize::from(v.0))?.get_mut(usize::from(slot))
    }
    /// Clears every this-run value (processing `EngineClock`).
    pub fn clear_run(&mut self) {
        for v in self.conns.iter_mut() {
            *v = [ConnLive::default(); SLOTS];
        }
        for r in self.rows.iter_mut() {
            r.bbo_member_mask = 0;
            r.bbo_flags &= !BBO_RESYNC;
        }
    }
    /// Rebuilds `funding[i]` after a change of the row or of `rx_from_seq`.
    pub fn refresh_funding(&mut self, i: InstId) {
        let ix = i.0 as usize;
        let (Some(r), Some(m)) = (self.rows.get(ix), self.mem.get(ix)) else {
            return;
        };
        let upd = if m.funding_seq > self.rx_from_seq {
            Tsc(m.funding_rx_tsc)
        } else {
            Tsc(0)
        };
        let fv = FundingView {
            rate: r.funding_rate,
            next_settle_ns: r.next_funding_ns,
            interval_s: r.interval_s,
            prev_interval_s: r.prev_interval_s,
            upd_tsc: upd,
        };
        if let Some(f) = self.funding.get_mut(ix) {
            *f = fv;
        }
    }
    /// Sets `rx_from_seq` (after a snapshot) and refreshes the funding views.
    pub fn set_rx_from(&mut self, seq: u64) {
        self.rx_from_seq = seq;
        for ix in 0..self.rows.len() {
            if self.mem.get(ix).is_some_and(|m| m.funding_seq != 0) {
                self.refresh_funding(InstId(ix as u32));
            }
        }
    }
    /// The by-value projection (README "连接活性").
    pub fn bbo_view(&self, books: &BookSet, i: InstId) -> BboView {
        let top: &Top = books.top(i);
        let row = self.row(i).copied().unwrap_or_default();
        let venue = self.inst_venue.get(i.0 as usize).copied().unwrap_or(VenueId(0));
        let mut conn_up = false;
        let mut best_live = 0u64;
        let mut best_any = 0u64;
        let mut any_clean = false;
        let mask = row.bbo_member_mask;
        if let Some(slots) = self.conns.get(usize::from(venue.0)) {
            for (k, c) in slots.iter().enumerate() {
                if mask & (1u8 << k) == 0 {
                    continue;
                }
                if c.up {
                    conn_up = true;
                    best_any = best_any.max(c.live_tsc);
                    if !c.resync {
                        any_clean = true;
                        best_live = best_live.max(c.live_tsc);
                    }
                }
            }
        }
        let book_resync = books.l2(i).is_some_and(|b| b.state() == BookState::Resyncing);
        let resync = !any_clean || row.bbo_flags & BBO_RESYNC != 0 || book_resync;
        let rx_tsc = if top.seq > self.rx_from_seq { top.rx_tsc } else { Tsc(0) };
        let src = if row.bbo_hw_rx_ns == 0 {
            RxTsSrc::None
        } else if row.bbo_flags & BBO_SW_TS != 0 {
            RxTsSrc::Sw
        } else {
            RxTsSrc::Hw
        };
        BboView {
            bid_px: top.bid_px,
            bid_qty: top.bid_qty,
            ask_px: top.ask_px,
            ask_qty: top.ask_qty,
            update_id: top.update_id,
            ex_ts_ns: top.ex_ts_ns,
            rx_tsc,
            hw_rx_ns: row.bbo_hw_rx_ns,
            rx_ts_src: src,
            seq: top.seq,
            conn_live_tsc: Tsc(if any_clean { best_live } else { best_any }),
            conn_up,
            resync,
        }
    }
}


impl Default for MarketLastRow {
    fn default() -> MarketLastRow {
        <MarketLastRow as zerocopy::FromZeros>::new_zeroed()
    }
}

impl Default for ConnLiveRow {
    fn default() -> ConnLiveRow {
        <ConnLiveRow as zerocopy::FromZeros>::new_zeroed()
    }
}
