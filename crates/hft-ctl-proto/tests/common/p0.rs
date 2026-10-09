//! Refdata of baseline `P0` (crates/hft-ctl-proto/TESTS.md), built only through `RdbBuilder`.
//! Self-contained: also included by hft-ctl's tests (`#[path]`), whose baseline G0 uses the same
//! refdata. `fixtures/refdata/ctl-p0.blob` is this builder's output (CTP-140 checks it).
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]

use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule,
    FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LiqMarker, MarginData,
    MarginMode, PositionMode, PriceBand, RdbBuilder, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind,
    TsUnit, UidPolicy, VenueCaps, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, InstId, MarketKind, Notional, Px, Qty, Rate, StrategyId, VenueId,
};

pub const USDT: AssetId = AssetId(1);
pub const BTC: AssetId = AssetId(2);
pub const V_BN_USDM: VenueId = VenueId::BINANCE_USDM;
pub const V_BN_SPOT: VenueId = VenueId::BINANCE_SPOT;
pub const V_BG_FUT: VenueId = VenueId::BITGET_UTA_USDT_FUT;
pub const V_BG_SPOT: VenueId = VenueId::BITGET_UTA_SPOT;
pub const I_BN_USDM: InstId = InstId(1);
pub const I_BN_SPOT: InstId = InstId(2);
pub const I_BG_FUT: InstId = InstId(3);
pub const I_BG_SPOT: InstId = InstId(4);
pub const S1: StrategyId = StrategyId(1);
pub const S2: StrategyId = StrategyId(2);
pub const S9: StrategyId = StrategyId(3);
pub const ACCT_A: AccountId = AccountId(1);
pub const ACCT_B: AccountId = AccountId(2);
pub const SIM_A: AccountId = AccountId(3);
/// `g1` = `(1 << 24) | 101`, `g2` = `(2 << 24) | 101`, `g9` = `(3 << 24) | 101`.
pub const G1: u32 = (1 << 24) | 101;
pub const G2: u32 = (2 << 24) | 101;
pub const G9: u32 = (3 << 24) | 101;
pub const KIND_TAKER: u16 = 0xFF01;
pub const KIND_TARGET: u16 = 0xFF02;
pub const CREATED: i64 = 1_790_000_000_000_000_000;

pub fn vbits(vs: &[VenueId]) -> u64 {
    vs.iter().fold(0u64, |m, v| m | (1u64 << v.0))
}

