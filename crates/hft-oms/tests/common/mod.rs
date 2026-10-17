//! Shared fixtures of the hft-oms tests (TESTS.md "公共夹具"): refdata (VS/VX/VG, a1/a2/s1,
//! X/X2/Y/Z), configuration P0, groups g1/g2/g3, link plan L1 and a small driving harness.
#![allow(dead_code, clippy::too_many_arguments)]
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code

use std::sync::OnceLock;

use hft_event::{
    ConnRole, ConnState, ConnStateKind, GroupSpec, GwOp, LegDef, Note, OrderEvent, OrderEventKind,
    ReconcileReport, RecordHeader, SendReport, SendStatus,
};
use hft_oms::*;
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LiqMarker, MarginData, MarginMode,
    PositionMode, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit,
    VenueCaps, VenueSpec,
};
use hft_strategy_sdk::{
    BboView, GroupTable, HedgeKind, HedgePlan, IntentClass, LegLink, LegLinkOpt, LinkRole, OnFail,
    OrderIntent, OrderRef, PxMode, ResidualPolicy, RxTsSrc,
};
use hft_types::{
    AccountId, AssetExp, AssetId, ConnId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, Side, SlotId,
    StrategyId, Tif, Tsc, VenueId,
};

pub const VS: VenueId = VenueId::BINANCE_SPOT;
pub const VX: VenueId = VenueId::OKX_SWAP;
pub const VG: VenueId = VenueId::BITGET_UTA_USDT_FUT;
pub const A1: AccountId = AccountId(1);
pub const A2: AccountId = AccountId(2);
pub const S1: AccountId = AccountId(3);
pub const X: InstId = InstId(11);
pub const X2: InstId = InstId(12);
pub const Y: InstId = InstId(21);
pub const Z: InstId = InstId(31);
pub const BTC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
pub const BNB: AssetId = AssetId(3);
pub const ETH: AssetId = AssetId(4);
pub const EPOCH: u32 = 7;
/// 1 GHz (`tsc_hz_milli`).
pub const HZ: u64 = 1_000_000_000_000;
pub const MS: u64 = 1_000_000;

pub fn ms(x: u64) -> Tsc {
    Tsc(x * MS)
}
/// BTC quantity from a decimal string (raw = value × 10⁸).
pub fn q(s: &str) -> Qty {
    Qty(hft_types::parse_exact(s.as_bytes(), -8).unwrap())
}
/// BTC price from a decimal string (raw = value × 100).
pub fn p(s: &str) -> Px {
    Px(hft_types::parse_exact(s.as_bytes(), -2).unwrap())
}
/// Rate from a decimal string (raw = value × 10⁹).
pub fn r(s: &str) -> Rate {
    Rate(hft_types::parse_exact(s.as_bytes(), -9).unwrap())
}
/// USDT notional (raw = value × 10⁶).
pub fn usdt(s: &str) -> i64 {
    hft_types::parse_exact(s.as_bytes(), -6).unwrap()
}

pub fn g1() -> GroupId {
    GroupId::compose(StrategyId(1), 1).unwrap()
}
pub fn g2() -> GroupId {
    GroupId::compose(StrategyId(1), 2).unwrap()
}
pub fn g3() -> GroupId {
    GroupId::compose(StrategyId(3), 1).unwrap()
}

/// Refdata knobs a few cases vary.
#[derive(Clone, Debug)]
pub struct RdOpts {
    pub y_min_qty: Qty,
    pub y_min_notional: i64,
    pub x_min_qty: Qty,
    pub x_min_notional: i64,
    pub vx_amend_down_only: bool,
    pub vx_perp_cash: bool,
    pub s1_perp: bool,
    /// Total accounts (a1, a2, s1, then Live fillers on VS/VX).
    pub n_accounts: u8,
    /// 3 = VS, VX, VG; 2 = VS, VX.
    pub n_venues: u8,
}

impl Default for RdOpts {
    fn default() -> Self {
        RdOpts {
            y_min_qty: Qty(100_000),
            y_min_notional: 5_000_000,
            x_min_qty: Qty(1000),
            x_min_notional: 5_000_000,
            vx_amend_down_only: false,
            vx_perp_cash: false,
            s1_perp: false,
            n_accounts: 3,
            n_venues: 3,
        }
    }
}

