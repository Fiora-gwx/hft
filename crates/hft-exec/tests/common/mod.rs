//! Shared fixture of the hft-exec tests (TESTS.md "基准夹具"): refdata (venues A spot, B perp,
//! C perp, D spot; insts S, S2, P, P2, P3, P4, Q, …), accounts a1/a2 (Live) and s1 (Sim),
//! instances i1/i2/i3, parameter set C0, target specs T0/T1, risk view R0, the 1 GHz clock and a
//! `Views` wrapper over the SDK `MockViews` with book-status overrides.
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use hft_event::{ExecDecision, GroupSpec, LegDef, Level, Note, NoteLevel};
use hft_exec::*;
use hft_refdata::{
    AccountSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LiqMarker, MarginData, MarginMode,
    PositionMode, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit,
    UidPolicy, VenueCaps, VenueSpec,
};
use hft_strategy_sdk::testing::{MockBook, MockPortfolio, MockViews, RefFixture};
use hft_strategy_sdk::{
    BboView, BookRead, BookStatus, ExecSpec, ExecUpdate, ExecUpdateKind, FeeView, FirstLeg, FundingView,
    GroupPosView, GroupTable, InstRiskView, InventoryView, LegPos, LegTarget, LegView, MarginLevel, Now,
    OrderRef, OrderUpdate, OrderUpdateKind, OrderView, PairParams, PerpPosView, PxMode, RefView,
    ResidualPolicy, RiskStateView, TakerSliceParams, TargetIntent, TargetReason, TargetView, Urgency, Views,
};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, RejectBits, RunMode,
    Side, StrategyId, Tsc, VenueId,
};

// ------------------------------------------------------------------------------------ ids

/// Venue A (spot), B (perp), C (perp), D (spot); real ids so refdata accepts their products.
pub const A: VenueId = VenueId::BINANCE_SPOT;
pub const B: VenueId = VenueId::OKX_SWAP;
pub const C: VenueId = VenueId::BITGET_UTA_USDT_FUT;
pub const D: VenueId = VenueId::GATE_SPOT;
pub const S: InstId = InstId(11);
pub const S2: InstId = InstId(12);
pub const P: InstId = InstId(21);
pub const P2: InstId = InstId(23);
pub const P3: InstId = InstId(24);
pub const P4: InstId = InstId(25);
/// Extra perps on B (ungrouped scans).
pub const PX: [InstId; 4] = [InstId(26), InstId(27), InstId(28), InstId(29)];
pub const Q: InstId = InstId(31);
/// Lot-test insts (spot on A, perp on B); lots set per test.
pub const SL: InstId = InstId(13);
pub const PL: InstId = InstId(22);
pub const BASE: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
pub const USDC: AssetId = AssetId(3);
pub const FILL: AssetId = AssetId(4);
pub const A1: AccountId = AccountId(1);
pub const A2: AccountId = AccountId(2);
pub const S1: AccountId = AccountId(3);
pub const I1: StrategyId = StrategyId(1);
pub const I2: StrategyId = StrategyId(2);
pub const I3: StrategyId = StrategyId(3);
pub const G7: GroupId = GroupId(0x0100_0007);
pub const G8: GroupId = GroupId(0x0100_0008);
pub const G6: GroupId = GroupId(0x0100_0006);
pub const G9: GroupId = GroupId(0x0200_0009);
pub const G10: GroupId = GroupId(0x0100_000A);
pub const G11: GroupId = GroupId(0x0100_000B);
pub const G12: GroupId = GroupId(0x0100_000C);
pub const G13: GroupId = GroupId(0x0100_000D);
pub const G14: GroupId = GroupId(0x0100_000E);
pub const G15: GroupId = GroupId(0x0100_000F);
pub const G16: GroupId = GroupId(0x0100_0010);
pub const G17: GroupId = GroupId(0x0100_0011);
pub const G20: GroupId = GroupId(0x0300_0014);
pub const G21: GroupId = GroupId(0x0300_0015);
pub const G22: GroupId = GroupId(0x0300_0016);
pub const GNONE: GroupId = GroupId(0);

// ------------------------------------------------------------------------------------ numbers

/// Decimal → raw at exponent −`e`.
pub fn dec(s: &str, e: u32) -> i64 {
    let neg = s.starts_with('-');
    let s = s.trim_start_matches(['-', '+']);
    let (i, f) = s.split_once('.').unwrap_or((s, ""));
    let mut v: i128 = i.replace([' ', '_'], "").parse::<i128>().unwrap_or(0);
    let mut fd = f.replace([' ', '_'], "");
    assert!(fd.len() <= e as usize, "too many decimals in {s}");
    while fd.len() < e as usize {
        fd.push('0');
    }
    v = v * 10i128.pow(e)
        + if fd.is_empty() {
            0
        } else {
            fd.parse::<i128>().unwrap()
        };
    (if neg { -v } else { v }) as i64
}
/// Quantity (exponent −8).
pub fn q(s: &str) -> Qty {
    Qty(dec(s, 8))
}
/// Price (exponent −8).
pub fn px(s: &str) -> Px {
    Px(dec(s, 8))
}
/// USDT notional (micro).
pub fn usd(s: &str) -> Notional {
    Notional(dec(s, 6))
}
/// Rate (1e-9).
pub fn rate(s: &str) -> Rate {
    Rate(dec(s, 9))
}
/// TSC of the test's `t 0` (non-zero: 0 means "unset" in many fields).
pub const T0: u64 = 1_000_000_000_000;

/// `t` ms on the 1 GHz clock.
pub fn t(ms: u64) -> Tsc {
    Tsc(T0 + ms * 1_000_000)
}

// ------------------------------------------------------------------------------------ refdata

