//! The engine configuration of a DST world (docs/tests/scenarios.md §2.2): refdata, `EngineConfig`
//! sections, risk parameters, the strategy parameter blobs and the sequencer's `EngineSetup`, all
//! derived from [`WorldSpec`] only (no randomness).

use hft_event::{BlobKind, BlobRef, GroupSpec, LegDef, ParamSource, StrategyParamsHdr};
use hft_exec::{ExecAccount, ExecConfig, ExecParams};
use hft_ledger::LedgerConfig;
use hft_oms::{OmsConfig, PerTable};
use hft_refdata::{
    AccountSpec as RdAccount, AccountVenueSpec, AssetClass, AssetSpec, CapFlags, DeadManCap, EcfBuilder,
    EngineSection, Exchange, FeeAssetRule, FeePair, FeeSpec, FundingSettleKind, InstKind, InstSpec as RdInst,
    InstStatus, InstanceSpec, LimitBucket, LimitScope, LiqMarker, MarginData, MarginMode, PositionMode,
    RateLimit, RdbBuilder, RefData, SimBalanceSpec, SimSpec, StaleGuardKind, TestnetKind, TsUnit, UidPolicy,
    VenueCaps, VenueSpec as RdVenue,
};
use hft_risk::{
    AcctLimits, DEFAULT_MARGIN, FixedVec, GroupLimits, GroupLimitsRow, GwMap, MarginLimits, PreVec, PxBand,
    RiskParams, VenueLimits,
};
use hft_sequencer::{CapacitiesRow, ConfigSet, EngineSetup, ParsedParams, RingLimit, SegClock, SeqParamsRow};
use hft_strategy_sdk::params::SchemaView;
use hft_strategy_sdk::params::{RawParams, encode_body};
use hft_strategy_sdk::{StrategySet, VenueSet};
use hft_types::{
    AccountId, AssetExp, AssetId, GroupId, HostClass, InstId, MarketKind, Notional, Px, Qty, Rate,
    StrategyId, VenueId,
};
use zerocopy::IntoBytes;

use crate::dst::world::{AccountKind, StrategyBinding, VenueMarket, WorldSpec};

pub use crate::dst::world::UTC0;
/// The test kind of a binding with `kind_id = 0` (`probe-taker`).
pub const DEFAULT_KIND: u16 = 0xFF01;
/// Leverage the venues report for every perp (W1 "实际杠杆 5×").
pub const LEVERAGE_X: i64 = 5;

/// `Exchange` of a fixed `VenueId`.
pub fn exchange_of(v: VenueId) -> Exchange {
    match v.0 {
        1 | 2 => Exchange::Binance,
        3 | 4 => Exchange::Gate,
        5 | 6 => Exchange::Okx,
        8 | 9 => Exchange::Bybit,
        10 | 11 => Exchange::Bitget,
        12 | 13 => Exchange::Kucoin,
        _ => Exchange::Hyperliquid,
    }
}

