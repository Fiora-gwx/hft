//! Shared fixtures of the hft-book tests (TESTS.md notation: `P`, `S0`, `S1`, `Δ`, `B`, `T`).
#![allow(dead_code)]

use hft_book::{BookParams, BookSet, L2Book, Outcome};
use hft_event::{Bbo, L2BatchHdr, L2Delta, L2SnapHdr, Level, MarketHdr};
use hft_refdata::{
    AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule, FundingSettleKind, InstKind,
    InstSpec, InstStatus, LiqMarker, MarginData, MarginMode, PositionMode, RdbBuilder, RefData,
    StaleGuardKind, TestnetKind, TsUnit, VenueCaps, VenueSpec,
};
use hft_types::{AssetExp, AssetId, InstId, MarketKind, Notional, Px, Qty, Side, VenueId};
use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout};

#[allow(unused_imports)]
pub use hft_types::Side::{Buy, Sell};

/// `P` = `{tick 10, window 64, ovf_cap 4, crossed_tolerance 0}`.
pub const P: BookParams = BookParams {
    tick: 10,
    window: 64,
    ovf_cap: 4,
    crossed_tolerance: 0,
};

pub fn p_tol(t: u16) -> BookParams {
    BookParams {
        crossed_tolerance: t,
        ..P
    }
}

pub const EXCHANGE: u8 = L2SnapHdr::EXCHANGE;
pub const KEYFRAME: u8 = L2SnapHdr::KEYFRAME;
pub const TOP_N: u8 = L2SnapHdr::TOP_N;

pub fn lv(px: i64, qty: i64) -> Level {
    Level {
        px: Px(px),
        qty: Qty(qty),
    }
}

pub fn some(px: i64, qty: i64) -> Option<Level> {
    Some(lv(px, qty))
}

fn mh() -> MarketHdr {
    MarketHdr::new_zeroed()
}

pub fn snap_hdr(origin: u8, snap_id: u64, nb: usize, na: usize) -> L2SnapHdr {
    let mut h = L2SnapHdr::new_zeroed();
    h.h = mh();
    h.snap_id = snap_id;
    h.origin = origin;
    h.n_bids = nb as u16;
    h.n_asks = na as u16;
    h
}

/// Snapshot from (px, qty) pairs.
pub fn snap(b: &mut L2Book, origin: u8, snap_id: u64, bids: &[(i64, i64)], asks: &[(i64, i64)]) -> Outcome {
    let h = snap_hdr(origin, snap_id, bids.len(), asks.len());
    let lv: Vec<Level> = bids.iter().chain(asks).map(|&(p, q)| lv(p, q)).collect();
    b.apply_snapshot(&h, &lv)
}

pub const S0_BIDS: &[(i64, i64)] = &[(10000, 5), (9990, 3)];
pub const S0_ASKS: &[(i64, i64)] = &[(10010, 2), (10020, 4)];

/// `S0` (Exchange, snap_id 100).
pub fn s0(b: &mut L2Book) -> Outcome {
    snap(b, EXCHANGE, 100, S0_BIDS, S0_ASKS)
}

/// `S1` (Exchange, snap_id 100, one level per side).
pub fn s1(b: &mut L2Book) -> Outcome {
    snap(b, EXCHANGE, 100, &[(10000, 5)], &[(10010, 2)])
}

/// `T(snap_id; bids; asks)`.
pub fn t(b: &mut L2Book, snap_id: u64, bids: &[(i64, i64)], asks: &[(i64, i64)]) -> Outcome {
    snap(b, TOP_N, snap_id, bids, asks)
}

pub fn book_s0(p: &BookParams) -> L2Book {
    let mut b = L2Book::new(p);
    s0(&mut b);
    b
}

pub fn book_s1(p: &BookParams) -> L2Book {
    let mut b = L2Book::new(p);
    s1(&mut b);
    b
}

/// `Δ(prev, last, side, px, qty, idx[, LAST])`.
#[allow(clippy::too_many_arguments)]
pub fn delta(prev: u64, last: u64, side: Side, px: i64, qty: i64, idx: u16, is_last: bool) -> L2Delta {
    L2Delta {
        h: mh(),
        prev_id: prev,
        last_id: last,
        px: Px(px),
        qty: Qty(qty),
        side,
        dflags: if is_last { L2Delta::LAST } else { 0 },
        idx,
        _pad: 0,
    }
}

pub fn batch_hdr(prev: u64, last: u64, nb: usize, na: usize) -> L2BatchHdr {
    let mut h = L2BatchHdr::new_zeroed();
    h.h = mh();
    h.prev_id = prev;
    h.last_id = last;
    h.n_bids = nb as u16;
    h.n_asks = na as u16;
    h
}