fn venue(b: &mut RdbBuilder, id: VenueId, exchange: Exchange, product: InstKind) {
    b.venue(VenueSpec {
        id,
        exchange,
        product,
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
    let perp = product == InstKind::Perp;
    b.caps(VenueCaps {
        venue: id,
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
        required_multi_asset: None,
        required_spot_borrow: None,
        funding_settle: if perp {
            FundingSettleKind::PerSymbolPush
        } else {
            FundingSettleKind::NotApplicable
        },
        margin_data: if perp {
            MarginData::Poll { interval_ms: 2000 }
        } else {
            MarginData::NotApplicable
        },
        own_liquidation_marker: LiqMarker::NotIdentifiable,
        liq_marker_arg: "".into(),
        ignorable_user_events: Vec::new().into(),
        flags: CapFlags::POST_ONLY
            | CapFlags::IOC
            | if perp {
                CapFlags::POSITION_CLOSE
            } else {
                CapFlags(0)
            },
        amend_qty_down_only: false,
        dms: DeadManCap::None,
        dms_min_ms: 0,
        dms_max_ms: 0,
        dms_timeout_ms: 0,
        dms_refresh_ms: 0,
        testnet: TestnetKind::Testnet,
        _pad: 0,
    });
    b.fee(FeeSpec {
        venue: id,
        tier: 0,
        fee: FeePair {
            maker: Rate(200_000),
            taker: Rate(500_000),
        },
    });
}

/// Inst parameters (lot, min_qty as decimals; min_notional USDT).
#[derive(Clone, Copy, Debug)]
pub struct InstP {
    pub id: InstId,
    pub venue: VenueId,
    pub perp: bool,
    pub asset: AssetId,
    pub quote: AssetId,
    pub lot: i64,
    pub min_qty: i64,
    pub min_notional: i64,
    pub tick: i64,
}

pub fn inst_p(id: InstId, venue: VenueId, perp: bool, lot: &str) -> InstP {
    InstP {
        id,
        venue,
        perp,
        asset: BASE,
        quote: USDT,
        lot: dec(lot, 8),
        min_qty: dec(lot, 8),
        min_notional: 5_000_000,
        tick: 1_000_000,
    }
}

/// The default inst table.
pub fn default_insts() -> Vec<InstP> {
    let mut v = vec![
        inst_p(S, A, false, "0.01"),
        inst_p(S2, D, false, "0.01"),
        inst_p(P, B, true, "0.1"),
        InstP {
            quote: USDC,
            ..inst_p(P2, B, true, "0.1")
        },
        inst_p(P3, B, true, "0.1"),
        inst_p(P4, B, true, "0.1"),
        inst_p(Q, C, true, "0.1"),
    ];
    for i in PX {
        v.push(inst_p(i, B, true, "0.1"));
    }
    v.push(inst_p(SL, A, false, "0.003"));
    v.push(inst_p(PL, B, true, "0.002"));
    v
}

/// Raw at −8 → decimal string without trailing zeros.
pub fn fmt8(x: i64) -> String {
    let s = format!("{}.{:08}", x / 100_000_000, x % 100_000_000);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn put_inst(b: &mut RdbBuilder, p: &InstP) {
    b.inst(InstSpec {
        id: p.id,
        venue: p.venue,
        symbol: format!("I{}", p.id.0),
        status: InstStatus::Trading,
        kind: if p.perp { InstKind::Perp } else { InstKind::Spot },
        asset: p.asset,
        quote: p.quote,
        tick: p.tick,
        lot: p.lot,
        min_qty: Qty(p.min_qty),
        max_qty: Qty(1_000_000_000_000_000),
        min_notional: Notional(p.min_notional),
        book_window: 64,
        l2_depth: 20,
        crossed_tolerance: 0,
        tick_str: fmt8(p.tick),
        lot_str: fmt8(p.lot),
        min_qty_str: fmt8(p.min_qty),
        funding_interval_s: if p.perp { 28_800 } else { 0 },
        max_leverage: if p.perp { 20 } else { 0 },
        ..InstSpec::default()
    });
}

/// Builds the refdata with `insts` (the rest of the dense ids 1..=max(40, largest id) are filler spot
/// insts on A).
pub fn refdata_with(insts: &[InstP]) -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class, pe, qe) in [
        (BASE, "BTC", AssetClass::Crypto, -8i8, -8i8),
        (USDT, "USDT", AssetClass::Stable, -6, -6),
        (USDC, "USDC", AssetClass::Stable, -6, -6),
        (FILL, "FILL", AssetClass::Crypto, -8, -8),
    ] {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: pe,
                qty_exp: qe,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    venue(&mut b, A, Exchange::Binance, InstKind::Spot);
    venue(&mut b, D, Exchange::Gate, InstKind::Spot);
    venue(&mut b, B, Exchange::Okx, InstKind::Perp);
    venue(&mut b, C, Exchange::Bitget, InstKind::Perp);
    let max_id = insts.iter().map(|p| p.id.0).max().unwrap_or(0).max(40);
    for id in 1..=max_id {
        match insts.iter().find(|p| p.id.0 == id) {
            Some(p) => put_inst(&mut b, p),
            None => put_inst(
                &mut b,
                &InstP {
                    asset: FILL,
                    ..inst_p(InstId(id), A, false, "0.01")
                },
            ),
        }
    }
    for (id, kind_id) in [(1u8, 0xFF01u16), (2, 0xFF02), (3, 0xFF03)] {
        b.instance(InstanceSpec {
            id: StrategyId(id),
            kind_id,
            name: format!("i{id}").into(),
        });
    }
    let all = (1u64 << A.0) | (1u64 << B.0) | (1u64 << C.0) | (1u64 << D.0);
    for (id, sim, inst) in [(A1, false, I1), (A2, false, I2), (S1, true, I3)] {
        b.account(AccountSpec {
            id,
            sim,
            bound_instance: inst,
            is_prod: false,
            venues: all,
            label: format!("acct{}", id.0),
            uid_policy: UidPolicy::Verified,
        });
        if !sim {
            for v in [A, D, B, C] {
                b.account_venue(hft_refdata::AccountVenueSpec {
                    account: id,
                    venue: v,
                    uid: format!("u{}-{}", id.0, v.0),
                });
            }
        }
        if sim {
            b.sim(SimSpec {
                account: id,
                fee_tier: 0,
                latency_model: 0,
                margin_model: 0,
            });
            b.sim_balance(SimBalanceSpec {
                account: id,
                venue: A,
                asset: USDT,
                qty: Qty(1_000_000_000_000),
            });
        }
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => r,
        Err(e) => panic!("test refdata rejected: {e:?}"),
    }
}

// ------------------------------------------------------------------------------------ specs

pub fn leg(i: InstId, v: VenueId) -> LegDef {
    LegDef {
        inst: i,
        venue: v,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }
}

pub fn gspec(id: GroupId, legs: &[(InstId, VenueId)]) -> GroupSpec {
    let mut l = [LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }; 4];
    for (d, (i, v)) in l.iter_mut().zip(legs.iter()) {
        *d = leg(*i, *v);
    }
    GroupSpec {
        id,
        asset: BASE,
        n_legs: legs.len() as u8,
        open_allowed: 1,
        legs: l,
        _rsv: 0,
    }
}

/// g7 = (S, P), g8 = (P).
pub fn g7() -> GroupSpec {
    gspec(G7, &[(S, A), (P, B)])
}
pub fn g8() -> GroupSpec {
    gspec(G8, &[(P, B)])
}

/// C0.
pub fn c0() -> ExecParams {
    ExecParams {
        band_floor_notional: usd("20"),
        dust_notional: usd("5"),
        fix_tolerance_default: rate("0.02"),
        fix_max: 3,
        fix_cooldown_ms: 1000,
        fix_deadline_ms: 10_000,
        broken_auto_fix_ms: 120_000,
        external_deadline_ms: 5000,
        gate_naked_alarm_ms: 300_000,
        stuck_after_ms: 2000,
        gated_backoff_ms: 500,
        max_slice_fails: 3,
        max_nofill: 5,
        urgent_cooldown_ms: 100,
        passive_cooldown_mult: 4,
        force_slice: TakerSliceParams {
            depth_frac: rate("0.3"),
            depth_levels: 5,
            _pad0: [0; 7],
            max_cross: rate("0.005"),
            max_chunk_notional: usd("1000"),
            min_chunk_notional: usd("0"),
            cooldown_ms: 200,
            _pad1: 0,
        },
    }
}

pub fn cfg(p: ExecParams) -> ExecConfig {
    ExecConfig {
        default: p,
        accounts: vec![
            ExecAccount {
                account: A1,
                sim: false,
                params: None,
            },
            ExecAccount {
                account: A2,
                sim: false,
                params: None,
            },
            ExecAccount {
                account: S1,
                sim: true,
                params: None,
            },
        ],
    }
}

pub fn caps(n: u32) -> Capacities {
    Capacities {
        max_cells_total: n,
        max_subs_per_inst: 8,
    }
}

/// T0's slice.
pub fn slice0() -> TakerSliceParams {
    TakerSliceParams {
        depth_frac: rate("0.5"),
        depth_levels: 5,
        _pad0: [0; 7],
        max_cross: rate("0.005"),
        max_chunk_notional: usd("1000"),
        min_chunk_notional: usd("10"),
        cooldown_ms: 300,
        _pad1: 0,
    }
}

pub fn pair0() -> PairParams {
    PairParams {
        slice: slice0(),
        first_leg: FirstLeg::ByLiquidity,
        first_leg_ix: 0,
        hedge_px_mode: PxMode::Cross,
        _pad0: [0; 5],
        hedge_cross: rate("0.003"),
        hedge_max_slip: rate("0.004"),
        ratio_num: 1,
        ratio_den: 1,
        fix_tolerance: rate("0.02"),
        residual: ResidualPolicy::TrimExcess,
        _pad1: [0; 7],
    }
}

/// A target of (a, g) with `legs = [(leg_ix, qty)]`.
pub fn target(a: AccountId, g: GroupId, legs: &[(u8, Qty)], exec: ExecSpec) -> TargetIntent {
    let mut l = [LegTarget::ZERO; 4];
    for (d, (i, x)) in l.iter_mut().zip(legs.iter()) {
        *d = LegTarget::new(*i, *x);
    }
    TargetIntent {
        account: a,
        _pad0: [0; 3],
        group: g,
        legs: l,
        n_legs: legs.len() as u8,
        reason: TargetReason::Rebalance,
        urgency: Urgency::Normal,
        _pad1: [0; 5],
        band: rate("0.01"),
        exec,
        tag: 0x10,
        _pad2: 0,
    }
}

/// T0 on g7 (S, P).
pub fn t0(s: &str, p: &str) -> TargetIntent {
    target(A1, G7, &[(0, q(s)), (1, q(p))], ExecSpec::PairOnFill(pair0()))
}

/// T1 on g8 (P).
pub fn t1(p: &str) -> TargetIntent {
    target(A1, G8, &[(0, q(p))], ExecSpec::TakerSlice(slice0()))
}

pub fn r0() -> RiskStateView {
    RiskStateView {
        conn_liveness_ms: 2000,
        max_order_notional: usd("2000"),
        max_reduce_notional: usd("2000"),
        max_hedge_notional: usd("2000"),
        dust_notional: usd("5"),
        ..RiskStateView::EMPTY
    }
}

pub fn rf0() -> InstRiskView {
    InstRiskView {
        max_px_dev: rate("0.05"),
        ioc_max_cross: rate("0.005"),
        rr_max_cross: rate("0.01"),
        l3_max_cross: Rate(0),
        quote_rate: rate("1"),
        rate_backoff: false,
    }
}

// ------------------------------------------------------------------------------------ views

/// `Views` over `MockViews` with book-status overrides.
#[derive(Clone, Debug)]
pub struct TV {
    pub m: MockViews,
    pub book_status: Vec<(InstId, BookStatus)>,
}

#[derive(Clone, Copy, Debug)]
pub struct TBook<'a> {
    b: MockBook<'a>,
    st: Option<BookStatus>,
}

