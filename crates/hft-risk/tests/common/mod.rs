//! Baselines `R0` and `G0` of crates/hft-risk/TESTS.md, the mock views and intent builders.
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_arguments
)] // allow-list: test code, not a hot path

use std::collections::{HashMap, HashSet};

use hft_event::{
    AccountConfigReportHdr, CmdScope, GroupSpec, LegDef, LeverageItem, OrderFlags, ReconcileReport,
};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule,
    FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LimitBucket, LimitScope,
    LiqMarker, MarginData, MarginMode, PositionMode, PriceBand, RateLimit, RdbBuilder, RefData,
    SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit, UidPolicy, VenueCaps, VenueSpec,
};
use hft_risk::mode::{
    CellKey, Effective, GateEvent, KillCmd, ModeErr, ModeInput, ModeOutcome, ModeView, RecoveryFlags,
};
use hft_risk::{
    AcctLimits, Admission, Capacities, Classification, DEFAULT_MARGIN, EquityView, ExecBounds, FixedVec,
    GroupLimits, GroupLimitsRow, GwMap, InstClassRow, IntentDraft, IntentView, MarginLimits, MarginOut,
    MarginSample, ModeCfg, Origin, PosFacts, PreVec, PxBand, ReconcileGroup, Risk, RiskParams, RiskView,
    Scope, VenueLimits,
};
use hft_strategy_sdk::{BboView, EngineClockAnchor, IntentClass, LinkRole, Now, RxTsSrc};
use hft_types::{
    AccountId, AssetExp, AssetId, CapSource, GROUP_ALL, GROUP_NONE, GroupId, InstId, KillLevel, MarketKind,
    Notional, Px, Qty, Rate, RejectBits, RunMode, Side, StrategyId, Tif, Tsc, VenueId,
};

// ---------------------------------------------------------------------------------------- ids

pub const A: VenueId = VenueId(2);
pub const B: VenueId = VenueId(3);
pub const C: VenueId = VenueId(5);
pub const A1: InstId = InstId(1);
pub const A2: InstId = InstId(2);
pub const B1: InstId = InstId(3);
pub const B2: InstId = InstId(4);
pub const C1: InstId = InstId(5);
pub const BTC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
pub const ETH: AssetId = AssetId(3);
pub const G1: GroupId = GroupId(0x0100_0001);
pub const G3: GroupId = GroupId(0x0100_0002);
pub const G2: GroupId = GroupId(0x0200_0001);
pub const GS: GroupId = GroupId(0x0300_0001);
pub const T0: i64 = 1_000_000_000_000;
pub const SEC: i64 = 1_000_000_000;
pub const MS: i64 = 1_000_000;
pub const EXP: AssetExp = AssetExp {
    px_exp: -2,
    qty_exp: -8,
};

pub const fn acc(a: u8) -> AccountId {
    AccountId(a)
}
pub const fn sid(s: u8) -> StrategyId {
    StrategyId(s)
}
pub const fn cell(s: u8, a: u8, g: GroupId) -> CellKey {
    CellKey {
        strategy: StrategyId(s),
        account: AccountId(a),
        group: g,
    }
}

// ---------------------------------------------------------------------------------------- numbers

fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '_').collect()
}
fn dec(s: &str, exp: i8) -> i64 {
    let t = clean(s);
    let (neg, body) = match t.strip_prefix('-').or_else(|| t.strip_prefix('−')) {
        Some(b) => (true, b.to_string()),
        None => (false, t),
    };
    let v = hft_types::parse_exact(body.as_bytes(), exp).unwrap_or_else(|e| panic!("bad decimal {s}: {e:?}"));
    if neg { -v } else { v }
}
/// Price at exponent −2.
pub fn px(s: &str) -> Px {
    Px(dec(s, -2))
}
/// Quantity at exponent −8.
pub fn q(s: &str) -> Qty {
    Qty(dec(s, -8))
}
/// Notional (1e-6 USDT).
pub fn n(s: &str) -> Notional {
    Notional(dec(s, -6))
}
/// Rate (1e-9).
pub fn r(s: &str) -> Rate {
    Rate(dec(s, -9))
}
pub fn notional(qty: Qty, p: Px) -> Notional {
    Risk::notional(qty, p, EXP, Rate(1_000_000_000))
}

// ---------------------------------------------------------------------------------------- time

pub fn anchor(base_ens: i64) -> EngineClockAnchor {
    EngineClockAnchor {
        base_engine_ns: base_ens,
        base_tsc: Tsc(0),
        tsc_hz_milli: 1_000_000_000_000,
        utc_tsc: Tsc(0),
        utc_ns: 0,
    }
}

/// `Now` at engine_ns `ens` of a run anchored at engine_ns `base` with TSC 0 (1 tick = 1 ns).
pub fn now_run(base: i64, ens: i64) -> Now {
    Now::new(Tsc((ens - base).max(0) as u64), 0, 0, true, anchor(base))
}

/// `Now` of the first run (tsc = engine_ns).
pub fn now_at(ens: i64) -> Now {
    now_run(0, ens)
}

// ---------------------------------------------------------------------------------------- refdata

/// Editable refdata input.
#[derive(Clone, Debug)]
pub struct Spec {
    pub insts: Vec<InstSpec>,
    pub accounts: Vec<AccountSpec>,
    pub instances: Vec<InstanceSpec>,
    pub venues: Vec<VenueSpec>,
    pub caps: Vec<VenueCaps>,
    pub limits: Vec<(VenueId, RateLimit)>,
    pub sims: Vec<SimSpec>,
    pub uids: Vec<AccountVenueSpec>,
}

fn asset(id: u16, name: &str, class: AssetClass, dust: i64) -> AssetSpec {
    AssetSpec {
        id: AssetId(id),
        name: name.into(),
        class,
        exp: AssetExp {
            px_exp: -2,
            qty_exp: -8,
        },
        px_max: Px(1_000_000_000_000_000),
        qty_max: Qty(1_000_000_000_000_000_000),
        dust: Qty(dust),
    }
}

pub fn venue_spec(id: VenueId, ex: Exchange, product: InstKind) -> VenueSpec {
    VenueSpec {
        id,
        exchange: ex,
        product,
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
    }
}

pub fn caps_spec(v: VenueId, perp: bool, margin: MarginData) -> VenueCaps {
    VenueCaps {
        venue: v,
        market: if perp {
            MarketKind::LinearPerp
        } else {
            MarketKind::Spot
        },
        fee_asset_rule: if perp {
            FeeAssetRule::Quote
        } else {
            FeeAssetRule::ReceivedAsset
        },
        reduce_only: perp,
        required_position_mode: if perp {
            PositionMode::OneWay
        } else {
            PositionMode::NotApplicable
        },
        required_margin_mode: if perp {
            MarginMode::Cross
        } else {
            MarginMode::NotApplicable
        },
        required_multi_asset: if perp { Some(false) } else { None },
        required_spot_borrow: None,
        funding_settle: if perp {
            FundingSettleKind::PerSymbolPush
        } else {
            FundingSettleKind::NotApplicable
        },
        margin_data: margin,
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
    }
}

