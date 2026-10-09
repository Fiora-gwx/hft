//! Baseline `B0` of crates/hft-sequencer/TESTS.md: refdata, EngineConfig, risk parameters, the
//! initial strategy parameters, the in-memory live environment (`TestIo`) and the harness `H`.
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::too_many_arguments
)] // allow-list: test code, not a hot path

pub mod strat;

use std::collections::{HashMap, VecDeque};

use hft_event::{
    AccountConfigReportHdr, BlobHdr, BlobKind, BlobRef, Body, CmdHdr, CmdScope, ClockStatus, ConnRole, ConnState,
    ConnStateKind, ControlCmd, EventKind, FeedCmd, GroupSpec, GwCmd, HostProfile, LegDef, LeverageItem, MarketHdr,
    ParamSource, RecordFlags, ReconcileReport, StrategyParamsHdr, Timer,
};
use hft_exec::{ExecConfig, ExecParams};
use hft_journal::RollReason;
use hft_ledger::LedgerConfig;
use hft_oms::OmsConfig;
use hft_refdata::{
    AccountSpec, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, EcfBuilder, EngineSection, Exchange,
    FeeAssetRule, FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec, InstStatus, InstanceSpec, LimitBucket,
    LimitScope, LiqMarker, MarginData, MarginMode, PositionMode, RateLimit, RdbBuilder, RefData, SimBalanceSpec, SimSpec,
    StaleGuardKind, TestnetKind, TsUnit, UidPolicy, VenueCaps, VenueSpec,
};
use hft_risk::{
    AcctLimits, DEFAULT_MARGIN, FixedVec, GroupLimits, GwMap, MarginLimits, PreVec, PxBand, RiskParams, VenueLimits,
};
use hft_rt::{CalibPair, Heartbeat};
use hft_sequencer::driver::live::{LiveCfg, LiveDriver, LiveIo, RingId};
use hft_sequencer::driver::RollInfo;
use hft_sequencer::{
    CapacitiesRow, ConfigSet, EngineSetup, MemJournal, MemRecord, ParsedConfig, ParsedParams, RingLimit, SegClock,
    SeqParamsRow, Sequencer, StepOutcome,
};
use hft_sequencer::types::{InputSlot, OutRing};
use hft_sequencer::parsed::parse_params;
use hft_strategy_sdk::{StrategySet, VenueSet};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, HostClass, InstId, KillBits, MarketKind, Notional, Px, Qty, Rate, RunMode,
    StrategyId, Tsc, VenueId,
};
use sha2::{Digest, Sha256};
use zerocopy::{FromZeros, IntoBytes};

pub use strat::*;

// ------------------------------------------------------------------------------- ids

/// Venue A: spot (Binance). TESTS.md B0 originally said `VenueId 1`; the fixed venue table makes
/// `VenueId 1` Binance USD-M, so A = `VenueId 2` (docs/changes.md).
pub const A: VenueId = VenueId::BINANCE_SPOT;
/// Venue B: USDT perpetual of another exchange (Gate).
pub const B: VenueId = VenueId::GATE_USDT_FUT;
pub const A1: InstId = InstId(11);
pub const A2: InstId = InstId(12);
pub const A3: InstId = InstId(13);
pub const B1: InstId = InstId(21);
pub const BTC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
pub const ETH: AssetId = AssetId(3);
pub const FILL: AssetId = AssetId(4);
pub const AC1: AccountId = AccountId(1);
pub const AC2: AccountId = AccountId(2);
pub const AC9: AccountId = AccountId(9);
pub const S1: StrategyId = StrategyId(1);
pub const S2: StrategyId = StrategyId(2);
pub const S9: StrategyId = StrategyId(9);
pub const G1: GroupId = GroupId(0x0100_0001);
pub const G2: GroupId = GroupId(0x0200_0001);
pub const G9: GroupId = GroupId(0x0900_0001);

/// 1 GHz in mHz (1 tick = 1 ns).
pub const HZ: u64 = 1_000_000_000_000;
/// 2026-10-01T00:30:00Z.
pub const UTC0: i64 = 1_790_814_600_000_000_000;
pub const MS: u64 = 1_000_000;
pub const SEC: u64 = 1_000_000_000;
/// First TSC reading of a run.
pub const T0: u64 = 1_000_000;

// ------------------------------------------------------------------------------- numbers

