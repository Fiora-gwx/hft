//! Shared fixtures of the hft-sim tests (TESTS.md "夹具": `rd_sim()`, accounts `A9`/`A8`,
//! `setup_sim()`, `seed9(…)` and the record / command notation).
#![allow(dead_code, clippy::too_many_arguments, clippy::type_complexity)]

use hft_event::{
    Bbo, Funding, GwCmd, GwOp, L2BatchHdr, L2BatchView, L2SnapHdr, L2SnapshotPart, Level, MarkPrice,
    MarketHdr, OrderEvent, OrderEventKind, RecordHeader, RepMsg, Trade,
};
use hft_refdata::{
    AccountSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, LiqMarker, MarginData, MarginMode, MmrTierSpec,
    PositionMode, PriceBand, RateLimit, RdbBuilder, RefData, SimAccountCfg, SimBalance, SimBalanceSpec,
    SimSpec, StaleGuardKind, TestnetKind, TsUnit, VenueCaps, VenueSpec,
};
use hft_sim::{
    LatencyModel, MarginModel, MarketEventRef, MmrSource, SimExchange, SimInput, SimSeedState, SimSetup,
    SimTuning,
};
use hft_types::{
    AccountId, AssetExp, AssetId, ConnId, InstId, MarketKind, Notional, Px, Qty, Rate, Side, SlotId, Tif,
    Tsc, VenueId,
};

pub const USDT: AssetId = AssetId(1);
pub const BTC: AssetId = AssetId(2);
pub const ETH: AssetId = AssetId(3);
pub const BNB: AssetId = AssetId(4);
/// `S1`: spot, exchange E1 (Binance).
pub const S1: VenueId = VenueId::BINANCE_SPOT;
/// `P2`: USDT perpetual, exchange E2 (OKX).
pub const P2: VenueId = VenueId::OKX_SWAP;
/// `S4`: spot, exchange E2 (OKX); only for the same-exchange cases.
pub const S4: VenueId = VenueId::OKX_SPOT;
pub const BTC_S1: InstId = InstId(11);
pub const BNB_S1: InstId = InstId(12);
pub const BTC_P2: InstId = InstId(21);
pub const ETH_P2: InstId = InstId(22);
pub const A9: AccountId = AccountId(9);
pub const A8: AccountId = AccountId(8);

#[allow(unused_imports)]
pub use hft_types::Side::{Buy, Sell};

pub mod scen;

// ---------------------------------------------------------------- numbers

/// Decimal string at exponent `exp`.
pub fn dec(s: &str, exp: i8) -> i64 {
    match hft_types::parse_exact(s.as_bytes(), exp) {
        Ok(v) => v,
        Err(e) => panic!("bad decimal {s} at {exp}: {e:?}"),
    }
}
/// Price of BTC/ETH/BNB (exponent −4).
pub fn px(s: &str) -> Px {
    Px(dec(s, -4))
}
/// Quantity of BTC/ETH/BNB (exponent −8).
pub fn q(s: &str) -> Qty {
    Qty(dec(s, -8))
}
/// USDT quantity (exponent −6).
pub fn usdt(s: &str) -> Qty {
    Qty(dec(s, -6))
}
/// Notional (exponent −6).
pub fn notional(s: &str) -> Notional {
    Notional(dec(s, -6))
}
/// Rate (exponent −9).
pub fn rate(s: &str) -> Rate {
    Rate(dec(s, -9))
}
/// ms → event time (1 GHz TSC: 1 tick = 1 ns).
pub const fn ms(x: u64) -> u64 {
    x * 1_000_000
}

// ---------------------------------------------------------------- refdata

fn asset(id: AssetId, name: &str, exp: (i8, i8)) -> AssetSpec {
    AssetSpec {
        id,
        name: name.into(),
        class: if id == USDT {
            AssetClass::Stable
        } else {
            AssetClass::Crypto
        },
        exp: AssetExp {
            px_exp: exp.0,
            qty_exp: exp.1,
        },
        px_max: Px(1_000_000_000_000_000),
        qty_max: Qty(1_000_000_000_000_000_000),
        dust: Qty(1),
    }
}