pub fn inst_spec(
    id: InstId,
    v: VenueId,
    sym: &str,
    perp: bool,
    asset: AssetId,
    tick: (&str, i64),
    lot: (&str, i64),
) -> InstSpec {
    InstSpec {
        id,
        venue: v,
        symbol: sym.into(),
        status: InstStatus::Trading,
        kind: if perp { InstKind::Perp } else { InstKind::Spot },
        asset,
        quote: USDT,
        tick: tick.1,
        lot: lot.1,
        min_qty: Qty(lot.1),
        max_qty: Qty(100_000_000_000_000),
        min_notional: Notional(5_000_000),
        book_window: 4096,
        l2_depth: 20,
        tick_str: tick.0.into(),
        lot_str: lot.0.into(),
        min_qty_str: lot.0.into(),
        funding_interval_s: if perp { 28_800 } else { 0 },
        max_leverage: if perp { 125 } else { 0 },
        price_band: PriceBand::default(),
        ..InstSpec::default()
    }
}

pub fn vbits(vs: &[VenueId]) -> u64 {
    vs.iter().fold(0, |m, v| m | 1u64 << v.0)
}

pub fn account_spec(id: u8, sim: bool, bound: u8, venues: &[VenueId]) -> AccountSpec {
    AccountSpec {
        id: AccountId(id),
        sim,
        bound_instance: StrategyId(bound),
        is_prod: false,
        venues: vbits(venues),
        label: format!("acct-{id}"),
        uid_policy: UidPolicy::Verified,
    }
}

pub fn order_limits(v: VenueId, l10: u32, l60: u32) -> Vec<(VenueId, RateLimit)> {
    vec![
        (
            v,
            RateLimit {
                scope: LimitScope::Account,
                bucket: LimitBucket::Orders,
                limit: l10,
                window_ms: 10_000,
                default_cost: 1,
            },
        ),
        (
            v,
            RateLimit {
                scope: LimitScope::Account,
                bucket: LimitBucket::Orders,
                limit: l60,
                window_ms: 60_000,
                default_cost: 1,
            },
        ),
    ]
}

/// The R0 refdata input.
pub fn r0_spec() -> Spec {
    let insts = vec![
        inst_spec(A1, A, "BTCUSDT", false, BTC, ("0.01", 1), ("0.0001", 10_000)),
        inst_spec(A2, A, "ETHUSDT", false, ETH, ("0.01", 1), ("0.001", 100_000)),
        inst_spec(B1, B, "BTC_USDT", true, BTC, ("0.10", 10), ("0.001", 100_000)),
        inst_spec(B2, B, "ETH_USDT", true, ETH, ("0.01", 1), ("0.01", 1_000_000)),
        inst_spec(C1, C, "BTC-USDT-SWAP", true, BTC, ("0.1", 10), ("0.001", 100_000)),
    ];
    let accounts = vec![
        account_spec(1, false, 1, &[A, B, C]),
        account_spec(2, false, 2, &[A, B]),
        account_spec(3, true, 3, &[A, B]),
    ];
    let instances = (1..=3)
        .map(|i| InstanceSpec {
            id: StrategyId(i),
            kind_id: 0xFF00,
            name: format!("inst-{i}").into(),
        })
        .collect();
    let venues = vec![
        venue_spec(A, Exchange::Binance, InstKind::Spot),
        venue_spec(B, Exchange::Gate, InstKind::Perp),
        venue_spec(C, Exchange::Okx, InstKind::Perp),
    ];
    let caps = vec![
        caps_spec(A, false, MarginData::NotApplicable),
        caps_spec(B, true, MarginData::Push),
        caps_spec(C, true, MarginData::Poll { interval_ms: 2_000 }),
    ];
    let mut limits = order_limits(A, 100, 600);
    limits.extend(order_limits(B, 100, 600));
    limits.extend(order_limits(C, 100, 600));
    let sims = vec![SimSpec {
        account: AccountId(3),
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    }];
    Spec {
        insts,
        accounts,
        instances,
        venues,
        caps,
        limits,
        sims,
        uids: Vec::new(),
    }
}

/// Loads a spec (UIDs of every Live account's venues are generated).
pub fn load(sp: &Spec) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    b.asset(asset(1, "BTC", AssetClass::Crypto, 1_000));
    b.asset(asset(2, "USDT", AssetClass::Stable, 1_000_000));
    b.asset(asset(3, "ETH", AssetClass::Crypto, 1_000));
    for v in &sp.venues {
        b.venue(v.clone());
    }
    for c in &sp.caps {
        b.caps(c.clone());
    }
    for i in &sp.insts {
        b.inst(i.clone());
    }
    for i in &sp.instances {
        b.instance(i.clone());
    }
    for a in &sp.accounts {
        b.account(a.clone());
    }
    for a in &sp.accounts {
        if a.sim {
            continue;
        }
        for v in 0u8..64 {
            if a.venues & (1u64 << v) != 0 {
                b.account_venue(AccountVenueSpec {
                    account: a.id,
                    venue: VenueId(v),
                    uid: format!("uid-{}-{v}", a.id.0),
                });
            }
        }
    }
    for v in &sp.venues {
        for tier in 0..=1u8 {
            b.fee(FeeSpec {
                venue: v.id,
                tier,
                fee: hft_refdata::FeePair {
                    maker: Rate(200_000),
                    taker: Rate(500_000),
                },
            });
        }
    }
    for s in &sp.sims {
        b.sim(*s);
        for v in 0u8..64 {
            let m = sp
                .accounts
                .iter()
                .find(|a| a.id == s.account)
                .map_or(0, |a| a.venues);
            if m & (1u64 << v) != 0 {
                b.sim_balance(SimBalanceSpec {
                    account: s.account,
                    venue: VenueId(v),
                    asset: USDT,
                    qty: Qty(1_000_000_000_000_000),
                });
            }
        }
    }
    for (v, l) in &sp.limits {
        b.limit(*v, *l);
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => Box::leak(Box::new(r)),
        Err(e) => panic!("test refdata rejected: {e:?}"),
    }
}

pub fn r0_refdata() -> &'static RefData {
    load(&r0_spec())
}

// ---------------------------------------------------------------------------------------- params

pub fn band(dev: &str, ioc: &str, rr: &str) -> PxBand {
    PxBand {
        max_px_dev: r(dev),
        ioc_max_cross: r(ioc),
        rr_max_cross: r(rr),
    }
}

