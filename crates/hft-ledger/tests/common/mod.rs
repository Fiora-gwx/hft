//! Shared fixtures of the hft-ledger tests (TESTS.md "夹具": `rd_lf`, `cfg_lf`, `boot_a1`, groups
//! `G1`/`G3`/`G4`, the `F(..)` fill notation and decimal → raw helpers).
#![allow(dead_code, clippy::too_many_arguments)]

use hft_event::{
    BalanceReport, ExternalCause, ExternalFill, FundingSettle, GroupSpec, LegDef, MarginItem, MarginReportHdr,
};
use hft_ledger::{
    AssetName, Capacities, FeeDelta, FillClass, FillDelta, FixedVec, Ledger, LedgerConfig, MarginReportView,
};
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, Exchange, FeeAssetRule, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, MarginData, MarginMode, PositionMode,
    RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TsUnit, VenueCaps, VenueSpec,
};
use hft_strategy_sdk::GroupTable;
use hft_types::{
    ASSET_NONE, AccountId, AssetExp, AssetId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, Side,
    StrategyId, Tsc, VenueId,
};
use zerocopy::{FromZeros, IntoBytes};

#[allow(unused_imports)]
pub use hft_types::Side::{Buy, Sell};

pub mod journal;
pub mod opgen;

// ---------------------------------------------------------------- ids

pub const USDT: AssetId = AssetId(1);
pub const USDC: AssetId = AssetId(2);
pub const BTC: AssetId = AssetId(3);
pub const ETH: AssetId = AssetId(4);
pub const BNB: AssetId = AssetId(5);

/// Spot venue on exchange E1 (`VenueId::BINANCE_SPOT`: refdata ties a VenueId to its market).
pub const S1: VenueId = VenueId::BINANCE_SPOT;
/// USDT perpetual venue on exchange E2.
pub const P2: VenueId = VenueId::OKX_SWAP;
/// Spot venue on exchange E3.
pub const S3: VenueId = VenueId::GATE_SPOT;

pub const BTC_S1: InstId = InstId(11);
pub const BNB_S1: InstId = InstId(12);
pub const USDC_S1: InstId = InstId(13);
pub const BTC_P2: InstId = InstId(21);
pub const ETH_P2: InstId = InstId(22);
pub const BTCUSDC_S3: InstId = InstId(32);
/// Filler inst with status `Halted`.
pub const HALTED: InstId = InstId(1);

pub const A1: AccountId = AccountId(1);
pub const A2: AccountId = AccountId(2);
pub const A9: AccountId = AccountId(3);

/// Strategy instances bound to `A1`, `A2`, `A9`.
pub const I1: StrategyId = StrategyId(1);
pub const I2: StrategyId = StrategyId(2);
pub const I9: StrategyId = StrategyId(3);

pub const fn gid(inst: StrategyId, n: u32) -> GroupId {
    GroupId((inst.0 as u32) << 24 | n)
}
pub const G1: GroupId = gid(I1, 1);
pub const G3: GroupId = gid(I1, 3);
pub const G4: GroupId = gid(I1, 4);
pub const G5: GroupId = gid(I1, 5);
pub const G7: GroupId = gid(I2, 7);

/// `D` = 2026-10-01T00:00:00Z in UTC ns.
pub const D: i64 = 1_790_812_800_000_000_000;
pub const H: i64 = 3_600_000_000_000;
pub const T1: i64 = D + 8 * H;
pub const T2: i64 = D + 16 * H;

/// Seconds → engine ns.
pub const fn s(x: i64) -> i64 {
    x * 1_000_000_000
}

// ---------------------------------------------------------------- decimals

/// Parses a decimal literal (`"-0.004"`, `"60 000"`, `"1e-3"` not supported) to raw at `exp`.
pub fn dec(v: &str, exp: i32) -> i64 {
    let t: String = v.chars().filter(|c| *c != ' ' && *c != '_').collect();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.to_string()),
        None => (false, t.strip_prefix('+').unwrap_or(&t).to_string()),
    };
    let (ip, fp) = t.split_once('.').unwrap_or((&t, ""));
    let digits = (-exp).max(0) as usize;
    assert!(
        fp.len() <= digits || fp[digits..].chars().all(|c| c == '0'),
        "{v} not exact at 1e{exp}"
    );
    let mut f: String = fp.chars().take(digits).collect();
    while f.len() < digits {
        f.push('0');
    }
    let whole: i128 = format!("{ip}{f}").parse().unwrap_or(0);
    let scaled = if exp > 0 {
        whole / 10i128.pow(exp as u32)
    } else {
        whole
    };
    let r = i64::try_from(scaled).expect("decimal overflow");
    if neg { -r } else { r }
}

