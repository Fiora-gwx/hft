//! Shared fixtures of the latency-probe tests (TESTS.md "共用夹具"): refdata `RL1`, parameters
//! `PP0`, the groups, views `MV0` (SDK `testing::MockViews` plus an order table) and the driver
//! `H` (a `CtxHarness`-equivalent over `CtxBufs` that records each call's intents by kind, checks Note
//! registration and feeds the default outcomes `Placed(r1)`, `Placed(r2)` …).
#![allow(dead_code)]
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use hft_event::{GroupSpec, LegDef};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule,
    FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LiqMarker, MarginData,
    MarginMode, PositionMode, PriceBand, RdbBuilder, RefData, StaleGuardKind, TestnetKind, TsUnit, UidPolicy,
    VenueCaps, VenueSpec,
};
use hft_strategy_sdk::ctx::{CancelReq, NoteReq, Op, TimerReq};
use hft_strategy_sdk::params::{ParamOverride, RawParams};
use hft_strategy_sdk::testing::{MockViews, RefFixture};
use hft_strategy_sdk::{
    BboView, CtxBufs, EngineClockAnchor, EventRef, GroupTable, IntentHandle, IntentOutcome, LegTrack,
    LegView, Now, OrderIntent, OrderRef, OrderView, RefView, RiskStateView, SnapshotReader, SnapshotWriter,
    Strategy, StrategyInstance, TargetIntent, TargetView, Views,
};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, RunMode, StrategyId,
    Tsc, VenueId,
};
use latency_probe::{LatencyProbe, ProbeParams};

pub use hft_strategy_sdk::testing::ev;

pub const V1: VenueId = VenueId::BINANCE_SPOT;
pub const V2: VenueId = VenueId::OKX_SWAP;
pub const V3: VenueId = VenueId::BITGET_UTA_USDT_FUT;
pub const I1: InstId = InstId(1);
pub const I2: InstId = InstId(2);
pub const I3: InstId = InstId(3);
/// ETH spot on V1 / perp on V2 (LPR-006).
pub const I4: InstId = InstId(4);
pub const I5: InstId = InstId(5);
pub const BTC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
pub const ETH: AssetId = AssetId(3);
pub const T1: AccountId = AccountId(1);
pub const S1: StrategyId = StrategyId(1);
/// engine_ns of `+0`.
pub const E0: i64 = 1_000_000_000_000;
/// UTC ns at `+0`.
pub const U: i64 = 1_759_708_800_000_000_000;
pub const TSC_PER_MS: u64 = 3_000_000;
/// TSC of `+0` (non-zero: a zero `conn_live_tsc` means "no evidence").
pub const BASE_TSC: u64 = 1_000_000_000;

pub const GQ: GroupId = GroupId(0x0500_0001);
pub const GX: GroupId = GroupId(0x0500_0002);
pub const GX3: GroupId = GroupId(0x0500_0003);
pub const GPS: GroupId = GroupId(0x0500_0004);
pub const GSP: GroupId = GroupId(0x0500_0005);
pub const GPP: GroupId = GroupId(0x0500_0006);
pub const GTS: GroupId = GroupId(0x0500_0007);

/// `+ms` → engine_ns.
pub const fn ms(x: i64) -> i64 {
    E0 + x * 1_000_000
}

// ------------------------------------------------------------------------------------ refdata

