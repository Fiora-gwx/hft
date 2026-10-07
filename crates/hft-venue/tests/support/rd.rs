//! `rd_stub()` (TESTS.md "夹具"): venue V7 (value `VenueId::BYBIT_LINEAR`; number 7 is reserved and
//! cannot load), insts S1–S3 = 701–703 (px −2, qty −4), P001–P300 = 7001–7300, accounts A1 (Live,
//! ix 0), A2 (Live, ix 11; ten filler Live accounts sit between them), A9 (Sim).

use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, Exchange, FeeAssetRule,
    FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, LimitBucket, LimitScope, LiqMarker,
    MarginData, MarginMode, PositionMode, RateLimit, RdbBuilder, RefData, SimBalanceSpec, SimSpec,
    StaleGuardKind, TestnetKind, TsUnit, VenueCaps, VenueSpec,
};
use hft_types::{AccountId, AssetExp, AssetId, InstId, MarketKind, Notional, Px, Qty, Rate, VenueId};

pub const V7: VenueId = VenueId::BYBIT_LINEAR;
pub const S1: InstId = InstId(701);
pub const S2: InstId = InstId(702);
pub const S3: InstId = InstId(703);
pub const A1: AccountId = AccountId(1);
pub const A2: AccountId = AccountId(12);
pub const A9: AccountId = AccountId(13);
pub const USDT: AssetId = AssetId(1);
pub const BNB: AssetId = AssetId(2);
pub const BASE: AssetId = AssetId(3);

/// `P001` … `P300` → 7001 … 7300.
pub fn p(n: u32) -> InstId {
    InstId(7000 + n)
}

/// Parameters of `rd_stub` that cases vary.
#[derive(Clone, Debug)]
pub struct RdOpts {
    pub streams_per_conn: u16,
    pub in_msgs_per_s: u16,
    pub conn_age_s: u32,
    pub margin: MarginData,
}

impl Default for RdOpts {
    fn default() -> Self {
        RdOpts {
            streams_per_conn: 50,
            in_msgs_per_s: 10,
            conn_age_s: 86_400,
            margin: MarginData::Poll { interval_ms: 2000 },
        }
    }
}

/// The default `rd_stub` (built once per test binary).
pub fn rd_stub() -> &'static RefData {
    static RD: std::sync::OnceLock<&'static RefData> = std::sync::OnceLock::new();
    RD.get_or_init(|| rd_with(&RdOpts::default()))
}

/// Built once per call and leaked (`&'static`, as hft-core does at startup).
pub fn rd_with(o: &RdOpts) -> &'static RefData {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(1_759_276_800_000_000_000);
    for (id, name, class, px, qty) in [
        (USDT, "USDT", AssetClass::Stable, -6, -6),
        (BNB, "BNB", AssetClass::Crypto, -8, -8),
        (BASE, "STUB", AssetClass::Crypto, -2, -4),
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
            dust: Qty(1),
        });
    }
    b.venue(VenueSpec {
        id: V7,
        exchange: Exchange::Bybit,
        product: InstKind::Perp,
        ws_max_streams_per_conn: o.streams_per_conn,
        ws_max_in_msgs_per_s: o.in_msgs_per_s,
        ws_max_conn_age_s: o.conn_age_s,
        ws_new_conn_per_5min_ip: 300,
        app_ping_interval_s: 0,
        ts_unit: TsUnit::Ms,
        client_id_max_len: 36,
        client_id_prefix: "st-".into(),
        max_batch: 20,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: StaleGuardKind::RecvWindow,
    });
    b.caps(VenueCaps {
        venue: V7,
        market: MarketKind::LinearPerp,
        fee_asset_rule: FeeAssetRule::Quote,
        reduce_only: true,
        required_position_mode: PositionMode::OneWay,
        required_margin_mode: MarginMode::Cross,
        required_multi_asset: None,
        required_spot_borrow: None,
        funding_settle: FundingSettleKind::PerSymbolPush,
        margin_data: o.margin,
        own_liquidation_marker: LiqMarker::NotIdentifiable,
        liq_marker_arg: "".into(),
        ignorable_user_events: vec!["listenKeyExtended".into()].into(),
        flags: CapFlags::AMEND | CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::BATCH_CANCEL,
        amend_qty_down_only: false,
        dms: DeadManCap::CountdownPerInst,
        dms_min_ms: 5000,
        dms_max_ms: 0,
        dms_timeout_ms: 30_000,
        dms_refresh_ms: 10_000,
        testnet: TestnetKind::Testnet,
        _pad: 0,
    });
    let mut inst = |id: InstId, sym: String| {
        b.inst(InstSpec {
            id,
            venue: V7,
            symbol: sym,
            status: InstStatus::Trading,
            kind: InstKind::Perp,
            asset: BASE,
            quote: USDT,
            tick: 1,
            lot: 1,
            min_qty: Qty(1),
            max_qty: Qty(1_000_000_000_000),
            min_notional: Notional(1),
            book_window: 64,
            l2_depth: 20,
            crossed_tolerance: 0,
            tick_str: "0.01".into(),
            lot_str: "0.0001".into(),
            min_qty_str: "0.0001".into(),
            funding_interval_s: 28_800,
            max_leverage: 20,
            ..InstSpec::default()
        });
    };
    // Inst ids are dense from 1 (refdata step 3): fillers `X<id>` around S1–S3 and P001–P300.
    for id in 1..=7300u32 {
        let sym = match id {
            701 => "S1".to_string(),
            702 => "S2".to_string(),
            703 => "S3".to_string(),
            7001..=7300 => format!("P{:03}", id - 7000),
            _ => format!("X{id}"),
        };
        inst(InstId(id), sym);
    }
    b.fee(FeeSpec {
        venue: V7,
        tier: 0,
        fee: FeePair {
            maker: Rate(200_000),
            taker: Rate(500_000),
        },
    });
    // A1 (ix 0), ten fillers, A2 (ix 11): venue_account_ix is dense in account order.
    for id in 1..=12u8 {
        b.account(AccountSpec {
            id: AccountId(id),
            label: format!("live{id}"),
            venues: 1 << V7.0,
            ..AccountSpec::default()
        });
        b.account_venue(AccountVenueSpec {
            account: AccountId(id),
            venue: V7,
            uid: format!("10{id}"),
        });
    }
    b.account(AccountSpec {
        id: A9,
        sim: true,
        label: "sim".into(),
        venues: 1 << V7.0,
        ..AccountSpec::default()
    });
    b.sim(SimSpec {
        account: A9,
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    });
    b.sim_balance(SimBalanceSpec {
        account: A9,
        venue: V7,
        asset: USDT,
        qty: Qty(1_000_000_000),
    });
    for (scope, bucket, limit, window_ms) in [
        (LimitScope::Account, LimitBucket::Orders, 10, 1000),
        (LimitScope::Account, LimitBucket::Weight, 100, 60_000),
    ] {
        b.limit(
            V7,
            RateLimit {
                scope,
                bucket,
                limit,
                window_ms,
                default_cost: 1,
            },
        );
    }
    let blob = b.finish();
    match RefData::load(&blob, None) {
        Ok(r) => Box::leak(Box::new(r)),
        Err(e) => panic!("rd_stub rejected: {e:?}"),
    }
}