/// Price (every asset of `rd_lf` has `px_exp = −4`).
pub fn px(v: &str) -> Px {
    Px(dec(v, -4))
}

pub fn qty_exp(a: AssetId) -> i32 {
    if a == USDT || a == USDC { -6 } else { -8 }
}

/// Quantity of asset `a`.
pub fn qa(a: AssetId, v: &str) -> Qty {
    Qty(dec(v, qty_exp(a)))
}
pub fn btc(v: &str) -> Qty {
    qa(BTC, v)
}
pub fn usdt(v: &str) -> Qty {
    qa(USDT, v)
}
/// USDT notional (1e-6).
pub fn n(v: &str) -> Notional {
    Notional(dec(v, -6))
}
/// Rate (1e-9).
pub fn rate(v: &str) -> Rate {
    Rate(dec(v, -9))
}
/// `cost` / `realized_quote` raw (exponent −12 for every inst of `rd_lf`).
pub fn cq(v: &str) -> i128 {
    i128::from(dec(v, -6)) * 1_000_000
}

// ---------------------------------------------------------------- refdata

/// Case-specific knobs of `rd_lf`.
#[derive(Clone, Debug)]
pub struct RdOpts {
    pub s1_fee: FeeAssetRule,
    pub s3_fee: FeeAssetRule,
    pub p2_fee: FeeAssetRule,
    pub p2_funding: FundingSettleKind,
    /// Extra assets listed on `S1` (each with one spot inst), for capacity cases.
    pub s1_extra_assets: u16,
    pub btc_p2_interval_s: u32,
    pub eth_p2_interval_s: u32,
    /// `funding_anchor_s` of both perpetual insts.
    pub p2_anchor_s: u32,
}

impl Default for RdOpts {
    fn default() -> Self {
        RdOpts {
            s1_fee: FeeAssetRule::Quote,
            s3_fee: FeeAssetRule::Quote,
            p2_fee: FeeAssetRule::Quote,
            p2_funding: FundingSettleKind::PerSymbolPush,
            s1_extra_assets: 0,
            btc_p2_interval_s: 28_800,
            eth_p2_interval_s: 28_800,
            p2_anchor_s: 0,
        }
    }
}

/// The default `rd_lf` (built once per test binary).
pub fn rd_lf() -> &'static RefData {
    static RD: std::sync::OnceLock<&'static RefData> = std::sync::OnceLock::new();
    RD.get_or_init(|| rd_with(&RdOpts::default()))
}

pub fn rd_fee(s1_fee: FeeAssetRule) -> &'static RefData {
    rd_with(&RdOpts {
        s1_fee,
        ..RdOpts::default()
    })
}

pub fn rd_funding(kind: FundingSettleKind) -> &'static RefData {
    rd_with(&RdOpts {
        p2_funding: kind,
        ..RdOpts::default()
    })
}

fn venue_spec(id: VenueId, exchange: Exchange, product: InstKind) -> VenueSpec {
    VenueSpec {
        id,
        exchange,
        product,
        ws_max_streams_per_conn: 50,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "lf-".into(),
        max_batch: 20,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    }
}

fn spot_caps(v: VenueId, fee: FeeAssetRule) -> VenueCaps {
    let mut c = VenueCaps::none(v);
    c.fee_asset_rule = fee;
    c
}

fn perp_caps(v: VenueId, fee: FeeAssetRule, funding: FundingSettleKind) -> VenueCaps {
    let mut c = VenueCaps::none(v);
    c.market = MarketKind::LinearPerp;
    c.fee_asset_rule = fee;
    c.required_position_mode = PositionMode::OneWay;
    c.required_margin_mode = MarginMode::Cross;
    c.funding_settle = funding;
    c.margin_data = MarginData::Poll { interval_ms: 2_000 };
    c.reduce_only = true;
    c
}