fn venue(b: &mut RdbBuilder, id: VenueId, exchange: Exchange, spot: bool) {
    b.venue(VenueSpec {
        id,
        exchange,
        product: if spot { InstKind::Spot } else { InstKind::Perp },
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 32,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.caps(VenueCaps {
        venue: id,
        market: if spot {
            MarketKind::Spot
        } else {
            MarketKind::LinearPerp
        },
        fee_asset_rule: if spot {
            FeeAssetRule::ReceivedAsset
        } else {
            FeeAssetRule::Quote
        },
        reduce_only: !spot,
        required_position_mode: if spot {
            PositionMode::NotApplicable
        } else {
            PositionMode::OneWay
        },
        required_margin_mode: if spot {
            MarginMode::NotApplicable
        } else {
            MarginMode::Cross
        },
        required_multi_asset: if spot { None } else { Some(false) },
        required_spot_borrow: None,
        funding_settle: if spot {
            FundingSettleKind::NotApplicable
        } else {
            FundingSettleKind::PerSymbolPush
        },
        margin_data: if spot {
            MarginData::NotApplicable
        } else {
            MarginData::Push
        },
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
}

fn asset(id: AssetId, name: &str, class: AssetClass) -> AssetSpec {
    AssetSpec {
        id,
        name: name.into(),
        class,
        exp: AssetExp {
            px_exp: -2,
            qty_exp: -8,
        },
        px_max: Px(1_000_000_000_000),
        qty_max: Qty(100_000_000_000_000),
        dust: Qty(1_000),
    }
}

fn inst(id: InstId, v: VenueId, a: AssetId, spot: bool, min_notional: i64) -> InstSpec {
    let (tick, lot, ts, ls) = if spot {
        (1, 1_000, "0.01", "0.00001")
    } else {
        (10, 100_000, "0.1", "0.001")
    };
    InstSpec {
        id,
        venue: v,
        symbol: format!("S{}", id.0),
        status: InstStatus::Trading,
        kind: if spot { InstKind::Spot } else { InstKind::Perp },
        flags: 0,
        asset: a,
        quote: USDT,
        px_unit_shift: 0,
        qty_unit_shift: 0,
        tick,
        lot,
        min_qty: Qty(lot),
        max_qty: Qty(100_000_000_000),
        min_notional: Notional(min_notional),
        book_window: 4096,
        l2_depth: 20,
        crossed_tolerance: 0,
        venue_num_id: 0,
        tick_str: ts.into(),
        lot_str: ls.into(),
        min_qty_str: ls.into(),
        funding_interval_s: if spot { 0 } else { 28_800 },
        funding_anchor_s: 0,
        max_leverage: if spot { 0 } else { 125 },
        listed_utc_ns: 0,
        delivery_utc_ns: 0,
        price_band: PriceBand::default(),
    }
}

/// `RL1` with I2's `min_notional` replaced (LPR-035).
pub fn rl1_with(i2_min_notional: i64, is_prod: bool) -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(U);
    b.asset(asset(BTC, "BTC", AssetClass::Crypto));
    b.asset(asset(USDT, "USDT", AssetClass::Stable));
    b.asset(asset(ETH, "ETH", AssetClass::Crypto));
    venue(&mut b, V1, Exchange::Binance, true);
    venue(&mut b, V2, Exchange::Okx, false);
    venue(&mut b, V3, Exchange::Bitget, false);
    b.inst(inst(I1, V1, BTC, true, 5_000_000));
    b.inst(inst(I2, V2, BTC, false, i2_min_notional));
    b.inst(inst(I3, V3, BTC, false, 5_000_000));
    b.inst(inst(I4, V1, ETH, true, 5_000_000));
    b.inst(inst(I5, V2, ETH, false, 5_000_000));
    b.instance(InstanceSpec {
        id: S1,
        kind_id: 5,
        name: "probe".into(),
    });
    let venues = [V1, V2, V3].iter().fold(0u64, |m, v| m | (1u64 << v.0));
    b.account(AccountSpec {
        id: T1,
        sim: false,
        bound_instance: S1,
        is_prod,
        venues,
        label: "t1".into(),
        uid_policy: UidPolicy::Verified,
    });
    for (v, uid) in [(V1, "1"), (V2, "2"), (V3, "3")] {
        b.account_venue(AccountVenueSpec {
            account: T1,
            venue: v,
            uid: uid.into(),
        });
    }
    for v in [V1, V2, V3] {
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(0),
                taker: Rate(500_000),
            },
        });
    }
    match RefData::load(&b.finish(), None) {
        Ok(rd) => rd,
        Err(e) => panic!("RL1 does not load: {e:?}"),
    }
}

pub fn rl1() -> RefData {
    rl1_with(5_000_000, false)
}

// ------------------------------------------------------------------------------------- groups

fn leg(i: InstId) -> LegDef {
    let v = match i.0 {
        1 | 4 => V1,
        2 | 5 => V2,
        _ => V3,
    };
    LegDef {
        inst: i,
        venue: v,
        kind: 1,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }
}