impl BookRead for TBook<'_> {
    fn status(&self) -> BookStatus {
        self.st.unwrap_or_else(|| self.b.status())
    }
    fn upd_tsc(&self) -> Tsc {
        self.b.upd_tsc()
    }
    fn best(&self, s: Side) -> Option<Level> {
        self.b.best(s)
    }
    fn levels(&self, s: Side, out: &mut [Level]) -> usize {
        self.b.levels(s, out)
    }
}

impl Views for TV {
    type Book<'a> = TBook<'a>;
    type Portfolio<'a> = MockPortfolio<'a>;
    fn now(&self) -> Now {
        Views::now(&self.m)
    }
    fn bbo(&self, i: InstId) -> BboView {
        Views::bbo(&self.m, i)
    }
    fn book(&self, i: InstId) -> TBook<'_> {
        TBook {
            b: Views::book(&self.m, i),
            st: self.book_status.iter().find(|x| x.0 == i).map(|x| x.1),
        }
    }
    fn funding(&self, i: InstId) -> &FundingView {
        Views::funding(&self.m, i)
    }
    fn mark(&self, i: InstId) -> Px {
        Views::mark(&self.m, i)
    }
    fn portfolio(&self, a: AccountId) -> MockPortfolio<'_> {
        Views::portfolio(&self.m, a)
    }
    fn group_pos(&self, a: AccountId, g: GroupId) -> &GroupPosView {
        Views::group_pos(&self.m, a, g)
    }
    fn legs(&self, a: AccountId, g: GroupId) -> &LegView {
        Views::legs(&self.m, a, g)
    }
    fn target(&self, a: AccountId, g: GroupId) -> Option<&TargetView> {
        Views::target(&self.m, a, g)
    }
    fn order(&self, h: OrderRef) -> Option<OrderView> {
        Views::order(&self.m, h)
    }
    fn inst(&self, i: InstId) -> &hft_refdata::InstHot {
        Views::inst(&self.m, i)
    }
    fn caps(&self, v: VenueId) -> &VenueCaps {
        Views::caps(&self.m, v)
    }
    fn fee(&self, a: AccountId, i: InstId) -> FeeView {
        Views::fee(&self.m, a, i)
    }
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode {
        Views::mode(&self.m, a, g)
    }
    fn risk(&self, a: AccountId) -> RiskStateView {
        Views::risk(&self.m, a)
    }
    fn risk_for(&self, a: AccountId, i: InstId) -> InstRiskView {
        Views::risk_for(&self.m, a, i)
    }
    fn venue_ready(&self, a: AccountId, v: VenueId) -> bool {
        Views::venue_ready(&self.m, a, v)
    }
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32> {
        Views::leverage(&self.m, a, i)
    }
}