struct I {
    id: u32,
    venue: VenueId,
    sym: String,
    kind: InstKind,
    asset: AssetId,
    quote: AssetId,
    tick: i64,
    lot: i64,
    interval: u32,
}

/// Decimal string of a positive raw value at `exp` ("0.01").
fn raw_str(raw: i64, exp: i32) -> String {
    let d = (-exp) as usize;
    let s = format!("{raw:0>w$}", w = d + 1);
    let (i, f) = s.split_at(s.len() - d);
    let f = f.trim_end_matches('0');
    if f.is_empty() {
        i.to_string()
    } else {
        format!("{i}.{f}")
    }
}

/// Builds `rd_lf` with `o` (leaked, as hft-core does at startup).
pub fn rd_with(o: &RdOpts) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(D);
    let mut assets = vec![
        (
            USDT,
            "USDT".to_string(),
            AssetClass::Stable,
            -6i8,
            dec("0.01", -6),
        ),
        (USDC, "USDC".to_string(), AssetClass::Stable, -6, dec("0.01", -6)),
        (BTC, "BTC".to_string(), AssetClass::Crypto, -8, dec("0.00001", -8)),
        (ETH, "ETH".to_string(), AssetClass::Crypto, -8, dec("0.00001", -8)),
        (BNB, "BNB".to_string(), AssetClass::Crypto, -8, dec("0.0001", -8)),
    ];
    for k in 0..o.s1_extra_assets {
        assets.push((AssetId(6 + k), format!("X{k}"), AssetClass::Crypto, -8, 1));
    }
    for (id, name, class, qe, dust) in &assets {
        b.asset(AssetSpec {
            id: *id,
            name: name.as_str().into(),
            class: *class,
            exp: AssetExp {
                px_exp: -4,
                qty_exp: *qe,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(*dust),
        });
    }
    b.venue(venue_spec(S1, Exchange::Binance, InstKind::Spot));
    b.venue(venue_spec(S3, Exchange::Gate, InstKind::Spot));
    b.venue(venue_spec(P2, Exchange::Okx, InstKind::Perp));
    b.caps(spot_caps(S1, o.s1_fee));
    b.caps(spot_caps(S3, o.s3_fee));
    b.caps(perp_caps(P2, o.p2_fee, o.p2_funding));
    let named = [
        I {
            id: 11,
            venue: S1,
            sym: "BTCUSDT".into(),
            kind: InstKind::Spot,
            asset: BTC,
            quote: USDT,
            tick: 100,
            lot: 1_000,
            interval: 0,
        },
        I {
            id: 12,
            venue: S1,
            sym: "BNBUSDT".into(),
            kind: InstKind::Spot,
            asset: BNB,
            quote: USDT,
            tick: 100,
            lot: 1_000,
            interval: 0,
        },
        I {
            id: 13,
            venue: S1,
            sym: "USDCUSDT".into(),
            kind: InstKind::Spot,
            asset: USDC,
            quote: USDT,
            tick: 1,
            lot: 10_000,
            interval: 0,
        },
        I {
            id: 21,
            venue: P2,
            sym: "BTC-USDT-SWAP".into(),
            kind: InstKind::Perp,
            asset: BTC,
            quote: USDT,
            tick: 100,
            lot: 100_000,
            interval: o.btc_p2_interval_s,
        },
        I {
            id: 22,
            venue: P2,
            sym: "ETH-USDT-SWAP".into(),
            kind: InstKind::Perp,
            asset: ETH,
            quote: USDT,
            tick: 100,
            lot: 100_000,
            interval: o.eth_p2_interval_s,
        },
        I {
            id: 32,
            venue: S3,
            sym: "BTC_USDC".into(),
            kind: InstKind::Spot,
            asset: BTC,
            quote: USDC,
            tick: 100,
            lot: 1_000,
            interval: 0,
        },
    ];
    // Inst ids are dense from 1: fillers are delisted S3 spot insts on BTC/USDT (`F1` is Halted).
    let mut next_extra = 0u16;
    let total = 32 + u32::from(o.s1_extra_assets);
    for id in 1..=total {
        let spec = if let Some(x) = named.iter().find(|x| x.id == id) {
            (
                x.venue,
                x.sym.clone(),
                x.kind,
                x.asset,
                x.quote,
                x.tick,
                x.lot,
                x.interval,
                InstStatus::Trading,
            )
        } else if id > 32 {
            let a = AssetId(6 + next_extra);
            next_extra += 1;
            (
                S1,
                format!("X{}USDT", a.0),
                InstKind::Spot,
                a,
                USDT,
                100,
                1_000,
                0,
                InstStatus::Trading,
            )
        } else {
            let st = if id == 1 {
                InstStatus::Halted
            } else {
                InstStatus::Delisted
            };
            (S3, format!("F{id}"), InstKind::Spot, BTC, USDT, 100, 1_000, 0, st)
        };
        let (venue, symbol, kind, asset, quote, tick, lot, funding_interval_s, status) = spec;
        b.inst(InstSpec {
            id: InstId(id),
            venue,
            symbol,
            status,
            kind,
            asset,
            quote,
            tick,
            lot,
            min_qty: Qty(lot),
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 64,
            l2_depth: 20,
            crossed_tolerance: 0,
            tick_str: raw_str(tick, -4),
            lot_str: raw_str(lot, qty_exp(asset)),
            min_qty_str: raw_str(lot, qty_exp(asset)),
            funding_interval_s,
            funding_anchor_s: if kind == InstKind::Perp { o.p2_anchor_s } else { 0 },
            max_leverage: if kind == InstKind::Perp { 20 } else { 0 },
            ..InstSpec::default()
        });
    }
    for v in [S1, S3, P2] {
        b.fee(FeeSpec {
            venue: v,
            tier: 0,
            fee: FeePair {
                maker: Rate(200_000),
                taker: Rate(500_000),
            },
        });
    }
    for (id, name) in [(I1, "lf-a1"), (I2, "lf-a2"), (I9, "lf-sim")] {
        b.instance(InstanceSpec {
            id,
            kind_id: 0xFF00,
            name: name.into(),
        });
    }
    for (id, inst, venues, sim) in [
        (A1, I1, (1u64 << S1.0) | (1 << P2.0), false),
        (A2, I2, (1u64 << S3.0) | (1 << P2.0), false),
        (A9, I9, (1u64 << S1.0) | (1 << P2.0), true),
    ] {
        b.account(AccountSpec {
            id,
            sim,
            bound_instance: inst,
            label: format!("acc{}", id.0),
            venues,
            ..AccountSpec::default()
        });
        if !sim {
            for v in [S1, S3, P2] {
                if venues & (1 << v.0) != 0 {
                    b.account_venue(AccountVenueSpec {
                        account: id,
                        venue: v,
                        uid: format!("{}{}", id.0, v.0),
                    });
                }
            }
        }
    }
    b.sim(SimSpec {
        account: A9,
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    });
    for v in [S1, P2] {
        b.sim_balance(SimBalanceSpec {
            account: A9,
            venue: v,
            asset: USDT,
            qty: usdt("10000"),
        });
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => Box::leak(Box::new(r)),
        Err(e) => panic!("rd_lf rejected: {e:?}"),
    }
}