pub fn spec(id: GroupId, insts: &[InstId]) -> GroupSpec {
    let mut legs = [LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }; 4];
    for (d, i) in legs.iter_mut().zip(insts) {
        *d = leg(*i);
    }
    GroupSpec {
        id,
        asset: BTC,
        n_legs: insts.len() as u8,
        open_allowed: 1,
        legs,
        _rsv: 0,
    }
}

/// A group of the fixtures with its mode (enum index) and extra per-group overrides `(field, value)`.
#[derive(Clone, Debug)]
pub struct G {
    pub spec: GroupSpec,
    pub mode: i64,
    pub over: Vec<(u16, i64)>,
}

pub const QUOTE: i64 = 0;
pub const PAIR: i64 = 1;
pub const TARGET: i64 = 2;

pub fn g(id: GroupId, insts: &[InstId], mode: i64) -> G {
    G {
        spec: spec(id, insts),
        mode,
        over: Vec::new(),
    }
}
pub fn gq() -> G {
    g(GQ, &[I2], QUOTE)
}
pub fn gx() -> G {
    let mut x = g(GX, &[I2, I3], QUOTE);
    x.over.push((6, 1));
    x
}
pub fn gx3() -> G {
    let mut x = g(GX3, &[I2, I3, I1], QUOTE);
    x.over.push((6, 1));
    x
}
pub fn gps() -> G {
    g(GPS, &[I1, I2], PAIR)
}
pub fn gsp() -> G {
    g(GSP, &[I2, I1], PAIR)
}
pub fn gpp() -> G {
    g(GPP, &[I2, I3], PAIR)
}
pub fn gts() -> G {
    g(GTS, &[I1, I2], TARGET)
}

impl G {
    pub fn with(mut self, field: u16, value: i64) -> G {
        self.over.push((field, value));
        self
    }
}

/// `PP0` defaults in schema order.
pub fn pp0() -> Vec<i64> {
    vec![
        1,
        0,
        1_000,
        30_000_000,
        0,
        2_000,
        0,
        0,
        10_000,
        60_000,
        5_000,
        5_000_000,
        0,
        100_000_000,
        10_000_000,
        100_000_000,
        5,
    ]
}

/// Field indices of `ProbeParams`.
pub mod f {
    pub const ENABLED: u16 = 0;
    pub const MODE: u16 = 1;
    pub const PROBE_PERIOD_MS: u16 = 2;
    pub const FAR_BP: u16 = 3;
    pub const QUOTE_SIDE: u16 = 4;
    pub const QUOTE_REST_MS: u16 = 5;
    pub const QUOTE_XGW: u16 = 6;
    pub const BURST_PERIOD_MS: u16 = 7;
    pub const PAIR_PERIOD_MS: u16 = 8;
    pub const TARGET_PERIOD_MS: u16 = 9;
    pub const PAIR_FLAT_MS: u16 = 10;
    pub const PAIR_CROSS: u16 = 11;
    pub const PAIR_SIDE: u16 = 12;
    pub const SIZE_MARGIN: u16 = 13;
    pub const MAX_SPREAD: u16 = 14;
    pub const MAX_INV_NOTIONAL: u16 = 15;
    pub const MAX_CONSEC_FAIL: u16 = 16;
}

/// Raw parameters: `PP0` with `defaults` edits, plus each group's mode and overrides.
pub fn raw(defaults: &[(u16, i64)], groups: &[G]) -> RawParams {
    let mut d = pp0();
    for &(ix, v) in defaults {
        d[usize::from(ix)] = v;
    }
    let mut overrides = Vec::new();
    for (gix, g) in groups.iter().enumerate() {
        overrides.push(ParamOverride::new(gix as u16, f::MODE, g.mode));
        for &(ix, v) in &g.over {
            overrides.push(ParamOverride::new(gix as u16, ix, v));
        }
    }
    overrides.sort_by_key(|o| (o.group_ix, o.field_ix));
    overrides.dedup_by_key(|o| (o.group_ix, o.field_ix));
    RawParams {
        defaults: d,
        overrides,
    }
}

pub fn table(groups: &[G]) -> GroupTable {
    let specs: Vec<GroupSpec> = groups.iter().map(|g| g.spec).collect();
    GroupTable::new(&specs, 16)
}

pub fn params(defaults: &[(u16, i64)], groups: &[G]) -> ProbeParams {
    ProbeParams::from_raw(&raw(defaults, groups), groups.len() as u16)
}

