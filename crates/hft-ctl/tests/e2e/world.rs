//! The configuration of baseline `E2E`: refdata (P0 with the instance kinds of the E2E table),
//! strategy parameters of the testkit kinds, the test `RiskParams` encoding and the `EngineSetup`.

use hft_event::{GroupSpec, LegDef, ParamSource, StrategyParamsHdr};
use hft_exec::{ExecAccount, ExecConfig, ExecParams};
use hft_ledger::LedgerConfig;
use hft_oms::{OmsConfig, PerTable};
use hft_refdata::RefData;
use hft_risk::{
    AcctLimits, DEFAULT_MARGIN, FixedVec, GroupLimits, GwMap, MarginLimits, PreVec, PxBand, RiskParams,
    VenueLimits,
};
use hft_sequencer::{CapacitiesRow, EngineSetup, ParsedParams, RingLimit, SegClock, SeqParamsRow};
use hft_strategy_sdk::params::{RawParams, SchemaView, encode_body};
use hft_strategy_sdk::{StrategySet, VenueSet};
use hft_types::{AccountId, GroupId, InstId, Notional, Qty, Rate, StrategyId, VenueId};
use sha2::{Digest, Sha256};
use zerocopy::IntoBytes;

use crate::common::p0;

/// hft-testkit's `TestStrategies` behind the recording wrapper (`rec.rs`).
pub type Set = super::rec::RecSet;

pub use p0::{
    ACCT_A, ACCT_B, BTC, I_BG_FUT, I_BG_SPOT, I_BN_SPOT, I_BN_USDM, S1, S2, S9, SIM_A, USDT, V_BG_FUT,
    V_BG_SPOT, V_BN_SPOT, V_BN_USDM,
};

pub const KIND_TAKER: u16 = 0xFF01;
pub const KIND_TARGET: u16 = 0xFF02;
pub const G1: GroupId = GroupId(p0::G1);
pub const G2: GroupId = GroupId(p0::G2);
pub const G9: GroupId = GroupId(p0::G9);

/// 1 GHz in mHz (1 tick = 1 ns).
pub const HZ: u64 = 1_000_000_000_000;
/// UTC of TSC 0 (the gateway clock of G0; the sequencer's calibration pair agrees with it).
pub const UTC0: i64 = crate::common::UTC0;
/// First TSC of the run.
pub const T0: u64 = 1_000_000;
pub const MS: u64 = 1_000_000;
pub const SEC: u64 = 1_000_000_000;

/// The refdata input of E2E: P0, with `s1-main` and `s9-sim` of kind `probe-target`, `s2-main` of
/// kind `probe-taker` and `l2_depth = 0` (TESTS.md baseline E2E).
pub fn rd_spec() -> p0::P0 {
    let mut s = p0::p0_spec();
    for i in s.instances.iter_mut() {
        i.kind_id = if i.id == S2 { KIND_TAKER } else { KIND_TARGET };
    }
    // BBO only: no L2 book (a Live L2 book would need snapshots for the feeds gate).
    for i in s.insts.iter_mut() {
        i.l2_depth = 0;
    }
    s
}

pub fn rd_blob() -> Vec<u8> {
    rd_spec().build()
}

pub fn rd() -> RefData {
    rd_spec().load()
}