// ---------------------------------------------------------------- config

/// `cfg_lf()`: every field set explicitly (TESTS.md "夹具").
pub fn cfg_lf() -> LedgerConfig {
    let mut c = LedgerConfig::default();
    let mut par = FixedVec::new();
    par.push(AssetName::new("USDT"));
    let mut fb = FixedVec::new();
    fb.push(AssetName::new("USDC"));
    c.ref_inst = FixedVec::new();
    c.par_assets = par;
    c.stable_par_fallback = fb;
    c.rate_max_age_ms = 60_000;
    c.valuation = hft_ledger::Valuation {
        spot: hft_ledger::SpotVal::Bid,
        perp: hft_ledger::PerpVal::Mark,
        include_baseline: false,
        include_unassigned: true,
    };
    c.round_dust_notional = n("5");
    c.balance_check_quiet_ms = 2_000;
    c.funding_tol_abs = n("0.05");
    c.funding_tol_rel = rate("0.005");
    c.funding_recon_lag_ms = 600_000;
    c.funding_recon_window_ms = 172_800_000;
    c.funding_hist_cap = 4_096;
    c.funding_push_seen_cap = 64;
    c.ext_dedup_cap = 1_024;
    c
}

pub fn caps_lf() -> Capacities {
    Capacities {
        max_insts: 4_096,
        max_cells_total: 64,
        max_timers: 256,
        recent_cmd_ids: 64,
        max_subs_per_inst: 4,
        max_groups_per_instance: 16,
        max_instances: 8,
    }
}