// -------------------------------------------------------------------------------------- views

/// SDK `MockViews` plus an order table (`Views::order`).
#[derive(Clone, Debug)]
pub struct TV {
    pub m: MockViews,
    pub orders: Vec<OrderView>,
}

impl Views for TV {
    type Book<'a> = <MockViews as Views>::Book<'a>;
    type Portfolio<'a> = <MockViews as Views>::Portfolio<'a>;
    fn now(&self) -> Now {
        self.m.now()
    }
    fn bbo(&self, i: InstId) -> BboView {
        Views::bbo(&self.m, i)
    }
    fn book(&self, i: InstId) -> Self::Book<'_> {
        Views::book(&self.m, i)
    }
    fn funding(&self, i: InstId) -> &hft_strategy_sdk::FundingView {
        Views::funding(&self.m, i)
    }
    fn mark(&self, i: InstId) -> Px {
        Views::mark(&self.m, i)
    }
    fn portfolio(&self, a: AccountId) -> Self::Portfolio<'_> {
        Views::portfolio(&self.m, a)
    }
    fn group_pos(&self, a: AccountId, g: GroupId) -> &hft_strategy_sdk::GroupPosView {
        Views::group_pos(&self.m, a, g)
    }
    fn legs(&self, a: AccountId, g: GroupId) -> &LegView {
        Views::legs(&self.m, a, g)
    }
    fn target(&self, a: AccountId, g: GroupId) -> Option<&TargetView> {
        Views::target(&self.m, a, g)
    }
    fn order(&self, h: OrderRef) -> Option<OrderView> {
        self.orders.iter().find(|o| o.r == h).copied()
    }
    fn inst(&self, i: InstId) -> &hft_refdata::InstHot {
        Views::inst(&self.m, i)
    }
    fn caps(&self, v: VenueId) -> &VenueCaps {
        Views::caps(&self.m, v)
    }
    fn fee(&self, a: AccountId, i: InstId) -> hft_strategy_sdk::FeeView {
        Views::fee(&self.m, a, i)
    }
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode {
        Views::mode(&self.m, a, g)
    }
    fn risk(&self, a: AccountId) -> RiskStateView {
        Views::risk(&self.m, a)
    }
    fn risk_for(&self, a: AccountId, i: InstId) -> hft_strategy_sdk::InstRiskView {
        Views::risk_for(&self.m, a, i)
    }
    fn venue_ready(&self, a: AccountId, v: VenueId) -> bool {
        Views::venue_ready(&self.m, a, v)
    }
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32> {
        Views::leverage(&self.m, a, i)
    }
}

pub fn anchor(base_tsc: u64, tsc_per_ms: u64, base_ens: i64, utc_at_base: i64) -> EngineClockAnchor {
    EngineClockAnchor {
        base_engine_ns: base_ens,
        base_tsc: Tsc(base_tsc),
        tsc_hz_milli: tsc_per_ms * 1_000_000,
        utc_tsc: Tsc(base_tsc),
        utc_ns: utc_at_base,
    }
}

/// `MV0` quotes.
pub const QUOTES: [(InstId, i64, i64); 5] = [
    (I1, 6_000_000, 6_000_001),
    (I2, 6_000_000, 6_000_010),
    (I3, 6_000_100, 6_000_110),
    (I4, 300_000, 300_001),
    (I5, 300_000, 300_010),
];

// ------------------------------------------------------------------------------------- driver

/// What one `on_event` produced.
#[derive(Clone, Debug, Default)]
pub struct Rec {
    pub places: Vec<OrderIntent>,
    pub targets: Vec<TargetIntent>,
    pub clears: Vec<(AccountId, GroupId)>,
    pub cancels: Vec<CancelReq>,
    pub timers: Vec<TimerReq>,
    pub notes: Vec<NoteReq>,
    pub halts: Vec<(GroupId, u16)>,
    /// `OrderRef` given (as `Placed`) to each place of this call, in order.
    pub refs: Vec<OrderRef>,
}

impl Rec {
    pub fn notes_of(&self, code: u32) -> Vec<NoteReq> {
        self.notes.iter().filter(|n| n.code == code).copied().collect()
    }
    /// Places, targets and cancels (timers are not intents).
    pub fn intents(&self) -> usize {
        self.places.len() + self.targets.len() + self.cancels.len() + self.clears.len()
    }
}

