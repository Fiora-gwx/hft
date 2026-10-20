//! Test fixture of `binary-maker`: a binary venue `VB` with six outcome-token slot insts (`U0`, `D0`,
//! `U1`, `D1`, `U2`, `D2`: px_exp −3, qty_exp −6, quote `USDC`) and a reference spot venue `VR` with
//! one inst `R` (BTC/USDT, px_exp −2); account `A1` (Sim) bound to instance `S1` of this kind; the
//! group table is the six slots in order plus the reference.
#![allow(dead_code)]

use hft_event::{GroupSpec, LegDef, Level};
use hft_refdata::{
    AccountSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LiqMarker, MarginData, MarginMode,
    PositionMode, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit,
    VenueCaps, VenueSpec,
};
use hft_strategy_sdk::GroupTable;
use hft_strategy_sdk::testing::{MockViews, RefFixture};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, StrategyId, VenueId,
};

pub const VB: VenueId = VenueId::POLYMARKET_CLOB;
pub const VR: VenueId = VenueId::BINANCE_SPOT;
pub const SLOTS: [InstId; 6] = [InstId(1), InstId(2), InstId(3), InstId(4), InstId(5), InstId(6)];
pub const R: InstId = InstId(7);
pub const USDC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
pub const BTC: AssetId = AssetId(3);
/// Token assets of the six slots: AssetId 4..=9.
pub const TOKEN0: u16 = 4;
pub const A1: AccountId = AccountId(1);
pub const S1: StrategyId = StrategyId(1);
/// `max_groups_per_instance` of the fixture.
pub const CAPACITY: u16 = 16;

/// GroupId of group `k` (0..=6) of instance `S1`.
pub fn gid(k: u32) -> GroupId {
    GroupId(0x0100_0001 + k)
}

fn venue(b: &mut RdbBuilder, id: VenueId, exchange: Exchange) {
    b.venue(VenueSpec {
        id,
        exchange,
        product: InstKind::Spot,
        ws_max_streams_per_conn: 1024,
        ws_max_in_msgs_per_s: 10,
        ws_max_conn_age_s: 86_400,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "".into(),
        max_batch: 1,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.caps(VenueCaps {
        venue: id,
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
    b.fee(FeeSpec {
        venue: id,
        tier: 0,
        fee: FeePair {
            maker: Rate(0),
            taker: Rate(0),
        },
    });
}

#[allow(clippy::too_many_arguments)]
fn inst(
    b: &mut RdbBuilder,
    id: InstId,
    v: VenueId,
    asset: AssetId,
    quote: AssetId,
    sym: String,
    tick: &str,
    lot: &str,
) {
    b.inst(InstSpec {
        id,
        venue: v,
        symbol: sym,
        status: InstStatus::Trading,
        kind: InstKind::Spot,
        asset,
        quote,
        tick: 1,
        lot: 1,
        min_qty: Qty(1),
        max_qty: Qty(1_000_000_000_000),
        min_notional: Notional(1),
        book_window: 128,
        l2_depth: 100,
        crossed_tolerance: 0,
        tick_str: tick.into(),
        lot_str: lot.into(),
        min_qty_str: lot.into(),
        ..InstSpec::default()
    });
}

/// The fixture refdata.
pub fn refdata() -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    let mut assets = vec![
        (USDC, "USDC".to_string(), AssetClass::Stable, -6, -6),
        (USDT, "USDT".to_string(), AssetClass::Stable, -6, -6),
        (BTC, "BTC".to_string(), AssetClass::Crypto, -2, -8),
    ];
    for k in 0..6u16 {
        assets.push((AssetId(TOKEN0 + k), format!("TOK{k}"), AssetClass::Crypto, -3, -6));
    }
    for (id, name, class, px, qty) in assets {
        b.asset(AssetSpec {
            id,
            name: name.into(),
            class,
            exp: AssetExp {
                px_exp: px,
                qty_exp: qty,
            },
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000),
            dust: Qty(1),
        });
    }
    // Venue rows in ascending id order.
    venue(&mut b, VR, Exchange::Binance);
    venue(&mut b, VB, Exchange::Polymarket);
    for (k, i) in SLOTS.iter().enumerate() {
        let side = if k % 2 == 0 { "up" } else { "dn" };
        inst(
            &mut b,
            *i,
            VB,
            AssetId(TOKEN0 + k as u16),
            USDC,
            format!("btc-{side}-{}", k / 2),
            "0.001",
            "0.000001",
        );
    }
    inst(&mut b, R, VR, BTC, USDT, "BTCUSDT".into(), "0.01", "0.00000001");
    b.instance(InstanceSpec {
        id: S1,
        kind_id: 7,
        name: "binary-maker-1".into(),
    });
    b.account(AccountSpec {
        id: A1,
        sim: true,
        bound_instance: S1,
        is_prod: false,
        venues: 1 << VB.0,
        label: "sim-binary-maker".into(),
        ..AccountSpec::default()
    });
    b.sim(SimSpec {
        account: A1,
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    });
    b.sim_balance(SimBalanceSpec {
        account: A1,
        venue: VB,
        asset: USDC,
        qty: Qty(10_000_000_000),
    });
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => r,
        Err(e) => panic!("fixture refdata rejected: {e:?}"),
    }
}

/// The fixture as a `RefFixture` of account `A1`.
pub fn fixture() -> RefFixture {
    RefFixture::new(refdata(), A1)
}

/// A group with the legs `legs` (inst, venue) in order.
pub fn spec(id: GroupId, asset: AssetId, legs: &[(InstId, VenueId)]) -> GroupSpec {
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut l = [none; 4];
    for (d, (i, v)) in l.iter_mut().zip(legs.iter()) {
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
        asset,
        n_legs: legs.len() as u8,
        open_allowed: 1,
        legs: l,
        _rsv: 0,
    }
}

/// The six slot groups in order, then the reference group.
pub fn specs() -> Vec<GroupSpec> {
    let mut v: Vec<GroupSpec> = SLOTS
        .iter()
        .enumerate()
        .map(|(k, i)| spec(gid(k as u32), AssetId(TOKEN0 + k as u16), &[(*i, VB)]))
        .collect();
    v.push(spec(gid(6), BTC, &[(R, VR)]));
    v
}

pub fn groups() -> GroupTable {
    GroupTable::new(&specs(), CAPACITY)
}

/// Book of slot `i`: one bid level and one ask level (raw prices, quantity 100 shares).
pub fn slot_book(v: &mut MockViews, i: InstId, bid: i64, ask: i64) {
    let l = |px| Level {
        px: Px(px),
        qty: Qty(100_000_000),
    };
    v.book(i, &[l(bid)], &[l(ask)]);
}

/// Reference quote with mid `mid` (raw, spread 2 ticks).
pub fn ref_quote(v: &mut MockViews, mid: i64) {
    v.bbo(R, (Px(mid - 1), Qty(1_000_000)), (Px(mid + 1), Qty(1_000_000)));
}
