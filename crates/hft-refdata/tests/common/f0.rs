//! Fixture `F0` (crates/hft-refdata/TESTS.md) and `E0`, built only through the encoders.
//! Self-contained: also included by the in-crate unit test, the benches and the fuzz target.
#![allow(dead_code)]

use hft_refdata::{
    AccountSpec, AccountVenueSpec, AliasSpec, AssetClass, AssetSpec, CapFlags, ChainToken, DeadManCap,
    EcfBuilder, Endpoint, EndpointEnv, EndpointRole, EngineSection, Exchange, FeeAssetRule, FeeOverrideSpec,
    FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LimitBucket,
    LimitScope, LiqMarker, MaintWindow, MarginData, MarginMode, MmrTierSpec, PositionMode, PriceBand,
    RateLimit, RdbBuilder, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit, UidPolicy,
    VenueCaps, VenueSpec,
};
use hft_types::{
    AccountId, AssetExp, AssetId, HostClass, InstId, MarketKind, Notional, Px, Qty, Rate, StrategyId, VenueId,
};

pub const BTC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
pub const ETH: AssetId = AssetId(3);
pub const USDC: AssetId = AssetId(4);
pub const PEPE: AssetId = AssetId(5);
pub const V1: VenueId = VenueId::BINANCE_USDM;
pub const V2: VenueId = VenueId::BINANCE_SPOT;
pub const V5: VenueId = VenueId::OKX_SWAP;
pub const V6: VenueId = VenueId::OKX_SPOT;
pub const CREATED: i64 = 1_759_276_800_000_000_000;

/// Bitmap of venue ids.
pub fn vbits(vs: &[u8]) -> u64 {
    vs.iter().fold(0u64, |m, v| m | (1u64 << v))
}

/// `x` percent as a `Rate` (1 % = 10 000 000) given in hundredths of a percent (`bp100(40)` = 0.4 %).
pub const fn rate_e9(x: i64) -> Rate {
    Rate(x)
}

pub fn fee(maker: i64, taker: i64) -> FeePair {
    FeePair {
        maker: Rate(maker),
        taker: Rate(taker),
    }
}

/// Every editable input of a blob; `build()` replays it through `RdbBuilder` in a fixed order.
#[derive(Clone, Debug)]
pub struct Spec {
    pub version: u64,
    pub created: i64,
    pub assets: Vec<AssetSpec>,
    pub venues: Vec<VenueSpec>,
    pub caps: Vec<VenueCaps>,
    pub insts: Vec<InstSpec>,
    pub instances: Vec<InstanceSpec>,
    pub accounts: Vec<AccountSpec>,
    pub account_venues: Vec<AccountVenueSpec>,
    pub fees: Vec<FeeSpec>,
    pub fee_overrides: Vec<FeeOverrideSpec>,
    pub sims: Vec<SimSpec>,
    pub sim_balances: Vec<SimBalanceSpec>,
    pub limits: Vec<(VenueId, RateLimit)>,
    pub endpoints: Vec<(VenueId, Endpoint)>,
    pub maint: Vec<MaintWindow>,
    pub tokens: Vec<ChainToken>,
    pub aliases: Vec<AliasSpec>,
    pub mmr: Vec<MmrTierSpec>,
}

impl Spec {
    pub fn builder(&self) -> RdbBuilder {
        let mut b = RdbBuilder::new(self.version);
        b.created_utc_ns(self.created);
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
        for x in &self.fee_overrides {
            b.fee_override(*x);
        }
        for x in &self.sims {
            b.sim(*x);
        }
        for x in &self.sim_balances {
            b.sim_balance(*x);
        }
        for (v, x) in &self.limits {
            b.limit(*v, *x);
        }
        for (v, x) in &self.endpoints {
            b.endpoint(*v, x.clone());
        }
        for x in &self.maint {
            b.maint(*x);
        }
        for x in &self.tokens {
            b.token(*x);
        }
        for x in &self.aliases {
            b.alias(x.clone());
        }
        for x in &self.mmr {
            b.mmr_tier(*x);
        }
        b
    }

    pub fn build(&self) -> Vec<u8> {
        self.builder().finish()
    }

