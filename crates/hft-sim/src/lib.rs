//! `hft-sim`: the simulated exchange of Sim accounts. `SimExchange` matches taker orders level by
//! level on a replica of the real venue's book at "decision time + simulated latency", fills resting
//! orders by trade-through, and keeps the account truth: spot balances, perpetual positions, cross
//! margin, funding and liquidation. Pure: no I/O, no threads, no clock — time comes from the
//! records and commands it is fed; every state is preallocated in `new`.
//! Specification: `crates/hft-sim/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod account;
mod exec;
mod fx;
pub mod glue;
mod hash;
mod market;
mod models;
mod state;
mod types;
mod view;

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use hft_book::{BookParams, L2Book};
use hft_event::{
    AccountConfigReportHdr, ConnRole, ConnState, ConnStateKind, GwCmd, GwOp, Level, LeverageItem, MarginItem,
    RecordHeader, RepMsg,
};
use hft_refdata::{
    Exchange, FeePair, InstKind, InstStatus, LimitBucket, LimitScope, MarginMode, RefData, SimAccountCfg,
    blob::{FeeOverrideRow, FeeRow, RawBlob, sec},
};
use hft_types::{AccountId, AssetId, InstId, Px, Qty, Rate, Side, Tsc, VenueId};
use zerocopy::IntoBytes;

pub use types::{
    CMDS_PER_POLL, Dist, LatencyModel, MAX_LATENCY_MODELS, MAX_MARGIN_ITEMS, MAX_MARGIN_MODELS,
    MAX_SIM_ACCOUNTS, MAX_VENUES_PER_ACCOUNT, MEMORY_RING, MakerFill, MarginModel, MarketEventRef, MmrSource,
    ORDER_TABLE_CAP, PENDING_CMD_CAP, PENDING_REPLY_CAP, REPLY_RESERVE, RejectReason, SIM_CODE_BASE,
    SimBalance, SimCfgErr, SimCounters, SimInput, SimMargin, SimOrderView, SimPosition, SimReply,
    SimSeedOrder, SimSeedState, SimSetup, SimTuning,
};
pub use view::SimAccountView;

use crate::models::{Leg, account_rng, check_latency, sample_us};
use crate::state::{
    Acct, Bal, BboRow, Bulk, InstRow, LateFee, MarginRep, MemEntry, NONE16, Order, PendCmd, PendReply,
    PendTransfer, PosRow, RateWin, SnapAsm, VENUE_SLOTS, VenueRow,
};

const EMPTY_ITEM: MarginItem = MarginItem {
    inst: InstId(0),
    adl_quantile: 0,
    _pad: [0; 3],
    liq_px: hft_types::Px(0),
};

/// Margin-item slab entries (pending `MarginReport`s with items).
const MARGIN_SLAB: usize = 256;
/// Pending transfers.
const TRANSFER_CAP: usize = 256;
/// Pending late fees.
const LATE_FEE_CAP: usize = 4_096;
/// Levels of one matching view / snapshot part reassembly.
const LEVEL_SCRATCH: usize = 8_192;

/// The simulated exchange (README "纯核心 API").
pub struct SimExchange {
    tune: SimTuning,
    hz_milli: u64,
    n_assets: usize,
    asset_exp: Vec<(i8, i8, bool)>,
    venues: Vec<VenueRow>,
    insts: Vec<InstRow>,
    /// `InstId.0` → tracked index (`NONE16` = untracked).
    inst_ix: Vec<u16>,
    accts: Vec<Acct>,
    /// `AccountId.0` → index into `accts` (`u8::MAX` = not a Sim account).
    acct_ix: [u8; 256],
    cmds: BinaryHeap<Reverse<PendCmd>>,
    replies: BinaryHeap<Reverse<PendReply>>,
    margin_slab: Vec<[MarginItem; MAX_MARGIN_ITEMS]>,
    margin_n: Vec<u16>,
    slab_free: Vec<u16>,
    transfers: Vec<PendTransfer>,
    late_fees: Vec<LateFee>,
    snap: SnapAsm,
    levels: Vec<(i64, i64)>,
    fills: Vec<(i64, i64)>,
    cand: Vec<u16>,
    bulk: Vec<u8>,
    seq: u64,
    last_tsc: u64,
    next_truth: u64,
    counters: SimCounters,
}

