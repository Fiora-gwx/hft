//! Snapshot sections 1 (Positions), 13 (LedgerGroups) and 22 (LedgerInventory): row formats,
//! `snapshot` and `restore` (README "快照行格式").

// zerocopy's derive helpers inside `row!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_refdata::RefData;
use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};
use hft_types::{AccountId, AssetId, GroupId, INST_NONE, InstId, Notional, Px, Qty, Rate, VenueId};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::config::{Capacities, LedgerConfig};
use crate::dedup::ExtKey;
use crate::fixed::{i128_words, words_i128};
use crate::ledger::{FundPend, FundRec, GF_ROUND_OPEN, Ledger, Leg, NoHist};
use crate::types::*;

pub const SECTION_POSITIONS: u16 = 1;
pub const SECTION_LEDGER_GROUPS: u16 = 13;
pub const SECTION_LEDGER_INVENTORY: u16 = 22;
/// Current row versions (`SectionHdr.version`).
pub const POSITIONS_VERSION: u16 = 2;
pub const LEDGER_GROUPS_VERSION: u16 = 8;
pub const LEDGER_CASH_RECEIPTS_VERSION: u16 = 9;
pub const LEDGER_INVENTORY_VERSION: u16 = 2;

/// `LedgerRowHdr.kind` codes.
pub mod row_kind {
    pub const ACCOUNT: u8 = 1;
    pub const RATE: u8 = 2;
    pub const GROUP: u8 = 3;
    pub const PENDING: u8 = 4;
    pub const CASH: u8 = 5;
    pub const INV: u8 = 6;
    pub const MARGIN: u8 = 7;
    pub const FUND_PENDING: u8 = 8;
    pub const FUND_MARK: u8 = 9;
    pub const FUND_HIST: u8 = 10;
    pub const FUND_NO_HIST: u8 = 11;
    pub const FUND_SEEN: u8 = 12;
    pub const EXT_SEEN: u8 = 13;
    pub const AUTO_HELD: u8 = 14;
    pub const PX: u8 = 15;
    pub const AUTO_CASH: u8 = 16;
}