pub fn sha(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

// ------------------------------------------------------------------------------- params

pub fn leg(inst: InstId, venue: VenueId) -> LegDef {
    LegDef {
        inst,
        venue,
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    }
}

pub fn group(id: GroupId, legs: &[LegDef]) -> GroupSpec {
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut l = [none; 4];
    l[..legs.len()].copy_from_slice(legs);
    GroupSpec {
        id,
        asset: BTC,
        n_legs: legs.len() as u8,
        open_allowed: 1,
        legs: l,
        _rsv: 0,
    }
}

/// A `probe-target` group of `acct-a`/`sim-a`: leg 0 the `bitget-spot` BTCUSDT (target +lot), leg
/// 1 the `binance-usdm` perpetual (target −lot).
pub fn g_target(id: GroupId) -> GroupSpec {
    group(id, &[leg(I_BG_SPOT, V_BG_SPOT), leg(I_BN_USDM, V_BN_USDM)])
}

/// The `probe-taker` group of `acct-b`: leg A the `bitget-usdt-futures` perpetual (sold when its bid
/// exceeds leg B's ask), leg B the `binance-spot` BTCUSDT.
pub fn g_taker(id: GroupId) -> GroupSpec {
    group(id, &[leg(I_BG_FUT, V_BG_FUT), leg(I_BN_SPOT, V_BN_SPOT)])
}

/// Field indices of `probe-target` (`TargetParams`).
pub mod tf {
    pub const WIN: usize = 0;
    pub const THR: usize = 1;
    pub const T_SLICE_MS: usize = 3;
    pub const MODEL_MEM_KIB: usize = 8;
    pub const MODEL_CKPT_KIB: usize = 9;
}
/// Field indices of `probe-taker` (`TakerParams`).
pub mod kf {
    pub const SPREAD_TH: usize = 0;
    pub const QTY_LOTS: usize = 1;
    pub const CLOSE_ON: usize = 2;
}

/// An SPB1 blob of a testkit kind: header, groups, body of `values` (one default per field, no
/// per-group overrides).
#[derive(Clone, Debug)]
pub struct Spb {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
    pub version: u64,
    pub groups: Vec<GroupSpec>,
    pub values: Vec<i64>,
}

fn defaults(kind_id: u16) -> Vec<i64> {
    let d = kind_desc(kind_id);
    (0..d.schema.n_fields())
        .filter_map(|i| d.schema.field(i).map(|f| f.default))
        .collect()
}

fn kind_desc(kind_id: u16) -> &'static hft_strategy_sdk::StrategyDesc {
    Set::kinds()
        .iter()
        .find(|d| d.kind_id == kind_id)
        .unwrap_or_else(|| panic!("kind {kind_id:#x}"))
}

impl Spb {
    pub fn new(
        strategy: StrategyId,
        account: AccountId,
        kind_id: u16,
        version: u64,
        groups: Vec<GroupSpec>,
    ) -> Spb {
        Spb {
            strategy,
            account,
            kind_id,
            version,
            groups,
            values: defaults(kind_id),
        }
    }
    /// `(s1-main, acct-a)`: `probe-target`, `g1`; its TakerSlice point at engine time
    /// [`T_SLICE_MS`] (how `E2e::position_g1` gives `g1` a position).
    pub fn s1(version: u64) -> Spb {
        let mut p = Spb::new(S1, ACCT_A, KIND_TARGET, version, vec![g_target(G1)]);
        p.values[tf::T_SLICE_MS] = T_SLICE_MS;
        p
    }
    /// `(s2-main, acct-b)`: `probe-taker`, `g2`; `close_on = 0` (no exit target of its own).
    pub fn s2(version: u64) -> Spb {
        let mut p = Spb::new(S2, ACCT_B, KIND_TAKER, version, vec![g_taker(G2)]);
        p.values[kf::CLOSE_ON] = 0;
        p
    }
    /// `(s9-sim, sim-a)`: `probe-target`, `g9`.
    pub fn s9(version: u64) -> Spb {
        Spb::new(S9, SIM_A, KIND_TARGET, version, vec![g_target(G9)])
    }
    pub fn bytes(&self) -> Vec<u8> {
        let d = kind_desc(self.kind_id);
        let raw = RawParams {
            defaults: self.values.clone(),
            overrides: Vec::new(),
        };
        let body = encode_body(d.schema, self.groups.len() as u16, &raw).unwrap_or_else(|e| panic!("{e:?}"));
        let hwm = self
            .groups
            .iter()
            .map(|g| g.id.counter())
            .max()
            .unwrap_or(0)
            .max(1000);
        let h = StrategyParamsHdr {
            strategy: self.strategy,
            account: self.account,
            kind_id: self.kind_id,
            schema_id: d.schema.id,
            schema_ver: d.schema.version,
            source: ParamSource::Manual,
            _pad: 0,
            n_groups: self.groups.len() as u16,
            _pad2: 0,
            version: self.version,
            generated_at_ns: 0,
            body_len: body.len() as u32,
            group_ctr_hwm: hwm,
            schema_digest: hft_strategy_sdk::params::schema_digest(d, d.schema.version),
        };
        let mut b = h.as_bytes().to_vec();
        for g in &self.groups {
            b.extend_from_slice(g.as_bytes());
        }
        b.extend_from_slice(&body);
        b
    }
    /// The decoded object (as the assembler hands it to the sequencer and the gateway).
    pub fn parsed(&self, rd: &RefData) -> ParsedParams<Set> {
        let b = self.bytes();
        hft_sequencer::parsed::parse_params::<Set>(&b, sha(&b), rd, 8, None)
            .unwrap_or_else(|d| panic!("params rejected: {d:#x}"))
    }
}