// ---------------------------------------------------------------- groups

pub fn leg(inst: InstId, venue: VenueId) -> LegDef {
    LegDef {
        inst,
        venue,
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }
}

pub fn spec(id: GroupId, asset: AssetId, legs: &[(InstId, VenueId)]) -> GroupSpec {
    let mut g = GroupSpec::new_zeroed();
    g.id = id;
    g.asset = asset;
    g.n_legs = legs.len() as u8;
    g.open_allowed = 1;
    for (k, &(i, v)) in legs.iter().enumerate() {
        g.legs[k] = leg(i, v);
    }
    g
}

pub fn g1() -> GroupSpec {
    spec(G1, BTC, &[(BTC_S1, S1), (BTC_P2, P2)])
}
pub fn g3() -> GroupSpec {
    spec(G3, ETH, &[(ETH_P2, P2)])
}
pub fn g4() -> GroupSpec {
    spec(G4, BNB, &[(BNB_S1, S1)])
}

pub fn table(specs: &[GroupSpec]) -> GroupTable {
    GroupTable::new(specs, 16)
}

/// `A1`'s groups `G1`, `G3` (+ `extra`).
pub fn set_groups(l: &mut Ledger, extra: &[GroupSpec]) {
    let mut v = vec![g1(), g3()];
    v.extend_from_slice(extra);
    l.on_groups_changed(I1, &table(&v));
}

// ---------------------------------------------------------------- ledgers

pub fn ledger_with(rd: &RefData, cfg: &LedgerConfig) -> Ledger {
    let mut l = Ledger::new(rd, cfg, &caps_lf()).expect("Ledger::new");
    set_groups(&mut l, &[]);
    l
}

/// `rd_lf`, `cfg_lf`, groups `G1`/`G3`, nothing booked.
pub fn ledger_lf() -> Ledger {
    ledger_with(rd_lf(), &cfg_lf())
}

pub fn bal(a: AccountId, v: VenueId, asset: AssetId, free: Qty, locked: Qty, ex_ts_ns: i64) -> BalanceReport {
    let mut b = BalanceReport::new_zeroed();
    b.account = a;
    b.venue = v;
    b.asset = asset;
    b.free = free;
    b.locked = locked;
    b.ex_ts_ns = ex_ts_ns;
    b.rx_tsc = Tsc(0);
    b
}

/// `boot_a1(t0)` with `A1/S1` BNB initialized to `bnb`.
pub fn boot_a1_bnb(l: &mut Ledger, t0: i64, bnb: &str) {
    boot_a1_init(l, t0, "0", bnb);
}

/// `boot_a1(t0)` with `A1/S1` BTC and BNB initialized to `btc`, `bnb`.
pub fn boot_a1_init(l: &mut Ledger, t0: i64, btc: &str, bnb: &str) {
    l.on_run_start(t0);
    for (v, a, free) in [
        (S1, USDT, usdt("100000")),
        (S1, BTC, self::btc(btc)),
        (S1, BNB, qa(BNB, bnb)),
        (P2, USDT, usdt("50000")),
    ] {
        l.on_balance(&bal(A1, v, a, free, Qty(0), D), false, t0);
    }
    l.on_ref_px(BTC_S1, px("59999.99"), px("60000.01"), t0);
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), t0);
    l.on_ref_px(USDC_S1, px("0.9999"), px("1.0001"), t0);
}

/// `boot_a1(t0)`.
pub fn boot_a1(l: &mut Ledger, t0: i64) {
    boot_a1_bnb(l, t0, "10");
}