pub fn group_limits(max_pos: &str) -> GroupLimits {
    GroupLimits {
        max_group_notional: n("50000"),
        max_pos_qty: q(max_pos),
        naked_gate: r("0.5"),
        naked_gate_min: n("100"),
    }
}

pub fn acct_limits(a: u8) -> AcctLimits {
    AcctLimits {
        account: AccountId(a),
        max_order_notional: n("10000"),
        default_group: group_limits("1"),
        max_account_notional: n("100000"),
        account_notional_from_equity: r("1.0"),
        max_hedge_notional: n("20000"),
        max_reduce_notional: n("10000"),
        daily_loss_kill: r("0.05"),
        daily_loss_abs: n("5000"),
        daily_loss_max_resumes: 1,
        recon_closeonly_notional: n("100"),
        unknown_blocks_open: true,
        funds_buffer: r("0.05"),
        margin: MarginLimits {
            margin_stale_ms: 15_000,
            ..DEFAULT_MARGIN
        },
    }
}

pub fn venue_limits(v: VenueId, a: u8) -> VenueLimits {
    VenueLimits {
        venue: v,
        account: AccountId(a),
        max_venue_notional: n("80000"),
        inst_bucket_per_s: 1_000,
        backoff_base_ms: 1_000,
        backoff_max_ms: 8_000,
        rr_reserve: r("0.20"),
    }
}

/// `RiskParams` P0 over the accounts and venues of `rd`.
pub fn p0_for(rd: &RefData, n_accounts: u8) -> RiskParams {
    let mut classes = FixedVec::new();
    classes.push(band("0.03", "0.005", "0.01"));
    classes.push(band("0.05", "0.01", "0.02"));
    let mut accounts = Vec::new();
    let mut venues = Vec::new();
    for a in 1..=n_accounts {
        accounts.push(acct_limits(a));
        let m = rd.account(AccountId(a));
        for v in 0u8..64 {
            if m.venues & (1u64 << v) != 0 {
                venues.push(venue_limits(VenueId(v), a));
            }
        }
    }
    let mut groups = vec![
        GroupLimitsRow {
            account: acc(1),
            group: G1,
            l: group_limits("1"),
        },
        GroupLimitsRow {
            account: acc(1),
            group: G3,
            l: group_limits("20"),
        },
        GroupLimitsRow {
            account: acc(2),
            group: G2,
            l: group_limits("1"),
        },
        GroupLimitsRow {
            account: acc(3),
            group: GS,
            l: group_limits("1"),
        },
    ];
    groups.retain(|g| g.account.0 <= n_accounts);
    let risk_class = vec![
        InstClassRow { inst: A1, class: 0 },
        InstClassRow { inst: A2, class: 1 },
        InstClassRow { inst: B1, class: 0 },
        InstClassRow { inst: B2, class: 1 },
        InstClassRow { inst: C1, class: 0 },
    ]
    .into_iter()
    .filter(|r| (r.inst.0 as usize) <= rd.inst_count())
    .collect();
    RiskParams {
        version: 1,
        generated_at_ns: 0,
        author: [0; 16],
        rate_headroom: Rate(0),
        conn_liveness_ms: 2_000,
        max_perp_leverage_x100: 300,
        l3_max_cross: Rate(0),
        risk_class: PreVec::from_vec(risk_class),
        accounts: PreVec::from_vec(accounts),
        venues: PreVec::from_vec(venues),
        classes,
        groups: PreVec::from_vec(groups),
    }
}

pub fn exec_bounds(n_accounts: usize) -> Vec<ExecBounds> {
    let mut v = vec![ExecBounds::default(); n_accounts + 1];
    for b in v.iter_mut().skip(1) {
        *b = ExecBounds {
            force_max_cross: r("0.01"),
            force_max_chunk: n("10000"),
            dust_notional: n("10"),
        };
    }
    v
}

pub const CAPS: Capacities = Capacities {
    max_insts: 64,
    max_cells_total: 32,
    max_caps: 256,
};

pub const MC: ModeCfg = ModeCfg {
    auto_resume_after_clean_drain: true,
    allow_missing_standby: false,
};

pub fn gws() -> GwMap {
    GwMap {
        venues: vec![A, B, C],
    }
}

/// Mutates params of an account row.
pub fn with_acct(p: &RiskParams, a: u8, f: impl Fn(&mut AcctLimits)) -> RiskParams {
    let mut p = p.clone();
    let mut v: Vec<AcctLimits> = p.accounts.iter().copied().collect();
    for x in v.iter_mut().filter(|x| x.account.0 == a) {
        f(x);
    }
    p.accounts = PreVec::from_vec(v);
    p
}

pub fn with_venue(p: &RiskParams, v: VenueId, a: u8, f: impl Fn(&mut VenueLimits)) -> RiskParams {
    let mut p = p.clone();
    let mut rows: Vec<VenueLimits> = p.venues.iter().copied().collect();
    for x in rows.iter_mut().filter(|x| x.venue == v && x.account.0 == a) {
        f(x);
    }
    p.venues = PreVec::from_vec(rows);
    p
}

pub fn with_group(p: &RiskParams, a: u8, g: GroupId, f: impl Fn(&mut GroupLimits)) -> RiskParams {
    let mut p = p.clone();
    let mut rows: Vec<GroupLimitsRow> = p.groups.iter().copied().collect();
    for x in rows.iter_mut().filter(|x| x.account.0 == a && x.group == g) {
        f(&mut x.l);
    }
    p.groups = PreVec::from_vec(rows);
    p
}

pub fn with_class(p: &RiskParams, k: usize, f: impl Fn(&mut PxBand)) -> RiskParams {
    let mut p = p.clone();
    let mut c = FixedVec::new();
    for (i, b) in p.classes.iter().enumerate() {
        let mut b = *b;
        if i == k {
            f(&mut b);
        }
        c.push(b);
    }
    p.classes = c;
    p
}

// ---------------------------------------------------------------------------------------- mock views