/// `B(prev, last; bids; asks)`.
pub fn batch(b: &mut L2Book, prev: u64, last: u64, bids: &[(i64, i64)], asks: &[(i64, i64)]) -> Outcome {
    let h = batch_hdr(prev, last, bids.len(), asks.len());
    let lv: Vec<Level> = bids.iter().chain(asks).map(|&(p, q)| lv(p, q)).collect();
    b.apply_batch(&h, &lv)
}

/// Visible levels of one side as (px, qty).
pub fn side(b: &L2Book, s: Side) -> Vec<(i64, i64)> {
    b.iter(s).map(|l| (l.px.0, l.qty.0)).collect()
}

pub fn bbo(update_id: u64, bid: (i64, i64), ask: (i64, i64), rx_tsc: u64) -> Bbo {
    let mut b = Bbo::new_zeroed();
    b.h = mh();
    b.h.rx_tsc = rx_tsc;
    b.update_id = update_id;
    b.bid_px = Px(bid.0);
    b.bid_qty = Qty(bid.1);
    b.ask_px = Px(ask.0);
    b.ask_qty = Qty(ask.1);
    b
}

#[repr(C, align(64))]
#[derive(Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
struct Line([u8; 64]);

/// Leaked 64 B aligned zeroed memory of at least `n` bytes.
pub fn aligned_mem(n: usize) -> &'static mut [u8] {
    let v = vec![Line([0; 64]); n.div_ceil(64).max(1)];
    let s: &'static mut [Line] = Box::leak(v.into_boxed_slice());
    let b = s.as_mut_bytes();
    let n = n.min(b.len());
    &mut b[..n]
}

/// Leaked memory whose start is 8 B past a 64 B boundary.
pub fn misaligned_mem(n: usize) -> &'static mut [u8] {
    let m = aligned_mem(n + 64);
    &mut m[8..8 + n]
}

/// Refdata with insts 1..=n (Binance spot, BTC/USDT at −8), each `tick`, `book_window`,
/// `crossed_tolerance`.
pub fn refdata(n: u32, tick: i64, window: u32, tol: u16) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class) in [(1u16, "BTC", AssetClass::Crypto), (2, "USDT", AssetClass::Stable)] {
        b.asset(AssetSpec {
            id: AssetId(id),
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: -8,
                qty_exp: -8,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1_000),
        });
    }
    let v = VenueId::BINANCE_SPOT;
    b.venue(VenueSpec {
        id: v,
        exchange: Exchange::Binance,
        product: InstKind::Spot,
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.caps(VenueCaps {
        venue: v,
        market: MarketKind::Spot,
        fee_asset_rule: FeeAssetRule::ReceivedAsset,
        reduce_only: false,
        required_position_mode: PositionMode::NotApplicable,
        required_margin_mode: MarginMode::NotApplicable,
        required_multi_asset: None,
        required_spot_borrow: None,
        funding_settle: FundingSettleKind::NotApplicable,
        margin_data: MarginData::NotApplicable,
        own_liquidation_marker: LiqMarker::NotIdentifiable,
        liq_marker_arg: "".into(),
        ignorable_user_events: Vec::new().into(),
        flags: CapFlags::POST_ONLY | CapFlags::IOC,
        amend_qty_down_only: false,
        dms: DeadManCap::None,
        dms_min_ms: 0,
        dms_max_ms: 0,
        dms_timeout_ms: 0,
        dms_refresh_ms: 0,
        testnet: TestnetKind::Testnet,
        _pad: 0,
    });
    let tick_str = format!("0.{tick:0>8}");
    for id in 1..=n {
        b.inst(InstSpec {
            id: InstId(id),
            venue: v,
            symbol: format!("SYM{id}USDT"),
            status: InstStatus::Trading,
            kind: InstKind::Spot,
            asset: AssetId(1),
            quote: AssetId(2),
            tick,
            lot: 1,
            min_qty: Qty(1_000),
            max_qty: Qty(100_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: window,
            l2_depth: 20,
            crossed_tolerance: tol,
            tick_str: tick_str.clone(),
            lot_str: "0.00000001".into(),
            min_qty_str: "0.00001".into(),
            ..InstSpec::default()
        });
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => Box::leak(Box::new(r)),
        Err(e) => panic!("test refdata rejected: {e:?}"),
    }
}

/// BookSet over `refdata(n, 10, 64, 0)` with L2 books for `l2`.
pub fn bookset(n: u32, l2: &[u32]) -> BookSet {
    let rd = refdata(n, 10, 64, 0);
    let ids: Vec<InstId> = l2.iter().map(|i| InstId(*i)).collect();
    let mem = aligned_mem(BookSet::bytes_needed(rd, &ids));
    match BookSet::new_in(rd, &ids, mem) {
        Ok(s) => s,
        Err(e) => panic!("BookSet::new_in: {e:?}"),
    }
}