    pub fn inst_mut(&mut self, id: u32) -> &mut InstSpec {
        let ix = self
            .insts
            .iter()
            .position(|i| i.id == InstId(id))
            .unwrap_or(usize::MAX);
        &mut self.insts[ix]
    }
    pub fn asset_mut(&mut self, id: u16) -> &mut AssetSpec {
        let ix = self
            .assets
            .iter()
            .position(|a| a.id == AssetId(id))
            .unwrap_or(usize::MAX);
        &mut self.assets[ix]
    }
    pub fn account_mut(&mut self, id: u8) -> &mut AccountSpec {
        let ix = self
            .accounts
            .iter()
            .position(|a| a.id == AccountId(id))
            .unwrap_or(usize::MAX);
        &mut self.accounts[ix]
    }
    pub fn caps_mut(&mut self, venue: u8) -> &mut VenueCaps {
        let ix = self
            .caps
            .iter()
            .position(|c| c.venue == VenueId(venue))
            .unwrap_or(usize::MAX);
        &mut self.caps[ix]
    }
    pub fn venue_mut(&mut self, venue: u8) -> &mut VenueSpec {
        let ix = self
            .venues
            .iter()
            .position(|c| c.id == VenueId(venue))
            .unwrap_or(usize::MAX);
        &mut self.venues[ix]
    }
    /// Inserts a venue row keeping the Venues table sorted by id.
    pub fn add_venue(&mut self, v: VenueSpec) {
        let at = self
            .venues
            .iter()
            .position(|x| x.id.0 > v.id.0)
            .unwrap_or(self.venues.len());
        self.venues.insert(at, v);
    }
    /// Inserts a caps row keeping the VenueCaps table sorted by venue.
    pub fn add_caps(&mut self, c: VenueCaps) {
        let at = self
            .caps
            .iter()
            .position(|x| x.venue.0 > c.venue.0)
            .unwrap_or(self.caps.len());
        self.caps.insert(at, c);
    }
}

pub fn asset(id: u16, name: &str, class: AssetClass, exp: (i8, i8), px_max: i64, qty_max: i64) -> AssetSpec {
    AssetSpec {
        id: AssetId(id),
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

pub fn venue(
    id: VenueId,
    exchange: Exchange,
    product: InstKind,
    stale: StaleGuardKind,
    cid_len: u8,
) -> VenueSpec {
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
        client_id_max_len: cid_len,
        client_id_prefix: "".into(),
        max_batch: 5,
        max_inflight_amend: 1,
        crossed_tolerance_default: 0,
        stale_guard: stale,
    }
}

/// F0 perpetual caps row.
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
        ignorable_user_events: vec!["TRADE_LITE".into()].into(),
        flags: CapFlags::AMEND
            | CapFlags::POST_ONLY
            | CapFlags::IOC
            | if v == VenueId::BINANCE_USDM {
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
    }
}