fn caps(v: VenueId, market: MarketKind, flags: CapFlags, dms: DeadManCap, down_only: bool) -> VenueCaps {
    VenueCaps {
        venue: v,
        market,
        fee_asset_rule: if market == MarketKind::Spot {
            FeeAssetRule::ReceivedAsset
        } else {
            FeeAssetRule::Quote
        },
        reduce_only: market != MarketKind::Spot,
        required_position_mode: if market == MarketKind::Spot {
            PositionMode::NotApplicable
        } else {
            PositionMode::OneWay
        },
        required_margin_mode: if market == MarketKind::Spot {
            MarginMode::NotApplicable
        } else {
            MarginMode::Cross
        },
        required_multi_asset: None,
        required_spot_borrow: None,
        funding_settle: if market == MarketKind::Spot {
            FundingSettleKind::NotApplicable
        } else {
            FundingSettleKind::PerSymbolPush
        },
        margin_data: if market == MarketKind::Spot {
            MarginData::NotApplicable
        } else {
            MarginData::Push
        },
        own_liquidation_marker: LiqMarker::NotIdentifiable,
        liq_marker_arg: "".into(),
        ignorable_user_events: Vec::new().into(),
        flags,
        amend_qty_down_only: down_only,
        dms,
        dms_min_ms: 0,
        dms_max_ms: 0,
        dms_timeout_ms: if dms == DeadManCap::None { 0 } else { 30_000 },
        dms_refresh_ms: if dms == DeadManCap::None { 0 } else { 10_000 },
        testnet: TestnetKind::Testnet,
        _pad: 0,
    }
}

fn venue(v: VenueId, ex: Exchange, kind: InstKind, g: StaleGuardKind) -> VenueSpec {
    VenueSpec {
        id: v,
        exchange: ex,
        product: kind,
        ws_max_streams_per_conn: 50,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 20,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: g,
    }
}