/// `t_slice_ms` of `s1-main`'s start parameters (beyond every time advance of the tests that do
/// not ask for a `g1` position).
pub const T_SLICE_MS: i64 = 120_000;

/// The starting parameters of E2E: `(s1-main, acct-a)` version 41, `(s2-main, acct-b)` version 7,
/// `(s9-sim, sim-a)` version 3.
pub fn start_params() -> Vec<Spb> {
    vec![Spb::s1(41), Spb::s2(7), Spb::s9(3)]
}

// ------------------------------------------------------------------------------- risk

/// Notional in USDT (1e-6).
pub fn usdt(x: i64) -> Notional {
    Notional(x * 1_000_000)
}

fn band() -> PxBand {
    PxBand {
        max_px_dev: Rate(30_000_000),
        ioc_max_cross: Rate(5_000_000),
        rr_max_cross: Rate(10_000_000),
    }
}

fn acct_limits(a: AccountId, max_order: Notional) -> AcctLimits {
    AcctLimits {
        account: a,
        max_order_notional: max_order,
        default_group: GroupLimits {
            max_group_notional: usdt(50_000),
            max_pos_qty: Qty(100_000_000),
            naked_gate: Rate(500_000_000),
            naked_gate_min: usdt(100),
        },
        max_account_notional: usdt(100_000),
        account_notional_from_equity: Rate(1_000_000_000),
        max_hedge_notional: usdt(20_000),
        max_reduce_notional: usdt(10_000),
        daily_loss_kill: Rate(50_000_000),
        daily_loss_abs: usdt(5_000),
        daily_loss_max_resumes: 1,
        recon_closeonly_notional: usdt(100),
        unknown_blocks_open: true,
        funds_buffer: Rate(50_000_000),
        margin: MarginLimits {
            margin_stale_ms: 15_000,
            ..DEFAULT_MARGIN
        },
    }
}

/// The E2E `RiskParams` document (the fields the tests vary; the rest fixed).
#[derive(Copy, Clone, Debug)]
pub struct RiskDoc {
    pub version: u64,
    /// `acct-a`'s `max_order_notional` (USDT); the other accounts 10 000.
    pub max_order_a: i64,
    /// Every account's `margin_stale_ms`.
    pub margin_stale_ms: u32,
    /// `acct-b`'s `daily_loss_abs` (USDT); the other accounts 5 000.
    pub daily_loss_abs_b: i64,
    /// 1 = changes another account's fields (`OutOfScope` for an account-scoped `SetRisk`).
    pub flags: u8,
}

impl RiskDoc {
    /// Version 7, `acct-a` 1 000 USDT, margin stale after 15 s, daily loss 5 000 USDT.
    pub fn start() -> RiskDoc {
        RiskDoc {
            version: 7,
            max_order_a: 1_000,
            margin_stale_ms: 15_000,
            daily_loss_abs_b: 5_000,
            flags: 0,
        }
    }

