//! Refdata fixture `RF1` (TESTS.md "共用夹具"), built through the RDB1 encoder and loaded with the
//! real loader: account A1 (venues {V1, V2}), I1 (V1 spot, tick 1, lot 1 000), I2 (V2 USDT perp,
//! tick 10, lot 100 000), I3 (V3 perp; V3 not in A1's venues), account A2 (not the bound one);
//! inst 999 absent. Fees: I1 maker 0 / taker 10 bp, I2 maker −0.5 bp / taker 5 bp.
#![allow(dead_code)]

use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule,
    FeeOverrideSpec, FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec,
    LiqMarker, MarginData, MarginMode, PositionMode, PriceBand, RdbBuilder, RefData, StaleGuardKind,
    TestnetKind, TsUnit, UidPolicy, VenueCaps, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, InstId, MarketKind, Notional, Px, Qty, Rate, StrategyId, VenueId,
};

pub const BTC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
/// Spot venue of I1.
pub const V1: VenueId = VenueId::BINANCE_SPOT;
/// Perp venue of I2.
pub const V2: VenueId = VenueId::OKX_SWAP;
/// Perp venue of I3 (outside A1's venues).
pub const V3: VenueId = VenueId::BITGET_UTA_USDT_FUT;
pub const I1: InstId = InstId(1);
pub const I2: InstId = InstId(2);
pub const I3: InstId = InstId(3);
pub const A1: AccountId = AccountId(1);
pub const A2: AccountId = AccountId(2);

fn fee(maker: i64, taker: i64) -> FeePair {
    FeePair {
        maker: Rate(maker),
        taker: Rate(taker),
    }
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
        client_id_max_len: 32,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    }
}

fn caps(v: VenueId, spot: bool, rule: FeeAssetRule) -> VenueCaps {
    VenueCaps {
        venue: v,
        market: if spot {
            MarketKind::Spot
        } else {
            MarketKind::LinearPerp
        },
        fee_asset_rule: rule,
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
    }
}

fn inst(id: InstId, sym: &str, v: VenueId, perp: bool, tick: (&str, i64), lot: (&str, i64)) -> InstSpec {
    InstSpec {
        id,
        venue: v,
        symbol: sym.into(),
        status: InstStatus::Trading,
        kind: if perp { InstKind::Perp } else { InstKind::Spot },
        flags: 0,
        asset: BTC,
        quote: USDT,
        px_unit_shift: 0,
        qty_unit_shift: 0,
        tick: tick.1,
        lot: lot.1,
        min_qty: Qty(lot.1),
        max_qty: Qty(100_000_000_000),
        min_notional: Notional(5_000_000),
        book_window: 4096,
        l2_depth: 20,
        crossed_tolerance: 0,
        venue_num_id: 0,
        tick_str: tick.0.into(),
        lot_str: lot.0.into(),
        min_qty_str: lot.0.into(),
        funding_interval_s: if perp { 28_800 } else { 0 },
        funding_anchor_s: 0,
        max_leverage: if perp { 125 } else { 0 },
        listed_utc_ns: 0,
        delivery_utc_ns: 0,
        price_band: PriceBand::default(),
    }
}

/// `RF1`; `v1_rule` is V1's `fee_asset_rule` (`RF1'` changes it); `extra` adds per-inst fee
/// overrides of A1 (`(inst, maker, taker)`, insts I1/I2/I3 or I4.. = more V1/V2 insts).
pub fn build(v1_rule: FeeAssetRule, extra: &[(InstId, i64, i64)]) -> RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_708_800_000_000_000);
    b.asset(asset(BTC, "BTC", AssetClass::Crypto));
    b.asset(asset(USDT, "USDT", AssetClass::Stable));
    b.venue(venue(V1, Exchange::Binance, InstKind::Spot));
    b.venue(venue(V2, Exchange::Okx, InstKind::Perp));
    b.venue(venue(V3, Exchange::Bitget, InstKind::Perp));
    b.caps(caps(V1, true, v1_rule));
    b.caps(caps(V2, false, FeeAssetRule::Quote));
    b.caps(caps(V3, false, FeeAssetRule::Quote));
    b.inst(inst(I1, "BTCUSDT", V1, false, ("0.01", 1), ("0.00001", 1_000)));
    b.inst(inst(
        I2,
        "BTC-USDT-SWAP",
        V2,
        true,
        ("0.1", 10),
        ("0.001", 100_000),
    ));
    b.inst(inst(I3, "BTCUSDT", V3, true, ("0.1", 10), ("0.001", 100_000)));
    let max_extra = extra.iter().map(|e| e.0.0).max().unwrap_or(0);
    for id in 4..=max_extra {
        let (v, perp) = if id % 2 == 0 { (V1, false) } else { (V2, true) };
        let sym = format!("X{id}");
        b.inst(inst(InstId(id), &sym, v, perp, ("0.01", 1), ("0.00001", 1_000)));
    }
    b.instance(InstanceSpec {
        id: StrategyId(1),
        kind_id: 0xFF00,
        name: "slb-a".into(),
    });
    b.instance(InstanceSpec {
        id: StrategyId(2),
        kind_id: 0xFF00,
        name: "slb-b".into(),
    });
    let vbits = |vs: &[VenueId]| vs.iter().fold(0u64, |m, v| m | (1u64 << v.0));
    b.account(AccountSpec {
        id: A1,
        sim: false,
        bound_instance: StrategyId(1),
        is_prod: false,
        venues: vbits(&[V1, V2]),
        label: "a1".into(),
        uid_policy: UidPolicy::Verified,
    });
    b.account(AccountSpec {
        id: A2,
        sim: false,
        bound_instance: StrategyId(2),
        is_prod: false,
        venues: vbits(&[V1, V2]),
        label: "a2".into(),
        uid_policy: UidPolicy::Verified,
    });
    for (a, uid) in [(A1, "11"), (A2, "21")] {
        b.account_venue(AccountVenueSpec {
            account: a,
            venue: V1,
            uid: uid.into(),
        });
        b.account_venue(AccountVenueSpec {
            account: a,
            venue: V2,
            uid: format!("{uid}2"),
        });
    }
    b.fee(FeeSpec {
        venue: V1,
        tier: 0,
        fee: fee(0, 1_000_000),
    });
    b.fee(FeeSpec {
        venue: V2,
        tier: 0,
        fee: fee(-50_000, 500_000),
    });
    b.fee(FeeSpec {
        venue: V3,
        tier: 0,
        fee: fee(0, 500_000),
    });
    for &(i, maker, taker) in extra {
        let v = if i == I1 || (i.0 >= 4 && i.0 % 2 == 0) {
            V1
        } else {
            V2
        };
        b.fee_override(FeeOverrideSpec {
            account: A1,
            venue: v,
            inst: i,
            fee: fee(maker, taker),
        });
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(rd) => rd,
        Err(e) => panic!("RF1 does not load: {e:?}"),
    }
}

/// `RF1`.
pub fn rf1() -> RefData {
    build(FeeAssetRule::ReceivedAsset, &[])
}