// ------------------------------------------------------------------------------------ output log

/// Every output of one call (copied out of `ExecOut`).
#[derive(Clone, Debug, Default)]
pub struct Rec {
    pub orders: Vec<ExecOrder>,
    pub cancels: Vec<OrderRef>,
    pub cancel_resting: Vec<CancelResting>,
    pub decisions: Vec<ExecDecision>,
    pub notify: Vec<ExecNotify>,
    pub broken: Vec<BrokenReq>,
    pub timers: Vec<ExecTimerReq>,
    pub notes: Vec<Note>,
}

impl Rec {
    pub fn take(out: &mut ExecOut) -> Rec {
        let r = Rec {
            orders: out.orders.as_slice().to_vec(),
            cancels: out.cancels.as_slice().to_vec(),
            cancel_resting: out.cancel_resting.as_slice().to_vec(),
            decisions: out.decisions.as_slice().to_vec(),
            notify: out.notify.as_slice().to_vec(),
            broken: out.broken.as_slice().to_vec(),
            timers: out.timers.as_slice().to_vec(),
            notes: out.notes.as_slice().to_vec(),
        };
        out.clear();
        r
    }
    pub fn extend(&mut self, o: Rec) {
        self.orders.extend(o.orders);
        self.cancels.extend(o.cancels);
        self.cancel_resting.extend(o.cancel_resting);
        self.decisions.extend(o.decisions);
        self.notify.extend(o.notify);
        self.broken.extend(o.broken);
        self.timers.extend(o.timers);
        self.notes.extend(o.notes);
    }
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
            && self.cancels.is_empty()
            && self.cancel_resting.is_empty()
            && self.decisions.is_empty()
            && self.notify.is_empty()
            && self.broken.is_empty()
            && self.timers.is_empty()
            && self.notes.is_empty()
    }
    /// Decisions with `action`.
    pub fn dec(&self, action: u8) -> Vec<ExecDecision> {
        self.decisions
            .iter()
            .filter(|d| d.action == action)
            .copied()
            .collect()
    }
    pub fn has_dec(&self, action: u8, detail: u32) -> bool {
        self.decisions
            .iter()
            .any(|d| d.action == action && d.detail == detail)
    }
    pub fn states(&self) -> Vec<(u8, u8, u32)> {
        self.decisions
            .iter()
            .filter(|d| d.action == 6)
            .map(|d| (d.state_from, d.state_to, d.detail))
            .collect()
    }
    pub fn notes_n(&self, n: u32) -> Vec<Note> {
        self.notes
            .iter()
            .filter(|x| x.code == hft_exec::decision::note_code(n))
            .copied()
            .collect()
    }
    pub fn upd(&self) -> Vec<ExecUpdateKind> {
        self.notify.iter().map(|n| n.u.kind).collect()
    }
    pub fn timer(&self, kind: ExecTimerKind) -> Vec<ExecTimerReq> {
        self.timers
            .iter()
            .filter(|x| x.kind() == Some(kind))
            .copied()
            .collect()
    }
    pub fn timer_set(&self, kind: ExecTimerKind) -> Vec<Tsc> {
        self.timers
            .iter()
            .filter(|x| x.kind() == Some(kind) && !x.cancel)
            .map(|x| x.at)
            .collect()
    }
    pub fn ord(&self, kind: ExecOrderKind) -> Vec<ExecOrder> {
        self.orders.iter().filter(|o| o.kind == kind).copied().collect()
    }
}