fn venue(id: VenueId, exchange: Exchange, product: InstKind) -> VenueSpec {
    VenueSpec {
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
    }
}

pub fn spot_caps(v: VenueId) -> VenueCaps {
    VenueCaps {
        venue: v,
        market: MarketKind::Spot,
        fee_asset_rule: FeeAssetRule::Quote,
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
        flags: CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK | CapFlags::AMEND,
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

pub fn perp_caps(v: VenueId) -> VenueCaps {
    VenueCaps {
        venue: v,
        market: MarketKind::LinearPerp,
        fee_asset_rule: FeeAssetRule::Quote,
        reduce_only: true,
        required_position_mode: PositionMode::OneWay,
        required_margin_mode: MarginMode::Cross,
        required_multi_asset: Some(false),
        required_spot_borrow: None,
        funding_settle: FundingSettleKind::PerSymbolPush,
        margin_data: MarginData::Push,
        own_liquidation_marker: LiqMarker::ClientIdPrefix,
        liq_marker_arg: "autoclose-".into(),
        ignorable_user_events: Vec::new().into(),
        flags: CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK | CapFlags::AMEND,
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

#[allow(clippy::too_many_arguments)]
fn inst(
    id: u32,
    sym: &str,
    venue: VenueId,
    kind: InstKind,
    asset: AssetId,
    tick: &str,
    lot: &str,
    min_qty: &str,
    status: InstStatus,
    window: u32,
) -> InstSpec {
    InstSpec {
        id: InstId(id),
        venue,
        symbol: sym.into(),
        status,
        kind,
        flags: 0,
        asset,
        quote: USDT,
        px_unit_shift: 0,
        qty_unit_shift: 0,
        tick: dec(tick, -4),
        lot: dec(lot, -8),
        min_qty: Qty(dec(min_qty, -8)),
        max_qty: Qty(100_000_000_000_000),
        min_notional: Notional(5_000_000),
        book_window: window,
        l2_depth: 100,
        crossed_tolerance: 0,
        venue_num_id: 0,
        tick_str: tick.into(),
        lot_str: lot.into(),
        min_qty_str: min_qty.into(),
        funding_interval_s: if kind == InstKind::Perp { 28_800 } else { 0 },
        funding_anchor_s: 0,
        max_leverage: if kind == InstKind::Perp { 100 } else { 0 },
        listed_utc_ns: 0,
        delivery_utc_ns: 0,
        price_band: PriceBand::default(),
    }
}

/// Every editable input of `rd_sim()`.
#[derive(Clone, Debug)]
pub struct RdSpec {
    pub caps: Vec<VenueCaps>,
    pub insts: Vec<InstSpec>,
    pub fees: Vec<FeeSpec>,
    pub limits: Vec<(VenueId, RateLimit)>,
    pub mmr: Vec<MmrTierSpec>,
    pub accounts: Vec<AccountSpec>,
    /// Sim accounts: `(account, fee_tier)`.
    pub sims: Vec<(AccountId, u8)>,
}

impl RdSpec {
    pub fn caps_mut(&mut self, v: VenueId) -> &mut VenueCaps {
        let ix = self.caps.iter().position(|c| c.venue == v).unwrap_or(usize::MAX);
        &mut self.caps[ix]
    }
    pub fn inst_mut(&mut self, i: InstId) -> &mut InstSpec {
        let ix = self.insts.iter().position(|x| x.id == i).unwrap_or(usize::MAX);
        &mut self.insts[ix]
    }
}

fn fee(venue: VenueId, tier: u8, maker: &str, taker: &str) -> FeeSpec {
    FeeSpec {
        venue,
        tier,
        fee: FeePair {
            maker: rate(maker),
            taker: rate(taker),
        },
    }
}

pub fn vbits(vs: &[VenueId]) -> u64 {
    vs.iter().fold(0, |m, v| m | (1u64 << v.0))
}

pub fn rd_spec() -> RdSpec {
    let mut insts = Vec::new();
    for id in 1..=10u32 {
        insts.push(inst(
            id,
            &format!("FILL{id}"),
            S1,
            InstKind::Spot,
            BTC,
            "0.01",
            "0.00001",
            "0.00001",
            InstStatus::Delisted,
            64,
        ));
    }
    insts.push(inst(
        11,
        "BTCUSDT",
        S1,
        InstKind::Spot,
        BTC,
        "0.01",
        "0.00001",
        "0.00001",
        InstStatus::Trading,
        4096,
    ));
    insts.push(inst(
        12,
        "BNBUSDT",
        S1,
        InstKind::Spot,
        BNB,
        "0.01",
        "0.001",
        "0.001",
        InstStatus::Trading,
        4096,
    ));
    for id in 13..=20u32 {
        insts.push(inst(
            id,
            &format!("FILL{id}"),
            S1,
            InstKind::Spot,
            BTC,
            "0.01",
            "0.00001",
            "0.00001",
            InstStatus::Delisted,
            64,
        ));
    }
    insts.push(inst(
        21,
        "BTC-USDT-SWAP",
        P2,
        InstKind::Perp,
        BTC,
        "0.1",
        "0.001",
        "0.001",
        InstStatus::Trading,
        4096,
    ));
    insts.push(inst(
        22,
        "ETH-USDT-SWAP",
        P2,
        InstKind::Perp,
        ETH,
        "0.01",
        "0.01",
        "0.01",
        InstStatus::Trading,
        4096,
    ));
    let accounts = (1..=9u8)
        .map(|a| AccountSpec {
            id: AccountId(a),
            sim: true,
            venues: vbits(&[S1, P2]),
            label: format!("sim-{a}"),
            ..AccountSpec::default()
        })
        .collect();
    RdSpec {
        caps: vec![spot_caps(S1), perp_caps(P2), spot_caps(S4)],
        insts,
        fees: vec![
            fee(S1, 1, "0.0002", "0.0004"),
            fee(S1, 2, "0.0002", "0.0003"),
            fee(P2, 1, "0.0002", "0.0005"),
            fee(P2, 2, "0.0002", "0.0005"),
            fee(S4, 1, "0.0002", "0.0004"),
        ],
        limits: Vec::new(),
        mmr: Vec::new(),
        accounts,
        sims: (1..=9u8).map(|a| (AccountId(a), 1)).collect(),
    }
}

pub fn build_rd(s: &RdSpec) -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    b.asset(asset(USDT, "USDT", (-6, -6)));
    b.asset(asset(BTC, "BTC", (-4, -8)));
    b.asset(asset(ETH, "ETH", (-4, -8)));
    b.asset(asset(BNB, "BNB", (-4, -8)));
    b.venue(venue(S1, Exchange::Binance, InstKind::Spot));
    b.venue(venue(P2, Exchange::Okx, InstKind::Perp));
    b.venue(venue(S4, Exchange::Okx, InstKind::Spot));
    for c in &s.caps {
        b.caps(c.clone());
    }
    for i in &s.insts {
        b.inst(i.clone());
    }
    for a in &s.accounts {
        b.account(a.clone());
    }
    for f in &s.fees {
        b.fee(*f);
    }
    for (a, tier) in &s.sims {
        b.sim(SimSpec {
            account: *a,
            fee_tier: *tier,
            latency_model: 0,
            margin_model: 0,
        });
        b.sim_balance(SimBalanceSpec {
            account: *a,
            venue: S1,
            asset: USDT,
            qty: usdt("1000"),
        });
    }
    for (v, l) in &s.limits {
        b.limit(*v, *l);
    }
    for m in &s.mmr {
        b.mmr_tier(*m);
    }
    match RefData::load(&b.finish(), None) {
        Ok(r) => r,
        Err(e) => panic!("rd_sim does not load: {:?}", e.errors),
    }
}

pub fn rd_with(f: impl FnOnce(&mut RdSpec)) -> RefData {
    let mut s = rd_spec();
    f(&mut s);
    build_rd(&s)
}

/// `rd_sim()` of TESTS.md.
pub fn rd_sim() -> RefData {
    build_rd(&rd_spec())
}

/// `S1` `fee_asset_rule` override.
pub fn rd_fee_rule(rule: FeeAssetRule) -> RefData {
    rd_with(|s| s.caps_mut(S1).fee_asset_rule = rule)
}

/// BTC/P2 tiers of SIM-071.4 (≤ 50 000: 0.004; ≤ 200 000: 0.005; above: 0.01).
pub fn btc_p2_tiers() -> Vec<MmrTierSpec> {
    [("50000", "0.004"), ("200000", "0.005"), ("1000000000", "0.01")]
        .iter()
        .map(|(n, r)| MmrTierSpec {
            inst: BTC_P2,
            tier: hft_refdata::MmrTier {
                notional_max: notional(n),
                mmr: rate(r),
            },
        })
        .collect()
}

// ---------------------------------------------------------------- setup

pub fn bal(v: VenueId, a: AssetId, qty: Qty) -> SimBalance {
    SimBalance {
        venue: v,
        asset: a,
        qty,
    }
}

pub fn cfg_a9() -> SimAccountCfg {
    SimAccountCfg {
        initial_balances: vec![
            bal(S1, USDT, usdt("100000")),
            bal(S1, BNB, q("1")),
            bal(P2, USDT, usdt("18000")),
        ]
        .into(),
        latency_model: 0,
        fee_tier: 1,
        margin_model: 0,
    }
}

pub fn cfg_a8() -> SimAccountCfg {
    SimAccountCfg {
        initial_balances: vec![bal(S1, USDT, usdt("50000")), bal(P2, USDT, usdt("10000"))].into(),
        latency_model: 0,
        fee_tier: 1,
        margin_model: 0,
    }
}

pub fn tuning() -> SimTuning {
    SimTuning {
        seed: 42,
        consume_decay_ms: 1_000,
        take_avail_bp: 10_000,
        quiet_ms: 50,
        report_interval_s: 60,
        transfer_delay_ms: 300_000,
        rate_limit: false,
        late_fee: false,
        maker_fill: hft_sim::MakerFill::TradeThrough,
    }
}

pub fn mm0() -> MarginModel {
    MarginModel {
        mmr: MmrSource::Flat(rate("0.005")),
        leverage: 5,
        liq_fee: rate("0.005"),
        liq_slippage_bp: 50,
    }
}

/// `setup_sim()` of TESTS.md.
pub fn setup_sim() -> SimSetup {
    SimSetup {
        tuning: tuning(),
        tsc_hz_milli: 1_000_000_000_000,
        latency_models: vec![LatencyModel::Fixed {
            entry_us: 2_000,
            resp_us: 2_000,
        }],
        margin_models: vec![mm0()],
        accounts: vec![(A9, cfg_a9())],
    }
}

pub fn setup_with_a8() -> SimSetup {
    let mut s = setup_sim();
    s.accounts.push((A8, cfg_a8()));
    s
}

// ---------------------------------------------------------------- records

pub fn mhdr(venue: VenueId, inst: InstId, ex_ts: i64) -> MarketHdr {
    MarketHdr {
        venue,
        conn: ConnId(0),
        _pad: 0,
        inst,
        ex_ts_ns: ex_ts,
        ex_send_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        sw_rx_ns: 0,
        rx_empty_gap_ns: 0,
        _rsv: 0,
    }
}

pub fn rhdr(t: u64) -> RecordHeader {
    RecordHeader {
        len: 0,
        kind: 0,
        flags: 0,
        seq: 0,
        tsc: t,
    }
}

pub fn venue_of(i: InstId) -> VenueId {
    if i.0 >= 21 { P2 } else { S1 }
}

/// One delivered reply (bulk copied).
#[derive(Clone, Debug)]
pub struct Rep {
    pub account: AccountId,
    pub due: u64,
    pub msg: RepMsg,
    pub bulk: Vec<u8>,
}

/// Exchange under test plus the replies drained so far and the slot counters.
pub struct Sim {
    pub ex: SimExchange,
    pub out: Vec<Rep>,
    pub slots: [u32; 256],
    pub snap_id: u64,
}

impl Sim {
    pub fn new(setup: &SimSetup, rd: &RefData) -> Sim {
        let ex = match SimExchange::new(setup, rd) {
            Ok(x) => x,
            Err(e) => panic!("SimExchange::new: {e:?}"),
        };
        Sim {
            ex,
            out: Vec::new(),
            slots: [0; 256],
            snap_id: 0,
        }
    }

    /// `rd_sim()` + `setup_sim()`, start-up replies drained and discarded.
    pub fn std() -> Sim {
        let mut s = Sim::new(&setup_sim(), &rd_sim());
        s.drain(0);
        s.out.clear();
        s
    }

    pub fn with(setup: &SimSetup, rd: &RefData) -> Sim {
        let mut s = Sim::new(setup, rd);
        s.drain(0);
        s.out.clear();
        s
    }

    pub fn feed(&mut self, t: u64, m: MarketEventRef<'_>) {
        self.ex.on_record(&rhdr(t), SimInput::Market(m));
    }

    pub fn bbo_ex(&mut self, t: u64, ex: i64, inst: InstId, bid: (&str, &str), ask: (&str, &str)) {
        let b = Bbo {
            h: mhdr(venue_of(inst), inst, ex),
            bid_px: if bid.0.is_empty() { Px(0) } else { px(bid.0) },
            bid_qty: if bid.1.is_empty() { Qty(0) } else { q(bid.1) },
            ask_px: if ask.0.is_empty() { Px(0) } else { px(ask.0) },
            ask_qty: if ask.1.is_empty() { Qty(0) } else { q(ask.1) },
            update_id: 0,
        };
        self.feed(t, MarketEventRef::Bbo(&b));
    }

    pub fn bbo(&mut self, t: u64, inst: InstId, bid: (&str, &str), ask: (&str, &str)) {
        self.bbo_ex(t, t as i64, inst, bid, ask);
    }

    pub fn topn_ex(&mut self, t: u64, ex: i64, inst: InstId, asks: &[(&str, &str)], bids: &[(&str, &str)]) {
        self.snap_id += 1;
        let bids: Vec<Level> = bids.iter().map(|(p, x)| Level { px: px(p), qty: q(x) }).collect();
        let asks: Vec<Level> = asks.iter().map(|(p, x)| Level { px: px(p), qty: q(x) }).collect();
        let hdr = L2SnapHdr {
            h: mhdr(venue_of(inst), inst, ex),
            snap_id: self.snap_id,
            origin: L2SnapHdr::TOP_N,
            _pad: [0; 3],
            n_bids: bids.len() as u16,
            n_asks: asks.len() as u16,
        };
        let part = L2SnapshotPart {
            hdr,
            bids: &bids,
            asks: &asks,
            part: 0,
            parts: 1,
        };
        self.feed(t, MarketEventRef::L2Snapshot(part));
    }

    pub fn topn(&mut self, t: u64, inst: InstId, asks: &[(&str, &str)], bids: &[(&str, &str)]) {
        self.topn_ex(t, t as i64, inst, asks, bids);
    }

    /// Incremental-depth exchange snapshot (`origin = Exchange`, `snap_id` given).
    pub fn exch_snapshot(
        &mut self,
        t: u64,
        inst: InstId,
        snap_id: u64,
        asks: &[(&str, &str)],
        bids: &[(&str, &str)],
    ) {
        let bids: Vec<Level> = bids.iter().map(|(p, x)| Level { px: px(p), qty: q(x) }).collect();
        let asks: Vec<Level> = asks.iter().map(|(p, x)| Level { px: px(p), qty: q(x) }).collect();
        let hdr = L2SnapHdr {
            h: mhdr(venue_of(inst), inst, t as i64),
            snap_id,
            origin: L2SnapHdr::EXCHANGE,
            _pad: [0; 3],
            n_bids: bids.len() as u16,
            n_asks: asks.len() as u16,
        };
        let part = L2SnapshotPart {
            hdr,
            bids: &bids,
            asks: &asks,
            part: 0,
            parts: 1,
        };
        self.feed(t, MarketEventRef::L2Snapshot(part));
    }

    pub fn l2batch(&mut self, t: u64, inst: InstId, prev_id: u64, last_id: u64, asks: &[(&str, &str)]) {
        let levels: Vec<Level> = asks.iter().map(|(p, x)| Level { px: px(p), qty: q(x) }).collect();
        let hdr = L2BatchHdr {
            h: mhdr(venue_of(inst), inst, t as i64),
            prev_id,
            last_id,
            n_bids: 0,
            n_asks: levels.len() as u16,
            bflags: 0,
            _pad: 0,
        };
        self.feed(t, MarketEventRef::L2Batch(L2BatchView { hdr, levels: &levels }));
    }

    pub fn trade(&mut self, t: u64, inst: InstId, p: &str, x: &str) {
        let tr = Trade {
            h: mhdr(venue_of(inst), inst, t as i64),
            px: px(p),
            qty: q(x),
            trade_id: 1,
            taker_side: Side::Buy,
            _pad: [0; 7],
        };
        self.feed(t, MarketEventRef::Trade(&tr));
    }

    pub fn mark_ex(&mut self, t: u64, ex: i64, inst: InstId, p: Px) {
        let m = MarkPrice {
            h: mhdr(venue_of(inst), inst, ex),
            px: p,
        };
        self.feed(t, MarketEventRef::Mark(&m));
    }

    pub fn mark(&mut self, t: u64, inst: InstId, p: &str) {
        self.mark_ex(t, t as i64, inst, px(p));
    }

    pub fn funding_ex(&mut self, t: u64, ex: i64, inst: InstId, r: &str, next: i64) {
        let f = Funding {
            h: mhdr(venue_of(inst), inst, ex),
            rate: rate(r),
            next_funding_ns: next,
            interval_s: 0,
            _pad: 0,
        };
        self.feed(t, MarketEventRef::Funding(&f));
    }

    pub fn clock(&mut self, t: u64) {
        self.ex.on_record(&rhdr(t), SimInput::Clock);
    }

    pub fn advance(&mut self, t: u64) {
        self.ex.advance(Tsc(t));
    }

    pub fn drain(&mut self, now: u64) {
        let out = &mut self.out;
        self.ex.drain_due(Tsc(now), |r| {
            out.push(Rep {
                account: r.account,
                due: r.due_tsc.0,
                msg: r.msg,
                bulk: r.bulk.to_vec(),
            })
        });
    }

    /// Advance to `t` and drain everything due by then.
    pub fn run_to(&mut self, t: u64) {
        self.advance(t);
        self.drain(t);
    }

    pub fn next_slot(&mut self, a: AccountId) -> SlotId {
        let s = &mut self.slots[usize::from(a.0)];
        *s += 1;
        SlotId(*s)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn place_flags(
        &mut self,
        a: AccountId,
        inst: InstId,
        side: Side,
        qty: Qty,
        p: Px,
        tif: Tif,
        oflags: u16,
        decided: u64,
    ) -> SlotId {
        let slot = self.next_slot(a);
        let c = GwCmd {
            decision_seq: u64::from(slot.0),
            decided_tsc: Tsc(decided),
            px: p,
            qty,
            inst,
            slot,
            r#gen: 1,
            target_slot: SlotId(0),
            op: GwOp::Place,
            account: a,
            side,
            tif,
            oflags,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        };
        self.ex.on_command(&c, Tsc(decided));
        slot
    }

    /// `Place(inst, side, qty, px, tif) decided @t` with decimal strings.
    pub fn place(
        &mut self,
        a: AccountId,
        inst: InstId,
        side: Side,
        qty: &str,
        p: &str,
        tif: Tif,
        decided: u64,
    ) -> SlotId {
        self.place_flags(a, inst, side, q(qty), px(p), tif, 0, decided)
    }

    /// Non-place order op: its own slot is the target's (no new slot is consumed).
    pub fn op(
        &mut self,
        a: AccountId,
        op: GwOp,
        inst: InstId,
        target: SlotId,
        qty: Qty,
        p: Px,
        decided: u64,
    ) {
        let slot = target;
        let c = GwCmd {
            decision_seq: 0,
            decided_tsc: Tsc(decided),
            px: p,
            qty,
            inst,
            slot,
            r#gen: 1,
            target_slot: target,
            op,
            account: a,
            side: Side::Buy,
            tif: Tif::Gtc,
            oflags: 0,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        };
        self.ex.on_command(&c, Tsc(decided));
    }

    pub fn cancel(&mut self, a: AccountId, inst: InstId, target: SlotId, decided: u64) {
        self.op(a, GwOp::Cancel, inst, target, Qty(0), Px(0), decided);
    }

    pub fn query(&mut self, a: AccountId, inst: InstId, target: SlotId, decided: u64) {
        self.op(a, GwOp::Query, inst, target, Qty(0), Px(0), decided);
    }

    pub fn amend(&mut self, a: AccountId, inst: InstId, target: SlotId, p: &str, x: &str, decided: u64) {
        self.op(a, GwOp::Amend, inst, target, q(x), px(p), decided);
    }

    pub fn cancel_all(&mut self, a: AccountId, inst: InstId, decided: u64) {
        self.op(a, GwOp::CancelAll, inst, SlotId(0), Qty(0), Px(0), decided);
    }

    pub fn seed(&mut self, a: AccountId, s: &SimSeedState) {
        if let Err(e) = self.ex.seed_account_from(a, s) {
            panic!("seed: {e:?}");
        }
    }

    // ---- reply filters ----

    pub fn order_events(&self, a: AccountId) -> Vec<(u64, OrderEvent)> {
        self.out
            .iter()
            .filter(|r| r.account == a)
            .filter_map(|r| match r.msg {
                RepMsg::OrderEvent(e, _) => Some((r.due, e)),
                _ => None,
            })
            .collect()
    }

    pub fn events_of(&self, a: AccountId, slot: SlotId) -> Vec<OrderEvent> {
        self.order_events(a)
            .into_iter()
            .map(|(_, e)| e)
            .filter(|e| e.slot == slot)
            .collect()
    }

    pub fn fills_of(&self, a: AccountId, slot: SlotId) -> Vec<(Px, Qty)> {
        self.events_of(a, slot)
            .into_iter()
            .filter(|e| matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill))
            .map(|e| (e.px, e.qty))
            .collect()
    }

    pub fn whats(&self, a: AccountId, slot: SlotId) -> Vec<OrderEventKind> {
        self.events_of(a, slot).into_iter().map(|e| e.what).collect()
    }

    pub fn total(&self, a: AccountId, v: VenueId, asset: AssetId) -> i64 {
        self.ex.account(a).map_or(0, |x| x.balance(v, asset).total.0)
    }

    pub fn view(&self, a: AccountId) -> hft_sim::SimAccountView<'_> {
        match self.ex.account(a) {
            Some(v) => v,
            None => panic!("no account {a:?}"),
        }
    }
}

/// `seed9(P2 USDT wallet; positions…)`: only the listed balance and positions (TESTS.md "未列出的部分为空").
pub fn seed9_state(wallet: &str, positions: &[(InstId, &str, &str)]) -> SimSeedState {
    SimSeedState {
        balances: vec![(P2, USDT, usdt(wallet))],
        positions: positions.iter().map(|(i, x, p)| (*i, q(x), px(p))).collect(),
        orders: Vec::new(),
        funding: Vec::new(),
    }
}