/// The driver: one instance, its views and intent buffers. Every place gets `Placed(rN)` (N counts
/// from 1) and every target `TargetAccepted` unless `next_outcomes` replaces them.
pub struct H {
    pub s: LatencyProbe,
    pub v: TV,
    pub bufs: Box<CtxBufs>,
    pub groups: Vec<G>,
    pub table: GroupTable,
    pub p: ProbeParams,
    pub rd: &'static RefData,
    pub next_r: u32,
    pub healthy: bool,
    /// Outcomes for the intents of the previous call, by place/target order: `None` = default.
    pub next_outcomes: Option<Vec<IntentOutcome>>,
    pub all: Vec<Rec>,
}

impl std::fmt::Debug for H {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("H")
    }
}

/// `r<n>` of the fixtures (venue of `inst`).
pub fn r_of(n: u32, venue: VenueId) -> OrderRef {
    OrderRef::new(venue, T1, n, 1)
}

impl H {
    pub fn new(defaults: &[(u16, i64)], groups: Vec<G>) -> H {
        H::with_rd(rl1(), defaults, groups)
    }

    pub fn with_rd(rd: RefData, defaults: &[(u16, i64)], groups: Vec<G>) -> H {
        let rd: &'static RefData = Box::leak(Box::new(rd));
        let mut m = MockViews::new(RefFixture::new(rd.clone(), T1));
        MockViews::at(
            &mut m,
            Now::new(Tsc(BASE_TSC), 1, 1, true, anchor(BASE_TSC, TSC_PER_MS, E0, U)),
        );
        MockViews::risk(
            &mut m,
            T1,
            RiskStateView {
                conn_liveness_ms: 2_000,
                ..RiskStateView::EMPTY
            },
        );
        for (i, b, a) in QUOTES {
            MockViews::bbo(
                &mut m,
                i,
                (Px(b), Qty(1_000_000_000)),
                (Px(a), Qty(1_000_000_000)),
            );
        }
        let table = table(&groups);
        let p = params(defaults, &groups);
        let inst = StrategyInstance {
            strategy: S1,
            account: T1,
            kind_id: 5,
        };
        let s = LatencyProbe::new(inst, &p, &table, &RefView::new(rd, T1)).unwrap();
        let mut h = H {
            s,
            v: TV {
                m,
                orders: Vec::new(),
            },
            bufs: Box::new(CtxBufs::new()),
            groups,
            table,
            p,
            rd,
            next_r: 1,
            healthy: true,
            next_outcomes: None,
            all: Vec::new(),
        };
        h.at(0);
        h
    }

    /// Moves to `+x` ms (engine time); keeps every quote's connection live when `healthy`.
    pub fn at(&mut self, x: i64) -> &mut Self {
        self.at_ens(ms(x))
    }

    pub fn at_ens(&mut self, e: i64) -> &mut Self {
        MockViews::at_engine_ns(&mut self.v.m, e);
        if self.healthy {
            self.live_all();
        }
        self
    }

    pub fn now(&self) -> Now {
        self.v.m.now()
    }

    pub fn tsc_of(&self, e: i64) -> Tsc {
        self.now().tsc_of(e)
    }

    pub fn live_all(&mut self) {
        let t = self.now().tsc;
        for (i, _, _) in QUOTES {
            MockViews::conn(&mut self.v.m, i, t, true, false);
        }
    }

    /// Sets the quote of `i` (rx_tsc = now).
    pub fn quote(&mut self, i: InstId, bid: i64, ask: i64) -> &mut Self {
        MockViews::bbo(
            &mut self.v.m,
            i,
            (Px(bid), Qty(1_000_000_000)),
            (Px(ask), Qty(1_000_000_000)),
        );
        self
    }

    /// The current `BboView` of `i` with `rx_tsc` overridden when given.
    pub fn bbo_view(&self, i: InstId) -> BboView {
        self.v.m.bbo_of(i)
    }

    pub fn gix_of(&self, g: GroupId) -> u16 {
        self.table.gix_of(g).unwrap()
    }