#[derive(Clone, Debug, Default)]
pub struct Mv {
    pub tsc: u64,
    pub pos: HashMap<(u8, u32), i64>,
    pub open: HashMap<(u8, u32, u8), i64>,
    pub open_imm: HashMap<(u8, u32, u8), i64>,
    pub pend: HashMap<(u8, u32, u8), i64>,
    pub pend_imm: HashMap<(u8, u32, u8), i64>,
    pub held: HashMap<(u8, ScopeKey), i64>,
    pub open_n: HashMap<(u8, ScopeKey), i64>,
    pub not_ready: HashSet<(u8, u8)>,
    pub unknown: HashSet<(u8, u32)>,
    pub equity: HashMap<u8, EquityView>,
    pub bbo: HashMap<u32, BboView>,
    /// TSC ticks of connection silence per inst (default 0).
    pub silence: HashMap<u32, u64>,
    pub avail_quote: HashMap<(u8, u8), Option<i64>>,
    pub avail_margin: HashMap<(u8, u8), Option<i64>>,
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ScopeKey {
    Group(u32),
    Account,
    Venue(u8),
}

fn sk(s: Scope) -> ScopeKey {
    match s {
        Scope::Group(g) => ScopeKey::Group(g.0),
        Scope::Account => ScopeKey::Account,
        Scope::Venue(v) => ScopeKey::Venue(v.0),
    }
}

pub fn bbo_of(bid: &str, ask: &str) -> BboView {
    BboView {
        bid_px: px(bid),
        bid_qty: q("1"),
        ask_px: px(ask),
        ask_qty: q("1"),
        update_id: 1,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        hw_rx_ns: 0,
        rx_ts_src: RxTsSrc::None,
        seq: 1,
        conn_live_tsc: Tsc(0),
        conn_up: true,
        resync: false,
    }
}

const fn side_k(s: Side) -> u8 {
    s as u8
}

impl Mv {
    pub fn r0() -> Mv {
        let mut m = Mv::default();
        m.bbo.insert(A1.0, bbo_of("49999.00", "50001.00"));
        m.bbo.insert(A2.0, bbo_of("2499.50", "2500.50"));
        m.bbo.insert(B1.0, bbo_of("49995.0", "50005.0"));
        m.bbo.insert(B2.0, bbo_of("2499.0", "2501.0"));
        m.bbo.insert(C1.0, bbo_of("49995.0", "50005.0"));
        m
    }
    pub fn set_pos(&mut self, a: u8, i: InstId, qty: &str) {
        self.pos.insert((a, i.0), q(qty).0);
    }
    pub fn set_open(&mut self, a: u8, i: InstId, s: Side, qty: &str) {
        self.open.insert((a, i.0, side_k(s)), q(qty).0);
    }
    pub fn set_open_imm(&mut self, a: u8, i: InstId, s: Side, qty: &str) {
        self.open_imm.insert((a, i.0, side_k(s)), q(qty).0);
    }
    pub fn set_pend(&mut self, a: u8, i: InstId, s: Side, qty: &str) {
        self.pend.insert((a, i.0, side_k(s)), q(qty).0);
    }
    pub fn set_pend_imm(&mut self, a: u8, i: InstId, s: Side, qty: &str) {
        self.pend_imm.insert((a, i.0, side_k(s)), q(qty).0);
    }
    pub fn set_held(&mut self, a: u8, s: Scope, v: &str) {
        self.held.insert((a, sk(s)), n(v).0);
    }
    pub fn set_open_n(&mut self, a: u8, s: Scope, v: &str) {
        self.open_n.insert((a, sk(s)), n(v).0);
    }
    pub fn bbo_mut(&mut self, i: InstId) -> &mut BboView {
        self.bbo.get_mut(&i.0).unwrap()
    }
    /// Position facts for `classify`.
    pub fn pos_facts(&self, a: u8, i: InstId) -> PosFacts {
        PosFacts {
            pos: Qty(*self.pos.get(&(a, i.0)).unwrap_or(&0)),
            pending_buy: Qty(*self.pend.get(&(a, i.0, 1)).unwrap_or(&0)),
            pending_sell: Qty(*self.pend.get(&(a, i.0, 2)).unwrap_or(&0)),
        }
    }
}

impl RiskView for Mv {
    fn pos(&self, a: AccountId, i: InstId) -> Qty {
        Qty(*self.pos.get(&(a.0, i.0)).unwrap_or(&0))
    }
    fn held_notional(&self, a: AccountId, s: Scope) -> Notional {
        Notional(*self.held.get(&(a.0, sk(s))).unwrap_or(&0))
    }
    fn open_qty(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        Qty(*self.open.get(&(a.0, i.0, side_k(side))).unwrap_or(&0))
    }
    fn open_qty_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        Qty(*self.open_imm.get(&(a.0, i.0, side_k(side))).unwrap_or(&0))
    }
    fn open_notional(&self, a: AccountId, s: Scope) -> Notional {
        Notional(*self.open_n.get(&(a.0, sk(s))).unwrap_or(&0))
    }
    fn pending_reduce(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        Qty(*self.pend.get(&(a.0, i.0, side_k(side))).unwrap_or(&0))
    }
    fn pending_reduce_immediate(&self, a: AccountId, i: InstId, side: Side) -> Qty {
        Qty(*self.pend_imm.get(&(a.0, i.0, side_k(side))).unwrap_or(&0))
    }
    fn venue_ready(&self, v: VenueId, a: AccountId) -> bool {
        !self.not_ready.contains(&(v.0, a.0))
    }
    fn group_has_unknown(&self, a: AccountId, g: GroupId) -> bool {
        self.unknown.contains(&(a.0, g.0))
    }
    fn equity(&self, a: AccountId) -> EquityView {
        *self.equity.get(&a.0).unwrap_or(&EquityView {
            total: n("200000"),
            valid: true,
        })
    }
    fn bbo(&self, i: InstId) -> BboView {
        let mut b = self.bbo.get(&i.0).copied().unwrap_or(BboView::EMPTY);
        let s = *self.silence.get(&i.0).unwrap_or(&0);
        b.conn_live_tsc = Tsc(self.tsc.saturating_sub(s).max(1));
        b
    }
    fn avail_quote(&self, a: AccountId, v: VenueId, _quote: AssetId) -> Option<Notional> {
        match self.avail_quote.get(&(a.0, v.0)) {
            Some(x) => x.map(Notional),
            None => Some(n("100000")),
        }
    }
    fn avail_margin(&self, a: AccountId, v: VenueId) -> Option<Notional> {
        match self.avail_margin.get(&(a.0, v.0)) {
            Some(x) => x.map(Notional),
            None => Some(n("100000")),
        }
    }
}

/// `ModeView` mock: overrides, otherwise the values synced from the `Risk` before each apply.
#[derive(Clone, Debug, Default)]
pub struct Mm {
    pub not_flat: HashSet<CellKey>,
    pub naked: Option<bool>,
    pub naked_cell: HashMap<(u8, u32), bool>,
    pub recon: Option<bool>,
    pub daily: Option<bool>,
    pub unknown: HashMap<(u8, u8), u32>,
    pub pending: bool,
    pub lev_bad_override: HashMap<(u8, u8), bool>,
    // synced from the Risk
    pub s_lev: HashMap<(u8, u8), bool>,
    pub s_naked: HashMap<(u8, u32), bool>,
    pub s_recon: HashMap<(u8, u8), bool>,
    pub s_daily: HashMap<u8, bool>,
}