// ------------------------------------------------------------------------------------ fixture

#[allow(missing_debug_implementations)]
pub struct Fx {
    pub ex: Exec,
    pub v: TV,
    pub out: ExecOut,
    pub groups: Vec<GroupSpec>,
    pub rd: RefData,
    pub seq: u64,
    /// Insts whose connection evidence follows the clock (`conn_live_tsc = now`).
    pub live: Vec<InstId>,
    pub slot: u32,
    /// All records since construction.
    pub log: Rec,
}

pub fn bbo_book(m: &mut MockViews, i: InstId, bid: &str, ask: &str, qty: &str, levels: usize) {
    let (b, a, qq) = (px(bid), px(ask), q(qty));
    MockViews::bbo(m, i, (b, qq), (a, qq));
    let step = px("0.10").0;
    let bids: Vec<Level> = (0..levels)
        .map(|k| Level {
            px: Px(b.0 - step * k as i64),
            qty: qq,
        })
        .collect();
    let asks: Vec<Level> = (0..levels)
        .map(|k| Level {
            px: Px(a.0 + step * k as i64),
            qty: qq,
        })
        .collect();
    MockViews::book(m, i, &bids, &asks);
}

impl Fx {
    /// C0, R0, groups g7 and g8, all gates open.
    pub fn new() -> Fx {
        Fx::with(c0(), vec![g7(), g8()])
    }

    pub fn with(p: ExecParams, groups: Vec<GroupSpec>) -> Fx {
        Fx::with_cfg(cfg(p), groups, default_insts())
    }

    pub fn with_cfg(cf: ExecConfig, groups: Vec<GroupSpec>, insts: Vec<InstP>) -> Fx {
        let rd = refdata_with(&insts);
        let mut m = MockViews::new(RefFixture::new(rd.clone(), A1));
        m.seq(1);
        bbo_book(&mut m, S, "99.90", "100.10", "40", 5);
        bbo_book(&mut m, P, "99.80", "100.20", "50", 5);
        for i in [S2, P2, P3, P4, Q, SL, PL] {
            bbo_book(&mut m, i, "99.80", "100.20", "50", 5);
        }
        for i in PX {
            bbo_book(&mut m, i, "99.80", "100.20", "50", 5);
        }
        let mut live = vec![S, S2, P, P2, P3, P4, Q, SL, PL];
        live.extend(PX);
        for i in &live {
            MockViews::conn(&mut m, *i, Tsc(1), true, false);
            MockViews::mark(&mut m, *i, px("100"));
        }
        for a in [A1, A2, S1] {
            MockViews::risk(&mut m, a, r0());
            for i in &live {
                MockViews::risk_for(&mut m, a, *i, rf0());
            }
        }
        let n = (groups.len() as u32 + 16).max(256);
        let mut ex = Exec::new(&cf, &caps(n)).expect("new");
        let mut out = ExecOut::with_caps(OutCaps::all(4096));
        let gt = GroupTable::new(&groups, u16::try_from(n).unwrap_or(u16::MAX));
        ex.on_groups_changed(&gt, &RefView::new(&rd, A1), &mut out);
        out.clear();
        let mut fx = Fx {
            ex,
            v: TV {
                m,
                book_status: Vec::new(),
            },
            out,
            groups,
            rd,
            seq: 1,
            live,
            slot: 0,
            log: Rec::default(),
        };
        fx.at(0);
        for a in [A1, A2, S1] {
            let now = fx.now();
            fx.ex.on_gate_open(a, now);
        }
        let now = fx.now();
        let mut out = ExecOut::with_caps(OutCaps::all(4096));
        fx.ex.advance(&fx.v, now, &mut out);
        fx
    }

    /// Moves the clock to `ms` (live insts get `conn_live_tsc = now`).
    pub fn at(&mut self, ms: u64) {
        self.at_tsc(t(ms));
    }
    pub fn at_tsc(&mut self, x: Tsc) {
        self.v.m.at_tsc(x);
        for i in self.live.clone() {
            let b = self.v.m.bbo_of(i);
            MockViews::conn(&mut self.v.m, i, Tsc(x.0.max(1)), b.conn_up, b.resync);
        }
    }
    pub fn now(&mut self) -> Now {
        self.seq += 1;
        self.v.m.seq(self.seq);
        self.v.m.now()
    }
    pub fn now_ms(&self) -> u64 {
        (Views::now(&self.v.m).tsc.0 - T0) / 1_000_000
    }
    /// Stops the connection evidence of `i` at `ms` (quote unavailable 2000 ms later).
    pub fn freeze(&mut self, i: InstId, ms: u64) {
        self.live.retain(|x| *x != i);
        let b = self.v.m.bbo_of(i);
        MockViews::conn(&mut self.v.m, i, t(ms).max(Tsc(1)), b.conn_up, b.resync);
    }
    /// Quote unavailable now (connection down).
    pub fn quote_down(&mut self, i: InstId) {
        self.live.retain(|x| *x != i);
        let b = self.v.m.bbo_of(i);
        MockViews::conn(&mut self.v.m, i, b.conn_live_tsc, false, b.resync);
    }
    pub fn quote_up(&mut self, i: InstId) {
        if !self.live.contains(&i) {
            self.live.push(i);
        }
        let now = self.v.m.now().tsc;
        MockViews::conn(&mut self.v.m, i, Tsc(now.0.max(1)), true, false);
    }

    fn rec(&mut self) -> Rec {
        let r = Rec::take(&mut self.out);
        self.log.extend(r.clone());
        r
    }