    /// Test encoding (hft-risk has no `RiskParams` blob codec yet): `b"RSKE"`, version (u64 LE),
    /// `max_order_a` (i64 LE), `flags`, `margin_stale_ms` (u32 LE), `daily_loss_abs_b` (i64 LE),
    /// zero padding to 64 bytes.
    pub fn bytes(&self) -> Vec<u8> {
        let mut b = b"RSKE".to_vec();
        b.extend_from_slice(&self.version.to_le_bytes());
        b.extend_from_slice(&self.max_order_a.to_le_bytes());
        b.push(self.flags);
        b.extend_from_slice(&self.margin_stale_ms.to_le_bytes());
        b.extend_from_slice(&self.daily_loss_abs_b.to_le_bytes());
        b.resize(64, 0);
        b
    }

    pub fn params(&self, rd: &RefData) -> RiskParams {
        let mut classes = FixedVec::new();
        classes.push(band());
        let mut accounts = Vec::new();
        let mut venues = Vec::new();
        for a in [ACCT_A, ACCT_B, SIM_A] {
            let max = if a == ACCT_A {
                usdt(self.max_order_a)
            } else {
                usdt(10_000)
            };
            let mut l = acct_limits(a, max);
            l.margin.margin_stale_ms = self.margin_stale_ms;
            if a == ACCT_B {
                l.daily_loss_abs = usdt(self.daily_loss_abs_b);
            }
            accounts.push(l);
            let bits = rd.account(a).venues;
            for v in 0..64u8 {
                if bits & (1u64 << v) != 0 {
                    venues.push(VenueLimits {
                        venue: VenueId(v),
                        account: a,
                        max_venue_notional: usdt(80_000),
                        inst_bucket_per_s: 1_000,
                        backoff_base_ms: 1_000,
                        backoff_max_ms: 8_000,
                        rr_reserve: Rate(200_000_000),
                    });
                }
            }
        }
        RiskParams {
            version: self.version,
            generated_at_ns: 0,
            author: [0; 16],
            rate_headroom: Rate(0),
            conn_liveness_ms: 2_000,
            max_perp_leverage_x100: 2_000,
            l3_max_cross: Rate(0),
            risk_class: PreVec::from_vec(Vec::new()),
            accounts: PreVec::from_vec(accounts),
            venues: PreVec::from_vec(venues),
            classes,
            groups: PreVec::from_vec(Vec::new()),
        }
    }
}

/// A `SetRisk` document: [`RiskDoc::start`] with `version`, `acct-a`'s limit and `flags`.
pub fn risk_blob(version: u64, max_order_a: i64, flags: u8) -> Vec<u8> {
    RiskDoc {
        version,
        max_order_a,
        flags,
        ..RiskDoc::start()
    }
    .bytes()
}

/// The decoder of [`RiskDoc::bytes`] (gateway `RiskDecode` and sequencer `EngineSetup.risk_decode`).
pub fn risk_decode(b: &[u8], a: AccountId, rd: &RefData) -> Result<RiskParams, u16> {
    use hft_ctl_proto::codes::detail;
    if b.get(..4) != Some(b"RSKE") || b.len() < 33 {
        return Err(detail::BAD_VALUE);
    }
    let u64_at = |o: usize| {
        b.get(o..o + 8)
            .and_then(|x| x.try_into().ok())
            .map(u64::from_le_bytes)
    };
    let u32_at = |o: usize| {
        b.get(o..o + 4)
            .and_then(|x| x.try_into().ok())
            .map(u32::from_le_bytes)
    };
    let bad = detail::BAD_VALUE;
    let doc = RiskDoc {
        version: u64_at(4).ok_or(bad)?,
        max_order_a: u64_at(12).ok_or(bad)? as i64,
        flags: b[20],
        margin_stale_ms: u32_at(21).ok_or(bad)?,
        daily_loss_abs_b: u64_at(25).ok_or(bad)? as i64,
    };
    if doc.flags & 1 != 0 && a.0 != 0 {
        return Err(detail::OUT_OF_SCOPE);
    }
    Ok(doc.params(rd))
}