/// `A2` initialized: `S3` USDC `usdc`, `P2` USDT `p2_usdt`.
pub fn boot_a2(l: &mut Ledger, t: i64, usdc: &str, p2_usdt: &str) {
    l.on_balance(&bal(A2, S3, USDC, qa(USDC, usdc), Qty(0), D), false, t);
    l.on_balance(&bal(A2, S3, BTC, Qty(0), Qty(0), D), false, t);
    l.on_balance(&bal(A2, P2, USDT, usdt(p2_usdt), Qty(0), D), false, t);
}

/// `ledger_lf()` + `boot_a1(0 s)`.
pub fn booted() -> Ledger {
    let mut l = ledger_lf();
    boot_a1(&mut l, 0);
    l
}

pub fn booted_with(rd: &RefData) -> Ledger {
    let mut l = ledger_with(rd, &cfg_lf());
    boot_a1(&mut l, 0);
    l
}

// ---------------------------------------------------------------- events

pub fn venue_of(i: InstId) -> VenueId {
    match i.0 {
        11..=13 => S1,
        21 | 22 => P2,
        32 => S3,
        _ => S1,
    }
}

pub fn base_of(i: InstId) -> AssetId {
    match i {
        BTC_S1 | BTC_P2 | BTCUSDC_S3 => BTC,
        BNB_S1 => BNB,
        USDC_S1 => USDC,
        ETH_P2 => ETH,
        _ => BTC,
    }
}

pub fn is_perp(i: InstId) -> bool {
    venue_of(i) == P2
}

/// Net quantity by the README rule.
pub fn net_qty(side: Side, inst: InstId, q: Qty, fee: i64, fee_asset: AssetId) -> Qty {
    if is_perp(inst) {
        return q;
    }
    let fb = if fee_asset == base_of(inst) { fee } else { 0 };
    match side {
        Side::Buy => Qty(q.0 - fb),
        Side::Sell => Qty(q.0 + fb),
    }
}

/// `F(side, inst, qty, px, fee, fee_asset, group, leg)` of account `A1`; `fee` is a decimal of
/// `fee_asset` (`ASSET_NONE` → USDT exponent, value ignored).
pub fn f(
    side: Side,
    inst: InstId,
    q: &str,
    p: &str,
    fee: &str,
    fee_asset: AssetId,
    group: GroupId,
    leg_ix: u8,
) -> FillDelta {
    fa(A1, side, inst, q, p, fee, fee_asset, group, leg_ix)
}

pub fn fa(
    a: AccountId,
    side: Side,
    inst: InstId,
    q: &str,
    p: &str,
    fee: &str,
    fee_asset: AssetId,
    group: GroupId,
    leg_ix: u8,
) -> FillDelta {
    let qty = qa(base_of(inst), q);
    let fee_raw = if fee_asset == ASSET_NONE {
        0
    } else {
        dec(fee, qty_exp(fee_asset))
    };
    FillDelta {
        account: a,
        venue: venue_of(inst),
        side,
        class: FillClass::Open as u8,
        flags: 0,
        leg_ix,
        _pad: [0; 2],
        inst,
        group,
        qty,
        net_qty: net_qty(side, inst, qty, fee_raw, fee_asset),
        px: px(p),
        fee: fee_raw,
        fee_asset,
        _pad2: 0,
        slot: 0,
        r#gen: 0,
        _pad3: 0,
        seq: 0,
    }
}

pub fn fee_delta(side: Side, inst: InstId, group: GroupId, fee: &str, fee_asset: AssetId) -> FeeDelta {
    FeeDelta {
        account: A1,
        venue: venue_of(inst),
        inst,
        group,
        side,
        fee: dec(fee, qty_exp(fee_asset)),
        fee_asset,
        seq: 0,
    }
}

/// `ExternalFill` on `A1` with zeroed ids.
pub fn ext(cause: ExternalCause, side: Side, inst: InstId, q: &str, p: &str) -> ExternalFill {
    ExternalFill {
        venue: venue_of(inst),
        account: A1,
        side,
        cause,
        inst,
        qty: qa(base_of(inst), q),
        px: px(p),
        fee: Qty(0),
        fee_asset: ASSET_NONE,
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 0,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        fill_seq: 0,
    }
}