    pub fn adv(&mut self) -> Rec {
        let now = self.now();
        self.ex.advance(&self.v, now, &mut self.out);
        self.rec()
    }
    /// `advance` until nothing is pending (and the Continue loop with README caps if `out` is small).
    pub fn adv_all(&mut self) -> Rec {
        let mut r = Rec::default();
        for _ in 0..64 {
            r.extend(self.adv());
            if !self.ex.has_pending() {
                break;
            }
        }
        r
    }

    pub fn set(
        &mut self,
        owner: StrategyId,
        tg: &TargetIntent,
    ) -> (Result<(), hft_strategy_sdk::TargetReject>, Rec) {
        let now = self.now();
        let r = self.ex.set_target(owner, tg, false, &self.v, now, &mut self.out);
        (r, self.rec())
    }
    pub fn set_ro(
        &mut self,
        owner: StrategyId,
        tg: &TargetIntent,
        ro: bool,
    ) -> (Result<(), hft_strategy_sdk::TargetReject>, Rec) {
        let now = self.now();
        let r = self.ex.set_target(owner, tg, ro, &self.v, now, &mut self.out);
        (r, self.rec())
    }
    /// set_target (i1) + advance.
    pub fn go(&mut self, tg: &TargetIntent) -> Rec {
        let (r, mut a) = self.set(I1, tg);
        r.expect("target accepted");
        a.extend(self.adv());
        a
    }
    pub fn clear(&mut self, a: AccountId, g: GroupId) -> Rec {
        let now = self.now();
        let owner = g.strategy();
        self.ex.clear_target(owner, a, g, now, &mut self.out);
        self.rec()
    }

    // ---- views edits

    pub fn legs(&mut self, a: AccountId, g: GroupId, net: &[&str]) {
        let mut l = *Views::legs(&self.v.m, a, g);
        l.n_legs = net.len() as u8;
        for (i, n) in net.iter().enumerate() {
            l.leg[i].net = q(n);
        }
        MockViews::legs(&mut self.v.m, a, g, l);
        // group_pos mirrors the nets.
        let mut gp = GroupPosView::EMPTY;
        gp.n_legs = net.len() as u8;
        for (i, n) in net.iter().enumerate() {
            gp.legs[i] = LegPos {
                qty: q(n),
                entry_px: Px(0),
                upnl: Notional(0),
            };
        }
        MockViews::group_pos(&mut self.v.m, a, g, gp);
    }
    pub fn infl(&mut self, a: AccountId, g: GroupId, i: usize, x: &str) {
        let mut l = *Views::legs(&self.v.m, a, g);
        l.leg[i].inflight_immediate = q(x);
        MockViews::legs(&mut self.v.m, a, g, l);
    }
    pub fn lv(&mut self, a: AccountId, g: GroupId, f: impl FnOnce(&mut LegView)) {
        let mut l = *Views::legs(&self.v.m, a, g);
        f(&mut l);
        MockViews::legs(&mut self.v.m, a, g, l);
    }
    pub fn risk(&mut self, a: AccountId, f: impl FnOnce(&mut RiskStateView)) {
        let mut r = Views::risk(&self.v.m, a);
        f(&mut r);
        MockViews::risk(&mut self.v.m, a, r);
    }
    pub fn risk_for(&mut self, a: AccountId, i: InstId, f: impl FnOnce(&mut InstRiskView)) {
        let mut r = Views::risk_for(&self.v.m, a, i);
        f(&mut r);
        MockViews::risk_for(&mut self.v.m, a, i, r);
    }
    pub fn perp(&mut self, a: AccountId, v: VenueId, i: InstId, x: &str) {
        MockViews::portfolio(&mut self.v.m, a, |p| {
            p.perp(
                v,
                i,
                PerpPosView {
                    qty: q(x),
                    entry_px: Px(0),
                    upnl: Notional(0),
                    liq_px: Px(0),
                    adl_quantile: 0,
                },
            );
        });
    }
    /// Spot group inventory and free balance of BASE on `v`.
    pub fn spot(&mut self, a: AccountId, v: VenueId, g: GroupId, inv: &str, free: &str) {
        MockViews::portfolio(&mut self.v.m, a, |p| {
            p.inventory_of(v, g, q(inv));
            p.cash(v, BASE, q(free).0, 0);
            p.inventory(
                v,
                BASE,
                InventoryView {
                    total: q(inv),
                    assigned: q(inv),
                    unassigned: Qty(0),
                    baseline: Qty(0),
                },
            );
        });
    }
    pub fn quote(&mut self, i: InstId, bid: &str, ask: &str) {
        let b = self.v.m.bbo_of(i);
        MockViews::bbo(&mut self.v.m, i, (px(bid), b.bid_qty), (px(ask), b.ask_qty));
    }
    pub fn book(&mut self, i: InstId, bid: &str, ask: &str, qty: &str, levels: usize) {
        let b = self.v.m.bbo_of(i);
        bbo_book(&mut self.v.m, i, bid, ask, qty, levels);
        MockViews::conn(&mut self.v.m, i, b.conn_live_tsc, b.conn_up, b.resync);
    }

    pub fn mode(&mut self, a: AccountId, g: GroupId, m: RunMode) {
        MockViews::mode(&mut self.v.m, a, g, m);
    }
    pub fn mark(&mut self, i: InstId, p: &str) {
        MockViews::mark(&mut self.v.m, i, px(p));
    }
    pub fn ready(&mut self, a: AccountId, v: VenueId, r: bool) {
        MockViews::venue_ready(&mut self.v.m, a, v, r);
    }
    /// Stale quote: connection evidence `ms_ago` before now.
    pub fn stale(&mut self, i: InstId, ms_ago: u64) {
        self.live.retain(|x| *x != i);
        let now = Views::now(&self.v.m).tsc.0;
        let b = self.v.m.bbo_of(i);
        MockViews::conn(
            &mut self.v.m,
            i,
            Tsc(now - ms_ago * 1_000_000),
            b.conn_up,
            b.resync,
        );
    }

    // ---- order flow