fn dec(s: &str, exp: i8) -> i64 {
    let t: String = s.chars().filter(|c| !c.is_whitespace() && *c != '_').collect();
    let (neg, body) = match t.strip_prefix('-') {
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

// ------------------------------------------------------------------------------- refdata

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

pub fn caps_spec(v: VenueId, perp: bool) -> VenueCaps {
    VenueCaps {
        venue: v,
        market: if perp { MarketKind::LinearPerp } else { MarketKind::Spot },
        fee_asset_rule: if perp { FeeAssetRule::Quote } else { FeeAssetRule::ReceivedAsset },
        reduce_only: perp,
        required_position_mode: if perp { PositionMode::OneWay } else { PositionMode::NotApplicable },
        required_margin_mode: if perp { MarginMode::Cross } else { MarginMode::NotApplicable },
        required_multi_asset: if perp { Some(false) } else { None },
        required_spot_borrow: None,
        funding_settle: if perp { FundingSettleKind::PerSymbolPush } else { FundingSettleKind::NotApplicable },
        margin_data: if perp { MarginData::Push } else { MarginData::NotApplicable },
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

pub fn inst_spec(id: InstId, v: VenueId, sym: &str, perp: bool, asset: AssetId, tick: (&str, i64), lot: (&str, i64)) -> InstSpec {
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
        l2_depth: 0,
        tick_str: tick.0.into(),
        lot_str: lot.0.into(),
        min_qty_str: lot.0.into(),
        funding_interval_s: if perp { 28_800 } else { 0 },
        max_leverage: if perp { 125 } else { 0 },
        ..InstSpec::default()
    }
}

fn asset(id: u16, name: &str, class: AssetClass, dust: i64) -> AssetSpec {
    AssetSpec {
        id: AssetId(id),
        name: name.into(),
        class,
        exp: AssetExp { px_exp: -2, qty_exp: -8 },
        px_max: Px(1_000_000_000_000_000),
        qty_max: Qty(1_000_000_000_000_000_000),
        dust: Qty(dust),
    }
}

pub fn vbits(vs: &[VenueId]) -> u64 {
    vs.iter().fold(0, |m, v| m | 1u64 << v.0)
}

/// The editable refdata input of B0.
#[derive(Clone, Debug)]
pub struct Rd {
    pub version: u64,
    pub insts: Vec<InstSpec>,
    pub accounts: Vec<AccountSpec>,
    pub instances: Vec<InstanceSpec>,
    pub caps: Vec<VenueCaps>,
    pub limits: Vec<(VenueId, RateLimit)>,
}

pub fn order_limits(v: VenueId, l10: u32, l60: u32) -> Vec<(VenueId, RateLimit)> {
    vec![
        (v, RateLimit { scope: LimitScope::Account, bucket: LimitBucket::Orders, limit: l10, window_ms: 10_000, default_cost: 1 }),
        (v, RateLimit { scope: LimitScope::Account, bucket: LimitBucket::Orders, limit: l60, window_ms: 60_000, default_cost: 1 }),
    ]
}

/// The B0 refdata (ids dense from 1: filler insts, accounts and instances fill the gaps).
pub fn b0_rd() -> Rd {
    let mut insts = Vec::new();
    for id in 1..=21u32 {
        let i = InstId(id);
        let s = match id {
            11 => inst_spec(i, A, "BTCUSDT", false, BTC, ("0.01", 1), ("0.0001", 10_000)),
            12 => inst_spec(i, A, "ETHUSDT", false, ETH, ("0.01", 1), ("0.001", 100_000)),
            13 => inst_spec(i, A, "SOLUSDT", false, FILL, ("0.01", 1), ("0.001", 100_000)),
            21 => inst_spec(i, B, "BTC_USDT", true, BTC, ("0.1", 10), ("0.001", 100_000)),
            _ => {
                let mut s = inst_spec(i, A, &format!("F{id}USDT"), false, FILL, ("0.01", 1), ("0.001", 100_000));
                s.status = InstStatus::Trading;
                s
            }
        };
        insts.push(s);
    }
    let mut accounts = Vec::new();
    for a in 1..=9u8 {
        let s = match a {
            1 => AccountSpec {
                id: AccountId(1),
                sim: false,
                bound_instance: S1,
                is_prod: false,
                venues: vbits(&[A, B]),
                label: "acct-1".into(),
                uid_policy: UidPolicy::Verified,
            },
            2 => AccountSpec {
                id: AccountId(2),
                sim: false,
                bound_instance: S2,
                is_prod: false,
                venues: vbits(&[A, B]),
                label: "acct-2".into(),
                uid_policy: UidPolicy::Verified,
            },
            9 => AccountSpec {
                id: AccountId(9),
                sim: true,
                bound_instance: S9,
                is_prod: false,
                venues: vbits(&[A, B]),
                label: "sim-9".into(),
                uid_policy: UidPolicy::Verified,
            },
            _ => AccountSpec {
                id: AccountId(a),
                sim: false,
                bound_instance: StrategyId(0),
                is_prod: false,
                venues: 0,
                label: format!("fill-{a}"),
                uid_policy: UidPolicy::Verified,
            },
        };
        accounts.push(s);
    }
    let instances = (1..=9u8)
        .map(|i| InstanceSpec {
            id: StrategyId(i),
            kind_id: match i {
                2 => TARGET,
                _ => TAKER,
            },
            name: format!("s{i}").into(),
        })
        .collect();
    let mut limits = order_limits(A, 100, 600);
    limits.extend(order_limits(B, 100, 600));
    Rd {
        version: 1,
        insts,
        accounts,
        instances,
        caps: vec![caps_spec(A, false), caps_spec(B, true)],
        limits,
    }
}

impl Rd {
    pub fn blob(&self) -> Vec<u8> {
        let mut b = RdbBuilder::new(self.version);
        b.created_utc_ns(UTC0 - 86_400_000_000_000);
        b.asset(asset(1, "BTC", AssetClass::Crypto, 1_000));
        b.asset(asset(2, "USDT", AssetClass::Stable, 1_000_000));
        b.asset(asset(3, "ETH", AssetClass::Crypto, 1_000));
        b.asset(asset(4, "FILL", AssetClass::Crypto, 1_000));
        b.venue(venue_spec(A, Exchange::Binance, InstKind::Spot));
        b.venue(venue_spec(B, Exchange::Gate, InstKind::Perp));
        for c in &self.caps {
            b.caps(c.clone());
        }
        for i in &self.insts {
            b.inst(i.clone());
        }
        for i in &self.instances {
            b.instance(i.clone());
        }
        for a in &self.accounts {
            b.account(a.clone());
        }
        for a in &self.accounts {
            if a.sim {
                continue;
            }
            for v in [A, B] {
                if a.venues & (1u64 << v.0) != 0 {
                    b.account_venue(AccountVenueSpec { account: a.id, venue: v, uid: format!("uid-{}-{}", a.id.0, v.0) });
                }
            }
        }
        for v in [A, B] {
            for tier in 0..=1u8 {
                b.fee(FeeSpec { venue: v, tier, fee: FeePair { maker: Rate(200_000), taker: Rate(500_000) } });
            }
        }
        for a in &self.accounts {
            if !a.sim {
                continue;
            }
            b.sim(SimSpec { account: a.id, fee_tier: 0, latency_model: 0, margin_model: 0 });
            for v in [A, B] {
                if a.venues & (1u64 << v.0) != 0 {
                    b.sim_balance(SimBalanceSpec { account: a.id, venue: v, asset: USDT, qty: Qty(1_000_000_000_000_000) });
                }
            }
        }
        for (v, l) in &self.limits {
            b.limit(*v, *l);
        }
        b.finish()
    }
    pub fn load(&self) -> RefData {
        match RefData::load(&self.blob(), None) {
            Ok(r) => r,
            Err(e) => panic!("B0 refdata rejected: {e:?}"),
        }
    }
}

// ------------------------------------------------------------------------------- engine config

/// B0 section 3.
pub fn b0_caps() -> CapacitiesRow {
    CapacitiesRow {
        max_insts: 64,
        max_cells_total: 32,
        max_timers: 256,
        recent_cmd_ids: 64,
        max_subs_per_inst: 4,
        max_groups_per_instance: 8,
        max_instances: 4,
        _pad: 0,
    }
}

/// B0 section 5.
pub fn b0_seq() -> SeqParamsRow {
    let mut rings = [RingLimit::default(); 8];
    rings[1] = RingLimit { per_ring: 4, total: 4 };
    rings[2] = RingLimit { per_ring: 4, total: 4 };
    rings[3] = RingLimit { per_ring: 32, total: 64 };
    rings[4] = RingLimit { per_ring: 32, total: 0 };
    rings[5] = RingLimit { per_ring: 32, total: 128 };
    rings[6] = RingLimit { per_ring: 16, total: 16 };
    SeqParamsRow {
        rings,
        md_round_ns: 50_000,
        ctl_bytes_per_round: 8_192,
        keyframe_period_ms: 300_000,
        keyframe_max_events: 1_500_000,
        advisor_mem_budget_mib: 256,
        advisor_ckpt_max_mib: 1_024,
        sim_max_per_round: 32,
        conflate_backlog: 64,
        notify_per_continue: 64,
        replay_per_round: 1_024,
        auto_resume_after_clean_drain: 1,
        allow_missing_standby: 0,
        _pad: [0; 6],
    }
}

/// An ECF1 blob with sections 3 and 5 (raw rows).
pub fn engine_blob_rows(version: u64, caps: &[u8], seq: &[u8], seq_rows: u32, seq_row_size: u32) -> Vec<u8> {
    let mut b = EcfBuilder::new(version, HostClass::Dev2);
    b.created_utc_ns(UTC0 - 86_400_000_000_000);
    b.section(EngineSection::Capacities, 1, 24, 1, caps.to_vec());
    b.section(EngineSection::Sequencer, 1, seq_row_size, seq_rows, seq.to_vec());
    b.finish()
}

pub fn engine_blob(caps: &CapacitiesRow, seq: &SeqParamsRow) -> Vec<u8> {
    engine_blob_rows(1, caps.as_bytes(), seq.as_bytes(), 1, 72)
}

// ------------------------------------------------------------------------------- risk

pub fn band(dev: &str, ioc: &str, rr: &str) -> PxBand {
    PxBand { max_px_dev: r(dev), ioc_max_cross: r(ioc), rr_max_cross: r(rr) }
}

pub fn group_limits(max_pos: &str) -> GroupLimits {
    GroupLimits { max_group_notional: n("50000"), max_pos_qty: q(max_pos), naked_gate: r("0.5"), naked_gate_min: n("100") }
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
        margin: MarginLimits { margin_stale_ms: 15_000, ..DEFAULT_MARGIN },
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

/// B0 risk parameters (every account with a row; venue rows for the trading accounts).
pub fn b0_risk(rd: &RefData) -> RiskParams {
    let mut classes = FixedVec::new();
    classes.push(band("0.03", "0.005", "0.01"));
    let mut accounts = Vec::new();
    let mut venues = Vec::new();
    for a in 1..=9u8 {
        accounts.push(acct_limits(a));
        let m = rd.account(AccountId(a));
        for v in [A, B] {
            if m.venues & (1u64 << v.0) != 0 {
                venues.push(venue_limits(v, a));
            }
        }
    }
    RiskParams {
        version: 1,
        generated_at_ns: 0,
        author: [0; 16],
        rate_headroom: Rate(0),
        conn_liveness_ms: 2_000,
        max_perp_leverage_x100: 300,
        l3_max_cross: Rate(0),
        risk_class: PreVec::from_vec(Vec::new()),
        accounts: PreVec::from_vec(accounts),
        venues: PreVec::from_vec(venues),
        classes,
        groups: PreVec::from_vec(Vec::new()),
    }
}

/// Test `RiskParams` encoding (as hft-ctl's tests): `b"RSK1"`, a flag byte, anything.
pub fn risk_blob(flags: u8) -> Vec<u8> {
    let mut b = b"RSK1".to_vec();
    b.push(flags);
    b.extend(std::iter::repeat_n(7u8, 59));
    b
}

pub fn risk_decode(b: &[u8], a: AccountId, rd: &RefData) -> Result<RiskParams, u16> {
    if b.get(..4) != Some(b"RSK1") {
        return Err(hft_sequencer::codes::detail::BAD_VALUE);
    }
    let f = b.get(4).copied().unwrap_or(0);
    if f & 1 != 0 || (f & 2 != 0 && a.0 != 0) {
        return Err(104);
    }
    Ok(b0_risk(rd))
}

pub fn sha(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

// ------------------------------------------------------------------------------- params

pub fn leg(inst: InstId, venue: VenueId) -> LegDef {
    LegDef { inst, venue, kind: LegDef::CEX, _pad: 0, route: 0, _pad2: 0 }
}

pub fn group(id: GroupId, asset: AssetId, legs: &[LegDef]) -> GroupSpec {
    let none = LegDef { inst: InstId(0), venue: VenueId(0), kind: 0, _pad: 0, route: 0, _pad2: 0 };
    let mut l = [none; 4];
    l[..legs.len()].copy_from_slice(legs);
    GroupSpec { id, asset, n_legs: legs.len() as u8, open_allowed: 1, legs: l, _rsv: 0 }
}

/// The B0 group (legs a1 + b1, BTC).
pub fn gspec(id: GroupId) -> GroupSpec {
    group(id, BTC, &[leg(A1, A), leg(B1, B)])
}

/// An SPB1 blob of a test kind.
#[derive(Clone, Debug)]
pub struct Spb {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
    pub schema_ver: u16,
    pub version: u64,
    pub digest: u64,
    pub groups: Vec<GroupSpec>,
    pub values: [i64; 5],
    pub hwm: Option<u32>,
}

impl Spb {
    pub fn new(s: StrategyId, a: AccountId, kind_id: u16, version: u64, groups: Vec<GroupSpec>) -> Spb {
        Spb { strategy: s, account: a, kind_id, schema_ver: 1, version, digest: digest(kind_id), groups, values: [10, 5, 0, 0, 0], hwm: None }
    }
    pub fn s1(version: u64) -> Spb {
        Spb::new(S1, AC1, TAKER, version, vec![gspec(G1)])
    }
    pub fn s2(version: u64) -> Spb {
        Spb::new(S2, AC2, TARGET, version, vec![gspec(G2)])
    }
    pub fn s9(version: u64) -> Spb {
        Spb::new(S9, AC9, TAKER, version, vec![gspec(G9)])
    }
    pub fn bytes(&self) -> Vec<u8> {
        let body = body(self.values, self.groups.len() as u16);
        let hwm = self.hwm.unwrap_or_else(|| self.groups.iter().map(|g| g.id.counter()).max().unwrap_or(0).max(1000));
        let h = StrategyParamsHdr {
            strategy: self.strategy,
            account: self.account,
            kind_id: self.kind_id,
            schema_id: u32::from(self.kind_id),
            schema_ver: self.schema_ver,
            source: ParamSource::Manual,
            _pad: 0,
            n_groups: self.groups.len() as u16,
            _pad2: 0,
            version: self.version,
            generated_at_ns: 0,
            body_len: body.len() as u32,
            group_ctr_hwm: hwm,
            schema_digest: self.digest,
        };
        let mut b = h.as_bytes().to_vec();
        for g in &self.groups {
            b.extend_from_slice(g.as_bytes());
        }
        b.extend_from_slice(&body);
        b
    }
    pub fn blob_ref(&self) -> BlobRef {
        let b = self.bytes();
        BlobRef { kind: BlobKind::StrategyParams, encoding: 1, len: b.len() as u32, version: self.version, sha256: sha(&b) }
    }
}

pub fn parsed<S: StrategySet>(spb: &Spb, rd: &RefData) -> ParsedParams<S> {
    let b = spb.bytes();
    match parse_params::<S>(&b, sha(&b), rd, 8, None) {
        Ok(p) => p,
        Err(d) => panic!("params rejected: {d:#x}"),
    }
}

// ------------------------------------------------------------------------------- assembly

/// B0 `EngineSetup`.
pub fn b0_setup(rd: &RefData) -> EngineSetup {
    let caps = b0_caps();
    let seq = b0_seq();
    let mut oms = OmsConfig::new(1, HZ);
    oms.max_out_per_event = 16;
    oms.links_per_account = 4;
    oms.slots = hft_oms::PerTable::new(16);
    let mut exec = ExecConfig { default: ExecParams::default(), accounts: Vec::new() };
    exec.default = ExecParams::default();
    let mut keys = Vec::new();
    for a in [AC1, AC2] {
        for v in [A, B] {
            keys.push((a, v, 3u8));
        }
    }
    let _ = rd;
    EngineSetup {
        caps,
        seq,
        oms,
        ledger: LedgerConfig::default(),
        exec,
        gws: GwMap { venues: vec![A, B] },
        feeds: VenueSet(vbits(&[A, B])),
        keys_required: keys,
        role_prod: false,
        advisor_max_lag_ms: 5_000,
        drain_timeout_ms: 0,
        risk_decode: Some(risk_decode),
        clock: SegClock { tsc_hz_milli: HZ, calib_tsc: T0, calib_realtime_ns: UTC0 },
        base_engine_ns: 0,
    }
}

/// Everything a B0 boot needs; tests edit fields before `boot`.
pub struct B0<S: StrategySet = TestSet> {
    pub rd: Rd,
    pub caps: CapacitiesRow,
    pub seq: SeqParamsRow,
    pub setup_edit: Vec<fn(&mut EngineSetup)>,
    pub params: Vec<Spb>,
    pub live: LiveCfg,
    pub risk: Option<RiskParams>,
    _s: core::marker::PhantomData<S>,
}

pub fn b0() -> B0<TestSet> {
    B0::new()
}

impl<S: StrategySet> B0<S> {
    pub fn new() -> B0<S> {
        B0 {
            rd: b0_rd(),
            caps: b0_caps(),
            seq: b0_seq(),
            setup_edit: Vec::new(),
            params: vec![Spb::s1(1), Spb::s2(1), Spb::s9(1)],
            live: LiveCfg { feed_stage_cap: 1024, ..LiveCfg::default() },
            risk: None,
            _s: core::marker::PhantomData,
        }
    }
    pub fn configs(&self, rd: &RefData) -> ConfigSet<S> {
        let rb = risk_blob(0);
        ConfigSet {
            refdata_blob: self.rd.blob(),
            engine_blob: engine_blob(&self.caps, &self.seq),
            risk_sha256: sha(&rb),
            risk_blob: rb,
            risk: self.risk.clone().unwrap_or_else(|| b0_risk(rd)),
            params: self.params.iter().map(|p| (p.bytes(), parsed::<S>(p, rd))).collect(),
        }
    }
    pub fn setup(&self, rd: &RefData) -> EngineSetup {
        let mut s = b0_setup(rd);
        s.caps = self.caps;
        s.seq = self.seq;
        for f in &self.setup_edit {
            f(&mut s);
        }
        s
    }
    /// Boots an empty start with the live driver over `TestIo`.
    pub fn try_boot(self) -> Result<H<S>, hft_sequencer::BootError> {
        let rd = self.rd.load();
        let setup = self.setup(&rd);
        let cfg = self.configs(&rd);
        let io = TestIo::new(setup.clock);
        let drv = LiveDriver::new(io, self.live.clone());
        let hb = hb();
        let s = Sequencer::boot(hft_sequencer::BootSource::Empty { configs: cfg }, &setup, drv, hb)?;
        Ok(H { s, rd, setup, cmd_no: 0, hb, upd: 1 })
    }
    pub fn boot(self) -> H<S> {
        match self.try_boot() {
            Ok(h) => h,
            Err(e) => panic!("B0 boot failed: {e:?}"),
        }
    }
}

impl<S: StrategySet> Default for B0<S> {
    fn default() -> Self {
        B0::new()
    }
}

pub fn hb() -> &'static Heartbeat {
    Box::leak(Box::new(Heartbeat::new()))
}

// ------------------------------------------------------------------------------- TestIo

/// One ring element.
#[derive(Clone, Debug)]
pub struct Elem {
    pub kind: EventKind,
    pub flags: RecordFlags,
    pub body: Vec<u8>,
}

/// Arrival scheduled after the n-th hand-over.
#[derive(Clone, Debug)]
pub struct Arrival {
    pub after_take: u64,
    pub ring: RingId,
    pub elem: Elem,
}

/// The in-memory live environment.
pub struct TestIo<S: StrategySet> {
    pub j: MemJournal,
    pub tsc: u64,
    /// TSC added on every hand-over (processing time).
    pub tsc_per_take: u64,
    pub rings: HashMap<RingId, VecDeque<Elem>>,
    pub n_rep: u8,
    pub n_bbo: u8,
    pub n_md: u8,
    pub chain: bool,
    pub out: Vec<(OutRing, GwCmd)>,
    /// Free slots per out ring (`None` = unbounded).
    pub out_free: HashMap<OutRing, usize>,
    pub feed: Vec<(VenueId, FeedCmd)>,
    pub feed_free: Option<usize>,
    pub mailbox: Vec<ParsedConfig<S>>,
    pub retired: Vec<ParsedConfig<S>>,
    pub kills: Vec<Vec<KillBits>>,
    pub calib: CalibPair,
    pub hz: u64,
    pub takes: u64,
    pub arrivals: Vec<Arrival>,
    /// Ring ids handed over, in order.
    pub handed: Vec<Option<RingId>>,
    pub tsc_reads: u64,
    /// `HostProfile.build_git` of this binary.
    pub build: [u8; 20],
    /// The old journal of a cold start.
    pub old: Option<Log>,
}

impl<S: StrategySet> TestIo<S> {
    pub fn new(c: SegClock) -> TestIo<S> {
        TestIo {
            j: MemJournal::new(c),
            tsc: c.calib_tsc,
            tsc_per_take: 0,
            rings: HashMap::new(),
            n_rep: 2,
            n_bbo: 2,
            n_md: 2,
            chain: false,
            out: Vec::new(),
            out_free: HashMap::new(),
            feed: Vec::new(),
            feed_free: None,
            mailbox: Vec::new(),
            retired: Vec::new(),
            kills: Vec::new(),
            calib: CalibPair { tsc: Tsc(c.calib_tsc), realtime_ns: c.calib_realtime_ns, phc_ns: 0, err_ns: 0 },
            hz: c.tsc_hz_milli,
            takes: 0,
            arrivals: Vec::new(),
            handed: Vec::new(),
            tsc_reads: 0,
            build: [0; 20],
            old: None,
        }
    }
    pub fn ring(&mut self, r: RingId) -> &mut VecDeque<Elem> {
        self.rings.entry(r).or_default()
    }
}

impl<S: StrategySet> LiveIo<S> for TestIo<S> {
    type J = MemJournal;
    fn journal(&mut self) -> &mut MemJournal {
        &mut self.j
    }
    fn tsc(&mut self) -> Tsc {
        self.tsc_reads += 1;
        Tsc(self.tsc)
    }
    fn n_rep(&self) -> u8 {
        self.n_rep
    }
    fn n_bbo(&self) -> u8 {
        self.n_bbo
    }
    fn n_md(&self) -> u8 {
        self.n_md
    }
    fn has_chain(&self) -> bool {
        self.chain
    }
    fn len(&mut self, r: RingId) -> usize {
        self.rings.get(&r).map_or(0, |q| q.len())
    }
    fn peek_len(&mut self, r: RingId) -> Option<usize> {
        self.rings.get(&r).and_then(|q| q.front()).map(|e| e.body.len())
    }
    fn peek_inst(&mut self, r: RingId) -> Option<InstId> {
        let e = self.rings.get(&r)?.front()?;
        (e.kind == EventKind::BBO).then(|| hft_sequencer::types::read_body::<hft_event::Bbo>(&e.body).map_or(InstId(0), |b| b.h.inst))
    }
    fn pop(&mut self, r: RingId, out: &mut InputSlot) -> bool {
        let Some(e) = self.rings.get_mut(&r).and_then(|q| q.pop_front()) else { return false };
        out.set_bytes(e.kind, e.flags, Tsc(self.tsc), &e.body);
        true
    }
    fn push_out(&mut self, ring: OutRing, c: &GwCmd) -> bool {
        if let Some(f) = self.out_free.get_mut(&ring) {
            if *f == 0 {
                return false;
            }
            *f -= 1;
        }
        self.out.push((ring, *c));
        true
    }
    fn push_feed(&mut self, v: VenueId, c: &FeedCmd) -> bool {
        if let Some(f) = self.feed_free.as_mut() {
            if *f == 0 {
                return false;
            }
            *f -= 1;
        }
        self.feed.push((v, *c));
        true
    }
    fn take_parsed(&mut self, r: &BlobRef) -> Option<ParsedConfig<S>> {
        let ix = self.mailbox.iter().position(|p| p.sha256() == &r.sha256)?;
        Some(self.mailbox.remove(ix))
    }
    fn retire(&mut self, p: ParsedConfig<S>) {
        self.retired.push(p);
    }
    fn write_kill(&mut self, bits: &[KillBits]) {
        self.kills.push(bits.to_vec());
    }
    fn roll_info(&mut self) -> RollInfo {
        RollInfo { utc_ns: self.utc_of(Tsc(self.tsc)), calib: self.calib, tsc_hz_milli: self.hz }
    }
    fn host_profile(&self) -> HostProfile {
        let mut h = hft_sequencer::snapshot::host_profile(self.hz);
        h.build_git = self.build;
        h
    }
    fn recovery_log(&mut self) -> Option<Log> {
        self.old.take()
    }
    fn clock_status(&self) -> ClockStatus {
        hft_sequencer::snapshot::clock_status(self.calib, self.hz)
    }
    fn utc_of(&self, t: Tsc) -> i64 {
        let d = t.0 as i128 - self.calib.tsc.0 as i128;
        self.calib.realtime_ns + (d * 1_000_000_000_000 / i128::from(self.hz.max(1))) as i64
    }
    fn after_take(&mut self, r: Option<RingId>) {
        self.takes += 1;
        self.handed.push(r);
        self.tsc += self.tsc_per_take;
        let n = self.takes;
        let due: Vec<Arrival> = self.arrivals.iter().filter(|a| a.after_take == n).cloned().collect();
        self.arrivals.retain(|a| a.after_take != n);
        for a in due {
            self.ring(a.ring).push_back(a.elem);
        }
    }
}

// ------------------------------------------------------------------------------- harness

pub type Drv<S> = LiveDriver<S, TestIo<S>>;

/// The B0 harness: a booted sequencer over the live driver and `TestIo`.
pub struct H<S: StrategySet = TestSet> {
    pub s: Sequencer<Drv<S>, S>,
    pub rd: RefData,
    pub setup: EngineSetup,
    pub cmd_no: u64,
    pub hb: &'static Heartbeat,
    /// Next Bbo `update_id`.
    pub upd: u64,
}

pub fn elem<B: Body>(b: &B) -> Elem {
    Elem { kind: B::KIND, flags: RecordFlags(0), body: b.as_bytes().to_vec() }
}

pub fn bbo(i: InstId, venue: VenueId, conn: u8, bid: &str, ask: &str, upd: u64, rx: u64) -> hft_event::Bbo {
    let mut h = MarketHdr::new_zeroed();
    h.venue = venue;
    h.conn = hft_types::ConnId(conn);
    h.inst = i;
    h.rx_tsc = rx;
    hft_event::Bbo { h, bid_px: px(bid), bid_qty: q("1"), ask_px: px(ask), ask_qty: q("1"), update_id: upd }
}

pub fn conn(venue: VenueId, k: u8, state: ConnStateKind, role: ConnRole, inst: InstId, account: AccountId, rx: u64) -> ConnState {
    ConnState { venue, conn: k, state, role, inst, reason: 0, account, _pad2: [0; 3], rx_tsc: rx }
}

pub fn sys_timer(id: u32, tsc: u64) -> Timer {
    Timer { timer_id: id, owner: 1, _pad: 0, group: GroupId(0), _pad2: 0, deadline_tsc: tsc }
}

pub fn scope(s: StrategyId, a: AccountId, g: GroupId) -> CmdScope {
    CmdScope { strategy: s, venue: VenueId(0), account: a, _pad: 0, group: g, inst: InstId(0), _pad2: 0 }
}

pub fn cfg_hdr(a: AccountId, v: VenueId, n_lev: u16) -> AccountConfigReportHdr {
    let perp = v.market() == Some(MarketKind::LinearPerp);
    let (pm, mm, ma) = if perp { (2, 2, 2) } else { (1, 1, 1) };
    let mut h = AccountConfigReportHdr::new_zeroed();
    h.venue = v;
    h.account = a;
    h.position_mode = pm;
    h.margin_mode = mm;
    h.multi_asset = ma;
    h.source = 1;
    h.uid_keys = 3;
    h.n_lev = n_lev;
    let uid = format!("uid-{}-{}", a.0, v.0);
    h.venue_uid[..uid.len()].copy_from_slice(uid.as_bytes());
    h
}

pub fn recon_summary(a: AccountId, v: VenueId, report_id: u64, view_seq: u64) -> ReconcileReport {
    let mut x = ReconcileReport::new_zeroed();
    x.report_id = report_id;
    x.venue = v;
    x.account = a;
    x.item = 5;
    x.count = 1;
    x.view_seq = view_seq;
    x
}

impl<S: StrategySet> H<S> {
    pub fn io(&mut self) -> &mut TestIo<S> {
        &mut self.s.driver_mut().io
    }
    pub fn io_ref(&self) -> &TestIo<S> {
        &self.s.driver().io
    }
    pub fn j(&self) -> &MemJournal {
        &self.s.driver().io.j
    }
    pub fn recs(&self) -> Vec<MemRecord> {
        self.j().records().cloned().collect()
    }
    /// Number of records so far (a mark for `since`).
    pub fn mark(&self) -> usize {
        self.j().len()
    }
    pub fn since(&self, m: usize) -> Vec<MemRecord> {
        self.j().records().skip(m).cloned().collect()
    }
    pub fn at(&mut self, tsc: u64) {
        self.io().tsc = tsc;
    }
    pub fn adv(&mut self, d: u64) {
        self.io().tsc += d;
    }
    pub fn now(&self) -> u64 {
        self.io_ref().tsc
    }
    pub fn push_e(&mut self, r: RingId, e: Elem) {
        self.io().ring(r).push_back(e);
    }
    pub fn push<B: Body>(&mut self, r: RingId, b: &B) {
        self.push_e(r, elem(b));
    }
    pub fn push_bytes(&mut self, r: RingId, kind: EventKind, body: Vec<u8>) {
        self.push_e(r, Elem { kind, flags: RecordFlags(0), body });
    }
    pub fn step(&mut self) -> StepOutcome {
        self.s.step()
    }
    /// Steps until a round is Idle (at most `max` rounds).
    pub fn run_max(&mut self, max: usize) -> StepOutcome {
        let mut last = StepOutcome::Idle;
        for _ in 0..max {
            last = self.s.step();
            if !matches!(last, StepOutcome::Busy) {
                return last;
            }
        }
        last
    }
    pub fn run(&mut self) -> StepOutcome {
        self.run_max(10_000)
    }
    pub fn trace_on(&mut self) {
        let e = self.s.engine_mut();
        e.trace.on = true;
        e.trace.ev.clear();
    }
    pub fn trace(&self) -> Vec<&'static str> {
        self.s.engine().trace.names()
    }

    /// A ControlCmd element (`role`: Viewer 1, Trader 2, Admin 4).
    pub fn cmd_elem(&mut self, issuer: &str, role: u8, account: AccountId, c: &ControlCmd<'_>, f: impl FnOnce(&mut CmdHdr)) -> (Elem, [u8; 16]) {
        self.cmd_no += 1;
        let mut h = CmdHdr::zeroed();
        h.cmd_id[..8].copy_from_slice(&self.cmd_no.to_le_bytes());
        h.cmd_id[15] = 0xC7;
        h.issuer[..issuer.len()].copy_from_slice(issuer.as_bytes());
        h.role = role;
        h.account = account;
        h.cmd_ver = 1;
        f(&mut h);
        let mut buf = vec![0u8; 8192];
        let n = match c.encode(&h, &mut buf) {
            Ok(n) => n,
            Err(e) => panic!("encode: {e:?}"),
        };
        buf.truncate(n);
        let id = h.cmd_id;
        (Elem { kind: EventKind::CONTROL_CMD, flags: RecordFlags(0), body: buf }, id)
    }
    /// Queues a command on the ctl ring (Admin, `cli:test`).
    pub fn cmd(&mut self, account: AccountId, c: &ControlCmd<'_>) -> [u8; 16] {
        self.cmd_with("cli:test", 4, account, c, |_| {})
    }
    pub fn cmd_with(&mut self, issuer: &str, role: u8, account: AccountId, c: &ControlCmd<'_>, f: impl FnOnce(&mut CmdHdr)) -> [u8; 16] {
        let (e, id) = self.cmd_elem(issuer, role, account, c, f);
        self.push_e(RingId::Ctl, e);
        id
    }
    /// A confirmed command (`confirmed = 1`, `expect_state_version` = the current value).
    pub fn cmd_confirmed(&mut self, account: AccountId, c: &ControlCmd<'_>) -> [u8; 16] {
        let v = self.s.engine().risk.mode.ctl_state_version();
        self.cmd_with("cli:test", 4, account, c, |h| {
            h.confirmed = 1;
            h.expect_state_version = v;
        })
    }
    /// The `CmdResult` of a command in the journal.
    pub fn result_of(&self, id: [u8; 16]) -> Option<hft_event::CmdResult> {
        self.j()
            .records()
            .filter(|r| r.kind == EventKind::CMD_RESULT)
            .filter_map(|r| r.read::<hft_event::CmdResult>())
            .find(|c| c.cmd_id == id)
    }
    /// Queues a ConfigBlob + SetParams pair and puts the parsed object in the Mailbox.
    pub fn set_params(&mut self, spb: &Spb) -> [u8; 16] {
        let b = spb.bytes();
        let r = spb.blob_ref();
        let p = {
            let slot = self.s.instances().get(spb.strategy);
            let base = slot
                .and_then(|s| s.cur.as_ref())
                .map(|c| hft_sequencer::parsed::ParamBase { params: &c.params, groups: &c.groups });
            match parse_params::<S>(&b, r.sha256, &self.s.engine().rd, 8, base) {
                Ok(p) => p,
                Err(d) => panic!("params rejected: {d:#x}"),
            }
        };
        self.io().mailbox.push(ParsedConfig::StrategyParams(Box::new(p)));
        self.blob_input(r, scope(spb.strategy, spb.account, GroupId(0)), &b);
        self.cmd_confirmed(spb.account, &ControlCmd::SetParams { scope: scope(spb.strategy, spb.account, GroupId(0)), blob: r })
    }
    /// Queues a `ConfigBlob` Input on the ctl ring.
    pub fn blob_input(&mut self, r: BlobRef, sc: CmdScope, content: &[u8]) {
        let h = BlobHdr { blob: r, scope: sc, boot: 0, _pad: [0; 7] };
        let mut body = h.as_bytes().to_vec();
        body.extend_from_slice(content);
        self.push_bytes(RingId::Ctl, EventKind::CONFIG_BLOB, body);
    }

    /// The "steady" state of B0: EngineClock, every gate released, Live mode, BBO connections Up
    /// with every inst on slot 0, quotes on a1 / b1.
    pub fn steady(&mut self) {
        self.step();
        let t = self.now();
        for v in [A, B] {
            self.push(RingId::Rep(0), &conn(v, 0, ConnStateKind::Up, ConnRole::Md, InstId(0), AccountId(0), t));
            for i in [A1, A2, A3, B1] {
                if self.rd.inst(i).venue == v {
                    self.push(RingId::Rep(0), &conn(v, 0, ConnStateKind::Up, ConnRole::Md, i, AccountId(0), t));
                }
            }
        }
        for a in [AC1, AC2, AC9] {
            for v in [A, B] {
                for (k, role) in [(0u8, ConnRole::Order), (1, ConnRole::Order), (0, ConnRole::UserStream)] {
                    let ring = if a == AC9 { RingId::RepSim } else { RingId::Rep(0) };
                    self.push(ring, &conn(v, k, ConnStateKind::Up, role, InstId(0), a, t));
                }
            }
        }
        self.run();
        self.push(RingId::Hk, &sys_timer(8, t));
        let mut rid = 1_000;
        for a in [AC1, AC2] {
            for v in [A, B] {
                let levs: Vec<LeverageItem> = if v == B { vec![LeverageItem { inst: B1, _pad: 0, leverage: r("3.00") }] } else { Vec::new() };
                let h = cfg_hdr(a, v, levs.len() as u16);
                let mut body = h.as_bytes().to_vec();
                for l in &levs {
                    body.extend_from_slice(l.as_bytes());
                }
                self.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
                rid += 1;
                self.push(RingId::Rep(0), &recon_summary(a, v, rid, 0));
            }
            self.push(RingId::Hk, &sys_timer(16 | (u32::from(a.0) << 16), t));
        }
        self.run();
        self.quote(A1, "50000", "50001");
        self.quote(B1, "50010", "50011");
        self.run();
        let cells: Vec<(StrategyId, AccountId, GroupId)> = self
            .s
            .instances()
            .slots
            .iter()
            .filter_map(|x| x.cur.as_ref().map(|p| (x.id, x.account, p.groups.specs().iter().map(|g| g.id).collect::<Vec<_>>())))
            .flat_map(|(s, a, gs)| gs.into_iter().map(move |g| (s, a, g)))
            .collect();
        for (s, a, g) in cells {
            let id = self.cmd_confirmed(a, &ControlCmd::SetMode { scope: scope(s, a, g), mode: RunMode::Live });
            self.run();
            assert_eq!(self.result_of(id).map(|r| r.status), Some(0), "steady: SetMode Live of {s:?}");
        }
    }

    /// Next update id per inst.
    pub fn quote(&mut self, i: InstId, bid: &str, ask: &str) {
        let v = self.rd.inst(i).venue;
        let t = self.now();
        self.upd += 1;
        let upd = self.upd;
        let ring = if v == A { RingId::Bbo(0) } else { RingId::Bbo(1) };
        self.push(ring, &bbo(i, v, 0, bid, ask, upd, t));
    }

    /// The effective mode of a cell.
    pub fn mode(&self, s: StrategyId, a: AccountId, g: GroupId) -> hft_risk::mode::Effective {
        self.s.engine().risk.mode.effective(hft_risk::mode::CellKey { strategy: s, account: a, group: g })
    }
    /// The gate bits of an account.
    pub fn gate_bits(&self, a: AccountId) -> u32 {
        self.s.engine().risk.mode.gate_bits(a)
    }
    /// Records of one kind since a mark.
    pub fn kinds_since(&self, m: usize, k: EventKind) -> Vec<MemRecord> {
        self.since(m).into_iter().filter(|r| r.kind == k).collect()
    }
    pub fn notes_since(&self, m: usize) -> Vec<hft_event::Note> {
        self.since(m).iter().filter(|r| r.kind == EventKind::NOTE).filter_map(|r| r.read::<hft_event::Note>()).collect()
    }
}

/// Shorthand: `Roll` as the boot reason of a new segment.
pub const STARTUP: RollReason = RollReason::Startup;

// ------------------------------------------------------------------------------- intents

/// A single-leg order intent of the hook's instance.
pub fn intent(hx: &Hx<'_>, inst: InstId, side: hft_types::Side, tif: hft_types::Tif, class: hft_strategy_sdk::IntentClass, p: Px, qty: Qty) -> hft_strategy_sdk::OrderIntent {
    let group = hx.groups.specs().first().map_or(GroupId(0), |g| g.id);
    hft_strategy_sdk::OrderIntent {
        venue: inst_venue(inst),
        account: hx.s.account,
        side,
        tif,
        class,
        flags: 0,
        _pad: [0; 2],
        inst,
        group,
        px: p,
        qty,
        link: hft_strategy_sdk::LegLinkOpt::NONE,
        tag: 0,
        max_rest_ms: 0,
    }
}

/// The B0 venue of an inst.
pub fn inst_venue(i: InstId) -> VenueId {
    if i == B1 { B } else { A }
}

/// IOC open buy of 0.001 at the ask.
pub fn open_buy(hx: &Hx<'_>, inst: InstId) -> hft_strategy_sdk::OrderIntent {
    let b = hx.v.bbo(inst);
    intent(hx, inst, hft_types::Side::Buy, hft_types::Tif::Ioc, hft_strategy_sdk::IntentClass::Open, b.ask_px, q("0.001"))
}

// ------------------------------------------------------------------------------- restart / verify

use hft_sequencer::driver::replay::{Log, Mismatch, ReplayDriver, ReplayMode};
use hft_sequencer::{BootError, BootSource, RecoverPlan, RecoveryFlags, SnapshotSel};

pub fn plan(start: SnapshotSel, replay_tail: bool, epoch: u32, flags: u8) -> RecoverPlan {
    RecoverPlan { start, replay_tail, epoch, flags: RecoveryFlags(flags), quarantine: Vec::new() }
}

/// The run-time knobs of a restarted process.
#[derive(Clone, Debug)]
pub struct Rerun {
    /// TSC of the new run at boot.
    pub tsc: u64,
    pub hz: u64,
    pub utc: i64,
    pub live: LiveCfg,
    pub build: [u8; 20],
}

impl Default for Rerun {
    fn default() -> Rerun {
        Rerun { tsc: 5_000_000_000_000, hz: HZ, utc: UTC0 + 3_600_000_000_000, live: LiveCfg { feed_stage_cap: 1024, ..LiveCfg::default() }, build: [0; 20] }
    }
}

impl<S: StrategySet> H<S> {
    /// Simulates a crash at the current point (the unpublished tail is lost, as with a real
    /// writer) and a cold start of a new process over the same journal.
    pub fn crash_restart(self, p: RecoverPlan, rr: Rerun) -> Result<H<S>, BootError> {
        let H { s, rd: _, setup, .. } = self;
        let drv = s.into_driver();
        let mut j = drv.io.j;
        // Unpublished records never reached the disk.
        let keep = j.committed;
        let mut n = 0usize;
        for seg in j.segments.iter_mut() {
            let room = keep.saturating_sub(n);
            if seg.records.len() > room {
                seg.records.truncate(room);
            }
            n += seg.records.len();
        }
        Self::restart_over(j, setup, p, rr)
    }
    /// A cold start over `j` exactly as it is (records past the last EVENT_END included).
    pub fn restart_over(j: MemJournal, mut setup: EngineSetup, p: RecoverPlan, rr: Rerun) -> Result<H<S>, BootError> {
        setup.clock = SegClock { tsc_hz_milli: rr.hz, calib_tsc: rr.tsc, calib_realtime_ns: rr.utc };
        let mut io = TestIo::new(setup.clock);
        io.build = rr.build;
        io.old = Some(Log::from_segments(&j.segments));
        let mut j = j;
        j.truncate_to_event_end();
        io.j = j;
        let mut live = rr.live.clone();
        live.quarantine.extend_from_slice(&p.quarantine);
        let drv = LiveDriver::new(io, live);
        let hb = hb();
        let s = Sequencer::boot(BootSource::Journal(p), &setup, drv, hb)?;
        let rd = s.engine().rd.clone();
        Ok(H { s, rd, setup, cmd_no: 1_000_000, hb, upd: 1_000_000 })
    }
    /// `VerifyDriver` over this journal from the snapshot at or before `at_seq` (`None` = the
    /// first one): mismatches and logged Outputs never reached.
    pub fn verify_from(&self, at_seq: Option<u64>) -> Vec<Mismatch> {
        verify_journal::<S>(self.j(), &self.setup, at_seq)
    }
    pub fn verify(&self) -> Vec<Mismatch> {
        self.verify_from(None)
    }
}

/// Verify replay of a journal (README "验证回放").
pub fn verify_journal<S: StrategySet>(j: &MemJournal, setup: &EngineSetup, at_seq: Option<u64>) -> Vec<Mismatch> {
    let log = Log::from_segments(&j.segments);
    let first = log.recs.iter().find(|r| r.kind == EventKind::STATE_SNAPSHOT).map_or(0, |r| r.seq);
    let at = at_seq.unwrap_or(first);
    let drv = ReplayDriver::new(ReplayMode::Verify, log, 0, setup.seq.replay_per_round);
    let p = plan(SnapshotSel::AtSeq(at), true, 0, 0);
    let mut s: Sequencer<ReplayDriver, S> = match Sequencer::boot(BootSource::Journal(p), setup, drv, hb()) {
        Ok(s) => s,
        Err(e) => panic!("verify boot: {e:?}"),
    };
    for _ in 0..1_000_000 {
        match s.step() {
            StepOutcome::Busy => {}
            _ => break,
        }
    }
    let d = s.driver();
    for (seq, a, b) in d.sink.detail.iter().take(4) {
        eprintln!("verify mismatch at {seq}:
  log {a:?}
  got {b:?}");
    }
    let mut v = d.mismatches().to_vec();
    v.extend(d.leftover());
    v
}

// ------------------------------------------------------------------------------- snapshots

impl<S: StrategySet> H<S> {
    /// Every `StateSnapshot` record so far.
    pub fn snapshots(&self) -> Vec<MemRecord> {
        self.j().records().filter(|r| r.kind == EventKind::STATE_SNAPSHOT).cloned().collect()
    }
    /// Hands a `Timer{Sys, id}` over through the hk ring and runs.
    pub fn sys(&mut self, id: u32) {
        let t = self.now();
        self.push(RingId::Hk, &sys_timer(id, t));
        self.run();
    }
    /// A keyframe (`Timer{Sys, Snapshot}`).
    pub fn keyframe(&mut self) -> MemRecord {
        self.sys(1);
        self.snapshots().pop().expect("keyframe written")
    }
}

/// The header and this crate's sections of a snapshot record.
pub fn snap(r: &MemRecord) -> (hft_event::SnapshotHdr, hft_sequencer::snapshot::SeqSections) {
    use zerocopy::FromBytes;
    let h = hft_event::SnapshotHdr::read_from_bytes(&r.body[..96]).expect("hdr");
    let s = match hft_sequencer::snapshot::read_seq_sections(&r.body[96..]) {
        Ok(s) => s,
        Err(e) => panic!("section {e}"),
    };
    (h, s)
}

/// Shared capture cell for hook closures.
pub type Cap<T> = std::rc::Rc<std::cell::RefCell<Vec<T>>>;
pub fn cap<T>() -> Cap<T> {
    std::rc::Rc::new(std::cell::RefCell::new(Vec::new()))
}

// ------------------------------------------------------------------------------- order events

/// `OrderDecision`s of the journal since a mark.
pub fn decisions(recs: &[MemRecord]) -> Vec<(u64, hft_event::OrderDecision)> {
    recs.iter()
        .filter(|r| r.kind == EventKind::ORDER_DECISION)
        .filter_map(|r| hft_sequencer::types::read_body::<hft_event::OrderDecision>(&r.body).map(|d| (r.seq, d)))
        .collect()
}

pub fn order_event(d: &hft_event::OrderDecision, what: hft_event::OrderEventKind, epoch: u16) -> hft_event::OrderEvent {
    hft_event::OrderEvent {
        venue: d.venue,
        account: d.account,
        what,
        detail: 0,
        slot: d.slot,
        r#gen: d.r#gen,
        inst: d.inst,
        px: d.px,
        qty: d.qty,
        cum_filled: Qty(0),
        fee: Qty(0),
        fee_asset: AssetId(0),
        epoch,
        reject_code: 0,
        exch_order_id: 1_000 + u64::from(d.slot.0),
        ex_ts_ns: 0,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: d.px,
    }
}

impl<S: StrategySet> H<S> {
    /// Ring of an account's reports.
    pub fn rep_ring(&self, a: AccountId) -> RingId {
        if matches!(self.rd.account(a).kind, hft_refdata::AccountKind::Sim(_)) { RingId::RepSim } else { RingId::Rep(0) }
    }
    pub fn epoch(&self) -> u16 {
        self.s.view().engine_epoch() as u16
    }
    pub fn ack(&mut self, d: &hft_event::OrderDecision) {
        let ev = order_event(d, hft_event::OrderEventKind::Ack, self.epoch());
        let r = self.rep_ring(d.account);
        self.push(r, &ev);
        self.run();
    }
    /// A fill of `qty` at `px` (cumulative = `cum`).
    pub fn fill(&mut self, d: &hft_event::OrderDecision, qty: Qty, cum: Qty, px: Px) {
        let what = if cum.0 >= d.qty.0 { hft_event::OrderEventKind::Fill } else { hft_event::OrderEventKind::PartialFill };
        let mut ev = order_event(d, what, self.epoch());
        ev.qty = qty;
        ev.cum_filled = cum;
        ev.px = px;
        ev.avg_px = px;
        let r = self.rep_ring(d.account);
        self.push(r, &ev);
        self.run();
    }
    pub fn reject(&mut self, d: &hft_event::OrderDecision) {
        let ev = order_event(d, hft_event::OrderEventKind::Reject, self.epoch());
        let r = self.rep_ring(d.account);
        self.push(r, &ev);
        self.run();
    }
}