impl Mm {
    pub fn r0() -> Mm {
        Mm {
            naked: Some(true),
            recon: Some(true),
            daily: Some(true),
            ..Mm::default()
        }
    }
    pub fn sync(&mut self, risk: &Risk) {
        for a in 1..=4u8 {
            for v in [A, B, C] {
                self.s_lev.insert((a, v.0), risk.lev_bad(acc(a), v));
                self.s_recon.insert((a, v.0), risk.recon_resumable(acc(a), v));
            }
            for g in [G1, G2, G3, GS, GroupId(0x0100_0003)] {
                self.s_naked.insert((a, g.0), risk.naked_cleared(acc(a), g));
            }
            self.s_daily.insert(a, risk.daily_loss_resumable(acc(a)));
        }
    }
}

impl ModeView for Mm {
    fn cell_flat(&self, c: CellKey) -> bool {
        !self.not_flat.contains(&c)
    }
    fn naked_cleared(&self, a: AccountId, g: GroupId) -> bool {
        if let Some(x) = self.naked_cell.get(&(a.0, g.0)) {
            return *x;
        }
        self.naked
            .unwrap_or_else(|| *self.s_naked.get(&(a.0, g.0)).unwrap_or(&false))
    }
    fn recon_resumable(&self, a: AccountId, v: VenueId) -> bool {
        self.recon
            .unwrap_or_else(|| *self.s_recon.get(&(a.0, v.0)).unwrap_or(&false))
    }
    fn daily_loss_resumable(&self, a: AccountId) -> bool {
        self.daily
            .unwrap_or_else(|| *self.s_daily.get(&a.0).unwrap_or(&false))
    }
    fn unknown_count(&self, a: AccountId, v: VenueId) -> u32 {
        *self.unknown.get(&(a.0, v.0)).unwrap_or(&0)
    }
    fn lev_bad(&self, a: AccountId, v: VenueId) -> bool {
        if let Some(x) = self.lev_bad_override.get(&(a.0, v.0)) {
            return *x;
        }
        *self.s_lev.get(&(a.0, v.0)).unwrap_or(&false)
    }
    fn engine_reduce_pending(&self) -> bool {
        self.pending
    }
}

// ---------------------------------------------------------------------------------------- scopes & inputs

pub fn sc_any() -> CmdScope {
    hft_risk::scope_any()
}
pub fn sc_acct(a: u8) -> CmdScope {
    hft_risk::account_scope(acc(a))
}
pub fn sc_av(a: u8, v: VenueId) -> CmdScope {
    hft_risk::account_venue_scope(acc(a), v)
}
pub fn sc_ag(a: u8, g: GroupId) -> CmdScope {
    hft_risk::account_group_scope(acc(a), g)
}
pub fn sc_cell(s: u8, a: u8, g: GroupId) -> CmdScope {
    hft_risk::cell_scope(sid(s), acc(a), g)
}
pub fn sc_vi(v: VenueId, i: InstId) -> CmdScope {
    hft_risk::venue_inst_scope(v, i)
}

pub fn kill(
    level: KillLevel,
    scope: CmdScope,
    cancel_resting: bool,
    flatten: bool,
    hard: bool,
) -> ModeInput<'static> {
    ModeInput::Kill(KillCmd {
        level,
        scope,
        cancel_resting,
        flatten,
        hard,
    })
}
pub fn kill_global(hard: bool) -> ModeInput<'static> {
    kill(KillLevel::Global, sc_any(), true, false, hard)
}
pub fn kill_acct(a: u8, hard: bool) -> ModeInput<'static> {
    kill(KillLevel::Account, sc_acct(a), true, false, hard)
}
pub fn kill_flatten(a: u8) -> ModeInput<'static> {
    kill(KillLevel::Account, sc_acct(a), true, true, false)
}
/// `Kill{Venue(v) × a}` (a = 0: every account).
pub fn kill_venue(v: VenueId, a: u8) -> ModeInput<'static> {
    let mut s = sc_any();
    s.venue = v;
    s.account = acc(a);
    kill(KillLevel::Venue, s, true, false, false)
}
pub fn resume(source: CapSource, scope: CmdScope) -> ModeInput<'static> {
    ModeInput::Resume { source, scope }
}
pub fn set_mode(s: u8, a: u8, g: GroupId, mode: RunMode) -> ModeInput<'static> {
    ModeInput::SetMode {
        scope: sc_cell(s, a, g),
        mode,
    }
}
pub fn rec(source: CapSource, level: u8, cflags: u8, scope: CmdScope, p0: u64) -> hft_risk::mode::CapRecord {
    hft_risk::mode::CapRecord {
        source,
        level,
        cflags,
        _pad: [0; 5],
        scope,
        set_seq: 0,
        params: [p0, 0],
    }
}
pub fn auto(source: CapSource, scope: CmdScope) -> ModeInput<'static> {
    let cf = match source {
        CapSource::CLOCK_BAD | CapSource::MARGIN_STRESS | CapSource::STORAGE | CapSource::ACCOUNT_CFG => 2,
        CapSource::DRAINING => 4,
        _ => 0,
    };
    let level = if source == CapSource::MARGIN_STRESS { 1 } else { 0 };
    ModeInput::Auto(rec(source, level, cf, scope, 0))
}
pub fn broken(a: u8, g: GroupId) -> ModeInput<'static> {
    auto(CapSource::BROKEN, sc_ag(a, g))
}
pub fn margin_auto(a: u8, level: u8, bitmap: u64) -> ModeInput<'static> {
    ModeInput::Auto(rec(CapSource::MARGIN_STRESS, level, 2, sc_acct(a), bitmap))
}
pub fn halted(s: u8, a: u8, g: GroupId) -> ModeInput<'static> {
    auto(CapSource::HALTED, sc_cell(s, a, g))
}
pub fn daily(a: u8) -> ModeInput<'static> {
    ModeInput::Auto(rec(
        CapSource::DAILY_LOSS,
        0,
        0,
        sc_acct(a),
        (-5_000_000_000i64) as u64,
    ))
}
pub fn draining() -> ModeInput<'static> {
    ModeInput::Auto(rec(CapSource::DRAINING, 0, 4, sc_any(), 1))
}

// ---------------------------------------------------------------------------------------- world

/// R0 world.
pub struct W {
    pub rd: &'static RefData,
    pub p: RiskParams,
    pub risk: Risk,
    pub mv: Mv,
    pub mm: Mm,
    pub out: ModeOutcome,
    pub mo: MarginOut,
    pub seq: u64,
    pub now: i64,
    pub v0: u64,
}

