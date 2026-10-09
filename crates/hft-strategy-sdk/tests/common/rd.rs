//! `GT3` refdata and group tables (TESTS.md "共用夹具"): venues V1 (spot, X1), V2 (USDT perp,
//! X2), V3 (perp, X3, not in A1, has feed), V4 (spot, X4, not in A1, no feed); insts I1–I5;
//! assets BTC (1), asset 2, USDT (3); accounts A1 (Sim, {V1, V2}, bound to S3) and A2.

use hft_event::{GroupSpec, LegDef};
use hft_refdata::{
    AccountSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule, FeePair, FeeSpec,
    FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LiqMarker, MarginData, MarginMode,
    PositionMode, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit,
    VenueCaps, VenueSpec,
};
use hft_strategy_sdk::testing::{MockViews, RefFixture};
use hft_strategy_sdk::{GroupTable, VenueSet};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, InstId, MarketKind, Notional, Px, Qty, Rate, StrategyId, VenueId,
};

pub const V1: VenueId = VenueId::BINANCE_SPOT;
pub const V2: VenueId = VenueId::OKX_SWAP;
pub const V3: VenueId = VenueId::BITGET_UTA_USDT_FUT;
pub const V4: VenueId = VenueId::GATE_SPOT;
pub const I1: InstId = InstId(101);
pub const I2: InstId = InstId(102);
pub const I3: InstId = InstId(103);
pub const I4: InstId = InstId(104);
pub const I5: InstId = InstId(105);
pub const BTC_ID: AssetId = AssetId(1);
pub const ASSET2: AssetId = AssetId(2);
pub const USDT: AssetId = AssetId(3);
pub const A1: AccountId = AccountId(1);
pub const A2: AccountId = AccountId(2);
pub const S3: StrategyId = StrategyId(3);
pub const G4: GroupId = GroupId(0x0300_0004);

/// Venues with a feed in the test hft-core instance (V1, V2, V3).
pub const FEEDS: VenueSet = VenueSet((1 << 2) | (1 << 5) | (1 << 10));

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
            maker: Rate(200_000),
            taker: Rate(500_000),
        },
    });
}

fn inst(b: &mut RdbBuilder, id: InstId, v: VenueId, kind: InstKind, asset: AssetId) {
    b.inst(InstSpec {
        id,
        venue: v,
        symbol: format!("S{}", id.0),
        status: InstStatus::Trading,
        kind,
        asset,
        quote: USDT,
        tick: 1,
        lot: 1_000,
        min_qty: Qty(1_000),
        max_qty: Qty(100_000_000_000),
        min_notional: Notional(5_000_000),
        book_window: 64,
        l2_depth: 20,
        crossed_tolerance: 0,
        tick_str: "0.01".into(),
        lot_str: "0.00001".into(),
        min_qty_str: "0.00001".into(),
        funding_interval_s: if kind == InstKind::Perp { 28_800 } else { 0 },
        max_leverage: if kind == InstKind::Perp { 20 } else { 0 },
        ..InstSpec::default()
    });
}

/// The GT3 refdata.
pub fn gt3_refdata() -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class, px, qty) in [
        (BTC_ID, "BTC", AssetClass::Crypto, -2, -8),
        (ASSET2, "ETH", AssetClass::Crypto, -2, -8),
        (USDT, "USDT", AssetClass::Stable, -6, -6),
    ] {
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
            dust: Qty(1_000),
        });
    }
    // Venue rows in ascending id order (V1 = 2, V4 = 4, V2 = 5, V3 = 10).
    venue(&mut b, V1, Exchange::Binance, InstKind::Spot);
    venue(&mut b, V4, Exchange::Gate, InstKind::Spot);
    venue(&mut b, V2, Exchange::Okx, InstKind::Perp);
    venue(&mut b, V3, Exchange::Bitget, InstKind::Perp);
    // Inst ids are dense from 1: fillers 1..=100 (V1 spot) put I1–I5 at 101–105.
    for id in 1..=100u32 {
        inst(&mut b, InstId(id), V1, InstKind::Spot, ASSET2);
    }
    inst(&mut b, I1, V1, InstKind::Spot, BTC_ID);
    inst(&mut b, I2, V2, InstKind::Perp, BTC_ID);
    inst(&mut b, I3, V3, InstKind::Perp, BTC_ID);
    inst(&mut b, I4, V4, InstKind::Spot, BTC_ID);
    inst(&mut b, I5, V2, InstKind::Perp, ASSET2);
    for (id, kind_id) in [(1u8, 0xFF01u16), (2, 0xFF02), (3, 0xFF01)] {
        b.instance(InstanceSpec {
            id: StrategyId(id),
            kind_id,
            name: format!("probe{id}").into(),
        });
    }
    b.account(AccountSpec {
        id: A1,
        sim: true,
        bound_instance: S3,
        is_prod: false,
        venues: (1 << V1.0) | (1 << V2.0),
        label: "sim-a1".into(),
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
        venue: V1,
        asset: USDT,
        qty: Qty(1_000_000_000_000),
    });
    b.account(AccountSpec {
        id: A2,
        sim: true,
        bound_instance: StrategyId(1),
        venues: 1 << V1.0,
        label: "sim-a2".into(),
        ..AccountSpec::default()
    });
    b.sim(SimSpec {
        account: A2,
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    });
    b.sim_balance(SimBalanceSpec {
        account: A2,
        venue: V1,
        asset: USDT,
        qty: Qty(1_000_000_000_000),
    });
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => r,
        Err(e) => panic!("GT3 refdata rejected: {e:?}"),
    }
}

/// The GT3 `RefFixture` (bound account A1).
pub fn gt3_fixture() -> RefFixture {
    RefFixture::new(gt3_refdata(), A1)
}

/// A `MockViews` over GT3.
pub fn gt3_views() -> MockViews {
    MockViews::new(gt3_fixture())
}

/// One two-leg group: leg 0 = `l0` on its venue, leg 1 = `l1`.
pub fn spec(id: GroupId, asset: AssetId, legs: &[(InstId, VenueId)]) -> GroupSpec {
    let mut l = [LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }; 4];
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

/// The standard GT3 group (I1 spot + I2 perp, BTC).
pub fn gspec(id: GroupId) -> GroupSpec {
    spec(id, BTC_ID, &[(I1, V1), (I2, V2)])
}

/// `GroupTable` of the given specs with capacity 8.
pub fn table(specs: &[GroupSpec]) -> GroupTable {
    GroupTable::new(specs, 8)
}

/// GT3: [G1, G2, G3].
pub fn gt3() -> GroupTable {
    table(&[gspec(super::G1), gspec(super::G2), gspec(super::G3)])
}