    /// Sets the net quantity of leg `k` of `g` (and keeps the other legs).
    pub fn net(&mut self, g: GroupId, k: usize, q: i64) -> &mut Self {
        let mut l = *Views::legs(&self.v.m, T1, g);
        l.n_legs = l.n_legs.max(k as u8 + 1);
        l.leg[k].net = Qty(q);
        MockViews::legs(&mut self.v.m, T1, g, l);
        self
    }

    pub fn inflight(&mut self, g: GroupId, k: usize, q: i64) -> &mut Self {
        let mut l = *Views::legs(&self.v.m, T1, g);
        l.leg[k].inflight_immediate = Qty(q);
        MockViews::legs(&mut self.v.m, T1, g, l);
        self
    }

    pub fn flat(&mut self, g: GroupId) -> &mut Self {
        MockViews::legs(&mut self.v.m, T1, g, LegView::EMPTY);
        self
    }

    pub fn mode(&mut self, g: GroupId, m: RunMode) -> &mut Self {
        MockViews::mode(&mut self.v.m, T1, g, m);
        self
    }

    /// Delivers `e` at the current time and records the result.
    pub fn ev(&mut self, e: &ev::Ev) -> Rec {
        self.deliver(&e.as_ref())
    }

    pub fn deliver(&mut self, e: &EventRef<'_>) -> Rec {
        self.bufs.begin();
        {
            let mut ctx = self.bufs.ctx();
            self.s.on_event(e, &self.v, &mut ctx);
        }
        let mut rec = Rec::default();
        let mut outs: Vec<(IntentHandle, bool)> = Vec::new();
        for (h, op) in self.bufs.ops() {
            match op {
                Op::Place(p) => {
                    rec.places.push(*p);
                    outs.push((h, true));
                }
                Op::Target(t) => {
                    rec.targets.push(*t);
                    outs.push((h, false));
                }
                Op::ClearTarget(a, g) => rec.clears.push((a, g)),
                Op::Cancel(c) => rec.cancels.push(*c),
                Op::Timer(t) => rec.timers.push(*t),
                Op::CancelTimer(_) => {}
                Op::Halt { group, reason } => rec.halts.push((group, reason)),
            }
        }
        for n in self.bufs.notes() {
            let d = LatencyProbe::DESC.notes.iter().find(|d| d.code == n.code);
            assert!(
                d.is_some_and(|d| d.level == n.level),
                "unregistered Note {} / {:?}",
                n.code,
                n.level
            );
            rec.notes.push(*n);
        }
        self.bufs.clear_outcomes();
        let given = self.next_outcomes.take();
        for (k, (h, place)) in outs.iter().enumerate() {
            let o = match given.as_ref().and_then(|g| g.get(k)) {
                Some(o) => *o,
                None if *place => {
                    let v = rec
                        .places
                        .iter()
                        .filter(|_| true)
                        .nth(rec.refs.len())
                        .map_or(V1, |p| p.venue);
                    let r = r_of(self.next_r, v);
                    self.next_r += 1;
                    rec.refs.push(r);
                    IntentOutcome::Placed(r)
                }
                None => IntentOutcome::TargetAccepted,
            };
            if let IntentOutcome::Placed(r) = o
                && given.is_some()
                && !rec.refs.contains(&r)
            {
                rec.refs.push(r);
            }
            self.bufs.set_outcome(*h, o);
        }
        self.all.push(rec.clone());
        rec
    }

    /// `Bbo` of `i` for the group containing it (the first one) at the current time.
    pub fn bbo(&mut self, i: InstId) -> Rec {
        let gix = self.gix_containing(i);
        let mut b = self.bbo_view(i);
        b.rx_tsc = self.now().tsc;
        self.ev(&ev::bbo(i, gix, b))
    }

    /// `Bbo` with an explicit `rx_tsc`.
    pub fn bbo_rx(&mut self, i: InstId, rx: u64) -> Rec {
        let gix = self.gix_containing(i);
        let mut b = self.bbo_view(i);
        b.rx_tsc = Tsc(rx);
        MockViews::bbo_rx(&mut self.v.m, i, Tsc(rx), 0, hft_strategy_sdk::RxTsSrc::None);
        self.ev(&ev::bbo(i, gix, b))
    }

    pub fn gix_containing(&self, i: InstId) -> u16 {
        self.table
            .iter()
            .find(|(_, s)| s.legs.iter().take(usize::from(s.n_legs)).any(|l| l.inst == i))
            .map_or(0, |(g, _)| g)
    }