fn asset(id: AssetId, name: &str, class: AssetClass, exp: (i8, i8), px_max: i64, qty_max: i64) -> AssetSpec {
    AssetSpec {
        id,
        name: name.into(),
        class,
        exp: AssetExp {
            px_exp: exp.0,
            qty_exp: exp.1,
        },
        px_max: Px(px_max),
        qty_max: Qty(qty_max),
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

fn perp_caps(v: VenueId) -> VenueCaps {
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

fn spot_caps(v: VenueId) -> VenueCaps {
    VenueCaps {
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
    }
}

fn inst(id: InstId, venue: VenueId, kind: InstKind) -> InstSpec {
    let perp = kind == InstKind::Perp;
    InstSpec {
        id,
        venue,
        symbol: "BTCUSDT".into(),
        status: InstStatus::Trading,
        kind,
        flags: 0,
        asset: BTC,
        quote: USDT,
        px_unit_shift: 0,
        qty_unit_shift: 0,
        tick: 10_000_000,
        lot: 100_000,
        min_qty: Qty(100_000),
        max_qty: Qty(100_000_000_000),
        min_notional: Notional(5_000_000),
        book_window: 4096,
        l2_depth: 20,
        crossed_tolerance: 0,
        venue_num_id: 0,
        tick_str: "0.1".into(),
        lot_str: "0.001".into(),
        min_qty_str: "0.001".into(),
        funding_interval_s: if perp { 28_800 } else { 0 },
        funding_anchor_s: 0,
        max_leverage: if perp { 125 } else { 0 },
        listed_utc_ns: 0,
        delivery_utc_ns: 0,
        price_band: PriceBand::default(),
    }
}

fn fee(venue: VenueId) -> FeeSpec {
    FeeSpec {
        venue,
        tier: 0,
        fee: FeePair {
            maker: Rate(200_000),
            taker: Rate(500_000),
        },
    }
}

/// Every editable input of P0, replayed through `RdbBuilder` in a fixed order.
#[derive(Clone, Debug)]
pub struct P0 {
    pub version: u64,
    pub assets: Vec<AssetSpec>,
    pub venues: Vec<VenueSpec>,
    pub caps: Vec<VenueCaps>,
    pub insts: Vec<InstSpec>,
    pub instances: Vec<InstanceSpec>,
    pub accounts: Vec<AccountSpec>,
    pub account_venues: Vec<AccountVenueSpec>,
    pub fees: Vec<FeeSpec>,
    pub sims: Vec<SimSpec>,
    pub sim_balances: Vec<SimBalanceSpec>,
}

impl P0 {
    pub fn build(&self) -> Vec<u8> {
        let mut b = RdbBuilder::new(self.version);
        b.created_utc_ns(CREATED);
        for x in &self.assets {
            b.asset(x.clone());
        }
        for x in &self.venues {
            b.venue(x.clone());
        }
        for x in &self.caps {
            b.caps(x.clone());
        }
        for x in &self.insts {
            b.inst(x.clone());
        }
        for x in &self.instances {
            b.instance(x.clone());
        }
        for x in &self.accounts {
            b.account(x.clone());
        }
        for x in &self.account_venues {
            b.account_venue(x.clone());
        }
        for x in &self.fees {
            b.fee(*x);
        }
        for x in &self.sims {
            b.sim(*x);
        }
        for x in &self.sim_balances {
            b.sim_balance(*x);
        }
        b.finish()
    }

    pub fn load(&self) -> hft_refdata::RefData {
        match hft_refdata::RefData::load(&self.build(), None) {
            Ok(rd) => rd,
            Err(e) => panic!("P0 refdata does not load: {e:?}"),
        }
    }
}

pub fn p0_spec() -> P0 {
    let instance = |id: StrategyId, kind_id, name: &str| InstanceSpec {
        id,
        kind_id,
        name: name.into(),
    };
    let uid = |account, venue, uid: &str| AccountVenueSpec {
        account,
        venue,
        uid: uid.into(),
    };
    P0 {
        version: 1,
        assets: vec![
            asset(
                USDT,
                "USDT",
                AssetClass::Stable,
                (-8, -6),
                1_000_000_000,
                1_000_000_000_000_000_000,
            ),
            asset(
                BTC,
                "BTC",
                AssetClass::Crypto,
                (-8, -8),
                1_000_000_000_000_000,
                100_000_000_000_000,
            ),
        ],
        venues: vec![
            venue(V_BN_USDM, Exchange::Binance, InstKind::Perp),
            venue(V_BN_SPOT, Exchange::Binance, InstKind::Spot),
            venue(V_BG_FUT, Exchange::Bitget, InstKind::Perp),
            venue(V_BG_SPOT, Exchange::Bitget, InstKind::Spot),
        ],
        caps: vec![
            perp_caps(V_BN_USDM),
            spot_caps(V_BN_SPOT),
            perp_caps(V_BG_FUT),
            spot_caps(V_BG_SPOT),
        ],
        insts: vec![
            inst(I_BN_USDM, V_BN_USDM, InstKind::Perp),
            inst(I_BN_SPOT, V_BN_SPOT, InstKind::Spot),
            inst(I_BG_FUT, V_BG_FUT, InstKind::Perp),
            inst(I_BG_SPOT, V_BG_SPOT, InstKind::Spot),
        ],
        instances: vec![
            instance(S1, KIND_TAKER, "s1-main"),
            instance(S2, KIND_TARGET, "s2-main"),
            instance(S9, KIND_TAKER, "s9-sim"),
        ],
        accounts: vec![
            AccountSpec {
                id: ACCT_A,
                sim: false,
                bound_instance: S1,
                is_prod: false,
                venues: vbits(&[V_BN_USDM, V_BG_SPOT]),
                label: "acct-a".into(),
                uid_policy: UidPolicy::Verified,
            },
            AccountSpec {
                id: ACCT_B,
                sim: false,
                bound_instance: S2,
                is_prod: false,
                venues: vbits(&[V_BN_SPOT, V_BG_FUT]),
                label: "acct-b".into(),
                uid_policy: UidPolicy::Verified,
            },
            AccountSpec {
                id: SIM_A,
                sim: true,
                bound_instance: S9,
                is_prod: false,
                venues: vbits(&[V_BN_USDM, V_BG_SPOT]),
                label: "sim-a".into(),
                uid_policy: UidPolicy::Verified,
            },
        ],
        account_venues: vec![
            uid(ACCT_A, V_BN_USDM, "100000001"),
            uid(ACCT_A, V_BG_SPOT, "200000001"),
            uid(ACCT_B, V_BN_SPOT, "100000002"),
            uid(ACCT_B, V_BG_FUT, "200000002"),
        ],
        fees: vec![fee(V_BN_USDM), fee(V_BN_SPOT), fee(V_BG_FUT), fee(V_BG_SPOT)],
        sims: vec![SimSpec {
            account: SIM_A,
            fee_tier: 0,
            latency_model: 0,
            margin_model: 0,
        }],
        sim_balances: vec![
            SimBalanceSpec {
                account: SIM_A,
                venue: V_BN_USDM,
                asset: USDT,
                qty: Qty(1_000_000_000),
            },
            SimBalanceSpec {
                account: SIM_A,
                venue: V_BG_SPOT,
                asset: USDT,
                qty: Qty(1_000_000_000),
            },
        ],
    }
}

/// P0 refdata bytes.
pub fn p0_blob() -> Vec<u8> {
    p0_spec().build()
}

/// P0 refdata.
pub fn p0() -> hft_refdata::RefData {
    p0_spec().load()
}