/// F0 spot caps row.
pub fn spot_caps(v: VenueId, spot_borrow: Option<bool>) -> VenueCaps {
    VenueCaps {
        venue: v,
        market: MarketKind::Spot,
        fee_asset_rule: FeeAssetRule::ReceivedAsset,
        reduce_only: false,
        required_position_mode: PositionMode::NotApplicable,
        required_margin_mode: MarginMode::NotApplicable,
        required_multi_asset: None,
        required_spot_borrow: spot_borrow,
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

/// An inst with F0 defaults; steps given as (text, raw) pairs.
#[allow(clippy::too_many_arguments)]
pub fn inst(
    id: u32,
    symbol: &str,
    venue: VenueId,
    kind: InstKind,
    asset: AssetId,
    tick: (&str, i64),
    lot: (&str, i64),
    min_qty: (&str, i64),
) -> InstSpec {
    InstSpec {
        id: InstId(id),
        venue,
        symbol: symbol.into(),
        status: InstStatus::Trading,
        kind,
        flags: 0,
        asset,
        quote: USDT,
        px_unit_shift: 0,
        qty_unit_shift: 0,
        tick: tick.1,
        lot: lot.1,
        min_qty: Qty(min_qty.1),
        max_qty: Qty(100_000_000_000), // 1 000 BTC at −8
        min_notional: Notional(5_000_000),
        book_window: 4096,
        l2_depth: 20,
        crossed_tolerance: 0,
        venue_num_id: 0,
        tick_str: tick.0.into(),
        lot_str: lot.0.into(),
        min_qty_str: min_qty.0.into(),
        funding_interval_s: if kind == InstKind::Perp { 28_800 } else { 0 },
        funding_anchor_s: 0,
        max_leverage: if kind == InstKind::Perp { 125 } else { 0 },
        listed_utc_ns: 0,
        delivery_utc_ns: 0,
        price_band: PriceBand::default(),
    }
}

pub fn f0_spec() -> Spec {
    let assets = vec![
        asset(
            1,
            "BTC",
            AssetClass::Crypto,
            (-8, -8),
            1_000_000_000_000_000,
            100_000_000_000_000,
        ),
        asset(
            2,
            "USDT",
            AssetClass::Stable,
            (-8, -8),
            1_000_000_000,
            1_000_000_000_000_000_000,
        ),
        asset(
            3,
            "ETH",
            AssetClass::Crypto,
            (-8, -8),
            100_000_000_000_000,
            1_000_000_000_000_000,
        ),
        asset(
            4,
            "USDC",
            AssetClass::Stable,
            (-8, -8),
            1_000_000_000,
            1_000_000_000_000_000_000,
        ),
        asset(
            5,
            "PEPE",
            AssetClass::Crypto,
            (-12, 0),
            1_000_000_000_000,
            1_000_000_000_000_000,
        ),
    ];
    let venues = vec![
        venue(
            V1,
            Exchange::Binance,
            InstKind::Perp,
            StaleGuardKind::RecvWindow,
            36,
        ),
        venue(
            V2,
            Exchange::Binance,
            InstKind::Spot,
            StaleGuardKind::RecvWindow,
            36,
        ),
        venue(V5, Exchange::Okx, InstKind::Perp, StaleGuardKind::ExpireAt, 32),
        venue(V6, Exchange::Okx, InstKind::Spot, StaleGuardKind::ExpireAt, 32),
    ];
    let caps = vec![
        perp_caps(V1),
        spot_caps(V2, None),
        perp_caps(V5),
        spot_caps(V6, Some(false)),
    ];
    let mut i3 = inst(
        3,
        "BTC-USDT-SWAP",
        V5,
        InstKind::Perp,
        BTC,
        ("0.1", 10_000_000),
        ("0.01", 10_000),
        ("0.01", 10_000),
    );
    i3.qty_unit_shift = -2;
    i3.venue_num_id = 10_459;
    let mut i4 = inst(
        4,
        "BTC-USDT",
        V6,
        InstKind::Spot,
        BTC,
        ("0.1", 10_000_000),
        ("0.00000001", 1),
        ("0.00001", 1_000),
    );
    i4.venue_num_id = 20_001;
    let mut i5 = inst(
        5,
        "1000PEPEUSDT",
        V1,
        InstKind::Perp,
        PEPE,
        ("0.0000001", 100),
        ("1", 1_000),
        ("1", 1_000),
    );
    i5.px_unit_shift = 3;
    i5.qty_unit_shift = 3;
    i5.max_qty = Qty(1_000_000_000);
    let insts = vec![
        inst(
            1,
            "BTCUSDT",
            V1,
            InstKind::Perp,
            BTC,
            ("0.10", 10_000_000),
            ("0.001", 100_000),
            ("0.001", 100_000),
        ),
        inst(
            2,
            "BTCUSDT",
            V2,
            InstKind::Spot,
            BTC,
            ("0.01", 1_000_000),
            ("0.00001", 1_000),
            ("0.00001", 1_000),
        ),
        i3,
        i4,
        i5,
    ];
    let instances = vec![
        InstanceSpec {
            id: StrategyId(1),
            kind_id: 0xFF00,
            name: "pc-main".into(),
        },
        InstanceSpec {
            id: StrategyId(2),
            kind_id: 0xFF00,
            name: "pc-sim-a".into(),
        },
        InstanceSpec {
            id: StrategyId(3),
            kind_id: 0xFF01,
            name: "probe-a".into(),
        },
    ];
    let accounts = vec![
        AccountSpec {
            id: AccountId(1),
            sim: false,
            bound_instance: StrategyId(1),
            is_prod: true,
            venues: vbits(&[1, 6]),
            label: "acct-a".into(),
            ..AccountSpec::default()
        },
        AccountSpec {
            id: AccountId(2),
            sim: true,
            bound_instance: StrategyId(2),
            is_prod: false,
            venues: vbits(&[1, 6]),
            label: "sim-a".into(),
            uid_policy: UidPolicy::Verified,
        },
    ];
    let account_venues = vec![
        AccountVenueSpec {
            account: AccountId(1),
            venue: V1,
            uid: "1001".into(),
        },
        AccountVenueSpec {
            account: AccountId(1),
            venue: V6,
            uid: "2001".into(),
        },
    ];
    let fees = vec![
        FeeSpec {
            venue: V1,
            tier: 0,
            fee: fee(200_000, 500_000),
        },
        FeeSpec {
            venue: V1,
            tier: 1,
            fee: fee(100_000, 400_000),
        },
        FeeSpec {
            venue: V2,
            tier: 0,
            fee: fee(200_000, 500_000),
        },
        FeeSpec {
            venue: V5,
            tier: 0,
            fee: fee(200_000, 500_000),
        },
        FeeSpec {
            venue: V6,
            tier: 0,
            fee: fee(800_000, 1_000_000),
        },
        FeeSpec {
            venue: V6,
            tier: 1,
            fee: fee(600_000, 800_000),
        },
    ];
    let sims = vec![SimSpec {
        account: AccountId(2),
        fee_tier: 1,
        latency_model: 0,
        margin_model: 0,
    }];
    let sim_balances = vec![
        SimBalanceSpec {
            account: AccountId(2),
            venue: V1,
            asset: USDT,
            qty: Qty(1_000_000_000_000),
        },
        SimBalanceSpec {
            account: AccountId(2),
            venue: V6,
            asset: USDT,
            qty: Qty(1_000_000_000_000),
        },
    ];
    let limits = vec![
        (
            V1,
            RateLimit {
                scope: LimitScope::Account,
                bucket: LimitBucket::Orders,
                limit: 300,
                window_ms: 10_000,
                default_cost: 1,
            },
        ),
        (
            V1,
            RateLimit {
                scope: LimitScope::Ip,
                bucket: LimitBucket::Weight,
                limit: 2_400,
                window_ms: 60_000,
                default_cost: 1,
            },
        ),
    ];
    let ep = |role, env, url: &str| Endpoint {
        role,
        env,
        url: url.into(),
        conn_group: 0,
    };
    let endpoints = vec![
        (
            V1,
            ep(
                EndpointRole::OrderWs,
                EndpointEnv::Prod,
                "wss://ws-fapi.binance.com/ws-fapi/v1",
            ),
        ),
        (
            V1,
            ep(
                EndpointRole::MdPublic,
                EndpointEnv::Prod,
                "wss://fstream.binance.com/public",
            ),
        ),
        (
            V1,
            ep(
                EndpointRole::MdPublic,
                EndpointEnv::Testnet,
                "wss://fstream.binancefuture.com/public",
            ),
        ),
    ];
    let aliases = vec![AliasSpec {
        venue: V2,
        alias: "BTC_USDT".into(),
        inst: InstId(2),
    }];
    Spec {
        version: 10,
        created: CREATED,
        assets,
        venues,
        caps,
        insts,
        instances,
        accounts,
        account_venues,
        fees,
        fee_overrides: Vec::new(),
        sims,
        sim_balances,
        limits,
        endpoints,
        maint: Vec::new(),
        tokens: Vec::new(),
        aliases,
        mmr: Vec::new(),
    }
}

pub fn f0() -> Vec<u8> {
    f0_spec().build()
}

/// E0: ECF1 v3, host c8, sections 1–8 (row_ver 1, one 8-byte row filled with the section id).
pub fn e0_builder() -> EcfBuilder {
    let mut b = EcfBuilder::new(3, HostClass::C8);
    b.created_utc_ns(CREATED);
    for s in EngineSection::ALL {
        b.section(s, 1, 8, 1, vec![s as u8; 8]);
    }
    b
}

pub fn e0() -> Vec<u8> {
    e0_builder().finish()
}

/// Shorthand for the chain token of REF-056.
pub fn usdt_bsc_token() -> ChainToken {
    let mut address = [0u8; 32];
    address[..20].copy_from_slice(&[0x55; 20]);
    ChainToken {
        chain: VenueId::CHAIN_BSC,
        asset: USDT,
        address,
        decimals: 18,
    }
}

/// A Sim balance of 10 000 USDT.
pub fn sim_usdt(account: u8, venue: VenueId) -> SimBalanceSpec {
    SimBalanceSpec {
        account: AccountId(account),
        venue,
        asset: USDT,
        qty: Qty(1_000_000_000_000),
    }
}

/// Maintenance window helper.
pub fn window(venue: VenueId, inst: u32, start: i64, end: i64, kind: hft_refdata::MaintKind) -> MaintWindow {
    MaintWindow {
        venue,
        inst: InstId(inst),
        start_ns: start,
        end_ns: end,
        kind,
    }
}

/// `universe(n)`: F0 with venue 1 filled up to n insts in total (`T<k>USDT`, steps of inst 1).
pub fn universe(n: usize) -> Spec {
    let mut s = f0_spec();
    let mut k = 0usize;
    while s.insts.len() < n {
        k += 1;
        let id = s.insts.len() as u32 + 1;
        s.insts.push(inst(
            id,
            &format!("T{k}USDT"),
            V1,
            InstKind::Perp,
            BTC,
            ("0.10", 10_000_000),
            ("0.001", 100_000),
            ("0.001", 100_000),
        ));
    }
    s
}

/// `universe6(n, accounts)`: n insts spread over the six core venues, `accounts` Live accounts
/// (even ids on Binance USD-M / OKX spot / Bitget futures, odd ids on the other product lines).
pub fn universe6(n: usize, n_accounts: usize) -> Spec {
    let core = [
        (
            VenueId::BINANCE_USDM,
            Exchange::Binance,
            InstKind::Perp,
            StaleGuardKind::RecvWindow,
        ),
        (
            VenueId::BINANCE_SPOT,
            Exchange::Binance,
            InstKind::Spot,
            StaleGuardKind::RecvWindow,
        ),
        (
            VenueId::OKX_SWAP,
            Exchange::Okx,
            InstKind::Perp,
            StaleGuardKind::ExpireAt,
        ),
        (
            VenueId::OKX_SPOT,
            Exchange::Okx,
            InstKind::Spot,
            StaleGuardKind::ExpireAt,
        ),
        (
            VenueId::BITGET_UTA_USDT_FUT,
            Exchange::Bitget,
            InstKind::Perp,
            StaleGuardKind::None,
        ),
        (
            VenueId::BITGET_UTA_SPOT,
            Exchange::Bitget,
            InstKind::Spot,
            StaleGuardKind::None,
        ),
    ];
    let mut s = f0_spec();
    s.version = 1;
    s.assets = vec![asset(
        1,
        "USDT",
        AssetClass::Stable,
        (-8, -8),
        1_000_000_000,
        1_000_000_000_000_000_000,
    )];
    s.venues = core
        .iter()
        .map(|(v, x, k, g)| venue(*v, *x, *k, *g, 32))
        .collect();
    s.caps = core
        .iter()
        .map(|(v, _, k, _)| {
            if *k == InstKind::Perp {
                perp_caps(*v)
            } else {
                spot_caps(*v, None)
            }
        })
        .collect();
    s.insts.clear();
    s.aliases.clear();
    s.fees = core
        .iter()
        .map(|(v, ..)| FeeSpec {
            venue: *v,
            tier: 0,
            fee: fee(200_000, 500_000),
        })
        .collect();
    let n_assets = n.div_ceil(core.len());
    for a in 0..n_assets {
        s.assets.push(asset(
            a as u16 + 2,
            &format!("A{a}"),
            AssetClass::Crypto,
            (-8, -8),
            1_000_000_000_000_000,
            100_000_000_000_000,
        ));
    }
    for k in 0..n {
        let (v, _, kind, _) = core[k % core.len()];
        let a = AssetId((k / core.len()) as u16 + 2);
        let mut i = inst(
            k as u32 + 1,
            &format!("A{}USDT", k / core.len()),
            v,
            kind,
            a,
            ("0.10", 10_000_000),
            ("0.001", 100_000),
            ("0.001", 100_000),
        );
        i.quote = AssetId(1);
        i.venue_num_id = k as u64 + 1;
        s.insts.push(i);
    }
    s.instances = (0..n_accounts)
        .map(|k| InstanceSpec {
            id: StrategyId(k as u8 + 1),
            kind_id: 0xFF00,
            name: format!("inst-{k}").into(),
        })
        .collect();
    s.accounts.clear();
    s.account_venues.clear();
    for k in 0..n_accounts {
        let id = k as u8 + 1;
        let vs: [u8; 3] = if k % 2 == 0 { [1, 6, 10] } else { [2, 5, 11] };
        s.accounts.push(AccountSpec {
            id: AccountId(id),
            bound_instance: StrategyId(id),
            is_prod: true,
            venues: vbits(&vs),
            label: format!("acct-{k}"),
            ..AccountSpec::default()
        });
        for v in vs {
            s.account_venues.push(AccountVenueSpec {
                account: AccountId(id),
                venue: VenueId(v),
                uid: format!("{id}-{v}"),
            });
        }
    }
    s.sims.clear();
    s.sim_balances.clear();
    s
}