pub fn hdr(a: u8, v: VenueId, mismatches: u32, uid_keys: u8) -> AccountConfigReportHdr {
    let perp = v.market() == Some(MarketKind::LinearPerp);
    let (pm, mm, ma) = if perp { (2, 2, 2) } else { (1, 1, 1) };
    let mut h = <AccountConfigReportHdr as zerocopy::FromZeros>::new_zeroed();
    h.venue = v;
    h.account = acc(a);
    h.position_mode = pm;
    h.margin_mode = mm;
    h.multi_asset = ma;
    h.source = 1;
    h.mismatches = mismatches;
    h.uid_keys = uid_keys;
    h
}

pub fn lev(i: InstId, x: &str) -> LeverageItem {
    LeverageItem {
        inst: i,
        _pad: 0,
        leverage: r(x),
    }
}

pub const KEYS: u8 = 3;

/// One reconciliation report entry.
pub fn rep(a: u8, v: VenueId, report_id: u64, view_seq: u64, item: u8) -> ReconcileReport {
    let mut x = <ReconcileReport as zerocopy::FromZeros>::new_zeroed();
    x.report_id = report_id;
    x.venue = v;
    x.account = acc(a);
    x.item = item;
    x.view_seq = view_seq;
    x
}
pub fn summary(
    a: u8,
    v: VenueId,
    report_id: u64,
    view_seq: u64,
    sflags: u16,
    skipped: u16,
) -> ReconcileReport {
    let mut x = rep(a, v, report_id, view_seq, 5);
    x.sflags = sflags;
    x.skipped_inflight = skipped;
    x
}
/// Position item with `exch − engine = diff` and `|diff| notional = dn`.
pub fn pos_item(a: u8, v: VenueId, report_id: u64, i: InstId, diff: &str, dn: &str) -> ReconcileReport {
    let mut x = rep(a, v, report_id, 0, 1);
    x.inst = i;
    x.exch_qty = q(diff);
    x.engine_qty = Qty(0);
    x.diff = n(dn);
    x
}
pub fn bal_item(a: u8, v: VenueId, report_id: u64, asset: AssetId, diff: &str, dn: &str) -> ReconcileReport {
    let mut x = rep(a, v, report_id, 0, 2);
    x.asset = asset;
    x.exch_qty = q(diff);
    x.engine_qty = Qty(0);
    x.diff = n(dn);
    x
}

/// Steps of the release sequence (G0).
#[derive(Clone, Debug)]
pub enum Step {
    Feeds(bool),
    Conns(VenueId, bool, bool),
    Unknown(u32),
    Recon(VenueId, Vec<ReconcileReport>),
    Clock(bool),
    Cfg(AccountConfigReportHdr, Vec<LeverageItem>, u8),
    SelfTest,
}

impl W {
    pub fn base(rd: &'static RefData, p: RiskParams, mc: ModeCfg, caps: Capacities) -> W {
        let n = account_count(rd);
        let risk = Risk::new(&p, rd, &gws_for(rd), &caps, &exec_bounds(n), mc).unwrap();
        let out = ModeOutcome::with_capacity(caps.max_cells_total as usize, n, n * 3);
        W {
            rd,
            p,
            v0: risk.mode.ctl_state_version(),
            risk,
            mv: Mv::r0(),
            mm: Mm::r0(),
            out,
            mo: MarginOut::default(),
            seq: 1,
            now: T0,
        }
    }

    pub fn nowv(&self) -> Now {
        now_at(self.now)
    }

    pub fn apply(&mut self, inp: ModeInput<'_>) -> Result<(), ModeErr> {
        let s = self.seq;
        self.apply_seq(inp, s)
    }

    pub fn apply_seq(&mut self, inp: ModeInput<'_>, seq: u64) -> Result<(), ModeErr> {
        self.mm.sync(&self.risk);
        let r = self.risk.mode.apply(inp, seq, &self.mm, &mut self.out);
        self.seq = seq + 1;
        r
    }

    pub fn ok(&mut self, inp: ModeInput<'_>) {
        let r = self.apply(inp);
        assert_eq!(r, Ok(()), "apply rejected");
    }

    /// Release-sequence steps of a Live account (venues of `rd`), or of a Sim account.
    pub fn release_steps(&self, a: u8) -> Vec<Step> {
        let m = self.rd.account(acc(a));
        let venues: Vec<VenueId> = (0u8..64)
            .filter(|v| m.venues & (1u64 << v) != 0)
            .map(VenueId)
            .collect();
        let sim = matches!(m.kind, hft_refdata::AccountKind::Sim(_));
        let mut s = vec![Step::Feeds(true)];
        for v in &venues {
            s.push(Step::Conns(*v, true, true));
        }
        s.push(Step::Unknown(0));
        if !sim {
            for (k, v) in venues.iter().enumerate() {
                let id = 1_000 + u64::from(a) * 10 + k as u64;
                s.push(Step::Recon(
                    *v,
                    vec![summary(a, *v, id, self.seq + 100 + k as u64, 0, 0)],
                ));
            }
        }
        s.push(Step::Clock(true));
        if !sim {
            for v in &venues {
                let levs = self.levs_for(*v);
                s.push(Step::Cfg(hdr(a, *v, 0, KEYS), levs, KEYS));
            }
            s.push(Step::SelfTest);
        }
        s
    }

    pub fn levs_for(&self, v: VenueId) -> Vec<LeverageItem> {
        if v == B {
            vec![lev(B1, "3.00"), lev(B2, "3.00")]
        } else if v == C {
            vec![lev(C1, "3.00")]
        } else {
            Vec::new()
        }
    }

    /// Runs one step for account `a`.
    pub fn step(&mut self, a: u8, s: &Step) -> Result<(), ModeErr> {
        match s {
            Step::Feeds(ok) => self.apply(ModeInput::Gate(GateEvent::Feeds {
                account: acc(a),
                ok: *ok,
            })),
            Step::Conns(v, p, sb) => self.apply(ModeInput::Gate(GateEvent::OrderConns {
                account: acc(a),
                venue: *v,
                primary_up: *p,
                standby_up: *sb,
            })),
            Step::Unknown(c) => self.apply(ModeInput::Gate(GateEvent::Unknown {
                account: acc(a),
                count: *c,
            })),
            Step::Recon(_, items) => {
                let v = self.risk.on_reconcile(&ReconcileGroup { items });
                match v.gate {
                    Some(g) => self.apply(ModeInput::Gate(GateEvent::Recon(&g))),
                    None => Ok(()),
                }
            }
            Step::Clock(ok) => self.apply(ModeInput::Gate(GateEvent::Clock { ok: *ok })),
            Step::Cfg(h, levs, keys) => {
                self.risk.on_account_cfg(h, levs);
                self.apply(ModeInput::Gate(GateEvent::AccountCfg {
                    hdr: h,
                    keys_required: *keys,
                }))
            }
            Step::SelfTest => self.apply(ModeInput::Gate(GateEvent::KillSelfTest { account: acc(a) })),
        }
    }