/// Decimal text of a raw quantity at exponent −8 (trailing zeros trimmed).
pub fn fmt8(raw: i64) -> String {
    let s = format!("{}.{:08}", raw / 100_000_000, raw % 100_000_000);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Builds the fixture refdata (leaked, `&'static` as at startup).
pub fn build_rd(o: &RdOpts) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class, px, qx) in [
        (BTC, "BTC", AssetClass::Crypto, -2, -8),
        (USDT, "USDT", AssetClass::Stable, -6, -6),
        (BNB, "BNB", AssetClass::Crypto, -4, -8),
        (ETH, "ETH", AssetClass::Crypto, -2, -8),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qx,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    b.venue(venue(
        VS,
        Exchange::Binance,
        InstKind::Spot,
        StaleGuardKind::RecvWindow,
    ));
    b.venue(venue(VX, Exchange::Okx, InstKind::Perp, StaleGuardKind::ExpireAt));
    if o.n_venues >= 3 {
        b.venue(venue(VG, Exchange::Bitget, InstKind::Perp, StaleGuardKind::None));
    }
    b.caps(caps(
        VS,
        MarketKind::Spot,
        CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::BATCH_CANCEL,
        DeadManCap::None,
        false,
    ));
    b.caps(caps(
        VX,
        MarketKind::LinearPerp,
        CapFlags::AMEND
            | CapFlags::POST_ONLY
            | CapFlags::IOC
            | CapFlags::FOK
            | CapFlags::BATCH_CANCEL
            | if o.vx_perp_cash {
                CapFlags::PERP_CASH
            } else {
                CapFlags(0)
            },
        DeadManCap::CountdownAccount,
        o.vx_amend_down_only,
    ));
    if o.n_venues >= 3 {
        b.caps(caps(
            VG,
            MarketKind::LinearPerp,
            CapFlags::AMEND | CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK,
            DeadManCap::None,
            false,
        ));
    }
    for id in 1..=31u32 {
        let (v, kind, sym, lot, min_qty, min_not, tick, asset, tick_s, lot_s): (
            VenueId,
            InstKind,
            String,
            i64,
            Qty,
            i64,
            i64,
            AssetId,
            &str,
            &str,
        ) = match id {
            11 => (
                VS,
                InstKind::Spot,
                "BTCUSDT".into(),
                1000,
                o.x_min_qty,
                o.x_min_notional,
                1,
                BTC,
                "0.01",
                "0.00001",
            ),
            12 => (
                VS,
                InstKind::Spot,
                "ETHUSDT".into(),
                1000,
                Qty(1000),
                5_000_000,
                1,
                ETH,
                "0.01",
                "0.00001",
            ),
            21 => (
                VX,
                InstKind::Perp,
                "BTC-USDT-SWAP".into(),
                100_000,
                o.y_min_qty,
                o.y_min_notional,
                10,
                BTC,
                "0.1",
                "0.001",
            ),
            31 if o.n_venues >= 3 => (
                VG,
                InstKind::Perp,
                "BTCUSDT".into(),
                100_000,
                Qty(100_000),
                5_000_000,
                10,
                BTC,
                "0.1",
                "0.001",
            ),
            _ => (
                VS,
                InstKind::Spot,
                format!("F{id}USDT"),
                1000,
                Qty(1000),
                5_000_000,
                1,
                BTC,
                "0.01",
                "0.00001",
            ),
        };
        b.inst(InstSpec {
            id: InstId(id),
            venue: v,
            symbol: sym,
            status: InstStatus::Trading,
            kind,
            asset,
            quote: USDT,
            tick,
            lot,
            min_qty,
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(min_not),
            book_window: 64,
            l2_depth: 20,
            tick_str: tick_s.into(),
            lot_str: lot_s.into(),
            min_qty_str: fmt8(min_qty.0),
            funding_interval_s: if kind == InstKind::Perp { 28_800 } else { 0 },
            max_leverage: if kind == InstKind::Perp { 20 } else { 0 },
            ..InstSpec::default()
        });
    }
    let vs: &[VenueId] = if o.n_venues >= 3 { &[VS, VX, VG] } else { &[VS, VX] };
    for &v in vs {
        b.fee(hft_refdata::FeeSpec {
            venue: v,
            tier: 0,
            fee: hft_refdata::FeePair {
                maker: Rate(200_000),
                taker: Rate(500_000),
            },
        });
    }
    for (id, name) in [(1u8, "s1"), (2, "s2"), (3, "s3")] {
        b.instance(InstanceSpec {
            id: StrategyId(id),
            kind_id: 1,
            name: name.into(),
        });
    }
    let all = if o.n_venues >= 3 {
        1u64 << VS.0 | 1u64 << VX.0 | 1u64 << VG.0
    } else {
        1u64 << VS.0 | 1u64 << VX.0
    };
    b.account(AccountSpec {
        id: A1,
        bound_instance: StrategyId(1),
        venues: all,
        label: "a1".into(),
        ..AccountSpec::default()
    });
    b.account(AccountSpec {
        id: A2,
        bound_instance: StrategyId(2),
        venues: 1u64 << VS.0,
        label: "a2".into(),
        ..AccountSpec::default()
    });
    b.account(AccountSpec {
        id: S1,
        sim: true,
        bound_instance: StrategyId(3),
        venues: (1u64 << VS.0) | if o.s1_perp { 1u64 << VX.0 } else { 0 },
        label: "s1".into(),
        ..AccountSpec::default()
    });
    for id in 4..=o.n_accounts {
        b.account(AccountSpec {
            id: AccountId(id),
            sim: true,
            venues: 1u64 << VS.0 | 1u64 << VX.0,
            label: format!("f{id}"),
            ..AccountSpec::default()
        });
        b.sim(SimSpec {
            account: AccountId(id),
            fee_tier: 0,
            latency_model: 0,
            margin_model: 0,
        });
        b.sim_balance(SimBalanceSpec {
            account: AccountId(id),
            venue: VS,
            asset: USDT,
            qty: Qty(1_000_000),
        });
    }
    let avs: &[(AccountId, VenueId)] = if o.n_venues >= 3 {
        &[(A1, VS), (A1, VX), (A1, VG), (A2, VS)]
    } else {
        &[(A1, VS), (A1, VX), (A2, VS)]
    };
    for &(a, v) in avs {
        b.account_venue(AccountVenueSpec {
            account: a,
            venue: v,
            uid: format!("{}{}", a.0, v.0),
        });
    }
    b.sim(SimSpec {
        account: S1,
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    });
    b.sim_balance(SimBalanceSpec {
        account: S1,
        venue: VS,
        asset: USDT,
        qty: Qty(1_000_000_000_000),
    });
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => Box::leak(Box::new(r)),
        Err(e) => panic!("fixture refdata rejected: {e:?}"),
    }
}

pub fn rd() -> &'static RefData {
    static RD: OnceLock<&'static RefData> = OnceLock::new();
    RD.get_or_init(|| build_rd(&RdOpts::default()))
}

/// Configuration P0.
pub fn p0() -> OmsConfig {
    let mut c = OmsConfig::new(EPOCH, HZ);
    c.slots = PerTable::new(64);
    c.links_per_account = 16;
    c.max_groups = 256;
    c.ack_timeout_ms = PerVenue::new(2000);
    c.cancel_timeout_ms = PerVenue::new(2000);
    c.ioc_timeout_ms = PerVenue::new(1000);
    c.unknown_query_interval_ms = 1000;
    c.unknown_max_queries = 5;
    c.unknown_alarm_ms = 30_000;
    c.out_quota = PerTable::new(Some(64));
    c.not_found_grace_ms.set(VS, Some(5500));
    c.not_found_grace_ms.set(VX, Some(3500));
    c.not_found_grace_ms.set(VG, Some(2000));
    c.smoke_far_bp = r("0.03");
    c.reuse_quarantine_ms = 5000;
    c.tombstone_gens = 2;
    c.tombstone_keep_ms = 3_600_000;
    c.sweep_batch = 256;
    c.max_out_per_event = 16;
    c.hedge_retry_max = 3;
    c.hedge_retry_min_ms = 100;
    c.hedge_deadline_ms = 3000;
    c.resume_hold_max_ms = 30_000;
    c.foreign_fill_track = PerTable::new(64);
    c.ext_held_track = PerTable::new(64);
    c.decode_fail_down_n = 3;
    c
}