impl core::fmt::Debug for SimExchange {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SimExchange")
            .field("accounts", &self.accts.len())
            .field("insts", &self.insts.len())
            .field("pending_cmds", &self.cmds.len())
            .field("pending_replies", &self.replies.len())
            .field("last_tsc", &self.last_tsc)
            .finish_non_exhaustive()
    }
}

/// `(venue, tier)` fee rows and the account fee overrides read from the refdata blob.
struct FeeRows {
    tiers: Vec<FeeRow>,
    overrides: Vec<FeeOverrideRow>,
}

impl FeeRows {
    fn read(rd: &RefData) -> FeeRows {
        let raw = RawBlob::parse(rd.blob());
        let rows = |id: u16| -> Option<&hft_refdata::blob::RawSection> { raw.as_ref()?.section(id) };
        let tiers = rows(sec::FEES)
            .map(|s| {
                (0..s.count as usize)
                    .filter_map(|i| s.read_row::<FeeRow>(i))
                    .collect()
            })
            .unwrap_or_default();
        let overrides = rows(sec::FEE_OVERRIDES)
            .map(|s| {
                (0..s.count as usize)
                    .filter_map(|i| s.read_row::<FeeOverrideRow>(i))
                    .collect()
            })
            .unwrap_or_default();
        FeeRows { tiers, overrides }
    }

    fn tier(&self, venue: VenueId, tier: u8) -> Option<FeePair> {
        self.tiers
            .iter()
            .find(|r| r.venue == venue.0 && r.tier == tier)
            .map(|r| FeePair {
                maker: Rate(r.maker),
                taker: Rate(r.taker),
            })
    }

    fn resolve(&self, a: AccountId, venue: VenueId, inst: InstId, tier: u8) -> Option<FeePair> {
        let ov = |i: u32| {
            self.overrides
                .iter()
                .find(|r| r.account == a.0 && r.venue == venue.0 && r.inst == i)
                .map(|r| FeePair {
                    maker: Rate(r.maker),
                    taker: Rate(r.taker),
                })
        };
        ov(inst.0).or_else(|| ov(0)).or_else(|| self.tier(venue, tier))
    }
}

fn venue_exists(rd: &RefData, v: VenueId) -> bool {
    v.0 != 0 && usize::from(v.0) < VENUE_SLOTS && rd.venue(v).id == v
}

fn asset_exists(rd: &RefData, a: AssetId) -> bool {
    a.0 != 0 && rd.asset(a).id == a
}

/// Venue set of a Sim account: refdata `AccountMeta.venues` ∪ the venues of `initial_balances`.
fn account_venues(rd: &RefData, a: AccountId, cfg: &SimAccountCfg) -> Result<Vec<VenueId>, SimCfgErr> {
    let mut bits = rd.account(a).venues;
    for b in cfg.initial_balances.iter() {
        if !venue_exists(rd, b.venue) {
            return Err(SimCfgErr::UnknownVenue);
        }
        if !asset_exists(rd, b.asset) {
            return Err(SimCfgErr::UnknownAsset);
        }
        bits |= 1u64 << b.venue.0;
    }
    let vs: Vec<VenueId> = (0..64u8)
        .filter(|v| bits & (1u64 << v) != 0)
        .map(VenueId)
        .collect();
    if vs.iter().any(|v| !venue_exists(rd, *v)) {
        return Err(SimCfgErr::UnknownVenue);
    }
    if vs.len() > MAX_VENUES_PER_ACCOUNT {
        return Err(SimCfgErr::UnknownVenue);
    }
    for (i, v) in vs.iter().enumerate() {
        let x = rd.venue(*v).exchange;
        if vs.iter().skip(i + 1).any(|w| rd.venue(*w).exchange == x) {
            return Err(SimCfgErr::SameExchangeMarkets);
        }
    }
    Ok(vs)
}