macro_rules! row {
    ($(#[$m:meta])* $name:ident, $size:literal { $($f:ident : $t:ty),* $(,)? }) => {
        $(#[$m])*
        #[repr(C)]
        #[derive(Copy, Clone, Eq, PartialEq, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
        pub struct $name { $(pub $f: $t),* }
        const _: () = assert!(size_of::<$name>() == $size);
    };
}

row!(
    /// Section 1: one per (account, inst) with a non-zero `qty`, `cost`, `realized` or `fees`.
    PositionRow, 64 {
        account: AccountId, _pad: [u8; 3], inst: InstId, qty: Qty, cost: [u64; 2], realized_quote: [u64; 2],
        fees_quote: i64, last_fill_ens: i64,
    }
);
const _: () = assert!(core::mem::offset_of!(PositionRow, inst) == 4);
const _: () = assert!(core::mem::offset_of!(PositionRow, qty) == 8);
const _: () = assert!(core::mem::offset_of!(PositionRow, cost) == 16);
const _: () = assert!(core::mem::offset_of!(PositionRow, realized_quote) == 32);
const _: () = assert!(core::mem::offset_of!(PositionRow, fees_quote) == 48);
const _: () = assert!(core::mem::offset_of!(PositionRow, last_fill_ens) == 56);

row!(
    /// Prefix of every row of sections 13 and 22.
    LedgerRowHdr, 8 { kind: u8, account: AccountId, venue: VenueId, _pad: u8, id: u32 }
);
const _: () = assert!(core::mem::offset_of!(LedgerRowHdr, id) == 4);

row!(AccountRow, 88 { day: PnlBreakdown, held_notional: Notional, upnl: Notional, funding_residual: Notional });
row!(
    /// `rflags` is written 0: staleness is recomputed after the run start.
    RateRow, 32 { bid: Px, ask: Px, ens: i64, rflags: u8, _pad: [u8; 7] }
);
row!(
    /// `gflags`: REORG 1, STALE 2, ADOPTED 4, ROUND_OPEN 8. The trailing leg fields (last fill
    /// time, leg insts, net quantities and costs) persist the legs.
    GroupRow, 552 {
        pnl: PnlBreakdown, day: PnlBreakdown, held_notional: Notional, peak_notional: Notional, round_id: u32,
        tag: u32, gflags: u8, _pad: [u8; 7], open_seq: u64, open_ens: i64, round_pnl: PnlBreakdown,
        legs: [LegAgg; 4], last_fill_ens: i64, leg_inst: [u32; 4], leg_net: [Qty; 4], leg_cost: [[u64; 2]; 4],
    }
);
row!(
    CashRow, 64 {
        total: Qty, baseline: Qty, unassigned: Qty, rep_free: Qty, rep_locked: Qty, rep_ex_ts_ns: i64, rep_ens: i64,
        bflags: u8, _pad: [u8; 7],
    }
);
row!(InvRow, 16 { group: GroupId, _pad: u32, qty: Qty });
row!(
    MarginRow, 64 {
        rep_margin_balance: Notional, rep_maint: Notional, rep_avail: Notional, rep_ratio: Rate, rep_ens: i64,
        wallet: Notional, rep_perp_notional: Notional, n_items: u16, has_report: u8, _pad: [u8; 5],
    }
);
row!(MarginItemRowSnap, 16 { inst: InstId, adl_quantile: u8, _pad: [u8; 3], liq_px: Px });
row!(FundPendingRow, 32 { settle_ts_ns: i64, synth_sum: Notional, pushed: Notional, state: u8, _pad: [u8; 7] });
row!(FundMarkRow, 16 { hist_from_ns: i64, n: u32, _pad: u32 });
row!(FundHistRow, 32 { group: GroupId, hflags: u8, _pad: [u8; 3], settle_ts_ns: i64, booked: Notional, corrected: Notional });
const _: () = assert!(core::mem::offset_of!(FundHistRow, hflags) == 4);
const _: () = assert!(core::mem::offset_of!(FundHistRow, settle_ts_ns) == 8);
const _: () = assert!(core::mem::offset_of!(FundHistRow, booked) == 16);
const _: () = assert!(core::mem::offset_of!(FundHistRow, corrected) == 24);
row!(FundNoHistRow, 32 { settle_ts_ns: i64, exch_amount: Notional, engine_amount: Notional, report_id: u64 });
row!(FundSeenRow, 16 { settle_ts_ns: i64, amount: Notional });
row!(ExtSeenRow, 24 { kind: u8, _pad: [u8; 7], a: u64, b: u64 });
const _: () = assert!(core::mem::offset_of!(ExtSeenRow, a) == 8);
const _: () = assert!(core::mem::offset_of!(ExtSeenRow, b) == 16);
row!(AutoHeldRow, 16 { buy: Qty, sell: Qty });
row!(
    /// Exact reconciled execution, with no process-local receive timestamp.
    AutoCashRow, 80 { side: u8, cause: u8, fee_asset: AssetId, _pad: u32,
        qty: Qty, px: Px, fee: Qty, exch_order_id: u64, trade_id: u64,
        ex_ts_ns: i64, fill_seq: u64, cash_qty: Qty, cash_amount: Qty }
);
const _: () = {
    assert!(core::mem::align_of::<AutoCashRow>() == 8);
    assert!(core::mem::offset_of!(AutoCashRow, side) == 0);
    assert!(core::mem::offset_of!(AutoCashRow, cause) == 1);
    assert!(core::mem::offset_of!(AutoCashRow, fee_asset) == 2);
    assert!(core::mem::offset_of!(AutoCashRow, _pad) == 4);
    assert!(core::mem::offset_of!(AutoCashRow, qty) == 8);
    assert!(core::mem::offset_of!(AutoCashRow, px) == 16);
    assert!(core::mem::offset_of!(AutoCashRow, fee) == 24);
    assert!(core::mem::offset_of!(AutoCashRow, exch_order_id) == 32);
    assert!(core::mem::offset_of!(AutoCashRow, trade_id) == 40);
    assert!(core::mem::offset_of!(AutoCashRow, ex_ts_ns) == 48);
    assert!(core::mem::offset_of!(AutoCashRow, fill_seq) == 56);
    assert!(core::mem::offset_of!(AutoCashRow, cash_qty) == 64);
    assert!(core::mem::offset_of!(AutoCashRow, cash_amount) == 72);
};
row!(
    /// Valuation prices of one inst (every inst with a quote, mark or fill): `restore` rebuilds
    /// the unrealized and held-notional caches from them, so totals stay consistent.
    PxRow, 48 { bid: Px, ask: Px, mark: Px, last_px: Px, mark_ens: i64, pflags: u8, _pad: [u8; 7] }
);

/// `PxRow.pflags` bits.
pub mod px_flags {
    pub const BBO: u8 = 1;
    pub const MARK: u8 = 2;
}

fn hdr(kind: u8, account: AccountId, venue: VenueId, id: u32) -> LedgerRowHdr {
    LedgerRowHdr {
        kind,
        account,
        venue,
        _pad: 0,
        id,
    }
}

fn put(w: &mut SnapshotWriter, h: LedgerRowHdr, body: &[u8]) {
    w.put_row(&h);
    w.put_bytes(body);
}

fn pnl_nonzero(p: &PnlBreakdown) -> bool {
    p != &PnlBreakdown::zero()
}

impl Ledger {
    /// A cold reader crossing a recovery boot block must rebuild the same process-local
    /// views as a fresh Ledger::restore. Use its own booked facts, never the new snapshot's
    /// balances or positions; a divergent durable state must remain observable.
    pub(crate) fn restore_read_views(&mut self) {
        self.now = 0;
        self.run_start = 0;
        for book in &mut self.books {
            book.vflags = if book.cash.iter().any(|c| c.bflags & bflags::UNINIT == 0) {
                vflags::INIT
            } else {
                0
            };
        }
        self.restore_marks(&[]);
        self.recompute_next_stale();
    }

    /// Adopt only the process-local clock/flag lifetime from an independent replay of our
    /// latest checkpoint. All monetary values are recomputed from this ledger's own facts.
    pub(crate) fn recover_read_views(&mut self, replay: &Ledger) {
        self.now = replay.now;
        self.run_start = replay.run_start;
        for (book, source) in self.books.iter_mut().zip(&replay.books) {
            book.vflags = (book.vflags & !vflags::DRIFT) | (source.vflags & vflags::DRIFT);
        }
        self.restore_marks(&[]);
        self.recompute_next_stale();
        for (book, source) in self.books.iter_mut().zip(&replay.books) {
            book.summary.as_of_ens = source.summary.as_of_ens;
        }
    }

    /// Writes sections 1, 13 and 22 (in this order).
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        // Section 1.
        w.begin_section(SECTION_POSITIONS, POSITIONS_VERSION);
        for acc in 1..=self.n_acc {
            let a = AccountId(acc as u8);
            for i in 1..self.insts.len() {
                let inst = InstId(i as u32);
                let Some(p) = self.pos_ix(a, inst) else { continue };
                let Some(x) = self.pos.get(p) else { continue };
                if x.qty.0 == 0 && x.cost == 0 && x.realized_quote == 0 && x.fees_quote == 0 {
                    continue;
                }
                w.put_row(&PositionRow {
                    account: a,
                    _pad: [0; 3],
                    inst,
                    qty: x.qty,
                    cost: i128_words(x.cost),
                    realized_quote: i128_words(x.realized_quote),
                    fees_quote: x.fees_quote,
                    last_fill_ens: x.last_fill_ens,
                });
            }
        }
        w.end_section();
        // Section 13.
        let version = if self.avd.iter().any(|d| !d.auto_cash.is_empty()) {
            LEDGER_CASH_RECEIPTS_VERSION
        } else {
            LEDGER_GROUPS_VERSION
        };
        w.begin_section(SECTION_LEDGER_GROUPS, version);
        for acc in 1..=self.n_acc {
            let Some(b) = self.accounts.get(acc) else { continue };
            if b == &AccountBook::ZERO {
                continue;
            }
            put(
                w,
                hdr(row_kind::ACCOUNT, AccountId(acc as u8), VenueId(0), 0),
                AccountRow {
                    day: b.day,
                    held_notional: b.held_notional,
                    upnl: b.upnl,
                    funding_residual: b.funding_residual,
                }
                .as_bytes(),
            );
        }
        for (ix, r) in self.rates.iter().enumerate().skip(1) {
            if !r.has {
                continue;
            }
            put(
                w,
                hdr(row_kind::RATE, AccountId(0), VenueId(0), ix as u32),
                RateRow {
                    bid: r.bid,
                    ask: r.ask,
                    ens: r.ens,
                    rflags: 0,
                    _pad: [0; 7],
                }
                .as_bytes(),
            );
        }
        for (ix, p) in self.ipx.iter().enumerate().skip(1) {
            if !p.has_bbo && !p.has_mark && p.last_px.0 == 0 {
                continue;
            }
            put(
                w,
                hdr(row_kind::PX, AccountId(0), VenueId(0), ix as u32),
                PxRow {
                    bid: p.bid,
                    ask: p.ask,
                    mark: p.mark,
                    last_px: p.last_px,
                    mark_ens: p.mark_ens,
                    pflags: (u8::from(p.has_bbo) * px_flags::BBO) | (u8::from(p.has_mark) * px_flags::MARK),
                    _pad: [0; 7],
                }
                .as_bytes(),
            );
        }
        for &(_, _, c) in &self.cell_ix {
            let Some(cell) = self.cells.get(c as usize) else {
                continue;
            };
            if !cell.active && !cell_nonzero(cell) {
                continue;
            }
            let b = &cell.book;
            put(
                w,
                hdr(row_kind::GROUP, cell.account, VenueId(0), cell.group.0),
                GroupRow {
                    pnl: b.pnl,
                    day: b.day,
                    held_notional: b.held_notional,
                    peak_notional: b.round.peak_notional,
                    round_id: b.round.round_id,
                    tag: b.round.tag,
                    gflags: cell.gflags,
                    _pad: [0; 7],
                    open_seq: b.round.open_seq,
                    open_ens: b.round.open_ens,
                    round_pnl: b.round.pnl,
                    legs: b.round.legs,
                    last_fill_ens: cell.last_fill_ens,
                    leg_inst: cell.legs.map(|l| l.inst.0),
                    leg_net: cell.legs.map(|l| l.net),
                    leg_cost: cell.legs.map(|l| i128_words(l.cost)),
                }
                .as_bytes(),
            );
        }
        for (av, s) in self.avs.iter().enumerate() {
            let Some(b) = self.books.get(av) else { continue };
            let m = &b.margin;
            if !margin_nonzero(m) {
                continue;
            }
            put(
                w,
                hdr(row_kind::MARGIN, s.account, s.venue, 0),
                MarginRow {
                    rep_margin_balance: m.rep_margin_balance,
                    rep_maint: m.rep_maint,
                    rep_avail: m.rep_avail,
                    rep_ratio: m.rep_ratio,
                    rep_ens: m.rep_ens,
                    wallet: m.wallet,
                    rep_perp_notional: m.rep_perp_notional,
                    n_items: m.items.len() as u16,
                    has_report: u8::from(m.has_report),
                    _pad: [0; 5],
                }
                .as_bytes(),
            );
            for it in m.items.iter() {
                w.put_bytes(
                    MarginItemRowSnap {
                        inst: it.inst,
                        adl_quantile: it.adl_quantile,
                        _pad: [0; 3],
                        liq_px: it.liq_px,
                    }
                    .as_bytes(),
                );
            }
        }
        for (av, s) in self.avs.iter().enumerate() {
            let Some(d) = self.avd.get(av) else { continue };
            for p in d.pending.iter() {
                put(
                    w,
                    hdr(row_kind::FUND_PENDING, s.account, s.venue, 0),
                    FundPendingRow {
                        settle_ts_ns: p.settle_ts,
                        synth_sum: p.synth_sum,
                        pushed: p.pushed,
                        state: p.state,
                        _pad: [0; 7],
                    }
                    .as_bytes(),
                );
            }
        }
        for (av, s) in self.avs.iter().enumerate() {
            let Some(d) = self.avd.get(av) else { continue };
            let Some(from) = d.hist_from else { continue };
            put(
                w,
                hdr(row_kind::FUND_MARK, s.account, s.venue, 0),
                FundMarkRow {
                    hist_from_ns: from,
                    n: d.hist.len() as u32,
                    _pad: 0,
                }
                .as_bytes(),
            );
            let mut order: Vec<&FundRec> = d.hist.iter().collect();
            order.sort_by_key(|r| (r.inst.0, r.settle_ts));
            for r in order {
                put(
                    w,
                    hdr(row_kind::FUND_HIST, s.account, s.venue, r.inst.0),
                    FundHistRow {
                        group: r.group,
                        hflags: r.hflags,
                        _pad: [0; 3],
                        settle_ts_ns: r.settle_ts,
                        booked: r.booked,
                        corrected: r.corrected,
                    }
                    .as_bytes(),
                );
            }
        }
        for (av, s) in self.avs.iter().enumerate() {
            let Some(d) = self.avd.get(av) else { continue };
            for x in d.no_hist.iter() {
                put(
                    w,
                    hdr(row_kind::FUND_NO_HIST, s.account, s.venue, x.inst.0),
                    FundNoHistRow {
                        settle_ts_ns: x.settle_ts,
                        exch_amount: x.exch,
                        engine_amount: x.engine,
                        report_id: x.report_id,
                    }
                    .as_bytes(),
                );
            }
        }
        for (av, s) in self.avs.iter().enumerate() {
            let Some(d) = self.avd.get(av) else { continue };
            for (ts, a) in d.seen.iter() {
                put(
                    w,
                    hdr(row_kind::FUND_SEEN, s.account, s.venue, 0),
                    FundSeenRow {
                        settle_ts_ns: *ts,
                        amount: *a,
                    }
                    .as_bytes(),
                );
            }
        }
        for (av, s) in self.avs.iter().enumerate() {
            let Some(d) = self.avd.get(av) else { continue };
            for k in d.ext.iter() {
                put(
                    w,
                    hdr(row_kind::EXT_SEEN, s.account, s.venue, k.inst),
                    ExtSeenRow {
                        kind: k.kind,
                        _pad: [0; 7],
                        a: k.a,
                        b: k.b,
                    }
                    .as_bytes(),
                );
            }
        }
        for (av, s) in self.avs.iter().enumerate() {
            let n = self.venues.get(usize::from(s.venue.0)).map_or(0, |v| v.n_insts) as usize;
            for p in (s.pos_base as usize)..(s.pos_base as usize + n) {
                let Some(d) = self.posd.get(p) else { continue };
                if d.auto_buy.0 == 0 && d.auto_sell.0 == 0 {
                    continue;
                }
                let inst = self.inst_at(av, p).unwrap_or(INST_NONE);
                put(
                    w,
                    hdr(row_kind::AUTO_HELD, s.account, s.venue, inst.0),
                    AutoHeldRow {
                        buy: d.auto_buy,
                        sell: d.auto_sell,
                    }
                    .as_bytes(),
                );
            }
        }
        for d in &self.avd {
            for e in &d.auto_cash {
                put(
                    w,
                    hdr(row_kind::AUTO_CASH, e.account, e.venue, e.inst.0),
                    AutoCashRow {
                        side: e.side as u8,
                        cause: e.cause as u8,
                        fee_asset: e.fee_asset,
                        _pad: 0,
                        qty: e.qty,
                        px: e.px,
                        fee: e.fee,
                        exch_order_id: e.exch_order_id,
                        trade_id: e.trade_id,
                        ex_ts_ns: e.ex_ts_ns,
                        fill_seq: e.fill_seq,
                        cash_qty: e.cash_qty,
                        cash_amount: e.cash_amount,
                    }
                    .as_bytes(),
                );
            }
        }
        w.end_section();
        // Section 22.
        w.begin_section(SECTION_LEDGER_INVENTORY, LEDGER_INVENTORY_VERSION);
        for (av, s) in self.avs.iter().enumerate() {
            let Some(b) = self.books.get(av) else { continue };
            for e in b.cash.iter() {
                if cash_pristine(e) {
                    continue;
                }
                put(
                    w,
                    hdr(row_kind::CASH, s.account, s.venue, u32::from(e.asset.0)),
                    CashRow {
                        total: e.total,
                        baseline: e.baseline,
                        unassigned: e.unassigned,
                        rep_free: e.rep_free,
                        rep_locked: e.rep_locked,
                        rep_ex_ts_ns: e.rep_ex_ts_ns,
                        rep_ens: e.rep_ens,
                        bflags: e.bflags,
                        _pad: [0; 7],
                    }
                    .as_bytes(),
                );
            }
        }
        for (av, s) in self.avs.iter().enumerate() {
            let Some(b) = self.books.get(av) else { continue };
            for e in b.inv.iter() {
                if e.qty.0 == 0 {
                    continue;
                }
                put(
                    w,
                    hdr(row_kind::INV, s.account, s.venue, u32::from(e.asset.0)),
                    InvRow {
                        group: e.group,
                        _pad: 0,
                        qty: e.qty,
                    }
                    .as_bytes(),
                );
            }
        }
        w.end_section();
    }

    /// Rebuilds a ledger from sections 1, 13 and 22 of a snapshot. Rejects higher row versions,
    /// illegal kinds, unknown accounts/venues/insts and any input that is not exactly what
    /// `snapshot` writes for the restored state (`BadValue`).
    pub fn restore(
        r: &mut SnapshotReader<'_>,
        rd: &RefData,
        cfg: &LedgerConfig,
        caps: &Capacities,
    ) -> Result<Ledger, RestoreError> {
        let mut l = Ledger::new(rd, cfg, caps).map_err(|_| RestoreError::Capacity)?;
        let s1 = r.find_section(SECTION_POSITIONS)?;
        let s13 = r.find_section(SECTION_LEDGER_GROUPS)?;
        let s22 = r.find_section(SECTION_LEDGER_INVENTORY)?;
        for (s, v) in [
            (&s1, POSITIONS_VERSION),
            (&s13, LEDGER_GROUPS_VERSION),
            (&s22, LEDGER_INVENTORY_VERSION),
        ] {
            if let Some(s) = s
                && s.version != v
                && !(v == LEDGER_GROUPS_VERSION && s.version == LEDGER_CASH_RECEIPTS_VERSION)
            {
                return Err(RestoreError::VersionUnsupported);
            }
        }
        if let Some(s) = &s1 {
            let mut rr = s.reader();
            for _ in 0..s.count {
                let row: PositionRow = rr.get_row()?;
                let p = l.pos_ix(row.account, row.inst).ok_or(RestoreError::BadValue)?;
                if let Some(x) = l.pos.get_mut(p) {
                    *x = InstPos {
                        qty: row.qty,
                        cost: words_i128(row.cost),
                        realized_quote: words_i128(row.realized_quote),
                        fees_quote: row.fees_quote,
                        last_fill_ens: row.last_fill_ens,
                    };
                }
            }
        }
        if let Some(s) = &s13 {
            let mut rr = s.reader();
            let mut k = 0u32;
            while k < s.count {
                let h: LedgerRowHdr = rr.get_row()?;
                k += 1;
                l.restore_row13(&h, &mut rr, &mut k, s.count)?;
            }
        }
        if let Some(s) = &s22 {
            let mut rr = s.reader();
            for _ in 0..s.count {
                let h: LedgerRowHdr = rr.get_row()?;
                let av = l.av(h.account, h.venue).ok_or(RestoreError::BadValue)?;
                let asset = AssetId(u16::try_from(h.id).map_err(|_| RestoreError::BadValue)?);
                match h.kind {
                    row_kind::CASH => {
                        let c: CashRow = rr.get_row()?;
                        let ci = l.cash_ix(av, asset).ok_or(RestoreError::Capacity)?;
                        if let Some(e) = l.books.get_mut(av).and_then(|b| b.cash.get_mut(ci)) {
                            *e = CashEntry {
                                asset,
                                bflags: c.bflags,
                                total: c.total,
                                baseline: c.baseline,
                                unassigned: c.unassigned,
                                rep_free: c.rep_free,
                                rep_locked: c.rep_locked,
                                rep_ex_ts_ns: c.rep_ex_ts_ns,
                                rep_ens: c.rep_ens,
                            };
                        }
                        if c.bflags & bflags::UNINIT == 0
                            && let Some(b) = l.books.get_mut(av)
                        {
                            b.vflags |= vflags::INIT;
                        }
                    }
                    row_kind::INV => {
                        let x: InvRow = rr.get_row()?;
                        let b = l.books.get_mut(av).ok_or(RestoreError::BadValue)?;
                        if !b.inv.push(InvEntry {
                            asset,
                            group: x.group,
                            qty: x.qty,
                        }) {
                            return Err(RestoreError::Capacity);
                        }
                    }
                    _ => return Err(RestoreError::BadValue),
                }
            }
        }
        for d in &l.avd {
            for e in &d.auto_cash {
                let sum: i128 = d
                    .auto_cash
                    .iter()
                    .filter(|r| r.inst == e.inst && r.side == e.side)
                    .map(|r| i128::from(r.qty.0))
                    .sum();
                if sum != i128::from(l.auto_held(e.account, e.inst, e.side).0) {
                    return Err(RestoreError::BadValue);
                }
            }
        }
        // Derived state (the Px rows restored the valuation prices the caches are built from).
        for a in 1..=l.n_acc {
            l.rebuild_candidates(AccountId(a as u8));
        }
        l.restore_marks(&[]);
        l.recompute_next_stale();
        // Canonical check: the restored state writes back exactly the input sections.
        let mut w = SnapshotWriter::new(r_len(&s1) + r_len(&s13) + r_len(&s22) + 1024);
        l.snapshot(&mut w);
        let mut back = SnapshotReader::new(w.as_bytes());
        for (sec, input) in [
            (SECTION_POSITIONS, &s1),
            (SECTION_LEDGER_GROUPS, &s13),
            (SECTION_LEDGER_INVENTORY, &s22),
        ] {
            let mine = back.find_section(sec)?;
            match (input, mine) {
                (Some(a), Some(b)) => {
                    if a.version != b.version || a.count != b.count || a.body != b.body {
                        return Err(RestoreError::BadValue);
                    }
                }
                (None, Some(b)) => {
                    if b.count != 0 {
                        return Err(RestoreError::BadValue);
                    }
                }
                _ => return Err(RestoreError::BadValue),
            }
        }
        let _ = &mut back;
        Ok(l)
    }

    fn restore_row13(
        &mut self,
        h: &LedgerRowHdr,
        rr: &mut SnapshotReader<'_>,
        k: &mut u32,
        count: u32,
    ) -> Result<(), RestoreError> {
        let av =
            || -> Result<usize, RestoreError> { self.av(h.account, h.venue).ok_or(RestoreError::BadValue) };
        match h.kind {
            row_kind::ACCOUNT => {
                let x: AccountRow = rr.get_row()?;
                let b = self
                    .accounts
                    .get_mut(usize::from(h.account.0))
                    .filter(|_| h.account.0 != 0)
                    .ok_or(RestoreError::BadValue)?;
                *b = AccountBook {
                    day: x.day,
                    held_notional: x.held_notional,
                    upnl: x.upnl,
                    funding_residual: x.funding_residual,
                };
            }
            row_kind::PX => {
                let x: PxRow = rr.get_row()?;
                if h.account.0 != 0 || h.venue.0 != 0 || x.pflags & !(px_flags::BBO | px_flags::MARK) != 0 {
                    return Err(RestoreError::BadValue);
                }
                let p = self
                    .ipx
                    .get_mut(h.id as usize)
                    .filter(|_| h.id != 0)
                    .ok_or(RestoreError::BadValue)?;
                p.bid = x.bid;
                p.ask = x.ask;
                p.has_bbo = x.pflags & px_flags::BBO != 0;
                p.mark = x.mark;
                p.mark_ens = x.mark_ens;
                p.has_mark = x.pflags & px_flags::MARK != 0;
                p.last_px = x.last_px;
            }
            row_kind::RATE => {
                let x: RateRow = rr.get_row()?;
                let r = self
                    .rates
                    .get_mut(h.id as usize)
                    .filter(|_| h.id != 0)
                    .ok_or(RestoreError::BadValue)?;
                r.bid = x.bid;
                r.ask = x.ask;
                r.ens = x.ens;
                r.has = true;
            }
            row_kind::GROUP => {
                let x: GroupRow = rr.get_row()?;
                if usize::from(h.account.0) > self.n_acc || h.account.0 == 0 || h.id == 0 {
                    return Err(RestoreError::BadValue);
                }
                let c = self
                    .cell_or_new(h.account, GroupId(h.id))
                    .ok_or(RestoreError::Capacity)?;
                let cell = self.cells.get_mut(c).ok_or(RestoreError::BadValue)?;
                cell.active = true;
                cell.book = GroupBook {
                    pnl: x.pnl,
                    day: x.day,
                    round: RoundBook {
                        round_id: x.round_id,
                        open_seq: x.open_seq,
                        open_ens: x.open_ens,
                        tag: x.tag,
                        legs: x.legs,
                        pnl: x.round_pnl,
                        peak_notional: x.peak_notional,
                    },
                    held_notional: x.held_notional,
                };
                cell.gflags = x.gflags;
                cell.last_fill_ens = x.last_fill_ens;
                let mut n = 0u8;
                for j in 0..4 {
                    if let (Some(l), Some(i), Some(q), Some(cw)) = (
                        cell.legs.get_mut(j),
                        x.leg_inst.get(j),
                        x.leg_net.get(j),
                        x.leg_cost.get(j),
                    ) {
                        *l = Leg {
                            inst: InstId(*i),
                            net: *q,
                            cost: words_i128(*cw),
                        };
                        if *i != 0 {
                            n = j as u8 + 1;
                        }
                    }
                }
                cell.n_legs = n;
                let _ = GF_ROUND_OPEN;
            }
            row_kind::MARGIN => {
                let x: MarginRow = rr.get_row()?;
                let av = av()?;
                let mut items = crate::fixed::FixedVec::new();
                for _ in 0..x.n_items {
                    let it: MarginItemRowSnap = rr.get_row()?;
                    if !items.push(MarginItemRow {
                        inst: it.inst,
                        liq_px: it.liq_px,
                        adl_quantile: it.adl_quantile,
                    }) {
                        return Err(RestoreError::Capacity);
                    }
                }
                let b = self.books.get_mut(av).ok_or(RestoreError::BadValue)?;
                let m = &mut b.margin;
                m.has_report = x.has_report != 0;
                m.rep_margin_balance = x.rep_margin_balance;
                m.rep_maint = x.rep_maint;
                m.rep_avail = x.rep_avail;
                m.rep_ratio = x.rep_ratio;
                m.rep_ens = x.rep_ens;
                m.wallet = x.wallet;
                m.rep_perp_notional = x.rep_perp_notional;
                m.items = items;
            }
            row_kind::FUND_PENDING => {
                let x: FundPendingRow = rr.get_row()?;
                let av = av()?;
                let d = self.avd.get_mut(av).ok_or(RestoreError::BadValue)?;
                if d.pending.len() >= crate::ledger::FUND_PENDING_CAP
                    || !d.pending.push(FundPend {
                        settle_ts: x.settle_ts_ns,
                        synth_sum: x.synth_sum,
                        pushed: x.pushed,
                        state: x.state,
                    })
                {
                    return Err(RestoreError::Capacity);
                }
            }
            row_kind::FUND_MARK => {
                let x: FundMarkRow = rr.get_row()?;
                let av = av()?;
                let cap = self.cfg.hist_cap;
                if x.n as usize > cap {
                    return Err(RestoreError::Capacity);
                }
                let mut recs = Vec::with_capacity(x.n as usize);
                for _ in 0..x.n {
                    if *k >= count {
                        return Err(RestoreError::Truncated);
                    }
                    let hh: LedgerRowHdr = rr.get_row()?;
                    *k += 1;
                    if hh.kind != row_kind::FUND_HIST || hh.account != h.account || hh.venue != h.venue {
                        return Err(RestoreError::BadValue);
                    }
                    let r: FundHistRow = rr.get_row()?;
                    recs.push(FundRec {
                        inst: InstId(hh.id),
                        group: r.group,
                        hflags: r.hflags,
                        settle_ts: r.settle_ts_ns,
                        booked: r.booked,
                        corrected: r.corrected,
                    });
                }
                recs.sort_by_key(|r| (r.settle_ts, r.inst.0));
                let d = self.avd.get_mut(av).ok_or(RestoreError::BadValue)?;
                d.hist_from = Some(x.hist_from_ns);
                d.hist.clear();
                d.hist.extend(recs);
            }
            row_kind::FUND_NO_HIST => {
                let x: FundNoHistRow = rr.get_row()?;
                let av = av()?;
                let d = self.avd.get_mut(av).ok_or(RestoreError::BadValue)?;
                if d.no_hist.len() >= crate::ledger::NO_HIST_CAP
                    || !d.no_hist.push(NoHist {
                        inst: InstId(h.id),
                        settle_ts: x.settle_ts_ns,
                        exch: x.exch_amount,
                        engine: x.engine_amount,
                        report_id: x.report_id,
                    })
                {
                    return Err(RestoreError::Capacity);
                }
            }
            row_kind::FUND_SEEN => {
                let x: FundSeenRow = rr.get_row()?;
                let av = av()?;
                let cap = self.cfg.seen_cap;
                let d = self.avd.get_mut(av).ok_or(RestoreError::BadValue)?;
                if d.seen.len() >= cap {
                    return Err(RestoreError::Capacity);
                }
                d.seen.push((x.settle_ts_ns, x.amount));
            }
            row_kind::EXT_SEEN => {
                let x: ExtSeenRow = rr.get_row()?;
                let av = av()?;
                let cap = self.cfg.ext_cap;
                let d = self.avd.get_mut(av).ok_or(RestoreError::BadValue)?;
                let key = ExtKey {
                    inst: h.id,
                    kind: x.kind,
                    a: x.a,
                    b: x.b,
                };
                if d.ext.len() >= cap || d.ext.contains(&key) {
                    return Err(RestoreError::BadValue);
                }
                d.ext.insert(key);
            }
            row_kind::AUTO_CASH => {
                let x: AutoCashRow = rr.get_row()?;
                let e = hft_event::ExternalFill {
                    venue: h.venue,
                    account: h.account,
                    inst: InstId(h.id),
                    side: hft_types::Side::try_from(x.side).map_err(|_| RestoreError::BadValue)?,
                    cause: hft_event::ExternalCause::try_from(x.cause).map_err(|_| RestoreError::BadValue)?,
                    qty: x.qty,
                    px: x.px,
                    fee: x.fee,
                    fee_asset: x.fee_asset,
                    _pad: [0; 6],
                    exch_order_id: x.exch_order_id,
                    trade_id: x.trade_id,
                    ex_ts_ns: x.ex_ts_ns,
                    rx_tsc: hft_types::Tsc(0),
                    fill_seq: x.fill_seq,
                    cash_qty: x.cash_qty,
                    cash_amount: x.cash_amount,
                };
                let av = self
                    .validate_external_cash(&e)
                    .map_err(|_| RestoreError::BadValue)?;
                if h._pad != 0
                    || x._pad != 0
                    || !crate::attrib::eligible_auto(&e)
                    || (e.trade_id == 0 && (e.exch_order_id == 0 || e.fill_seq == 0))
                    || (e.trade_id != 0 && e.fill_seq != 0)
                {
                    return Err(RestoreError::BadValue);
                }
                let d = self.avd.get_mut(av).ok_or(RestoreError::BadValue)?;
                if d.auto_cash.len() >= self.cfg.ext_cap {
                    return Err(RestoreError::Capacity);
                }
                if d.auto_cash
                    .iter()
                    .any(|r| crate::attrib::ext_key(r) == crate::attrib::ext_key(&e))
                {
                    return Err(RestoreError::BadValue);
                }
                d.auto_cash.push(e);
            }
            row_kind::AUTO_HELD => {
                let x: AutoHeldRow = rr.get_row()?;
                let p = self
                    .pos_ix(h.account, InstId(h.id))
                    .ok_or(RestoreError::BadValue)?;
                if self.inst(InstId(h.id)).is_none_or(|s| s.venue != h.venue) {
                    return Err(RestoreError::BadValue);
                }
                let d = self.posd.get_mut(p).ok_or(RestoreError::BadValue)?;
                d.auto_buy = x.buy;
                d.auto_sell = x.sell;
            }
            _ => return Err(RestoreError::BadValue),
        }
        Ok(())
    }
}

fn r_len(s: &Option<hft_strategy_sdk::SectionView<'_>>) -> usize {
    s.as_ref().map_or(0, |s| s.body.len() + 16)
}

fn cell_nonzero(c: &crate::ledger::Cell) -> bool {
    pnl_nonzero(&c.book.pnl)
        || pnl_nonzero(&c.book.day)
        || c.book.round != RoundBook::ZERO
        || c.book.held_notional.0 != 0
        || c.legs.iter().any(|l| l.net.0 != 0 || l.cost != 0)
        || c.gflags != 0
        || c.last_fill_ens != 0
}

fn margin_nonzero(m: &MarginState) -> bool {
    m.has_report
        || m.wallet.0 != 0
        || m.rep_margin_balance.0 != 0
        || m.rep_maint.0 != 0
        || m.rep_avail.0 != 0
        || m.rep_ratio.0 != 0
        || m.rep_ens != 0
        || m.rep_perp_notional.0 != 0
        || !m.items.is_empty()
}

fn cash_pristine(e: &CashEntry) -> bool {
    e.bflags == bflags::UNINIT
        && e.total.0 == 0
        && e.baseline.0 == 0
        && e.unassigned.0 == 0
        && e.rep_free.0 == 0
        && e.rep_locked.0 == 0
        && e.rep_ex_ts_ns == 0
        && e.rep_ens == 0
}