pub fn ext_tid(
    cause: ExternalCause,
    side: Side,
    inst: InstId,
    q: &str,
    p: &str,
    trade_id: u64,
) -> ExternalFill {
    let mut e = ext(cause, side, inst, q, p);
    e.trade_id = trade_id;
    e
}

/// `FundingSettle { A1, P2, inst, amount, synth, settle_ts }`.
pub fn fs(inst: InstId, amount: &str, settle_ts: i64, synth: u8) -> FundingSettle {
    let mut x = FundingSettle::new_zeroed();
    x.account = A1;
    x.venue = P2;
    x.inst = inst;
    x.amount = n(amount);
    x.settle_ts_ns = settle_ts;
    x.synth = synth;
    x
}

/// `Push(inst, amount, settle_ts)`.
pub fn push(inst: InstId, amount: &str, settle_ts: i64) -> FundingSettle {
    fs(inst, amount, settle_ts, 0)
}

/// Encoded `MarginReport` items (16 B each).
pub fn margin_items(items: &[(InstId, Px, u8)]) -> Vec<u8> {
    let mut v = Vec::new();
    for &(inst, liq_px, adl) in items {
        let mut it = MarginItem::new_zeroed();
        it.inst = inst;
        it.liq_px = liq_px;
        it.adl_quantile = adl;
        v.extend_from_slice(it.as_bytes());
    }
    v
}

pub fn margin_hdr(
    a: AccountId,
    v: VenueId,
    mb: &str,
    maint: &str,
    avail: &str,
    ratio: &str,
    n: usize,
) -> MarginReportHdr {
    let mut h = MarginReportHdr::new_zeroed();
    h.account = a;
    h.venue = v;
    h.n = n as u16;
    h.margin_balance = self::n(mb);
    h.maint_margin = self::n(maint);
    h.avail_margin = self::n(avail);
    h.margin_ratio = rate(ratio);
    h
}

pub fn on_margin(l: &mut Ledger, h: MarginReportHdr, items: &[u8], t: i64) {
    l.on_margin(&MarginReportView { hdr: h, items }, t);
}

pub fn mark(l: &mut Ledger, inst: InstId, p: &str, t: i64) -> Vec<(AccountId, VenueId)> {
    let mut touched = FixedVec::new();
    l.on_mark(inst, px(p), t, &mut touched);
    touched.iter().copied().collect()
}

/// Notes of `code` currently buffered.
pub fn notes_of(l: &Ledger, code: u32) -> Vec<hft_event::Note> {
    l.notes().iter().filter(|n| n.code == code).copied().collect()
}

pub fn cash(l: &Ledger, a: AccountId, v: VenueId, asset: AssetId) -> hft_ledger::CashEntry {
    l.venue_book(a, v)
        .cash
        .iter()
        .find(|e| e.asset == asset)
        .copied()
        .unwrap_or_else(|| panic!("no cash entry {asset:?} on {v:?}"))
}

// ---------------------------------------------------------------- snapshots

/// Sections 1, 13 and 22 written by `l`.
pub fn snap(l: &Ledger) -> Vec<u8> {
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    l.snapshot(&mut w);
    assert!(w.status().is_ok(), "snapshot overflow");
    w.as_bytes().to_vec()
}

pub fn restore_with(bytes: &[u8], rd: &RefData) -> Result<Ledger, hft_strategy_sdk::RestoreError> {
    let mut r = hft_strategy_sdk::SnapshotReader::new(bytes);
    Ledger::restore(&mut r, rd, &cfg_lf(), &caps_lf())
}

pub fn restore(bytes: &[u8]) -> Result<Ledger, hft_strategy_sdk::RestoreError> {
    restore_with(bytes, rd_lf())
}

/// Body of section `sec` in `bytes`.
pub fn section(bytes: &[u8], sec: u16) -> hft_strategy_sdk::SectionView<'_> {
    hft_strategy_sdk::SnapshotReader::new(bytes)
        .find_section(sec)
        .expect("framing")
        .unwrap_or_else(|| panic!("no section {sec}"))
}