impl SimExchange {
    /// Validates `setup` against `rd` and builds every state (the only allocation point). The
    /// start-up replies (`ConnState{Up}` per account, `AccountConfigReport` per (account, venue))
    /// are queued with `due_tsc = 0`.
    pub fn new(setup: &SimSetup, rd: &RefData) -> Result<SimExchange, SimCfgErr> {
        if setup.accounts.len() > MAX_SIM_ACCOUNTS {
            return Err(SimCfgErr::TooManyAccounts);
        }
        if setup.latency_models.len() > MAX_LATENCY_MODELS {
            return Err(SimCfgErr::LatencyModelIndex);
        }
        if setup.margin_models.len() > MAX_MARGIN_MODELS {
            return Err(SimCfgErr::MarginModelIndex);
        }
        for m in &setup.latency_models {
            check_latency(m)?;
        }
        let fees = FeeRows::read(rd);
        // Assets (dense from 1).
        let mut asset_exp = vec![(0i8, 0i8, false)];
        let mut id = 1u16;
        while asset_exists(rd, AssetId(id)) {
            let e = rd.asset(AssetId(id)).exp;
            asset_exp.push((e.px_exp, e.qty_exp, true));
            id = id.saturating_add(1);
            if id == u16::MAX {
                break;
            }
        }
        let n_assets = asset_exp.len();
        // Accounts: venue sets and model indices.
        let mut sorted: Vec<&(AccountId, SimAccountCfg)> = setup.accounts.iter().collect();
        sorted.sort_by_key(|(a, _)| *a);
        let mut acct_venues = Vec::with_capacity(sorted.len());
        let mut union = 0u64;
        for (a, cfg) in &sorted {
            let vs = account_venues(rd, *a, cfg)?;
            for v in &vs {
                union |= 1u64 << v.0;
            }
            acct_venues.push(vs);
        }
        // Venues.
        let mut venues: Vec<VenueRow> = Vec::with_capacity(VENUE_SLOTS);
        for v in 0..VENUE_SLOTS as u8 {
            let vid = VenueId(v);
            let m = rd.venue(vid);
            let c = rd.caps(vid);
            let limits = rd
                .rate_limits(vid)
                .iter()
                .filter(|l| {
                    l.bucket == LimitBucket::Orders
                        && matches!(l.scope, LimitScope::Ip | LimitScope::Uid | LimitScope::Account)
                        && l.limit > 0
                        && l.window_ms > 0
                })
                .take(4)
                .copied()
                .collect();
            venues.push(VenueRow {
                perp: m.product == InstKind::Perp,
                reduce_only: c.reduce_only,
                position_mode: c.required_position_mode,
                margin_mode: c.required_margin_mode,
                multi_asset: c.required_multi_asset,
                fee_rule: c.fee_asset_rule,
                funding_settle: c.funding_settle,
                margin_data: c.margin_data,
                limits,
                insts: Vec::new(),
                margin_asset: AssetId(0),
            });
        }
        // Tracked insts: every inst of a venue used by a Sim account.
        let n_rd = rd.inst_count();
        let mut inst_ix = vec![NONE16; n_rd + 1];
        let mut insts: Vec<InstRow> = Vec::new();
        for v in 0..VENUE_SLOTS as u8 {
            if union & (1u64 << v) == 0 {
                continue;
            }
            for &i in rd.insts_of_venue(VenueId(v)) {
                let h = rd.inst(i);
                let meta = rd.inst_meta(i);
                let ix = insts.len() as u16;
                if let Some(slot) = inst_ix.get_mut(i.0 as usize) {
                    *slot = ix;
                }
                let perp = h.kind == InstKind::Perp;
                let tradable = matches!(h.status, InstStatus::Trading | InstStatus::ReduceOnly);
                insts.push(InstRow {
                    id: i,
                    venue: h.venue,
                    perp,
                    base: h.asset,
                    quote: h.quote,
                    px_exp: h.px_exp,
                    qty_exp: h.qty_exp,
                    tick: h.tick.max(1),
                    lot: h.lot.max(1),
                    min_qty: h.min_qty.0,
                    min_notional: h.min_notional.0,
                    tradable,
                    reduce_only: h.status == InstStatus::ReduceOnly,
                    tiers: meta.mmr_tiers.to_vec(),
                    band: meta.price_band,
                    bbo: BboRow::default(),
                    book: L2Book::new(&BookParams::from_inst(h)),
                    book_ex_ts: 0,
                    book_set: false,
                    mark: 0,
                    mark_set: false,
                    rate: 0,
                    next_funding_ns: 0,
                    settled_ns: 0,
                    complement: NONE16,
                });
                if let Some(vr) = venues.get_mut(usize::from(v)) {
                    vr.insts.push(ix);
                    if perp && vr.margin_asset == AssetId(0) {
                        vr.margin_asset = h.quote;
                    }
                }
            }
        }
        // Binary outcome venues: pair each `<a>-up-<k>` slot with its `<a>-dn-<k>` token.
        for ix in 0..insts.len() {
            let Some(r) = insts.get(ix) else { continue };
            if rd.venue(r.venue).exchange != Exchange::Polymarket {
                continue;
            }
            let sym = &*rd.inst_meta(r.id).symbol;
            let other = if sym.contains("-up-") {
                sym.replacen("-up-", "-dn-", 1)
            } else if sym.contains("-dn-") {
                sym.replacen("-dn-", "-up-", 1)
            } else {
                continue;
            };
            let venue = r.venue;
            let j = insts
                .iter()
                .position(|o| o.venue == venue && *rd.inst_meta(o.id).symbol == *other)
                .map_or(NONE16, |j| j as u16);
            if let Some(r) = insts.get_mut(ix) {
                r.complement = j;
            }
        }
        let n_insts = insts.len();
        // Accounts.
        let mut accts = Vec::with_capacity(sorted.len());
        let mut acct_ix = [u8::MAX; 256];
        for ((a, cfg), vs) in sorted.iter().zip(acct_venues) {
            let lat = *setup
                .latency_models
                .get(usize::from(cfg.latency_model))
                .ok_or(SimCfgErr::LatencyModelIndex)?;
            let mm = *setup
                .margin_models
                .get(usize::from(cfg.margin_model))
                .ok_or(SimCfgErr::MarginModelIndex)?;
            let mut vslot = [u8::MAX; VENUE_SLOTS];
            for (k, v) in vs.iter().enumerate() {
                if let Some(s) = vslot.get_mut(usize::from(v.0)) {
                    *s = k as u8;
                }
                let vr = venues.get(usize::from(v.0)).ok_or(SimCfgErr::UnknownVenue)?;
                if fees.tier(*v, cfg.fee_tier).is_none() {
                    return Err(SimCfgErr::FeeTier);
                }
                if vr.perp && vr.margin_mode != MarginMode::Cross {
                    return Err(SimCfgErr::MarginMode);
                }
            }
            let fee_pairs = insts
                .iter()
                .map(|r| {
                    fees.resolve(*a, r.venue, r.id, cfg.fee_tier).unwrap_or(FeePair {
                        maker: Rate(0),
                        taker: Rate(0),
                    })
                })
                .collect();
            if let Some(s) = acct_ix.get_mut(usize::from(a.0)) {
                *s = accts.len() as u8;
            }
            let nv = vs.len();
            accts.push(Acct {
                id: *a,
                cfg: cfg.clone(),
                lat,
                mm,
                venues: vs,
                vslot,
                bal: vec![Bal::default(); nv * n_assets],
                pos: vec![PosRow::default(); n_insts],
                fees: fee_pairs,
                orders: vec![Order::EMPTY; ORDER_TABLE_CAP],
                n_resting: 0,
                mem: vec![MemEntry::default(); n_insts * 2 * MEMORY_RING],
                rng: account_rng(setup.tuning.seed, a.0),
                next_exch_id: 1,
                next_trade_id: 1,
                ins_seq: 0,
                term_seq: 0,
                conn_up: [true; VENUE_SLOTS],
                rate: vec![[RateWin::default(); 4]; nv],
                mrep: vec![MarginRep::default(); nv],
                counters: SimCounters::default(),
            });
        }
        if !accts.is_empty() {
            if setup.latency_models.is_empty() {
                return Err(SimCfgErr::LatencyModelIndex);
            }
            if setup.margin_models.is_empty() {
                return Err(SimCfgErr::MarginModelIndex);
            }
        }
        let max_perps = venues.iter().map(|v| v.insts.len()).max().unwrap_or(0);
        let mut ex = SimExchange {
            tune: setup.tuning,
            hz_milli: setup.tsc_hz_milli,
            n_assets,
            asset_exp,
            venues,
            insts,
            inst_ix,
            accts,
            acct_ix,
            cmds: BinaryHeap::with_capacity(PENDING_CMD_CAP),
            replies: BinaryHeap::with_capacity(PENDING_REPLY_CAP),
            margin_slab: vec![[EMPTY_ITEM; MAX_MARGIN_ITEMS]; MARGIN_SLAB],
            margin_n: vec![0; MARGIN_SLAB],
            slab_free: (0..MARGIN_SLAB as u16).rev().collect(),
            transfers: Vec::with_capacity(TRANSFER_CAP),
            late_fees: Vec::with_capacity(LATE_FEE_CAP),
            snap: SnapAsm {
                inst: NONE16,
                active: false,
                next_part: 0,
                bids: Vec::with_capacity(LEVEL_SCRATCH),
                asks: Vec::with_capacity(LEVEL_SCRATCH),
            },
            levels: Vec::with_capacity(LEVEL_SCRATCH),
            fills: Vec::with_capacity(LEVEL_SCRATCH),
            cand: Vec::with_capacity(ORDER_TABLE_CAP),
            bulk: Vec::with_capacity(
                16 * max_perps.max(MAX_MARGIN_ITEMS) + 2 * LEVEL_SCRATCH * size_of::<Level>(),
            ),
            seq: 0,
            last_tsc: 0,
            next_truth: 0,
            counters: SimCounters::default(),
        };
        for ai in 0..ex.accts.len() {
            ex.reset_balances(ai);
            ex.push_startup(ai, 0);
        }
        // First truth reports at event time 0; then every `report_interval_s`.
        for ai in 0..ex.accts.len() {
            ex.truth_reports(ai, 0);
        }
        ex.next_truth = ex.ms_to_tsc(u64::from(ex.tune.report_interval_s) * 1_000).max(1);
        Ok(ex)
    }