pub fn group_spec(id: GroupId, legs: &[(InstId, VenueId)]) -> GroupSpec {
    let z = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut l = [z; 4];
    for (d, (i, v)) in l.iter_mut().zip(legs) {
        *d = LegDef {
            inst: *i,
            venue: *v,
            kind: LegDef::CEX,
            _pad: 0,
            route: 0,
            _pad2: 0,
        };
    }
    GroupSpec {
        id,
        asset: BTC,
        n_legs: legs.len() as u8,
        open_allowed: 1,
        legs: l,
        _rsv: 0,
    }
}

pub fn groups_s1(with_g2: bool) -> GroupTable {
    let mut v = vec![group_spec(g1(), &[(X, VS), (Y, VX)])];
    if with_g2 {
        v.push(group_spec(g2(), &[(X, VS), (Z, VG)]));
    }
    GroupTable::new(&v, 64)
}

pub fn groups_s3() -> GroupTable {
    GroupTable::new(&[group_spec(g3(), &[(X, VS)])], 64)
}

/// Plan L1 (OnFill, sell Y on VX, Cross 0.001, max_slip 0.001, Retry, Ioc).
pub fn plan_l1() -> HedgePlan {
    HedgePlan {
        kind: HedgeKind::OnFill,
        px_mode: PxMode::Cross,
        on_fail: OnFail::Retry,
        tif: Tif::Ioc,
        venue: VX,
        account: A1,
        side: Side::Sell,
        reduce_only: 0,
        inst: Y,
        ratio_num: 1,
        ratio_den: 1,
        residual: ResidualPolicy::Defer,
        _pad: [0; 3],
        px_param: r("0.001"),
        max_slip: r("0.001"),
        limit_px: Px(0),
    }
}

pub fn link_l1(plan: HedgePlan) -> LegLink {
    LegLink {
        group: g1(),
        role: LinkRole::First,
        _pad: [0; 3],
        tolerance: r("0.02"),
        hedge: plan,
    }
}

pub fn strategy_of(a: AccountId) -> StrategyId {
    match a.0 {
        1 => StrategyId(1),
        2 => StrategyId(2),
        _ => StrategyId(3),
    }
}

/// A strategy Open intent.
pub fn intent(
    v: VenueId,
    a: AccountId,
    inst: InstId,
    side: Side,
    tif: Tif,
    px: Px,
    qty: Qty,
    g: GroupId,
) -> ApprovedIntent {
    let oi = OrderIntent {
        venue: v,
        account: a,
        side,
        tif,
        class: IntentClass::Open,
        flags: 0,
        _pad: [0; 2],
        inst,
        group: g,
        px,
        qty,
        link: LegLinkOpt::NONE,
        tag: 0,
        max_rest_ms: 0,
    };
    ApprovedIntent::new(oi, Origin::Strategy, strategy_of(a), IntentClass::Open)
}

/// Slot s: buy X 0.01 @ 60000.00 GTC on a1@VS in g1.
pub fn buy_x() -> ApprovedIntent {
    intent(VS, A1, X, Side::Buy, Tif::Gtc, p("60000.00"), q("0.01"), g1())
}

/// Sell Y 0.01 @ 60100.0 GTC on a1@VX in g1.
pub fn sell_y() -> ApprovedIntent {
    intent(VX, A1, Y, Side::Sell, Tif::Gtc, p("60100.00"), q("0.01"), g1())
}

/// First leg of L1 (buy X, `tif`), with plan `plan`.
pub fn first_leg(qty: Qty, tif: Tif, plan: HedgePlan) -> ApprovedIntent {
    let mut it = intent(VS, A1, X, Side::Buy, tif, p("60000.00"), qty, g1());
    it.intent.link = LegLinkOpt::some(link_l1(plan));
    it.hedge_ref_px = p("60010.0");
    it
}