    pub fn next_ref(&mut self, o: &ExecOrder) -> OrderRef {
        self.slot += 1;
        OrderRef::new(o.intent.venue, o.intent.account, self.slot, 1)
    }
    /// `on_place_result(Placed)` for `o`; returns its OrderRef.
    pub fn placed(&mut self, o: &ExecOrder) -> OrderRef {
        let r = self.next_ref(o);
        let now = self.now();
        self.ex
            .on_place_result(o.id, PlaceResult::Placed(r), now, &mut self.out);
        self.rec();
        r
    }
    pub fn place_res(&mut self, o: &ExecOrder, res: PlaceResult) -> Rec {
        let now = self.now();
        self.ex.on_place_result(o.id, res, now, &mut self.out);
        self.rec()
    }
    pub fn upd(&mut self, r: OrderRef, g: GroupId, kind: OrderUpdateKind) -> Rec {
        let u = OrderUpdate {
            r,
            group: g,
            tag: 0,
            kind,
        };
        let now = self.now();
        self.ex.on_order_update(&u, &self.v, now, &mut self.out);
        self.rec()
    }
    /// Fill `x` (decimal) of order `r`; `done` = terminal.
    pub fn fill(&mut self, r: OrderRef, g: GroupId, x: &str, done: bool) -> Rec {
        self.upd(
            r,
            g,
            OrderUpdateKind::Filled {
                d: q(x),
                px: px("100"),
                fee: 0,
                fee_asset: USDT,
                cum: q(x),
                done,
            },
        )
    }
    /// Zero-fill IOC end.
    pub fn nofill(&mut self, r: OrderRef, g: GroupId) -> Rec {
        self.upd(r, g, OrderUpdateKind::Cancelled)
    }
    pub fn exch_reject(&mut self, r: OrderRef, g: GroupId) -> Rec {
        self.upd(
            r,
            g,
            OrderUpdateKind::Rejected {
                bits: RejectBits(0),
                venue_code: 1,
            },
        )
    }
    pub fn local_fail(&mut self, r: OrderRef, g: GroupId, status: u32) -> Rec {
        self.upd(
            r,
            g,
            OrderUpdateKind::Rejected {
                bits: RejectBits::LOCAL_SEND_FAIL,
                venue_code: status,
            },
        )
    }

    pub fn timer(&mut self, x: &ExecTimerReq) -> Rec {
        let now = self.now();
        self.ex.on_timer(x.token(), now, &mut self.out);
        self.rec()
    }
    pub fn fire(&mut self, kind: ExecTimerKind, a: AccountId, g: GroupId) -> Rec {
        let token = (ExecTimerReq::id(kind, a) as u64) | ((g.0 as u64) << 32);
        let now = self.now();
        self.ex.on_timer(token, now, &mut self.out);
        self.rec()
    }
    pub fn touch(&mut self, i: InstId) {
        self.ex.touch_inst(i);
    }
    pub fn risk_ev(&mut self, a: AccountId, e: hft_strategy_sdk::RiskEvent) {
        let now = self.now();
        self.ex.on_risk(a, &e, now);
    }
    pub fn link(&mut self, l: &LinkNotice) -> Rec {
        let now = self.now();
        self.ex.on_link(l, now, &mut self.out);
        self.rec()
    }
    pub fn ext(&mut self, a: AccountId, f: &hft_strategy_sdk::ExternalFillView) -> (Tsc, Rec) {
        let now = self.now();
        let r = self.ex.on_external(a, f, &self.v, now, &mut self.out);
        (r, self.rec())
    }
    pub fn cmd_fix(&mut self, a: AccountId, g: GroupId) -> (Result<(), ExecCmdErr>, Rec) {
        let now = self.now();
        let r = self.ex.hedge_fix_cmd(a, g, &self.v, now, &mut self.out);
        (r, self.rec())
    }
    pub fn force(&mut self, a: AccountId, v: VenueId, frac: &str) {
        let now = self.now();
        self.ex.force_deleverage(a, v, rate(frac), now, &mut self.out);
        self.out.clear();
    }
    pub fn flatten(&mut self, a: AccountId) {
        let now = self.now();
        self.ex.engine_flatten(a, now, &mut self.out);
        self.out.clear();
    }
    pub fn leg_adjust(&mut self, a: AccountId, g: GroupId) {
        self.ex.on_leg_adjust(a, g);
    }

    pub fn cell(&self, a: AccountId, g: GroupId) -> TargetCell {
        *self.ex.cell(a, g).expect("cell")
    }
    pub fn cm(&mut self, a: AccountId, g: GroupId) -> &mut TargetCell {
        self.ex.cell_mut(a, g).expect("cell")
    }
    pub fn acct(&mut self, a: AccountId) -> &mut AcctState {
        self.ex.acct_mut(a).expect("acct")
    }
    pub fn snap(&self) -> Vec<u8> {
        let mut w = hft_strategy_sdk::SnapshotWriter::new(4 << 20);
        self.ex.snapshot(&mut w);
        w.as_bytes().to_vec()
    }
    pub fn restore(&self, bytes: &[u8]) -> Exec {
        let gt = GroupTable::new(&self.groups, 256);
        let mut r = hft_strategy_sdk::SnapshotReader::new(bytes);
        Exec::restore(&mut r, &cfg(c0()), &caps(256), &gt, &RefView::new(&self.rd, A1)).expect("restore")
    }
}

/// External fill view of (g, leg) on `inst`.
pub fn xfill(
    g: GroupId,
    leg_ix: u8,
    inst: InstId,
    venue: VenueId,
    side: Side,
    x: &str,
    cause: hft_strategy_sdk::ExternalCause,
) -> hft_strategy_sdk::ExternalFillView {
    hft_strategy_sdk::ExternalFillView {
        venue,
        inst,
        group: g,
        leg_ix,
        side,
        qty: q(x),
        px: px("100.10"),
        fee: 0,
        fee_asset: USDT,
        cause,
        ex_ts_ns: 0,
        deadline_tsc: Tsc(0),
    }
}