    /// Market data and time, in journal order (README "纯核心 API").
    pub fn on_record(&mut self, hdr: &RecordHeader, ev: SimInput<'_>) {
        let t = hdr.tsc;
        self.step_to(t);
        match ev {
            SimInput::Market(m) => self.apply_market(t, m),
            SimInput::VenueNotice { venue, kind, inst } => self.venue_notice(venue, kind, inst),
            SimInput::Clock => {}
        }
        self.last_tsc = self.last_tsc.max(t);
    }

    /// A command of the `out-sim` ring. Order-class ops are queued at `decided_tsc + entry`;
    /// `SimReset`/`SimTransfer` take effect at the time of the last consumed record; maintenance
    /// ops (and `SimFault`, M4) are ignored.
    pub fn on_command(&mut self, cmd: &GwCmd, decided_tsc: Tsc) {
        let Some(ai) = self.acct(cmd.account) else {
            return;
        };
        match cmd.op {
            GwOp::Place | GwOp::Cancel | GwOp::Amend | GwOp::CancelAll | GwOp::Query => {
                let entry = self.sample(ai, Leg::Entry);
                let arrive = decided_tsc.0.saturating_add(self.us_to_tsc(entry));
                self.seq = self.seq.wrapping_add(1);
                if self.cmds.len() >= PENDING_CMD_CAP {
                    self.counters.reply_overflow = self.counters.reply_overflow.wrapping_add(1);
                }
                self.cmds.push(Reverse(PendCmd {
                    arrive,
                    seq: self.seq,
                    acct: ai as u8,
                    cmd: *cmd,
                }));
            }
            GwOp::SimReset => {
                let t = self.last_tsc;
                self.sim_reset(ai, t);
            }
            GwOp::SimTransfer => {
                let t = self.last_tsc;
                self.sim_transfer(ai, cmd, t);
            }
            GwOp::SimSettle => {
                let t = self.last_tsc;
                self.sim_settle(ai, cmd, t);
            }
            _ => {}
        }
    }