fn venue_row(id: VenueId, perp: bool) -> RdVenue {
    RdVenue {
        id,
        exchange: exchange_of(id),
        product: if perp { InstKind::Perp } else { InstKind::Spot },
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

fn caps_row(v: VenueId, perp: bool, reduce_only: bool) -> VenueCaps {
    VenueCaps {
        venue: v,
        market: if perp {
            MarketKind::LinearPerp
        } else {
            MarketKind::Spot
        },
        fee_asset_rule: FeeAssetRule::Quote,
        reduce_only,
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
        required_multi_asset: if perp { Some(false) } else { None },
        required_spot_borrow: None,
        funding_settle: if perp {
            FundingSettleKind::PerSymbolPush
        } else {
            FundingSettleKind::NotApplicable
        },
        margin_data: if perp {
            MarginData::Push
        } else {
            MarginData::NotApplicable
        },
        own_liquidation_marker: LiqMarker::NotIdentifiable,
        liq_marker_arg: "".into(),
        ignorable_user_events: vec!["evt_ignorable".into()].into(),
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

/// Decimal string of a raw value at exponent `exp` (tick / lot strings of refdata).
fn dec_str(raw: i64, exp: i8) -> String {
    let neg = raw < 0;
    let mut s = raw.unsigned_abs().to_string();
    let e = usize::from(exp.unsigned_abs());
    if exp >= 0 {
        s.push_str(&"0".repeat(e));
    } else {
        if s.len() <= e {
            s = format!("{}{s}", "0".repeat(e + 1 - s.len()));
        }
        s.insert(s.len() - e, '.');
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if neg { format!("-{s}") } else { s }
}

/// Price and quantity exponents of an asset of the world (BTC −4 / −8; the quote asset −6 / −6).
pub fn asset_exp(w: &WorldSpec, a: AssetId) -> AssetExp {
    if w.insts.iter().any(|i| i.base == a) {
        AssetExp {
            px_exp: -4,
            qty_exp: -8,
        }
    } else {
        AssetExp {
            px_exp: -6,
            qty_exp: -6,
        }
    }
}

/// Asset names: the world's first base asset is `BTC`, its first quote asset `USDT` (the ledger
/// values quote assets by name), others are numbered.
pub fn asset_name(w: &WorldSpec, a: AssetId) -> String {
    let first_base = w.insts.first().map(|i| i.base);
    let first_quote = w.insts.first().map(|i| i.quote);
    if Some(a) == first_base {
        "BTC".into()
    } else if Some(a) == first_quote {
        "USDT".into()
    } else if w.insts.iter().any(|i| i.base == a) {
        format!("B{}", a.0)
    } else {
        format!("Q{}", a.0)
    }
}

fn assets(w: &WorldSpec) -> Vec<AssetId> {
    let mut v: Vec<AssetId> = w.insts.iter().flat_map(|i| [i.base, i.quote]).collect();
    for a in &w.accounts {
        v.extend(a.balances.iter().map(|b| b.1));
    }
    v.sort_by_key(|a| a.0);
    v.dedup();
    v
}

fn vbits(vs: &[VenueId]) -> u64 {
    vs.iter().fold(0, |m, v| m | 1u64 << v.0)
}

/// The world's strategy binding of account `a`, if any.
pub fn binding_of(w: &WorldSpec, a: AccountId) -> Option<&StrategyBinding> {
    w.strategies.iter().find(|s| s.account == a)
}

/// Kind of a binding (`0` = the default test kind).
pub fn kind_of(b: &StrategyBinding) -> u16 {
    if b.kind_id == 0 { DEFAULT_KIND } else { b.kind_id }
}

/// The refdata blob of the world. `all_sim` = every account becomes a Sim account with the world
/// balances as `SimBalances` (the private refdata of a `SimMatcher`).
pub fn refdata_blob(w: &WorldSpec, all_sim: bool) -> Vec<u8> {
    let mut b = RdbBuilder::new(1);
    b.created_utc_ns(UTC0 - 86_400_000_000_000);
    for a in assets(w) {
        let base = w.insts.iter().any(|i| i.base == a);
        b.asset(AssetSpec {
            id: a,
            name: asset_name(w, a).into(),
            class: if base {
                AssetClass::Crypto
            } else {
                AssetClass::Stable
            },
            exp: asset_exp(w, a),
            px_max: Px(1_000_000_000_000_000),
            qty_max: Qty(1_000_000_000_000_000_000),
            dust: Qty(if base { 1_000 } else { 1_000_000 }),
        });
    }
    for v in &w.venues {
        b.venue(venue_row(v.id, v.market == VenueMarket::Perp));
        b.caps(caps_row(v.id, v.market == VenueMarket::Perp, v.reduce_only));
        for tier in 0..=1u8 {
            let r = Rate(v.fee_ppm * 1_000);
            b.fee(FeeSpec {
                venue: v.id,
                tier,
                fee: FeePair { maker: r, taker: r },
            });
        }
        for (win, lim) in [(10_000u32, 1_000u32), (60_000, 6_000)] {
            b.limit(
                v.id,
                RateLimit {
                    scope: LimitScope::Account,
                    bucket: LimitBucket::Orders,
                    limit: lim,
                    window_ms: win,
                    default_cost: 1,
                },
            );
        }
    }
    for i in &w.insts {
        let e = asset_exp(w, i.base);
        b.inst(RdInst {
            id: i.id,
            venue: i.venue,
            symbol: format!("I{}", i.id.0).into(),
            status: InstStatus::Trading,
            kind: if i.perp { InstKind::Perp } else { InstKind::Spot },
            asset: i.base,
            quote: i.quote,
            tick: i.tick,
            lot: i.lot,
            min_qty: Qty(i.min_qty),
            max_qty: Qty(100_000_000_000_000),
            min_notional: Notional(5_000_000),
            book_window: 4096,
            l2_depth: 0,
            tick_str: dec_str(i.tick, e.px_exp).into(),
            lot_str: dec_str(i.lot, e.qty_exp).into(),
            min_qty_str: dec_str(i.min_qty, e.qty_exp).into(),
            funding_interval_s: if i.perp { 28_800 } else { 0 },
            max_leverage: if i.perp { 50 } else { 0 },
            ..RdInst::default()
        });
    }
    for s in &w.strategies {
        b.instance(InstanceSpec {
            id: s.instance,
            kind_id: kind_of(s),
            name: format!("s{}", s.instance.0).into(),
        });
    }
    for a in &w.accounts {
        let sim = all_sim || a.kind == AccountKind::Sim;
        b.account(RdAccount {
            id: a.id,
            sim,
            bound_instance: binding_of(w, a.id).map_or(StrategyId(0), |s| s.instance),
            is_prod: false,
            venues: vbits(&a.venues),
            label: if sim {
                format!("sim-{}", a.id.0)
            } else {
                format!("acct-{}", a.id.0)
            },
            uid_policy: UidPolicy::Verified,
        });
        if sim {
            b.sim(SimSpec {
                account: a.id,
                fee_tier: 0,
                latency_model: 0,
                margin_model: 0,
            });
            for (v, asset, amt) in &a.balances {
                b.sim_balance(SimBalanceSpec {
                    account: a.id,
                    venue: *v,
                    asset: *asset,
                    qty: Qty(i64::try_from(*amt).unwrap_or(i64::MAX)),
                });
            }
        } else {
            for v in &a.venues {
                b.account_venue(AccountVenueSpec {
                    account: a.id,
                    venue: *v,
                    uid: uid_of(a.id, *v),
                });
            }
        }
    }
    b.finish()
}

/// Exchange UID of a Live account on a venue (refdata and `AccountConfigReport` agree).
pub fn uid_of(a: AccountId, v: VenueId) -> String {
    format!("uid-{}-{}", a.0, v.0)
}

/// Loads the world refdata.
pub fn load_refdata(blob: &[u8]) -> Result<RefData, String> {
    RefData::load(blob, None).map_err(|e| format!("world refdata rejected: {e:?}"))
}

/// Sequencer capacities sized for the world.
pub fn caps_row_of(w: &WorldSpec) -> CapacitiesRow {
    let max_inst = w.insts.iter().map(|i| i.id.0).max().unwrap_or(1);
    CapacitiesRow {
        max_insts: (max_inst + 8).next_power_of_two().max(16),
        max_cells_total: 32,
        max_timers: 1_024,
        recent_cmd_ids: 256,
        max_subs_per_inst: 8,
        max_groups_per_instance: 16,
        max_instances: 8,
        _pad: 0,
    }
}

/// Sequencer parameters of a DST run (`keyframe_period_ms = 60 000`).
pub fn seq_row() -> SeqParamsRow {
    let mut rings = [RingLimit::default(); 8];
    rings[1] = RingLimit {
        per_ring: 4,
        total: 4,
    };
    rings[2] = RingLimit {
        per_ring: 4,
        total: 4,
    };
    rings[3] = RingLimit {
        per_ring: 32,
        total: 64,
    };
    rings[4] = RingLimit {
        per_ring: 32,
        total: 0,
    };
    rings[5] = RingLimit {
        per_ring: 32,
        total: 128,
    };
    rings[6] = RingLimit {
        per_ring: 16,
        total: 16,
    };
    SeqParamsRow {
        rings,
        md_round_ns: 50_000,
        ctl_bytes_per_round: 8_192,
        keyframe_period_ms: 60_000,
        keyframe_max_events: 200_000,
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

/// The ECF1 blob of the run (sections 3 and 5).
pub fn engine_blob(caps: &CapacitiesRow, seq: &SeqParamsRow) -> Vec<u8> {
    let mut b = EcfBuilder::new(1, HostClass::Dev2);
    b.created_utc_ns(UTC0 - 86_400_000_000_000);
    b.section(EngineSection::Capacities, 1, 24, 1, caps.as_bytes().to_vec());
    b.section(EngineSection::Sequencer, 1, 72, 1, seq.as_bytes().to_vec());
    b.finish()
}

/// `RiskParams` of the world (`RiskSpec` values; the rest generous).
pub fn risk_params(w: &WorldSpec) -> RiskParams {
    let r = &w.risk;
    let n = |v: i128| Notional(i64::try_from(v).unwrap_or(i64::MAX));
    let max_pos = w.groups.iter().map(|g| g.max_pos_qty).max().unwrap_or(10_000_000);
    let group = GroupLimits {
        max_group_notional: Notional(500_000_000_000),
        max_pos_qty: Qty(i64::try_from(max_pos).unwrap_or(i64::MAX)),
        naked_gate: Rate(500_000_000),
        naked_gate_min: Notional(100_000_000),
    };
    let margin = MarginLimits {
        margin_stale_ms: 120_000,
        force_step: Rate(r.force_step_bp * 100_000),
        ..DEFAULT_MARGIN
    };
    let mut classes = FixedVec::new();
    classes.push(PxBand {
        max_px_dev: Rate(30_000_000),
        ioc_max_cross: Rate(5_000_000),
        rr_max_cross: Rate(10_000_000),
    });
    let mut accounts = Vec::new();
    let mut venues = Vec::new();
    for a in &w.accounts {
        accounts.push(AcctLimits {
            account: a.id,
            max_order_notional: Notional(100_000_000_000),
            default_group: group,
            max_account_notional: Notional(1_000_000_000_000),
            account_notional_from_equity: Rate(5_000_000_000),
            max_hedge_notional: n(r.max_hedge_notional),
            max_reduce_notional: n(r.max_reduce_notional),
            daily_loss_kill: Rate(500_000_000),
            daily_loss_abs: Notional(1_000_000_000_000),
            daily_loss_max_resumes: 1,
            recon_closeonly_notional: Notional(100_000_000),
            unknown_blocks_open: r.unknown_blocks_open,
            funds_buffer: Rate(50_000_000),
            margin,
        });
        for v in &a.venues {
            venues.push(VenueLimits {
                venue: *v,
                account: a.id,
                max_venue_notional: Notional(1_000_000_000_000),
                inst_bucket_per_s: 1_000,
                backoff_base_ms: 500,
                backoff_max_ms: 8_000,
                rr_reserve: Rate(200_000_000),
            });
        }
    }
    let groups = w
        .groups
        .iter()
        .map(|g| GroupLimitsRow {
            account: g.account,
            group: g.group,
            l: GroupLimits {
                max_pos_qty: Qty(i64::try_from(g.max_pos_qty).unwrap_or(i64::MAX)),
                ..group
            },
        })
        .collect();
    RiskParams {
        version: 1,
        generated_at_ns: 0,
        author: [0; 16],
        rate_headroom: Rate(0),
        conn_liveness_ms: u32::try_from(r.conn_liveness_ms.max(1)).unwrap_or(u32::MAX),
        max_perp_leverage_x100: 500,
        l3_max_cross: Rate(0),
        risk_class: PreVec::from_vec(Vec::new()),
        accounts: PreVec::from_vec(accounts),
        venues: PreVec::from_vec(venues),
        classes,
        groups: PreVec::from_vec(groups),
    }
}

/// The DST `RiskParams` encoding: `b"DSTR"` + nothing (`SetRisk` decodes to the world values).
pub const RISK_BLOB: &[u8] = b"DSTR\0\0\0\0";

/// SHA-256 of a blob.
pub fn sha256(b: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(b).into()
}

/// The SPB1 blob of a binding: its groups and the kind's default parameters (or `params`).
pub fn spb_blob<S: StrategySet>(w: &WorldSpec, s: &StrategyBinding, version: u64) -> Result<Vec<u8>, String> {
    let kind = kind_of(s);
    let desc = S::kinds()
        .iter()
        .find(|d| d.kind_id == kind)
        .ok_or_else(|| format!("kind {kind:#x} not in the strategy set"))?;
    let groups: Vec<GroupSpec> = w
        .groups
        .iter()
        .filter(|g| g.instance == s.instance && g.account == s.account)
        .map(|g| {
            let none = LegDef {
                inst: InstId(0),
                venue: VenueId(0),
                kind: 0,
                _pad: 0,
                route: 0,
                _pad2: 0,
            };
            let mut legs = [none; 4];
            for (k, i) in leg_order(w, kind, g.first, g.second).into_iter().enumerate() {
                let v = w.inst(i).map_or(VenueId(0), |x| x.venue);
                legs[k] = LegDef {
                    inst: i,
                    venue: v,
                    kind: LegDef::CEX,
                    _pad: 0,
                    route: 0,
                    _pad2: 0,
                };
            }
            let asset = w.inst(g.first).map_or(AssetId(0), |x| x.base);
            GroupSpec {
                id: g.group,
                asset,
                n_legs: 2,
                open_allowed: 1,
                legs,
                _rsv: 0,
            }
        })
        .collect();
    let schema = desc.schema;
    let body = if s.params.is_empty() {
        let mut defaults: Vec<i64> = (0..schema.n_fields())
            .filter_map(|i| schema.field(i).map(|f| f.default))
            .collect();
        if is_taker(kind)
            && let Some(th) = defaults.first_mut()
        {
            *th = TAKER_SPREAD_TH;
        }
        let raw = RawParams {
            defaults,
            overrides: Vec::new(),
        };
        encode_body(schema, groups.len() as u16, &raw).map_err(|e| format!("params of {kind:#x}: {e:?}"))?
    } else {
        s.params.clone()
    };
    let hwm = groups
        .iter()
        .map(|g| g.id.counter())
        .max()
        .unwrap_or(0)
        .max(1_000);
    let h = StrategyParamsHdr {
        strategy: s.instance,
        account: s.account,
        kind_id: kind,
        schema_id: schema.id,
        schema_ver: schema.version,
        source: ParamSource::Manual,
        _pad: 0,
        n_groups: groups.len() as u16,
        _pad2: 0,
        version,
        generated_at_ns: 0,
        body_len: body.len() as u32,
        group_ctr_hwm: hwm,
        schema_digest: hft_strategy_sdk::params::schema_digest(desc, schema.version),
    };
    let mut b = h.as_bytes().to_vec();
    for g in &groups {
        b.extend_from_slice(g.as_bytes());
    }
    b.extend_from_slice(&body);
    Ok(b)
}

/// `probe-taker` / `probe-np`.
pub fn is_taker(kind: u16) -> bool {
    kind == 0xFF01 || kind == 0xFF03
}

/// Opening threshold of the takers in a DST world: 0.01 % (W1 quotes the perp ≈ 0.017 % above
/// the spot, so the seed's random walk opens and holds positions).
pub const TAKER_SPREAD_TH: i64 = 100_000;

/// Leg order of a group in the parameter blob. `probe-taker` sells leg A when its bid exceeds leg
/// B's ask: with W1's quotes (perp above spot) and no base inventory that is the perp, so a
/// spot-first world group is written perp first for the takers.
pub fn leg_order(w: &WorldSpec, kind: u16, first: InstId, second: InstId) -> [InstId; 2] {
    let perp = |i: InstId| w.inst(i).is_some_and(|x| x.perp);
    if is_taker(kind) && !perp(first) && perp(second) {
        [second, first]
    } else {
        [first, second]
    }
}

/// `BlobRef` of a parameter blob.
pub fn spb_ref(b: &[u8], version: u64) -> BlobRef {
    BlobRef {
        kind: BlobKind::StrategyParams,
        encoding: 1,
        len: b.len() as u32,
        version,
        sha256: sha256(b),
    }
}

/// Parses an SPB1 blob for the sequencer.
pub fn parse_spb<S: StrategySet>(b: &[u8], rd: &RefData) -> Result<ParsedParams<S>, String> {
    hft_sequencer::parsed::parse_params::<S>(b, sha256(b), rd, 16, None)
        .map_err(|d| format!("params rejected: {d:#x}"))
}

/// Accounts whose `ExecConfig` row is a Sim row.
fn exec_config(w: &WorldSpec) -> ExecConfig {
    let params = ExecParams {
        broken_auto_fix_ms: u32::try_from(w.risk.broken_auto_fix_ms.max(0)).unwrap_or(u32::MAX),
        ..ExecParams::default()
    };
    ExecConfig {
        default: params,
        accounts: w
            .accounts
            .iter()
            .map(|a| ExecAccount {
                account: a.id,
                sim: a.kind == AccountKind::Sim,
                params: None,
            })
            .collect(),
    }
}

/// The `EngineSetup` of the world at the virtual TSC frequency (`tsc_hz` ticks per second).
pub fn engine_setup(w: &WorldSpec, tsc_hz: u64, epoch: u32) -> EngineSetup {
    let hz_milli = tsc_hz.saturating_mul(1_000);
    let mut oms = OmsConfig::new(epoch, hz_milli);
    oms.max_out_per_event = 16;
    oms.links_per_account = 16;
    oms.slots = PerTable::new(64);
    oms.hedge_retry_min_ms = u32::try_from(w.risk.hedge_retry_ms.max(1)).unwrap_or(u32::MAX);
    oms.hedge_deadline_ms = u32::try_from(w.risk.hedge_deadline_ms.max(1)).unwrap_or(u32::MAX);
    let mut keys = Vec::new();
    let mut venues: Vec<VenueId> = w.venues.iter().map(|v| v.id).collect();
    venues.sort_by_key(|v| v.0);
    for a in w.accounts.iter().filter(|a| a.kind == AccountKind::Live) {
        for v in &a.venues {
            keys.push((a.id, *v, 3u8));
        }
    }
    EngineSetup {
        caps: caps_row_of(w),
        seq: seq_row(),
        oms,
        ledger: LedgerConfig::default(),
        exec: exec_config(w),
        gws: GwMap {
            venues: venues.clone(),
        },
        feeds: VenueSet(vbits(&venues)),
        keys_required: keys,
        role_prod: false,
        advisor_max_lag_ms: 5_000,
        drain_timeout_ms: 30_000,
        risk_decode: Some(risk_decode),
        clock: SegClock {
            tsc_hz_milli: hz_milli,
            calib_tsc: 0,
            calib_realtime_ns: UTC0,
        },
        base_engine_ns: 0,
    }
}

thread_local! {
    /// The world of the current run (`SetRisk` decoding needs its values).
    static RISK_OF_RUN: core::cell::RefCell<Option<RiskParams>> = const { core::cell::RefCell::new(None) };
}

/// Remembers the run's risk parameters for [`risk_decode`].
pub fn set_run_risk(p: RiskParams) {
    RISK_OF_RUN.with(|r| *r.borrow_mut() = Some(p));
}

/// `SetRisk` decoding of the DST encoding: `b"DSTR"` → the run's parameters.
pub fn risk_decode(b: &[u8], _a: AccountId, _rd: &RefData) -> Result<RiskParams, u16> {
    if b.get(..4) != Some(b"DSTR") {
        return Err(hft_sequencer::codes::detail::BAD_VALUE);
    }
    RISK_OF_RUN
        .with(|r| r.borrow().clone())
        .ok_or(hft_sequencer::codes::detail::BAD_VALUE)
}

/// The empty-start configuration of the world.
pub fn config_set<S: StrategySet>(
    w: &WorldSpec,
    rd: &RefData,
    rd_blob: Vec<u8>,
) -> Result<ConfigSet<S>, String> {
    let caps = caps_row_of(w);
    let seq = seq_row();
    let risk = risk_params(w);
    set_run_risk(risk.clone());
    let mut params = Vec::new();
    for s in &w.strategies {
        let b = spb_blob::<S>(w, s, 1)?;
        let p = parse_spb::<S>(&b, rd)?;
        params.push((b, p));
    }
    Ok(ConfigSet {
        refdata_blob: rd_blob,
        engine_blob: engine_blob(&caps, &seq),
        risk_blob: RISK_BLOB.to_vec(),
        risk_sha256: sha256(RISK_BLOB),
        risk,
        params,
    })
}

/// Group ids of an account in the world.
pub fn groups_of(w: &WorldSpec, a: AccountId) -> Vec<(StrategyId, GroupId)> {
    w.groups
        .iter()
        .filter(|g| g.account == a)
        .map(|g| (g.instance, g.group))
        .collect()
}