    pub fn order(
        &mut self,
        g: GroupId,
        r: OrderRef,
        tag: u32,
        kind: hft_strategy_sdk::OrderUpdateKind,
    ) -> Rec {
        let gix = self.gix_of(g);
        self.ev(&ev::order(
            gix,
            hft_strategy_sdk::OrderUpdate {
                r,
                group: g,
                tag,
                kind,
            },
        ))
    }

    pub fn timer(&mut self, g: GroupId, tag: u32) -> Rec {
        let gix = self.gix_of(g);
        self.ev(&ev::timer(tag, g, gix))
    }

    pub fn exec(&mut self, g: GroupId, kind: hft_strategy_sdk::ExecUpdateKind) -> Rec {
        let gix = self.gix_of(g);
        self.ev(&ev::exec(
            gix,
            hft_strategy_sdk::ExecUpdate {
                group: g,
                tag: 0,
                origin: hft_strategy_sdk::TargetOrigin::Strategy,
                kind,
                target: [Qty(0); 4],
                filled: [Qty(0); 4],
                inflight: [Qty(0); 4],
            },
        ))
    }

    pub fn params(&mut self, defaults: &[(u16, i64)], groups: Vec<G>) {
        let table = table(&groups);
        let p = params(defaults, &groups);
        self.s.on_params(&p, &table, &RefView::new(self.rd, T1)).unwrap();
        self.groups = groups;
        self.table = table;
        self.p = p;
    }

    pub fn snapshot(&self) -> Vec<u8> {
        let mut w = SnapshotWriter::new(262_144);
        self.s.snapshot(&mut w);
        w.as_bytes().to_vec()
    }

    /// Snapshot → `MockViews::restart` (engine time continues) → new instance `restore`.
    pub fn restart(&mut self, new_tsc: u64, boot_seq: u64) {
        let b = self.snapshot();
        MockViews::restart(&mut self.v.m, Tsc(new_tsc), boot_seq);
        let inst = self.s.inst;
        let mut s = LatencyProbe::new(inst, &self.p, &self.table, &RefView::new(self.rd, T1)).unwrap();
        s.restore(&mut SnapshotReader::with_desc_version(&b, 1)).unwrap();
        self.s = s;
        *self.bufs = CtxBufs::new();
        for (i, b, a) in QUOTES {
            MockViews::bbo(
                &mut self.v.m,
                i,
                (Px(b), Qty(1_000_000_000)),
                (Px(a), Qty(1_000_000_000)),
            );
        }
        if self.healthy {
            self.live_all();
        }
    }

    pub fn phase(&self, g: GroupId) -> latency_probe::Phase {
        self.s.group(self.gix_of(g)).unwrap().phase
    }
    pub fn group(&self, g: GroupId) -> latency_probe::ProbeGroup {
        *self.s.group(self.gix_of(g)).unwrap()
    }
}

pub fn acked() -> hft_strategy_sdk::OrderUpdateKind {
    hft_strategy_sdk::OrderUpdateKind::Acked
}
pub fn cancelled() -> hft_strategy_sdk::OrderUpdateKind {
    hft_strategy_sdk::OrderUpdateKind::Cancelled
}
pub fn filled(d: i64, px: i64, done: bool) -> hft_strategy_sdk::OrderUpdateKind {
    hft_strategy_sdk::OrderUpdateKind::Filled {
        d: Qty(d),
        px: Px(px),
        fee: 0,
        fee_asset: USDT,
        cum: Qty(d),
        done,
    }
}
pub fn rejected(bits: u32, code: u32) -> hft_strategy_sdk::OrderUpdateKind {
    hft_strategy_sdk::OrderUpdateKind::Rejected {
        bits: hft_types::RejectBits(bits),
        venue_code: code,
    }
}

/// Tags of the strategy's orders.
pub const fn probe_tag(leg: u32) -> u32 {
    (1 << 8) | leg
}
pub const OPEN_TAG: u32 = 2 << 8;
pub const CLOSE_TAG: u32 = 3 << 8;
pub const fn resid_tag(leg: u32) -> u32 {
    (4 << 8) | leg
}

pub fn legtrack(net: i64) -> LegTrack {
    LegTrack {
        net: Qty(net),
        ..LegTrack::ZERO
    }
}