    /// Consumed event time: executes commands with `arrive_tsc < t`, transfers and late fees due
    /// at or before `t`, the periodic truth and margin reports.
    pub fn advance(&mut self, t: Tsc) {
        self.step_to(t.0);
        self.last_tsc = self.last_tsc.max(t.0);
    }

    /// Delivers replies with `due_tsc ≤ now` in `(due_tsc, gen_seq)` order.
    pub fn drain_due<F: FnMut(SimReply<'_>)>(&mut self, now: Tsc, mut sink: F) {
        while let Some(Reverse(top)) = self.replies.peek() {
            if top.due > now.0 {
                break;
            }
            let Some(Reverse(r)) = self.replies.pop() else {
                break;
            };
            self.bulk.clear();
            let mut msg = r.msg;
            match r.bulk {
                Bulk::None => {}
                Bulk::Leverage { venue, lev } => {
                    let lev_rate = Rate(i64::from(lev) * 1_000_000_000);
                    if let Some(vr) = self.venues.get(usize::from(venue.0)) {
                        for &ii in &vr.insts {
                            if let Some(ir) = self.insts.get(usize::from(ii))
                                && ir.perp
                                && ir.tradable
                            {
                                let it = LeverageItem {
                                    inst: ir.id,
                                    _pad: 0,
                                    leverage: lev_rate,
                                };
                                self.bulk.extend_from_slice(it.as_bytes());
                            }
                        }
                    }
                    if let RepMsg::AccountConfigReport(h) = &mut msg {
                        h.n_lev = (self.bulk.len() / 16) as u16;
                    }
                }
                Bulk::Margin(k) => {
                    let n = usize::from(self.margin_n.get(usize::from(k)).copied().unwrap_or(0));
                    if let Some(items) = self.margin_slab.get(usize::from(k)) {
                        for it in items.iter().take(n) {
                            self.bulk.extend_from_slice(it.as_bytes());
                        }
                    }
                    self.slab_free.push(k);
                }
            }
            sink(SimReply {
                account: r.account,
                due_tsc: Tsc(r.due),
                gen_seq: r.seq,
                msg,
                bulk: &self.bulk,
            });
        }
    }