// ------------------------------------------------------------------------------- engine config

pub fn caps_row() -> CapacitiesRow {
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

pub fn seq_row() -> SeqParamsRow {
    let mut rings = [RingLimit::default(); 8];
    let set = |rings: &mut [RingLimit; 8], k: usize, per_ring: u16, total: u16| {
        rings[k] = RingLimit { per_ring, total };
    };
    set(&mut rings, 1, 4, 4);
    set(&mut rings, 2, 4, 4);
    set(&mut rings, 3, 32, 64);
    set(&mut rings, 4, 32, 0);
    set(&mut rings, 5, 32, 128);
    set(&mut rings, 6, 16, 16);
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

pub fn engine_blob() -> Vec<u8> {
    let mut b = hft_refdata::EcfBuilder::new(1, hft_types::HostClass::Dev2);
    b.created_utc_ns(UTC0 - 86_400_000_000_000);
    b.section(
        hft_refdata::EngineSection::Capacities,
        1,
        24,
        1,
        caps_row().as_bytes().to_vec(),
    );
    b.section(
        hft_refdata::EngineSection::Sequencer,
        1,
        72,
        1,
        seq_row().as_bytes().to_vec(),
    );
    b.finish()
}

pub const LIVE_VENUES: [VenueId; 4] = [V_BN_USDM, V_BN_SPOT, V_BG_FUT, V_BG_SPOT];

pub fn vbits(vs: &[VenueId]) -> u64 {
    vs.iter().fold(0u64, |m, v| m | (1u64 << v.0))
}

/// `acct-a`'s `force_slice.max_chunk_notional` (CTL-202 前置).
pub const ACCT_A_MAX_CHUNK_USDT: i64 = 200;

pub fn engine_setup() -> EngineSetup {
    let mut oms = OmsConfig::new(1, HZ);
    oms.max_out_per_event = 16;
    oms.links_per_account = 4;
    oms.slots = PerTable::new(16);
    let mut a_params = ExecParams::default();
    a_params.force_slice.max_chunk_notional = usdt(ACCT_A_MAX_CHUNK_USDT);
    let exec = ExecConfig {
        default: ExecParams::default(),
        accounts: vec![
            ExecAccount {
                account: ACCT_A,
                sim: false,
                params: Some(a_params),
            },
            ExecAccount {
                account: ACCT_B,
                sim: false,
                params: None,
            },
            ExecAccount {
                account: SIM_A,
                sim: true,
                params: None,
            },
        ],
    };
    let mut keys = Vec::new();
    for (a, vs) in [(ACCT_A, [V_BN_USDM, V_BG_SPOT]), (ACCT_B, [V_BN_SPOT, V_BG_FUT])] {
        for v in vs {
            keys.push((a, v, 3u8));
        }
    }
    EngineSetup {
        caps: caps_row(),
        seq: seq_row(),
        oms,
        ledger: LedgerConfig::default(),
        exec,
        gws: GwMap {
            venues: LIVE_VENUES.to_vec(),
        },
        feeds: VenueSet(vbits(&LIVE_VENUES)),
        keys_required: keys,
        role_prod: false,
        advisor_max_lag_ms: 5_000,
        drain_timeout_ms: 0,
        risk_decode: Some(risk_decode),
        clock: SegClock {
            tsc_hz_milli: HZ,
            calib_tsc: T0,
            calib_realtime_ns: UTC0 + T0 as i64,
        },
        base_engine_ns: 0,
    }
}

/// Quantity at exponent −8 from a decimal string.
pub fn q(s: &str) -> Qty {
    Qty(hft_types::parse_exact(s.as_bytes(), -8).unwrap_or_else(|e| panic!("{s}: {e:?}")))
}

/// Price at exponent −8 (BTC in P0) from a decimal string.
pub fn px(s: &str) -> hft_types::Px {
    hft_types::Px(hft_types::parse_exact(s.as_bytes(), -8).unwrap_or_else(|e| panic!("{s}: {e:?}")))
}