    pub fn run(&mut self, a: u8, steps: &[Step]) {
        for s in steps {
            self.step(a, s).unwrap();
        }
    }

    /// Release sequence of account `a`.
    pub fn release(&mut self, a: u8) {
        let s = self.release_steps(a);
        self.run(a, &s);
    }

    /// The group specs of an account in R0.
    pub fn specs(&self, a: u8) -> Vec<GroupSpec> {
        match a {
            1 => vec![
                gspec(G1, BTC, &[(A1, A), (B1, B)]),
                gspec(G3, ETH, &[(A2, A), (B2, B)]),
            ],
            2 => vec![gspec(G2, BTC, &[(A1, A), (B1, B)])],
            3 => vec![gspec(GS, BTC, &[(A1, A), (B1, B)])],
            _ => Vec::new(),
        }
    }

    pub fn cells(&self) -> Vec<CellKey> {
        vec![cell(1, 1, G1), cell(1, 1, G3), cell(2, 2, G2), cell(3, 3, GS)]
            .into_iter()
            .filter(|c| usize::from(c.account.0) <= account_count(self.rd))
            .collect()
    }

    /// Margin sample of R0 (fresh report, no perp position).
    pub fn r0_margin(&mut self) {
        for (a, v) in [(1u8, B), (1, C), (2, B)] {
            if usize::from(a) > account_count(self.rd) {
                continue;
            }
            let s = MarginSample {
                rep_liq_dist: Rate::MAX,
                est_liq_dist: Rate::MAX,
                reported_ratio: r("0.10"),
                est_ratio: r("0.10"),
                report_ens: T0,
                mark_ens: i64::MAX,
                has_report: true,
                truncated: false,
            };
            let now = now_at(T0);
            self.risk.on_margin(acc(a), v, &s, &now, &mut self.mo);
        }
        self.mo.clear();
    }
}

pub fn account_count(rd: &RefData) -> usize {
    (1..=255u8)
        .take_while(|a| rd.account(AccountId(*a)).id == AccountId(*a))
        .count()
}

pub fn gws_for(rd: &RefData) -> GwMap {
    let mut v: Vec<VenueId> = rd.venues_in_order().to_vec();
    v.sort();
    GwMap { venues: v }
}

pub fn gspec(id: GroupId, asset: AssetId, legs: &[(InstId, VenueId)]) -> GroupSpec {
    let mut g = <GroupSpec as zerocopy::FromZeros>::new_zeroed();
    g.id = id;
    g.asset = asset;
    g.n_legs = legs.len() as u8;
    g.open_allowed = 1;
    for (k, (i, v)) in legs.iter().enumerate() {
        g.legs[k] = LegDef {
            inst: *i,
            venue: *v,
            kind: LegDef::CEX,
            _pad: 0,
            route: 0,
            _pad2: 0,
        };
    }
    g
}

/// Builds a world over `rd`/`p`: groups, releases, Live modes, R0 margin; version recorded.
pub fn world(rd: &'static RefData, p: RiskParams, mc: ModeCfg, caps: Capacities) -> W {
    let mut w = W::base(rd, p, mc, caps);
    let n = account_count(rd) as u8;
    for a in 1..=n {
        let specs = w.specs(a);
        w.risk.on_group_specs(acc(a), &specs);
    }
    let cells = w.cells();
    w.ok(ModeInput::GroupsChanged {
        added: &cells,
        removed: &[],
    });
    for a in 1..=n {
        w.release(a);
    }
    for a in 1..=n {
        let b = rd.account(acc(a)).bound_instance;
        w.ok(ModeInput::SetMode {
            scope: sc_cell(b.0, a, GROUP_ALL),
            mode: RunMode::Live,
        });
    }
    w.r0_margin();
    w.out.clear();
    w.v0 = w.risk.mode.ctl_state_version();
    w
}

/// R0.
pub fn r0() -> W {
    let rd = r0_refdata();
    let p = p0_for(rd, 3);
    world(rd, p, MC, CAPS)
}

/// R0 with modified params.
pub fn r0_with(p: impl FnOnce(RiskParams) -> RiskParams) -> W {
    let rd = r0_refdata();
    let p = p(p0_for(rd, 3));
    world(rd, p, MC, CAPS)
}

/// G0: R0, `Auto(Recovery{1})`, `RecoveryFlags{1, flags}`.
pub fn g0_flags(flags: u8, mc: ModeCfg) -> W {
    let rd = r0_refdata();
    let mut w = world(rd, p0_for(rd, 3), mc, CAPS);
    w.ok(auto(CapSource::RECOVERY, sc_acct(1)));
    w.ok(ModeInput::RecoveryFlags {
        account: acc(1),
        flags: RecoveryFlags(flags),
    });
    w.out.clear();
    w
}

pub fn g0() -> W {
    g0_flags(0, MC)
}

// ---------------------------------------------------------------------------------------- intents

pub fn base_intent(a: u8, g: GroupId, i: InstId, side: Side, qty: &str, p: &str) -> IntentView {
    let v = match i.0 {
        1 | 2 => A,
        3 | 4 => B,
        _ => C,
    };
    let qq = q(qty);
    let pp = px(p);
    IntentView {
        venue: v,
        account: acc(a),
        side,
        class: IntentClass::Open,
        inst: i,
        group: g,
        strategy: g.strategy(),
        origin: Origin::Strategy,
        link_role: None,
        tif: Tif::Gtc,
        flags: 0,
        is_swap: false,
        px: pp,
        qty: qq,
        notional: notional(qq, pp),
        link_venue: VenueId(0),
        link_account: AccountId(0),
        link_side: Side::Buy,
        link_inst: InstId(0),
        link_qty: Qty(0),
        link_notional: Notional(0),
        hedge_cap_qty: Qty(0),
    }
}

/// `Open(a, g, inst, side, qty @ px, tif)`.
pub fn open(a: u8, g: GroupId, i: InstId, side: Side, qty: &str, p: &str, tif: Tif) -> IntentView {
    let mut it = base_intent(a, g, i, side, qty, p);
    it.tif = tif;
    it
}

/// `Hedge(…)`: OMS second leg, risk-reducing, `hedge_cap_qty = cap`.
pub fn hedge(a: u8, g: GroupId, i: InstId, side: Side, qty: &str, p: &str, cap: &str) -> IntentView {
    let mut it = base_intent(a, g, i, side, qty, p);
    it.class = IntentClass::Hedge;
    it.origin = Origin::Oms;
    it.link_role = Some(LinkRole::Second);
    it.tif = Tif::Ioc;
    it.flags = OrderFlags::RISK_REDUCING.0 | OrderFlags::LINKED.0;
    it.hedge_cap_qty = q(cap);
    it
}