    /// Restarts account `a`'s random sub-stream from `seed` (README "9. 确定性"): the research replay
    /// reseeds at fixed event-time boundaries so a split replay and a whole one draw the same
    /// latencies, and parameter variants share the draws of each boundary interval. Unknown account →
    /// no effect.
    pub fn reseed_account(&mut self, a: AccountId, seed: u64) {
        if let Some(ai) = self.acct(a)
            && let Some(x) = self.accts.get_mut(ai)
        {
            x.rng = account_rng(seed, a.0);
        }
    }

    /// Earliest pending `due_tsc`.
    pub fn next_due(&self) -> Option<Tsc> {
        self.replies.peek().map(|Reverse(r)| Tsc(r.due))
    }

    /// Number of pending replies.
    pub fn pending_replies(&self) -> usize {
        self.replies.len()
    }

    /// Truth view of a Sim account.
    pub fn account(&self, a: AccountId) -> Option<SimAccountView<'_>> {
        let ai = self.acct(a)?;
        Some(SimAccountView { ex: self, ai })
    }

    /// Exchange-wide diagnostic counters (the per-account ones are in `SimAccountView`).
    pub fn counters(&self) -> SimCounters {
        let mut c = self.counters;
        for a in &self.accts {
            c.top_only_fills += a.counters.top_only_fills;
            c.liquidity_memory_hits += a.counters.liquidity_memory_hits;
            c.liquidations += a.counters.liquidations;
            c.book_not_live_rejects += a.counters.book_not_live_rejects;
            c.order_table_full += a.counters.order_table_full;
        }
        c
    }

    /// Re-initialises the truth of account `a` from the recovered engine state (README "重启恢复").
    pub fn seed_account_from(&mut self, a: AccountId, s: &SimSeedState) -> Result<(), SimCfgErr> {
        let ai = self.acct(a).ok_or(SimCfgErr::UnknownVenue)?;
        self.seed(ai, s)
    }

    /// A fill of account `a` that no order of this exchange caused (DST external fills:
    /// liquidation, ADL, manual trade): settles `qty` of `inst` at `px` like a taker fill without
    /// a fee (position and average cost, realised PnL, spot balances), at the current event time;
    /// produces no report (README "纯核心 API"). Returns actual perpetual cash (spot: zero),
    /// excluding fees. None rejects an unknown identity, unbound venue or nonpositive quantity/
    /// price without changing state. Callers must not recover this fact from balance differences.
    pub fn external_fill(&mut self, a: AccountId, inst: InstId, side: Side, qty: Qty, px: Px) -> Option<Qty> {
        let (ai, ii) = (self.acct(a)?, self.inst_index(inst)?);
        let venue = self.insts.get(ii)?.venue;
        let bound = self.accts.get(ai)?.vslot.get(usize::from(venue.0)).copied()?;
        if qty.0 <= 0 || px.0 <= 0 || bound == u8::MAX {
            return None;
        }
        let t = self.last_tsc;
        let (_, _, cash) = self.settle_fill(ai, ii, side, px.0, qty.0, 0, t);
        Some(Qty(cash))
    }

    /// Simulated order connection of `(a, venue)` Down/Up at event time `t` (stress `GwDown`);
    /// commands arriving while Down get `SendReport{NoConn}`.
    pub fn set_order_conn(&mut self, a: AccountId, venue: VenueId, up: bool, t: Tsc) {
        self.step_to(t.0);
        let Some(ai) = self.acct(a) else {
            return;
        };
        let Some(acct) = self.accts.get_mut(ai) else {
            return;
        };
        let Some(slot) = acct.conn_up.get_mut(usize::from(venue.0)) else {
            return;
        };
        if *slot == up {
            return;
        }
        *slot = up;
        let cs = ConnState {
            venue,
            conn: 0,
            state: if up {
                ConnStateKind::Up
            } else {
                ConnStateKind::Down
            },
            role: ConnRole::Order,
            inst: InstId(0),
            reason: 0,
            account: a,
            _pad2: [0; 3],
            rx_tsc: t.0,
        };
        self.push_reply(a, t.0, RepMsg::ConnState(cs), Bulk::None);
    }

    /// Hash of the whole simulated state (books, accounts, pending queues).
    pub fn state_hash(&self) -> u64 {
        hash::state_hash(self)
    }

    // ---- shared helpers ----

    fn acct(&self, a: AccountId) -> Option<usize> {
        let ix = *self.acct_ix.get(usize::from(a.0))?;
        (ix != u8::MAX).then_some(usize::from(ix))
    }

    fn inst_index(&self, i: InstId) -> Option<usize> {
        let ix = *self.inst_ix.get(i.0 as usize)?;
        (ix != NONE16).then_some(usize::from(ix))
    }

    /// µs → event-time ticks.
    fn us_to_tsc(&self, us: u32) -> u64 {
        let v = u128::from(us) * u128::from(self.hz_milli) / 1_000_000_000;
        u64::try_from(v).unwrap_or(u64::MAX)
    }

    /// ms → event-time ticks.
    fn ms_to_tsc(&self, ms: u64) -> u64 {
        let v = u128::from(ms) * u128::from(self.hz_milli) / 1_000_000;
        u64::try_from(v).unwrap_or(u64::MAX)
    }

    /// Event-time ticks → ms (floor).
    fn tsc_to_ms(&self, t: u64) -> u64 {
        if self.hz_milli == 0 {
            return 0;
        }
        let v = u128::from(t) * 1_000_000 / u128::from(self.hz_milli);
        u64::try_from(v).unwrap_or(u64::MAX)
    }

    fn sample(&mut self, ai: usize, leg: Leg) -> u32 {
        match self.accts.get_mut(ai) {
            Some(a) => sample_us(&a.lat, leg, &mut a.rng),
            None => 0,
        }
    }

    /// `t + resp` of account `ai`.
    fn resp_due(&mut self, ai: usize, t: u64) -> u64 {
        let r = self.sample(ai, Leg::Resp);
        t.saturating_add(self.us_to_tsc(r))
    }

    fn push_reply(&mut self, account: AccountId, due: u64, msg: RepMsg, bulk: Bulk) {
        self.push_reply_keep(account, due, msg, bulk, false);
    }

    /// `keep`: start-up replies survive `seed_account_from`.
    fn push_reply_keep(&mut self, account: AccountId, due: u64, msg: RepMsg, bulk: Bulk, keep: bool) {
        self.seq = self.seq.wrapping_add(1);
        if self.replies.len() >= PENDING_REPLY_CAP {
            self.counters.reply_overflow = self.counters.reply_overflow.wrapping_add(1);
        }
        self.replies.push(Reverse(PendReply {
            due,
            seq: self.seq,
            account,
            msg,
            bulk,
            keep,
        }));
    }

    /// `ConnState{Up}` of the account and one `AccountConfigReport` per venue.
    fn push_startup(&mut self, ai: usize, t: u64) {
        let Some(a) = self.accts.get(ai) else {
            return;
        };
        let id = a.id;
        let lev = a.mm.leverage;
        let cs = ConnState {
            venue: VenueId(0),
            conn: 0,
            state: ConnStateKind::Up,
            role: ConnRole::Order,
            inst: InstId(0),
            reason: 0,
            account: id,
            _pad2: [0; 3],
            rx_tsc: t,
        };
        self.push_reply_keep(id, t, RepMsg::ConnState(cs), Bulk::None, true);
        self.push_config_reports_keep(ai, t, lev, true);
    }

    fn push_config_reports(&mut self, ai: usize, t: u64, lev: u8) {
        self.push_config_reports_keep(ai, t, lev, false);
    }

    fn push_config_reports_keep(&mut self, ai: usize, t: u64, lev: u8, keep: bool) {
        let Some(a) = self.accts.get(ai) else {
            return;
        };
        let id = a.id;
        let n = a.venues.len();
        for k in 0..n {
            let Some(v) = self.accts.get(ai).and_then(|a| a.venues.get(k)).copied() else {
                continue;
            };
            let Some(vr) = self.venues.get(usize::from(v.0)) else {
                continue;
            };
            let h = AccountConfigReportHdr {
                venue: v,
                account: id,
                position_mode: vr.position_mode as u8,
                margin_mode: vr.margin_mode as u8,
                multi_asset: match vr.multi_asset {
                    None => 1,
                    Some(false) => 2,
                    Some(true) => 3,
                },
                source: 1,
                n_lev: 0,
                mismatches: 0,
                rflags: 0,
                uid_keys: 0,
                _pad: 0,
                venue_uid: [0; 24],
                ex_ts_ns: 0,
                rx_tsc: Tsc(t),
                _rsv: 0,
            };
            let bulk = if vr.perp {
                Bulk::Leverage { venue: v, lev }
            } else {
                Bulk::None
            };
            self.push_reply_keep(id, t, RepMsg::AccountConfigReport(h), bulk, keep);
        }
    }

    /// Balances back to `initial_balances`; everything else of the account untouched.
    fn reset_balances(&mut self, ai: usize) {
        let n_assets = self.n_assets;
        let Some(a) = self.accts.get_mut(ai) else {
            return;
        };
        for b in a.bal.iter_mut() {
            b.total = 0;
            b.locked = 0;
        }
        for ib in a.cfg.initial_balances.iter() {
            let Some(&vs) = a.vslot.get(usize::from(ib.venue.0)) else {
                continue;
            };
            if vs == u8::MAX {
                continue;
            }
            let ix = usize::from(vs) * n_assets + usize::from(ib.asset.0);
            if let Some(b) = a.bal.get_mut(ix) {
                b.total = b.total.saturating_add(ib.qty.0);
            }
        }
    }

    /// Balance row of `(ai, venue, asset)`.
    fn bal_mut(&mut self, ai: usize, venue: VenueId, asset: AssetId) -> Option<&mut Bal> {
        let n = self.n_assets;
        let a = self.accts.get_mut(ai)?;
        let vs = *a.vslot.get(usize::from(venue.0))?;
        if vs == u8::MAX || usize::from(asset.0) >= n {
            return None;
        }
        a.bal.get_mut(usize::from(vs) * n + usize::from(asset.0))
    }

    fn bal(&self, ai: usize, venue: VenueId, asset: AssetId) -> Bal {
        let n = self.n_assets;
        let Some(a) = self.accts.get(ai) else {
            return Bal::default();
        };
        let Some(&vs) = a.vslot.get(usize::from(venue.0)) else {
            return Bal::default();
        };
        if vs == u8::MAX || usize::from(asset.0) >= n {
            return Bal::default();
        }
        a.bal
            .get(usize::from(vs) * n + usize::from(asset.0))
            .copied()
            .unwrap_or_default()
    }

    fn qty_exp(&self, asset: AssetId) -> i32 {
        self.asset_exp
            .get(usize::from(asset.0))
            .map_or(0, |e| i32::from(e.1))
    }
}