pub fn bbo(bid: Px, ask: Px, now: Tsc) -> BboView {
    BboView {
        bid_px: bid,
        bid_qty: q("1"),
        ask_px: ask,
        ask_qty: q("1"),
        update_id: 1,
        ex_ts_ns: 0,
        rx_tsc: now,
        hw_rx_ns: 0,
        rx_ts_src: RxTsSrc::None,
        seq: 1,
        conn_live_tsc: now,
        conn_up: true,
        resync: false,
    }
}

pub fn lim() -> hft_types::HedgePxLimits {
    hft_types::HedgePxLimits {
        conn_liveness_ms: 2000,
        rr_max_cross: r("0.005"),
    }
}

/// The driving harness: one `Oms`, one `OmsOut` cleared before every call (as the sequencer).
pub struct H {
    pub o: Oms,
    pub out: OmsOut,
    pub seq: u64,
}

impl H {
    /// P0, groups {g1, g2} (s1: {g3}), every order connection Up.
    pub fn new() -> H {
        Self::with(|_| {})
    }
    pub fn with(f: impl FnOnce(&mut OmsConfig)) -> H {
        let mut c = p0();
        f(&mut c);
        Self::from_cfg(&c, rd())
    }
    pub fn from_cfg(c: &OmsConfig, rd: &RefData) -> H {
        let mut o = Oms::new(c, rd).unwrap();
        o.on_groups_changed(&groups_s1(true));
        o.on_groups_changed(&groups_s3());
        let mut h = H {
            o,
            out: OmsOut::new(),
            seq: 100,
        };
        for (v, a) in [(VS, A1), (VX, A1), (VG, A1), (VS, A2), (VS, S1)] {
            h.conn(v, a, ConnRole::Order, ConnStateKind::Up, 0);
        }
        h.out.clear();
        h
    }
    pub fn hdr(&mut self, t_ms: u64) -> RecordHeader {
        self.seq += 1;
        RecordHeader {
            len: 0,
            kind: 0x0203,
            flags: 0,
            seq: self.seq,
            tsc: t_ms * MS,
        }
    }
    pub fn place(&mut self, it: &ApprovedIntent, t_ms: u64) -> Result<OrderRef, PlaceError> {
        self.out.clear();
        self.seq += 1;
        let s = self.seq;
        self.o.place(it, ms(t_ms), s, &mut self.out)
    }
    pub fn place_ok(&mut self, it: &ApprovedIntent, t_ms: u64) -> OrderRef {
        self.place(it, t_ms).unwrap()
    }
    pub fn cancel(&mut self, r: OrderRef, t_ms: u64) -> Result<(), OpError> {
        self.out.clear();
        self.seq += 1;
        let s = self.seq;
        self.o
            .cancel(r, &TrigStamps::default(), ms(t_ms), s, &mut self.out)
    }
    pub fn amend(&mut self, r: OrderRef, px: Px, qty: Qty, t_ms: u64) -> Result<(), OpError> {
        self.out.clear();
        self.seq += 1;
        let s = self.seq;
        self.o
            .amend(r, px, qty, &TrigStamps::default(), ms(t_ms), s, &mut self.out)
    }
    /// An `OrderEvent` matching slot `r` (epoch, slot, gen, inst from the slot).
    pub fn oe(&self, r: OrderRef, what: OrderEventKind) -> OrderEvent {
        let (sl, _, _) = self.o.raw_slot(r.venue, r.account, r.slot).unwrap();
        OrderEvent {
            venue: r.venue,
            account: r.account,
            what,
            detail: 0,
            slot: SlotId(r.slot),
            r#gen: r.r#gen,
            inst: sl.inst,
            px: Px(0),
            qty: Qty(0),
            cum_filled: Qty(0),
            fee: Qty(0),
            fee_asset: AssetId(0),
            epoch: if sl.epoch == 0 { EPOCH as u16 } else { sl.epoch },
            reject_code: 0,
            exch_order_id: 0,
            ex_ts_ns: 0,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: Px(0),
            cash_qty: hft_types::Qty(0),
            cash_sum: hft_types::Qty(0),
            trade_id: 0,
            fill_cash_qty: hft_types::Qty(0),
            fill_cash_amount: hft_types::Qty(0),
        }
    }
    pub fn ev(&mut self, ev: &OrderEvent, t_ms: u64) {
        self.out.clear();
        let h = self.hdr(t_ms);
        self.o.on_order_event(ev, &h, &mut self.out);
    }
    /// Delivers `what` for `r` with `cum_filled`/`avg_px`.
    pub fn fill(&mut self, r: OrderRef, cum: Qty, avg: Px, t_ms: u64) {
        let mut e = self.oe(r, OrderEventKind::PartialFill);
        let full = self.o.raw_slot(r.venue, r.account, r.slot).unwrap().0.qty.0 <= cum.0;
        if full {
            e.what = OrderEventKind::Fill;
        }
        e.cum_filled = cum;
        e.avg_px = avg;
        e.fee_asset = USDT;
        self.ev(&e, t_ms);
    }
    pub fn what(&mut self, r: OrderRef, what: OrderEventKind, t_ms: u64) {
        let mut e = self.oe(r, what);
        e.cum_filled = self.o.raw_slot(r.venue, r.account, r.slot).unwrap().0.filled;
        self.ev(&e, t_ms);
    }
    pub fn ack(&mut self, r: OrderRef, eid: u64, t_ms: u64) {
        let mut e = self.oe(r, OrderEventKind::Ack);
        e.exch_order_id = eid;
        self.ev(&e, t_ms);
    }
    pub fn query(&mut self, r: OrderRef, status: u8, cum: Qty, avg: Px, t_ms: u64) {
        let mut e = self.oe(r, OrderEventKind::QueryResult);
        let sl = self.o.raw_slot(r.venue, r.account, r.slot).unwrap().0;
        e.detail = status;
        e.cum_filled = cum;
        e.avg_px = avg;
        e.px = sl.px;
        e.qty = sl.qty;
        self.ev(&e, t_ms);
    }
    pub fn send(
        &mut self,
        v: VenueId,
        a: AccountId,
        op: GwOp,
        slot: u32,
        r#gen: u32,
        st: SendStatus,
        sent_ms: u64,
        t_ms: u64,
    ) {
        self.out.clear();
        let h = self.hdr(t_ms);
        let (s, g, ts, tg) = if op == GwOp::Place {
            (slot, r#gen, 0, 0)
        } else {
            (0, 0, slot, r#gen)
        };
        let e = SendReport {
            decision_seq: 0,
            sent_tsc: ms(sent_ms),
            slot: SlotId(s),
            r#gen: g,
            target_slot: SlotId(ts),
            target_gen: tg,
            venue: v,
            account: a,
            conn: ConnId(0),
            status: st,
            op,
            _pad: [0; 3],
            os_errno: 0,
            _pad2: 0,
        };
        self.o.on_send_report(&e, &h, &mut self.out);
    }
    pub fn send_r(&mut self, r: OrderRef, op: GwOp, st: SendStatus, t_ms: u64) {
        self.send(r.venue, r.account, op, r.slot, r.r#gen, st, t_ms, t_ms);
    }
    pub fn conn(&mut self, v: VenueId, a: AccountId, role: ConnRole, st: ConnStateKind, t_ms: u64) {
        self.conn_reason(v, a, role, st, 0, t_ms);
    }
    pub fn conn_reason(
        &mut self,
        v: VenueId,
        a: AccountId,
        role: ConnRole,
        st: ConnStateKind,
        reason: u32,
        t_ms: u64,
    ) {
        self.out.clear();
        let h = self.hdr(t_ms);
        let e = ConnState {
            venue: v,
            conn: 0,
            state: st,
            role,
            inst: InstId(0),
            reason,
            account: a,
            _pad2: [0; 3],
            rx_tsc: 0,
        };
        self.o.on_conn_state(&e, &h, &mut self.out);
    }
    pub fn recon(&mut self, e: &ReconcileReport, t_ms: u64) {
        self.out.clear();
        let h = self.hdr(t_ms);
        self.o.on_reconcile(e, &h, &mut self.out);
    }
    pub fn sweep(&mut self, t_ms: u64) {
        self.out.clear();
        self.o.on_sweep(ms(t_ms), &mut self.out);
    }
    pub fn uq(&mut self, r: OrderRef, t_ms: u64) {
        self.out.clear();
        self.seq += 1;
        let t = self.o.table_view(r.venue, r.account).unwrap().table_ix;
        let id = OmsTimerReq::unknown_query_id(t, r.slot);
        let s = self.seq;
        self.o.on_unknown_query(id, ms(t_ms), s, &mut self.out);
    }
    pub fn retry(&mut self, link: u32, t_ms: u64) {
        self.out.clear();
        self.seq += 1;
        let s = self.seq;
        self.o.on_hedge_retry(link, ms(t_ms), s, &mut self.out);
    }
    pub fn cont(&mut self) {
        self.out.clear();
        self.seq += 1;
        let s = self.seq;
        self.o.on_continue(Tsc(0), s, &mut self.out);
    }
    pub fn slot(&self, r: OrderRef) -> OrderSlot {
        *self.o.raw_slot(r.venue, r.account, r.slot).unwrap().0
    }
    pub fn ext(&self, r: OrderRef) -> SlotExt {
        *self.o.raw_slot(r.venue, r.account, r.slot).unwrap().1
    }
    pub fn stamps(&self, r: OrderRef) -> SlotStamps {
        *self.o.raw_slot(r.venue, r.account, r.slot).unwrap().2
    }
    pub fn state(&self, r: OrderRef) -> OrderState {
        self.slot(r).state
    }
    pub fn flags(&self, r: OrderRef) -> u16 {
        self.slot(r).sflags.0
    }
    pub fn set_state(&mut self, r: OrderRef, st: OrderState) {
        self.o.test_set_state(r, st, OrderState::PendingNew);
    }
    /// Puts slot `r` into `st` with the given ACKED bit (and `prev` for Unknown).
    pub fn force(&mut self, r: OrderRef, st: OrderState, acked: bool, prev: OrderState) {
        self.o.test_slot_mut(r.venue, r.account, r.slot, |sl, _| {
            sl.state = st;
            if st == OrderState::Unknown {
                sl.prev_state = prev;
            }
            if acked {
                sl.sflags.0 |= SlotFlags::ACKED;
            } else {
                sl.sflags.0 &= !SlotFlags::ACKED;
            }
        });
    }
    pub fn link(&self, id: u32) -> LinkEntry {
        *self.o.link(id).unwrap()
    }
    pub fn legs(&self, a: AccountId, g: GroupId) -> LegState {
        *self.o.legs(a, g)
    }
    pub fn notes(&self, c: hft_oms::notes::OmsNoteCode) -> Vec<Note> {
        let code = hft_oms::notes::code_of(c);
        self.out
            .notes
            .iter()
            .filter(|n| n.code == code)
            .copied()
            .collect()
    }
    pub fn n_notes(&self, c: hft_oms::notes::OmsNoteCode) -> usize {
        self.notes(c).len()
    }
    pub fn decisions(&self, a: hft_event::OrderAction) -> Vec<hft_event::OrderDecision> {
        self.out
            .decisions
            .iter()
            .filter(|d| d.action == a)
            .copied()
            .collect()
    }
    pub fn timers(&self, k: OmsTimerKind) -> Vec<OmsTimerReq> {
        self.out.timers.iter().filter(|t| t.kind == k).copied().collect()
    }
}

/// Transition note fields (from, to, cause).
pub fn tr(n: &Note) -> (u8, u8, u8) {
    let v = n.v[0];
    (
        ((v >> 16) & 0xFF) as u8,
        ((v >> 24) & 0xFF) as u8,
        ((v >> 32) & 0xFF) as u8,
    )
}

pub fn transitions(h: &H) -> Vec<(u8, u8, u8)> {
    h.notes(hft_oms::notes::TRANSITION).iter().map(tr).collect()
}

/// Places the L1 first leg, returns (slot, link_id).
pub fn place_first(h: &mut H, qty: Qty, tif: Tif, plan: HedgePlan, t_ms: u64) -> (OrderRef, u32) {
    let r = h.place_ok(&first_leg(qty, tif, plan), t_ms);
    let id = h.slot(r).link_id;
    (r, id)
}

/// Places one emitted hedge request (price `px`), returns its slot.
pub fn place_hedge(h: &mut H, req: &HedgeRequest, t_ms: u64) -> Result<OrderRef, PlaceError> {
    let plan = h.o.link_plan(req.link_id).unwrap().1;
    let hp = HedgePx {
        px: if req.side == Side::Sell {
            p("59930.1")
        } else {
            p("60070.0")
        },
        ..HedgePx::default()
    };
    let it = ApprovedIntent::hedge(req, &plan, hp, IntentClass::Hedge);
    h.place(&it, t_ms)
}

/// The single emitted hedge request.
pub fn one_hedge(h: &H) -> HedgeRequest {
    assert_eq!(h.out.hedges.len(), 1, "hedges: {:?}", h.out.hedges.as_slice());
    h.out.hedges[0]
}

/// A `ReconcileReport` entry for slot `r` (epoch 7).
pub fn recon(r: OrderRef, item: u8, status: u8, hist: u8, exch_qty: Qty) -> ReconcileReport {
    ReconcileReport {
        report_id: 1,
        venue: r.venue,
        account: r.account,
        item,
        severity: 0,
        inst: InstId(0),
        asset: AssetId(0),
        idx: 0,
        count: 1,
        _pad: 0,
        engine_qty: Qty(0),
        exch_qty,
        diff: Notional(0),
        exch_ts_ns: 0,
        slot: SlotId(r.slot),
        order_status: status,
        history_checked: hist,
        _pad2: [0; 2],
        view_seq: 0,
        r#gen: r.r#gen,
        epoch: EPOCH as u16,
        _pad3: 0,
        exch_amount: Notional(0),
        engine_amount: Notional(0),
        sflags: 0,
        skipped_inflight: 0,
        _pad4: 0,
    }
}

/// Mixed state: 40 non-free slots on three tables, 5 non-Done links (one RESUME_HOLD) and one
/// Done link, non-zero LegStates for g1/g3 and a zero one for g2, 6 live tombstones,
/// foreign/ext_held 2 each, 20 queued decisions, resume_hold_until ≠ 0, decode_fails = 2.
pub fn mixed() -> (H, Vec<OrderRef>, Vec<u32>) {
    let mut h = H::new();
    // A zero LegState (a2's group) that the snapshot does not write.
    let g21 = hft_types::GroupId::compose(hft_types::StrategyId(2), 1).unwrap();
    h.o.on_groups_changed(&hft_strategy_sdk::GroupTable::new(
        &[group_spec(g21, &[(X, VS)])],
        8,
    ));
    // An expired tombstone, cleared by the sweep (not written).
    h.o.test_tomb_mut(VS, A1, 63, 1, |tb| {
        tb.r#gen = 1;
        tb.freed_tsc = Tsc(1);
    });
    h.sweep(3_600_002);
    let mut slots = Vec::new();
    let mut links = Vec::new();
    for i in 0..5 {
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Gtc, plan_l1(), 0);
        h.ack(f, 500 + i, 0);
        h.fill(f, q("0.004"), p("60000.00"), 1);
        slots.push(f);
        links.push(l);
    }
    h.o.test_link_mut(links[0], |e| {
        e.flags |= link_flags::RESUME_HOLD;
        e.held_ms = 1200;
    });
    let (fd, _ld) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.what(fd, OrderEventKind::Expired, 1);
    slots.push(fd);
    let kinds = [
        OrderState::PendingNew,
        OrderState::Live,
        OrderState::PendingCancel,
        OrderState::PendingAmend,
        OrderState::Filled,
        OrderState::Cancelled,
        OrderState::Rejected,
        OrderState::Unknown,
    ];
    let mut k = 0usize;
    let mut add = |h: &mut H, v, inst, n: usize, slots: &mut Vec<OrderRef>| {
        for _ in 0..n {
            let g = if v == VG { g2() } else { g1() };
            let it = intent(v, A1, inst, Side::Sell, Tif::Gtc, p("60100.0"), q("0.001"), g);
            let r = h.place_ok(&it, 2);
            let st = kinds[k % kinds.len()];
            k += 1;
            h.force(r, st, st != OrderState::PendingNew, OrderState::Live);
            slots.push(r);
        }
    };
    add(&mut h, VS, X, 8, &mut slots);
    add(&mut h, VX, Y, 13, &mut slots);
    add(&mut h, VG, Z, 13, &mut slots);
    assert_eq!(slots.len(), 40);
    let live: Vec<_> = slots
        .iter()
        .copied()
        .filter(|r| h.state(*r) == OrderState::Live)
        .collect();
    h.o.test_slot_mut(live[0].venue, live[0].account, live[0].slot, |sl, _| {
        sl.sflags.0 |= SlotFlags::VERIFY
    });
    let unk: Vec<_> = slots
        .iter()
        .copied()
        .filter(|r| h.state(*r) == OrderState::Unknown)
        .collect();
    h.o.test_set_smoke_slot(unk[0].venue, unk[0].account, unk[0].slot);
    h.o.test_slot_mut(unk[0].venue, unk[0].account, unk[0].slot, |sl, _| {
        sl.sflags.0 |= SlotFlags::SMOKE_DUE
    });
    for s in 50..53u32 {
        for (v, g) in [(VS, 1u32), (VX, 2)] {
            h.o.test_tomb_mut(v, A1, s, g, |tb| {
                tb.r#gen = g;
                tb.inst = X;
                tb.filled = q("0.001");
                tb.freed_tsc = ms(100);
            });
        }
    }
    for e in [7001u64, 7002] {
        h.o.test_set_foreign(VS, A1, e, q("0.001"));
        h.o.test_set_ext_held(VS, A1, e + 10, q("0.002"));
    }
    for r in slots.iter().take(20) {
        h.o.test_enqueue_decision(*r, hft_event::OrderAction::Query, 9);
    }
    h.o.test_set_resume_hold_until(ms(40_000));
    h.o.test_set_decode_fails(VX, A1, 2);
    h.o.test_set_net(S1, g3(), X, q("0.02"));
    h.out.clear();
    h.o.external_adjust(
        A1,
        VS,
        hft_types::GROUP_NONE,
        X,
        Qty(1),
        0,
        hft_event::ExternalCause::Manual,
        &mut h.out,
    );
    (h, slots, links)
}