/// Exec HedgeFix (risk-reducing; reduce-only on perps).
pub fn hedge_fix(a: u8, g: GroupId, i: InstId, side: Side, qty: &str, p: &str, cap: &str) -> IntentView {
    let mut it = hedge(a, g, i, side, qty, p, cap);
    it.origin = Origin::Exec;
    it.link_role = Some(LinkRole::HedgeFix);
    it.flags = OrderFlags::RISK_REDUCING.0 | OrderFlags::HEDGE_FIX.0;
    if it.venue != A {
        it.flags |= OrderFlags::REDUCE_ONLY.0;
    }
    it
}

/// Applies a classification to an intent (class and flags).
pub fn classified(mut it: IntentView, c: Classification) -> IntentView {
    it.class = c.class;
    it.flags &= !(OrderFlags::RISK_REDUCING.0 | OrderFlags::REDUCE_ONLY.0);
    if c.risk_reducing {
        it.flags |= OrderFlags::RISK_REDUCING.0;
    }
    if c.reduce_only {
        it.flags |= OrderFlags::REDUCE_ONLY.0;
    }
    it
}

impl W {
    fn caps_of(&self, i: InstId) -> &hft_refdata::VenueCaps {
        self.rd.caps(self.rd.inst(i).venue)
    }

    /// Strategy `Reduce` classified against the mock positions.
    pub fn reduce(&self, a: u8, g: GroupId, i: InstId, side: Side, qty: &str, p: &str) -> IntentView {
        let it = base_intent(a, g, i, side, qty, p);
        let d = draft(&it, IntentClass::Reduce);
        let c = Risk::classify(&d, Origin::Strategy, &self.mv.pos_facts(a, i), self.caps_of(i));
        classified(it, c)
    }

    /// `Flatten` (`origin` Exec or Strategy).
    pub fn flatten(
        &self,
        a: u8,
        g: GroupId,
        i: InstId,
        side: Side,
        qty: &str,
        p: &str,
        origin: Origin,
    ) -> IntentView {
        let mut it = base_intent(a, g, i, side, qty, p);
        it.origin = origin;
        it.tif = Tif::Ioc;
        let d = draft(&it, IntentClass::Flatten);
        let c = Risk::classify(&d, origin, &self.mv.pos_facts(a, i), self.caps_of(i));
        classified(it, c)
    }

    pub fn eval(&mut self, it: &IntentView) -> Result<Admission, RejectBits> {
        self.mv.tsc = self.now as u64;
        let now = self.nowv();
        self.risk.evaluate(it, &self.mv, &now)
    }

    pub fn eval_commit(&mut self, it: &IntentView) -> Result<Admission, RejectBits> {
        let r = self.eval(it);
        if let Ok(a) = &r {
            let now = self.nowv();
            self.risk.commit(a, &now);
        }
        r
    }

    /// `k` evaluate + commit of `it` at the current time (all must pass).
    pub fn commits(&mut self, it: &IntentView, k: usize) {
        for i in 0..k {
            let r = self.eval_commit(it);
            assert!(r.is_ok(), "commit {i} rejected: {r:?}");
        }
    }

    pub fn margin(&mut self, a: u8, v: VenueId, s: &MarginSample, at: i64) {
        let now = now_at(at);
        self.risk.on_margin(acc(a), v, s, &now, &mut self.mo);
    }

    pub fn snapshot(&self) -> Vec<u8> {
        let mut w = hft_strategy_sdk::SnapshotWriter::new(1 << 22);
        self.risk.snapshot(&mut w);
        assert!(w.status().is_ok());
        w.as_bytes().to_vec()
    }

    pub fn restore_from(&mut self, bytes: &[u8]) {
        let mut r = hft_strategy_sdk::SnapshotReader::new(bytes);
        let n = account_count(self.rd);
        self.risk = Risk::restore(
            &mut r,
            &self.p,
            self.rd,
            &gws_for(self.rd),
            &CAPS,
            &exec_bounds(n),
            MC,
        )
        .unwrap();
        let n8 = n as u8;
        for a in 1..=n8 {
            let specs = self.specs(a);
            self.risk.on_group_specs(acc(a), &specs);
        }
    }
}

pub fn draft(it: &IntentView, class: IntentClass) -> IntentDraft {
    IntentDraft {
        account: it.account,
        inst: it.inst,
        side: it.side,
        qty: it.qty,
        tif: it.tif,
        class,
        link_role: it.link_role,
        simultaneous: false,
        hedge_reduce_only: false,
        flags: it.flags,
    }
}

/// `LinkOpen(…; link = (la, lv, li, lside, lqty, lnotional))`.
pub fn link_open(
    a: u8,
    g: GroupId,
    i: InstId,
    side: Side,
    qty: &str,
    p: &str,
    link: (u8, VenueId, InstId, Side, &str, &str),
) -> IntentView {
    let mut it = base_intent(a, g, i, side, qty, p);
    it.link_role = Some(LinkRole::First);
    it.flags = OrderFlags::LINKED.0;
    it.link_account = acc(link.0);
    it.link_venue = link.1;
    it.link_inst = link.2;
    it.link_side = link.3;
    it.link_qty = q(link.4);
    it.link_notional = n(link.5);
    it
}

/// Smoke order (account 1, a1 Buy 0.0002 @ 47 500.00, PostOnly, strategy 1, GROUP_NONE).
pub fn smoke(a: u8, strategy: u8) -> IntentView {
    let mut it = base_intent(a, GROUP_NONE, A1, Side::Buy, "0.0002", "47500.00");
    it.origin = Origin::Command;
    it.flags = OrderFlags::SMOKE.0 | OrderFlags::POST_ONLY.0;
    it.tif = Tif::PostOnly;
    it.strategy = sid(strategy);
    it
}

pub fn err(b: RejectBits) -> Result<(), RejectBits> {
    Err(b)
}
pub fn res(r: Result<Admission, RejectBits>) -> Result<(), RejectBits> {
    r.map(|_| ())
}

/// The effective mode of a cell.
pub fn eff(w: &W, c: CellKey) -> Effective {
    w.risk.mode.effective(c)
}

pub const LIVE: Effective = Effective::Live;
pub const CLOSE: Effective = Effective::CloseOnly {
    deferred_shadow: false,
};

/// R0 margin sample with the given reported/estimated liquidation distance, at `report_ens`.
pub fn msample(liq: &str, ratio: &str, report_ens: i64) -> MarginSample {
    MarginSample {
        rep_liq_dist: r(liq),
        est_liq_dist: r(liq),
        reported_ratio: r(ratio),
        est_ratio: r(ratio),
        report_ens,
        mark_ens: i64::MAX,
        has_report: true,
        truncated: false,
    }
}