pub fn link(id: u32, a: AccountId, g: GroupId, state: LinkPhase, broken: LinkBrokenCause) -> LinkNotice {
    LinkNotice {
        link_id: id,
        account: a,
        group: g,
        first: OrderRef::new(VenueId(0), a, 0, 0),
        state,
        broken,
        flags: 0,
        first_net: Qty(0),
        hedge_filled: Qty(0),
        owed: Qty(0),
        tolerance: rate("0.02"),
        residual: ResidualPolicy::TrimExcess,
    }
}

pub fn level_of(n: &Note) -> NoteLevel {
    n.level
}

pub fn done(u: &ExecUpdate) -> Option<bool> {
    match u.kind {
        ExecUpdateKind::Done { cleared_by_engine } => Some(cleared_by_engine),
        _ => None,
    }
}

pub fn ms_margin(l: MarginLevel) -> hft_strategy_sdk::RiskEvent {
    hft_strategy_sdk::RiskEvent::MarginStress {
        venue: B,
        level: l,
        liq_dist: Rate::MAX,
        ratio: Rate(0),
        deadline_tsc: Tsc(0),
    }
}

pub fn scope(a: AccountId, g: GroupId) -> hft_strategy_sdk::CapScopeView {
    hft_strategy_sdk::CapScopeView {
        strategy: StrategyId(0),
        venue: VenueId(0),
        account: a,
        group: g,
        inst: InstId(0),
    }
}

pub fn cap_set(src: hft_types::CapSource, a: AccountId, g: GroupId) -> hft_strategy_sdk::RiskEvent {
    hft_strategy_sdk::RiskEvent::CapSet {
        source: src,
        scope: scope(a, g),
    }
}
pub fn cap_clr(src: hft_types::CapSource, a: AccountId, g: GroupId) -> hft_strategy_sdk::RiskEvent {
    hft_strategy_sdk::RiskEvent::CapCleared {
        source: src,
        scope: scope(a, g),
    }
}

/// Rows of section 21 in a snapshot: `(kind, body)`.
pub fn rows(snap: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    let body = &snap[16..];
    let count = u32::from_le_bytes(snap[4..8].try_into().unwrap()) as usize;
    let mut off = 0;
    for _ in 0..count {
        let kind = body[off];
        let len = u32::from_le_bytes(body[off + 4..off + 8].try_into().unwrap()) as usize;
        out.push((kind, body[off + 8..off + 8 + len].to_vec()));
        off += 8 + len;
    }
    out
}

/// Cell row (first 400 B) of (a, g).
pub fn cell_row(snap: &[u8], a: AccountId, g: GroupId) -> Option<ExecCellRow> {
    use zerocopy::FromBytes;
    rows(snap)
        .into_iter()
        .filter(|r| r.0 == ROW_CELL)
        .find_map(|(_, b)| {
            let r = ExecCellRow::read_from_bytes(&b[..400]).ok()?;
            (r.account == a.0 && r.group == g.0).then_some(r)
        })
}

pub fn acct_row(snap: &[u8], a: AccountId) -> Option<ExecAcctRow> {
    use zerocopy::FromBytes;
    rows(snap)
        .into_iter()
        .filter(|r| r.0 == ROW_ACCT)
        .find_map(|(_, b)| {
            let r = ExecAcctRow::read_from_bytes(&b[..24]).ok()?;
            (r.account == a.0).then_some(r)
        })
}

pub fn gate_row(snap: &[u8], a: AccountId) -> Option<ExecGateRow> {
    use zerocopy::FromBytes;
    rows(snap)
        .into_iter()
        .filter(|r| r.0 == ROW_GATE)
        .find_map(|(_, b)| {
            let r = ExecGateRow::read_from_bytes(&b[..24]).ok()?;
            (r.account == a.0).then_some(r)
        })
}

/// Rebuilds the cell table with `groups`.
pub fn regroup(fx: &mut Fx, groups: Vec<GroupSpec>) -> Rec {
    fx.groups = groups;
    let gt = GroupTable::new(&fx.groups, 256);
    let rd = fx.rd.clone();
    fx.ex.on_groups_changed(&gt, &RefView::new(&rd, A1), &mut fx.out);
    Rec::take(&mut fx.out)
}

/// Installs `tg` as a Working strategy target without emitting anything (fixture state).
pub fn inject(fx: &mut Fx, tg: &TargetIntent) {
    let c = fx.cm(tg.account, tg.group);
    c.view.intent = *tg;
    c.view.origin = hft_strategy_sdk::TargetOrigin::Strategy;
    c.view.state = hft_strategy_sdk::TargetState::Working;
    c.has_target = true;
    fx.ex.refresh(tg.account, tg.group);
}

/// Taker target of a leg list on (a, g).
pub fn taker(a: AccountId, g: GroupId, legs: &[(u8, &str)]) -> TargetIntent {
    let l: Vec<(u8, Qty)> = legs.iter().map(|(i, x)| (*i, q(x))).collect();
    target(a, g, &l, ExecSpec::TakerSlice(slice0()))
}

/// Fixture with `n` two-leg (S, P) groups of a1 (`GroupId(0x0100_0100 + k)`) only.
pub fn fx_many(n: u32) -> Fx {
    let mut groups = vec![];
    for k in 0..n {
        groups.push(gspec(GroupId(0x0100_0100 + k), &[(S, A), (P, B)]));
    }
    Fx::with(c0(), groups)
}

pub fn many(k: u32) -> GroupId {
    GroupId(0x0100_0100 + k)
}

/// Strips `trigger_seq` (differs between batches).
pub fn norm(d: &[ExecDecision]) -> Vec<ExecDecision> {
    d.iter()
        .map(|x| {
            let mut y = *x;
            y.trigger_seq = 0;
            y
        })
        .collect()
}

/// `"-" + s` (decimal string of the opposite sign).
pub fn negate(s: &str) -> String {
    format!("-{s}")
}