/// Byte offset of section `sec`'s 16 B header in `bytes`.
pub fn section_at(bytes: &[u8], sec: u16) -> usize {
    let mut at = 0;
    while at + 16 <= bytes.len() {
        let id = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let len = u32::from_le_bytes([bytes[at + 8], bytes[at + 9], bytes[at + 10], bytes[at + 11]]) as usize;
        if id == sec {
            return at;
        }
        at += 16 + len;
    }
    panic!("no section {sec}")
}

/// Rows of a section 13 or 22 body: (header, body bytes); Margin rows carry their items.
pub fn ledger_rows(
    sec: &hft_strategy_sdk::SectionView<'_>,
) -> Vec<(hft_ledger::snapshot::LedgerRowHdr, Vec<u8>)> {
    use hft_ledger::snapshot::{LedgerRowHdr, row_kind};
    let mut rr = hft_strategy_sdk::SnapshotReader::new(sec.body);
    let mut out = Vec::new();
    for _ in 0..sec.count {
        let h: LedgerRowHdr = rr.get_row().expect("row header");
        let len = match h.kind {
            row_kind::ACCOUNT => 88,
            row_kind::RATE | row_kind::FUND_PENDING | row_kind::FUND_HIST | row_kind::FUND_NO_HIST => 32,
            row_kind::GROUP => 552,
            row_kind::MARGIN => {
                let n_items = u16::from_le_bytes([sec.body[rr.pos() + 56], sec.body[rr.pos() + 57]]);
                64 + usize::from(n_items) * 16
            }
            row_kind::FUND_MARK | row_kind::FUND_SEEN | row_kind::AUTO_HELD | row_kind::INV => 16,
            row_kind::PX => 48,
            row_kind::CASH => 64,
            row_kind::EXT_SEEN => 24,
            k => panic!("unexpected row kind {k}"),
        };
        out.push((h, rr.get_bytes(len).expect("row body").to_vec()));
    }
    out
}

/// Snapshot equality with the first differing row named on failure.
pub fn assert_snap_eq(a: &Ledger, b: &Ledger) {
    use hft_ledger::snapshot::*;
    let (x, y) = (snap(a), snap(b));
    if x == y {
        return;
    }
    let s1x = section(&x, SECTION_POSITIONS);
    let s1y = section(&y, SECTION_POSITIONS);
    assert_eq!(s1x.body, s1y.body, "section 1 differs");
    for sec in [SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY] {
        let rx = ledger_rows(&section(&x, sec));
        let ry = ledger_rows(&section(&y, sec));
        for (k, (p, q)) in rx.iter().zip(ry.iter()).enumerate() {
            assert_eq!(p, q, "section {sec} row {k} (kind {}) differs", p.0.kind);
        }
        assert_eq!(rx.len(), ry.len(), "section {sec} row count differs");
    }
    panic!("snapshots differ outside rows");
}

/// A `JournalLedger` started from `l`'s snapshot (no OMS slot section).
pub fn cold_from(l: &Ledger) -> hft_ledger::JournalLedger {
    let bytes = snap(l);
    let mut r = hft_strategy_sdk::SnapshotReader::new(&bytes);
    hft_ledger::JournalLedger::from_snapshot(&mut r, rd_lf(), &cfg_lf(), &caps_lf()).expect("from_snapshot")
}

/// `bytes` with section 13 replaced by `rows` (sections 1 and 22 copied).
pub fn with_rows13(bytes: &[u8], rows: &[(hft_ledger::snapshot::LedgerRowHdr, Vec<u8>)]) -> Vec<u8> {
    use hft_ledger::snapshot::*;
    let mut w = hft_strategy_sdk::SnapshotWriter::new(64 << 20);
    for sec in [SECTION_POSITIONS, SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY] {
        let s = section(bytes, sec);
        w.begin_section(sec, s.version);
        if sec == SECTION_LEDGER_GROUPS {
            for (h, b) in rows {
                w.put_row(h);
                w.put_bytes(b);
            }
        } else {
            w.put_bytes(s.body);
            w.add_rows(s.count);
        }
        w.end_section();
    }
    w.as_bytes().to_vec()
}
