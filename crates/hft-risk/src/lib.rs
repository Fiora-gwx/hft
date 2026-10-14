//! `hft-risk`: hot-path risk and the mode state. `classify()` (the only source of
//! `IntentClass`, `RISK_REDUCING` and exchange-side `reduceOnly`), the check chain for every
//! class, versioned `RiskParams`, token buckets with the risk-reducing reserve and backoff,
//! daily loss, the exposure gate, margin tiers by distance to liquidation, the perp leverage
//! cap, the flatten lock, reconciliation grading, and `mode` (desired modes, caps, effective
//! mode, start-up gates, `ctl_state_version`, kill projection).
//!
//! No allocation after construction, no system calls, no clocks, no atomics, no floating point;
//! nothing panics on any input. Specification: `crates/hft-risk/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[cfg(target_endian = "big")]
compile_error!("hft-risk: snapshot rows are little-endian only");

mod blob;
mod bucket;
mod chain;
mod classify;
mod leverage;
mod margin;
pub mod mode;
mod params;
#[cfg(feature = "planted")]
pub mod planted;
mod pnl_gate;
pub mod snapshot;
mod types;
mod vec;

pub use blob::{RISK_BLOB_MAGIC, RiskBlobError};

use hft_event::{AccountConfigReportHdr, CmdScope, LeverageItem, Note, NoteLevel};
use hft_refdata::{AccountKind, InstHot, InstStatus, RefData};
use hft_strategy_sdk::{CapBits, GroupTable, InstRiskView, MarginLevel, Now, RiskEvent, RiskStateView};
use hft_types::{
    AccountId, AssetExp, AssetId, CapSource, GroupId, HedgePxLimits, InstId, MarketKind, Notional, Px, Qty,
    Rate, StrategyId, Tsc, VenueId,
};

pub use classify::{classify, classify_amend};
pub use leverage::acr_rflags;
pub use margin::{ForceDeleverage, MarginOut, MarginSample, RiskTimer, mflags};
pub use mode::{Mode, ModeInput, ModeOutcome, ModeView};
pub use params::{
    AcctLimits, AcctSpec, BandSpec, DEFAULT_CONN_LIVENESS_MS, DEFAULT_FUNDS_BUFFER, DEFAULT_MARGIN,
    DEFAULT_MAX_PERP_LEVERAGE_X100, DEFAULT_RR_RESERVE, GroupLimits, GroupLimitsRow, InstClassRow,
    MarginLimits, ParamsBuilder, PxBand, RiskParams, VenueLimits, VenueSpec, default_margin_stale_ms,
};
pub use pnl_gate::{ASSET_KEY, ReconVerdicts, ReconcileGroup, eq_flags};
pub use types::{
    AccountScope, Admission, Capacities, Classification, EquitySource, EquityView, ExecBounds, GwMap,
    IntentDraft, IntentView, LegSummary, ModeCfg, NOTE_CODES, Origin, PosFacts, PosView, RiskParamError,
    RiskView, Scope, VENUE_NONE, cap_of_note,
};
pub use vec::{FixedVec, PreVec};

use bucket::{Buckets, InstBucket, VenueBucket};
use leverage::Leverage;
use margin::MState;
use params::{Lim, ParamIx, RowCaps};
use pnl_gate::{AcctPnl, Legs, ReconState};

/// Refdata facts of one inst kept by `Risk`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct InstFacts {
    pub venue: VenueId,
    pub status: InstStatus,
    pub perp: bool,
    pub exp: AssetExp,
    pub quote: AssetId,
    pub tick: i64,
    pub lot: i64,
}

impl InstFacts {
    fn of(h: &InstHot) -> InstFacts {
        InstFacts {
            venue: h.venue,
            status: h.status,
            perp: h.venue.market() == Some(MarketKind::LinearPerp),
            exp: AssetExp {
                px_exp: h.px_exp,
                qty_exp: h.qty_exp,
            },
            quote: h.quote,
            tick: h.tick,
            lot: h.lot,
        }
    }
}

/// Everything of `Risk` except the mode state (split so `Risk::apply` can lend it to the mode
/// as the risk side of `ModeView`).
#[derive(Clone, Debug)]
pub(crate) struct Side {
    pub n_accounts: usize,
    pub insts: Vec<InstFacts>,
    pub dust: Vec<i64>,
    pub acct_venues: Vec<u64>,
    pub acct_live: Vec<bool>,
    pub acct_bound: Vec<StrategyId>,
    pub params: RiskParams,
    pub scratch: RiskParams,
    pub ix: ParamIx,
    pub scratch_ix: ParamIx,
    pub rowcaps: RowCaps,
    pub exec: Vec<ExecBounds>,
    pub buckets: Buckets,
    pub margin: Vec<MState>,
    pub lev: Leverage,
    pub pnl: Vec<AcctPnl>,
    pub legs: Legs,
    pub recon: ReconState,
    pub run_start: i64,
}

impl Side {
    #[inline]
    pub fn lim(&self) -> Lim<'_> {
        Lim {
            p: &self.params,
            ix: &self.ix,
        }
    }
    #[inline]
    pub fn inst(&self, i: InstId) -> Option<&InstFacts> {
        if i.0 == 0 {
            return None;
        }
        self.insts.get(i.0 as usize).filter(|f| f.venue.0 != 0)
    }
    pub fn venues(&self, a: AccountId) -> u64 {
        self.acct_venues.get(usize::from(a.0)).copied().unwrap_or(0)
    }
    pub fn mstate(&self, a: AccountId, v: VenueId) -> Option<&MState> {
        self.margin.get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
    }
    pub fn exec_of(&self, a: AccountId) -> ExecBounds {
        self.exec.get(usize::from(a.0)).copied().unwrap_or_default()
    }
    pub fn naked_cleared(&self, a: AccountId, g: GroupId) -> bool {
        let dust = self.exec_of(a).dust_notional;
        self.legs
            .get(a, g)
            .is_some_and(|s| s.quiescent && s.naked_filled_notional <= dust)
    }
    pub fn daily_loss_resumable(&self, a: AccountId) -> bool {
        let max = self.lim().acct(a).daily_loss_max_resumes;
        self.pnl
            .get(usize::from(a.0))
            .is_some_and(|p| p.resumes_today < max)
    }
    /// Account margin tier and the bitmap of venues at L1 or above, over every venue of the
    /// account except `skip`.
    fn margin_level_except(&self, a: AccountId, skip: VenueId) -> (u8, u64) {
        let mut lvl = 0u8;
        let mut bits = 0u64;
        let mut rest = self.venues(a) & !(1u64 << (skip.0 & 63));
        while rest != 0 {
            let v = rest.trailing_zeros() as u8;
            rest &= rest - 1;
            if let Some(s) = self.mstate(a, VenueId(v)) {
                let c = s.contribution();
                if c > 0 {
                    lvl = lvl.max(c);
                    bits |= 1u64 << v;
                }
            }
        }
        (lvl, bits)
    }
}

/// Facts of the reconciliation grading (borrowed apart from the grading state).
struct GradeView<'a> {
    insts: &'a [InstFacts],
    dust: &'a [i64],
    lim: Lim<'a>,
}

impl pnl_gate::GradeFacts for GradeView<'_> {
    fn lot(&self, i: InstId) -> i64 {
        if i.0 == 0 {
            return 1;
        }
        self.insts.get(i.0 as usize).map_or(1, |f| f.lot.max(1))
    }
    fn dust(&self, asset: u16) -> i64 {
        self.dust.get(usize::from(asset)).copied().unwrap_or(1).max(1)
    }
    fn recon_line(&self, a: AccountId) -> Notional {
        self.lim.acct(a).recon_closeonly_notional
    }
}

/// Risk side of `ModeView` combined with the sequencer's adapter for the rest.
struct Combined<'a, V> {
    s: &'a Side,
    ext: &'a V,
}

impl<V: ModeView> ModeView for Combined<'_, V> {
    fn cell_flat(&self, c: mode::CellKey) -> bool {
        self.ext.cell_flat(c)
    }
    fn naked_cleared(&self, a: AccountId, g: GroupId) -> bool {
        self.s.naked_cleared(a, g)
    }
    fn recon_resumable(&self, a: AccountId, v: VenueId) -> bool {
        self.s.recon.streak(a, v) >= 2
    }
    fn daily_loss_resumable(&self, a: AccountId) -> bool {
        self.s.daily_loss_resumable(a)
    }
    fn unknown_count(&self, a: AccountId, v: VenueId) -> u32 {
        self.ext.unknown_count(a, v)
    }
    fn lev_bad(&self, a: AccountId, v: VenueId) -> bool {
        self.s.lev.is_bad(a, v)
    }
    fn engine_reduce_pending(&self) -> bool {
        self.ext.engine_reduce_pending()
    }
}

/// Hot-path risk and the mode state (single writer: the sequencer thread).
#[derive(Clone, Debug)]
pub struct Risk {
    pub mode: Mode,
    pub(crate) s: Side,
}

impl Risk {
    /// Allocates every table once (accounts, (account, venue) pairs and `caps` from refdata);
    /// `exec` is indexed by `AccountId`.
    pub fn new(
        p: &RiskParams,
        rd: &RefData,
        gws: &GwMap,
        caps: &Capacities,
        exec: &[ExecBounds],
        mc: ModeCfg,
    ) -> Result<Risk, RiskParamError> {
        let rowcaps = RowCaps::of(rd, caps);
        rowcaps.check(p)?;
        let n = params::account_count(rd);
        let n_insts = rd.inst_count();
        let mut params = params::with_caps(&rowcaps);
        if !params::copy_params(p, &mut params) {
            return Err(RiskParamError::Capacity);
        }
        params
            .groups
            .as_mut_slice()
            .sort_unstable_by_key(|r| (r.account, r.group));
        let mut insts = vec![InstFacts::of(rd.inst(InstId(0))); n_insts + 1];
        for (i, f) in insts.iter_mut().enumerate().skip(1) {
            *f = InstFacts::of(rd.inst(InstId(i as u32)));
        }
        let mut dust = vec![1i64; usize::from(u16::MAX) + 1];
        for (k, d) in dust.iter_mut().enumerate().skip(1) {
            let m = rd.asset(AssetId(k as u16));
            if m.id.0 as usize != k {
                break;
            }
            *d = m.dust.0;
        }
        let mut acct_venues = vec![0u64; n + 1];
        let mut acct_live = vec![false; n + 1];
        let mut acct_bound = vec![StrategyId(0); n + 1];
        for a in 1..=n {
            let m = rd.account(AccountId(a as u8));
            if let Some(x) = acct_venues.get_mut(a) {
                *x = m.venues;
            }
            if let Some(x) = acct_live.get_mut(a) {
                *x = matches!(m.kind, AccountKind::Live);
            }
            if let Some(x) = acct_bound.get_mut(a) {
                *x = m.bound_instance;
            }
        }
        params::validate_rows_n(&params, n, &acct_venues)?;
        let mut ix = ParamIx::new(n, n_insts);
        ix.rebuild(&params);
        let mut exec_v = vec![ExecBounds::default(); n + 1];
        for (i, b) in exec.iter().enumerate().take(n + 1) {
            if let Some(x) = exec_v.get_mut(i) {
                *x = *b;
            }
        }
        params::check_exec_facts(&Lim { p: &params, ix: &ix }, &insts, &acct_venues, &exec_v)?;
        let mut limits = vec![(None, None); 64];
        for (v, l) in limits.iter_mut().enumerate() {
            *l = params::window_limits(rd, VenueId(v as u8));
        }
        let n_av = params::account_venue_count(rd);
        let max_trading = (caps.max_cells_total as usize).saturating_mul(4).max(16);
        let mut s = Side {
            n_accounts: n,
            insts,
            dust,
            acct_venues,
            acct_live,
            acct_bound,
            scratch: params::with_caps(&rowcaps),
            params,
            scratch_ix: ix.clone(),
            ix,
            rowcaps,
            exec: exec_v,
            buckets: Buckets {
                venue: vec![VenueBucket::default(); (n + 1) * 64],
                inst: vec![InstBucket::default(); (n + 1) * (n_insts + 1)],
                n_insts,
                limits,
            },
            margin: vec![MState::default(); (n + 1) * 64],
            lev: Leverage::new(n, n_insts, max_trading),
            pnl: vec![AcctPnl::default(); n + 1],
            legs: Legs {
                rows: PreVec::with_capacity(caps.max_cells_total as usize),
            },
            recon: ReconState::new(n, n_av),
            run_start: 0,
        };
        s.fill_buckets();
        for a in 1..=n {
            let acc = AccountId(a as u8);
            let q = s.quota(acc);
            if let Some(p) = s.pnl.get_mut(a) {
                p.loss_quota = q;
                p.loss_trigger = Notional(q.0.saturating_neg());
            }
        }
        Ok(Risk {
            mode: Mode::new(rd, gws, caps, mc),
            s,
        })
    }

    /// The notional definition of check #6 (exponents of the inst's asset).
    pub fn notional(qty: Qty, px: Px, exp: AssetExp, quote_rate: Rate) -> Notional {
        types::notional(qty, px, exp, quote_rate)
    }

    /// Pure classification (see `classify`).
    pub fn classify(
        it: &IntentDraft,
        origin: Origin,
        pos: &impl PosView,
        caps: &hft_refdata::VenueCaps,
    ) -> Classification {
        classify::classify(it, origin, pos, caps)
    }

    /// The check chain (read-only).
    pub fn evaluate<V: RiskView>(
        &self,
        it: &IntentView,
        v: &V,
        now: &Now,
    ) -> Result<Admission, hft_types::RejectBits> {
        chain::evaluate(&self.s, &self.mode, it, v, now)
    }

    /// Deducts the tokens of an intent that leaves the ring.
    pub fn commit(&mut self, a: &Admission, now: &Now) {
        chain::commit(&mut self.s, a, now);
    }

    /// `mode.apply` with the risk side of `ModeView` (`naked_cleared`, `recon_resumable`,
    /// `daily_loss_resumable`, `lev_bad`) answered by this `Risk`; `ext` answers the rest.
    pub fn apply(
        &mut self,
        inp: ModeInput<'_>,
        seq: u64,
        ext: &impl ModeView,
        out: &mut ModeOutcome,
    ) -> Result<(), mode::ModeErr> {
        let r = {
            let v = Combined { s: &self.s, ext };
            self.mode.apply(inp, seq, &v, out)
        };
        if r.is_ok() {
            // The latch lasts exactly as long as the account's `DailyLoss` cap (restore derives it
            // the same way).
            for (a, e) in out.risk_events.iter() {
                let latched = match e {
                    RiskEvent::CapSet { source, .. } if *source == CapSource::DAILY_LOSS => true,
                    RiskEvent::CapCleared { source, .. } if *source == CapSource::DAILY_LOSS => false,
                    _ => continue,
                };
                if let Some(p) = self.s.pnl.get_mut(usize::from(a.0)) {
                    p.latched = latched;
                }
            }
        }
        r
    }

    /// `PREVIEW` with the previous-round differences filled in.
    pub fn preview(&self, inp: &ModeInput<'_>, ext: &impl ModeView, out: &mut mode::Preview) {
        let v = Combined { s: &self.s, ext };
        self.mode.preview(inp, &v, out);
        let n = out.items.len();
        let mut items = out.items;
        items.clear();
        for k in 0..n {
            if let Some(mut it) = out.items.get(k).copied() {
                let key = if it.key.item == mode::ReconItemKey::BALANCE {
                    it.key.id | ASSET_KEY
                } else {
                    it.key.id
                };
                it.last_diff = self
                    .s
                    .recon
                    .last_diff(it.account, it.venue, key)
                    .unwrap_or(Qty(0));
                items.push(it);
            }
        }
        out.items = items;
    }

    /// The checks `apply` makes before changing anything (`InvalidScope` … source preconditions),
    /// without applying: the status of a `PREVIEW` (control-plane "版本与确认").
    pub fn check_mode(&self, inp: &ModeInput<'_>, ext: &impl ModeView) -> Result<(), mode::ModeErr> {
        let v = Combined { s: &self.s, ext };
        self.mode.check(inp, &v)
    }

    // ------------------------------------------------------------------------- non-mode inputs

    /// `day_pnl` below the trigger line → `Auto(DailyLoss)` (latched).
    pub fn on_pnl(
        &mut self,
        acct: AccountId,
        day_pnl: Notional,
        eq: EquityView,
    ) -> Option<ModeInput<'static>> {
        let a = usize::from(acct.0);
        let valid = self
            .s
            .pnl
            .get(a)
            .is_some_and(|p| p.eq_flags & eq_flags::SOD_VALID != 0);
        if !valid && eq.valid {
            if let Some(p) = self.s.pnl.get_mut(a) {
                p.equity_sod = eq.total;
                p.eq_flags |= eq_flags::SOD_VALID;
            }
            let q = self.s.quota(acct);
            if let Some(p) = self.s.pnl.get_mut(a) {
                p.loss_quota = q;
                if !p.latched && p.resumes_today == 0 {
                    p.loss_trigger = Notional(q.0.saturating_neg());
                }
            }
        }
        let p = self.s.pnl.get_mut(a)?;
        p.day_pnl_base = day_pnl;
        if p.latched || day_pnl >= p.loss_trigger {
            return None;
        }
        p.latched = true;
        Some(pnl_gate::daily_loss_input(acct, p.loss_trigger))
    }

    /// After `Resume{DailyLoss}`: trigger = `day_pnl − quota`, one more resume used.
    pub fn on_daily_loss_resumed(&mut self, acct: AccountId, day_pnl: Notional) {
        let q = self.s.quota(acct);
        if let Some(p) = self.s.pnl.get_mut(usize::from(acct.0)) {
            p.loss_quota = q;
            p.loss_trigger = Notional(day_pnl.0.saturating_sub(q.0));
            p.resumes_today = p.resumes_today.saturating_add(1);
            p.latched = false;
        }
    }

    /// Records the latest leg summary; Broken + quiescent + filled naked ≤ dust → `AutoClear{Broken}`.
    pub fn on_legs(&mut self, acct: AccountId, g: GroupId, s: &LegSummary) -> Option<ModeInput<'static>> {
        self.s.legs.put(acct, g, *s);
        let scope = mode::account_group_scope(acct, g);
        if self.mode.cap(CapSource::BROKEN, &scope).is_some() && self.s.naked_cleared(acct, g) {
            return Some(ModeInput::AutoClear {
                source: CapSource::BROKEN,
                scope,
            });
        }
        None
    }

    pub fn naked_cleared(&self, a: AccountId, g: GroupId) -> bool {
        self.s.naked_cleared(a, g)
    }
    pub fn recon_resumable(&self, a: AccountId, v: VenueId) -> bool {
        self.s.recon.streak(a, v) >= 2
    }
    pub fn daily_loss_resumable(&self, a: AccountId) -> bool {
        self.s.daily_loss_resumable(a)
    }

    /// Rebuilds the traded perp insts of `acct` from its group table and re-judges `lev_bad`.
    pub fn on_groups_changed(&mut self, acct: AccountId, groups: &GroupTable) {
        self.on_group_specs(acct, groups.specs());
    }

    /// `on_groups_changed` over a plain `GroupSpec` slice.
    pub fn on_group_specs(&mut self, acct: AccountId, specs: &[hft_event::GroupSpec]) {
        let s = &mut self.s;
        s.lev.trading.retain(|(a, _, _)| *a != acct);
        for g in specs {
            for leg in g.legs.iter().take(usize::from(g.n_legs).min(4)) {
                let Some(f) = s.inst(leg.inst).copied() else {
                    continue;
                };
                if !f.perp {
                    continue;
                }
                let e = (acct, f.venue, leg.inst);
                let rows = s.lev.trading.as_slice();
                if let Err(at) = rows.binary_search(&e) {
                    let _ = s.lev.trading.insert(at, e);
                }
            }
        }
        let max = s.params.max_perp_leverage_x100;
        let venues = s.venues(acct);
        for v in 0u8..64 {
            if venues & (1u64 << v) != 0 {
                s.lev.recheck(acct, VenueId(v), max);
            }
        }
    }

    pub fn lev_bad(&self, a: AccountId, v: VenueId) -> bool {
        self.s.lev.is_bad(a, v)
    }

    /// A `MarginReport` or a mark change of (account, venue); O(1).
    pub fn on_margin(
        &mut self,
        acct: AccountId,
        v: VenueId,
        sample: &MarginSample,
        now: &Now,
        out: &mut MarginOut,
    ) {
        out.clear();
        if self.s.venues(acct) & (1u64 << (v.0 & 63)) == 0 || v.market() != Some(MarketKind::LinearPerp) {
            return;
        }
        let ix = usize::from(acct.0) * 64 + usize::from(v.0 & 63);
        let c0 = self.s.margin.get(ix).map_or(0, MState::contribution);
        let m = self.s.lim().acct(acct).margin;
        let c = margin::Ctx {
            a: acct,
            v,
            now,
            run_start: self.s.run_start,
            m: &m,
        };
        if let Some(st) = self.s.margin.get_mut(ix) {
            margin::on_sample(st, sample, &c, out);
        }
        self.margin_cap(acct, v, c0, out);
    }

    /// A `Timer{Risk, …}` of this crate.
    pub fn on_timer(&mut self, t: RiskTimer, now: &Now, out: &mut MarginOut) {
        out.clear();
        let (a, v) = match t {
            RiskTimer::MarginStale { a, v }
            | RiskTimer::MarginDeadline { a, v }
            | RiskTimer::MarginForce { a, v } => (a, v),
        };
        let ix = usize::from(a.0) * 64 + usize::from(v.0 & 63);
        let c0 = self.s.margin.get(ix).map_or(0, MState::contribution);
        let m = self.s.lim().acct(a).margin;
        let c = margin::Ctx {
            a,
            v,
            now,
            run_start: self.s.run_start,
            m: &m,
        };
        match self.s.margin.get_mut(ix) {
            Some(st) if st.present => margin::on_timer(st, t, &c, out),
            _ => return,
        }
        self.margin_cap(a, v, c0, out);
    }

    /// Account tier input when (a, v)'s contribution changed from `c0`; the account tier and its
    /// venue bitmap depend on nothing else, so an unchanged contribution skips the scan.
    fn margin_cap(&self, a: AccountId, v: VenueId, c0: u8, out: &mut MarginOut) {
        let ix = usize::from(a.0) * 64 + usize::from(v.0 & 63);
        let c1 = self.s.margin.get(ix).map_or(0, MState::contribution);
        if c1 == c0 {
            return;
        }
        let (lvl, bits) = self.s.margin_level_except(a, v);
        let with = |c: u8| {
            if c > 0 {
                (lvl.max(c), bits | 1u64 << (v.0 & 63))
            } else {
                (lvl, bits)
            }
        };
        let after = with(c1);
        if after != with(c0) {
            out.mode.push(margin::cap_input(a, after.0, after.1));
        }
    }

    /// `AccountConfigReport`: effective leverage of the venue's perp insts.
    pub fn on_account_cfg(&mut self, hdr: &AccountConfigReportHdr, lev: &[LeverageItem]) {
        let s = &mut self.s;
        let insts = &s.insts;
        s.lev.on_report(hdr, lev, |i| {
            insts
                .get(i.0 as usize)
                .filter(|_| i.0 != 0)
                .map_or(VenueId(0), |f| f.venue)
        });
        let max = s.params.max_perp_leverage_x100;
        s.lev.recheck(hdr.account, hdr.venue, max);
    }

    /// Effective leverage of an inst on the account's venue; unknown → `None`.
    pub fn leverage(&self, a: AccountId, v: VenueId, i: InstId) -> Option<Rate> {
        match self.s.inst(i) {
            Some(f) if f.venue == v => self.s.lev.get(a, i),
            _ => None,
        }
    }

    /// This run's start (`EngineClock.base_engine_ns`).
    pub fn set_run_start(&mut self, run_start_ens: i64) {
        self.s.run_start = run_start_ens;
    }

    /// The only grading point of reconciliation reports.
    pub fn on_reconcile(&mut self, grp: &ReconcileGroup<'_>) -> ReconVerdicts {
        let Side {
            recon,
            insts,
            dust,
            params,
            ix,
            ..
        } = &mut self.s;
        let f = GradeView {
            insts,
            dust,
            lim: Lim { p: params, ix },
        };
        let (v, us) = pnl_gate::grade(recon, &f, grp);
        if let Some((a, vv, view_seq, down)) = us
            && let Some(u) = self.mode.us_row_mut(a, vv)
        {
            u.prev_view_seq = view_seq;
            u.uflags = if down { mode::PREV_US_DOWN } else { 0 };
        }
        v
    }

    /// `AUTO_EXTERNAL` user-stream precondition (read-only).
    pub fn auto_external_ok(&self, a: AccountId, v: VenueId, summary_sflags: u16) -> bool {
        let u = self.mode.us_row(a, v);
        summary_sflags & hft_event::ReconcileReport::USERSTREAM_DOWN == 0
            && u.uflags & mode::PREV_US_DOWN == 0
            && u.us_up_seq != 0
            && u.us_up_seq < u.prev_view_seq
    }

    pub fn on_user_stream_down(&mut self, a: AccountId, v: VenueId) {
        self.s.recon.set_streak(a, v, 0);
    }

    /// Exchange rate-limit hit: clears both window buckets and backs off exponentially.
    pub fn on_rate_limit_hit(&mut self, v: VenueId, a: AccountId, now: &Now) -> Note {
        chain::rate_limit_hit(&mut self.s, v, a, now)
    }

    /// Day roll (only for `day > last_day_rolled`).
    pub fn on_day_roll(&mut self, day: u32, eqs: &impl EquitySource) {
        let _ = day;
        for a in 1..=self.s.n_accounts {
            let acc = AccountId(a as u8);
            let eq = eqs.equity(acc);
            if let Some(p) = self.s.pnl.get_mut(a) {
                p.day_pnl_base = Notional(0);
                p.resumes_today = 0;
                if eq.valid {
                    p.equity_sod = eq.total;
                    p.eq_flags |= eq_flags::SOD_VALID;
                } else {
                    p.equity_sod = Notional(0);
                    p.eq_flags &= !eq_flags::SOD_VALID;
                }
            }
            let q = self.s.quota(acc);
            if let Some(p) = self.s.pnl.get_mut(a) {
                p.loss_quota = q;
                if !p.latched {
                    p.loss_trigger = Notional(q.0.saturating_neg());
                }
            }
        }
    }

    /// Hedge pricing limits (same values as check #5).
    pub fn hedge_px_limits(&self, a: AccountId, v: VenueId, i: InstId) -> HedgePxLimits {
        let _ = (a, v);
        HedgePxLimits {
            conn_liveness_ms: self.s.params.conn_liveness_ms,
            rr_max_cross: self.s.lim().band(i).rr_max_cross,
        }
    }

    /// `SetRisk`.
    pub fn set_params(&mut self, p: &RiskParams, scope: AccountScope) -> Result<(), RiskParamError> {
        self.s.set_params(p, scope)
    }

    /// Whether `p` loosens the current params under `scope`.
    pub fn loosens(&self, p: &RiskParams, scope: AccountScope) -> bool {
        params::loosens(&self.s.params, p, scope)
    }

    /// Current params.
    pub fn params(&self) -> &RiskParams {
        &self.s.params
    }

    /// `Views::risk` of an account.
    pub fn view(&self, a: AccountId, now: &Now) -> RiskStateView {
        let s = &self.s;
        let mut level = 0u8;
        let mut liq = Rate::MAX;
        let mut ratio = Rate(0);
        let mut deadline: Option<i64> = None;
        for v in 0u8..64 {
            if s.venues(a) & (1u64 << v) == 0 {
                continue;
            }
            if let Some(st) = s.mstate(a, VenueId(v))
                && st.present
            {
                level = level.max(st.level);
                liq = liq.min(st.liq());
                ratio = ratio.max(st.ratio());
                if st.level >= 2 && st.deadline_ens != 0 {
                    deadline = Some(deadline.map_or(st.deadline_ens, |d| d.min(st.deadline_ens)));
                }
            }
        }
        let mask = self.mode.global_mask | self.mode.acct_mask_of(a);
        let l = s.lim();
        let ac = l.acct(a);
        RiskStateView {
            margin_level: match level {
                1 => MarginLevel::L1,
                2 => MarginLevel::L2,
                3 => MarginLevel::L3,
                _ => MarginLevel::None,
            },
            liq_dist: liq,
            margin_ratio: ratio,
            deadline_tsc: deadline.map_or(Tsc(0), |d| now.tsc_of(d)),
            open_blocked: mask != 0,
            caps: CapBits(mask),
            hard: self.mode.hard(a),
            flatten_lock: self.mode.flatten_lock(a),
            max_hedge_notional: ac.max_hedge_notional,
            max_reduce_notional: ac.max_reduce_notional,
            max_order_notional: ac.max_order_notional,
            conn_liveness_ms: s.params.conn_liveness_ms,
            dust_notional: s.exec_of(a).dust_notional,
        }
    }

    /// `Views::risk_for` of an inst (price band of its class, `l3_max_cross`, backoff).
    pub fn view_for(&self, a: AccountId, i: InstId, now: &Now) -> InstRiskView {
        let b = self.s.lim().band(i);
        let v = self.s.inst(i).map_or(VenueId(0), |f| f.venue);
        InstRiskView {
            max_px_dev: b.max_px_dev,
            ioc_max_cross: b.ioc_max_cross,
            rr_max_cross: b.rr_max_cross,
            l3_max_cross: self.s.params.l3_max_cross,
            quote_rate: Rate(0),
            rate_backoff: self.backoff_until(v, a) > now.engine_ns,
        }
    }

    /// Backoff end of the (venue, account) bucket (engine_ns); 0 = never backed off.
    pub fn backoff_until(&self, v: VenueId, a: AccountId) -> i64 {
        self.s
            .buckets
            .venue
            .get(Buckets::vix(a, v))
            .map_or(0, |b| b.backoff_until_ens)
    }

    /// Daily-loss state of `a` (snapshot section 17 `RiskAcctRow`): `(loss_trigger,
    /// resumes_today, latched)`; read-only, for diagnostics and tests.
    pub fn daily_loss_state(&self, a: AccountId) -> Option<(Notional, u8, bool)> {
        self.s
            .pnl
            .get(usize::from(a.0))
            .map(|p| (p.loss_trigger, p.resumes_today, p.latched))
    }
}

impl Side {
    /// Today's quota: `min(daily_loss_abs, equity_sod × daily_loss_kill)` (abs only before the
    /// first valid equity).
    pub fn quota(&self, a: AccountId) -> Notional {
        let ac = self.lim().acct(a);
        let abs = ac.daily_loss_abs;
        match self.pnl.get(usize::from(a.0)) {
            Some(p) if p.eq_flags & eq_flags::SOD_VALID != 0 => abs.min(
                p.equity_sod
                    .mul_rate(ac.daily_loss_kill, hft_types::RoundMode::Floor),
            ),
            _ => abs,
        }
    }

    /// Fills every bucket to its capacity (construction).
    fn fill_buckets(&mut self) {
        let headroom = self.params.rate_headroom;
        for a in 1..=self.n_accounts {
            let acc = AccountId(a as u8);
            for v in 0u8..64 {
                if self.venues(acc) & (1u64 << v) == 0 {
                    continue;
                }
                let (w10, w60) = self.buckets.windows(VenueId(v), headroom);
                if let Some(b) = self.buckets.venue.get_mut(Buckets::vix(acc, VenueId(v))) {
                    b.tokens_10s = w10.full();
                    b.tokens_min = w60.full();
                }
                let per_s = self.lim().venue(acc, VenueId(v)).inst_bucket_per_s;
                let w = Buckets::inst_window(per_s);
                for i in 1..self.insts.len() {
                    if self.insts.get(i).is_some_and(|f| f.venue == VenueId(v)) {
                        let ix = self.buckets.iix(acc, InstId(i as u32));
                        if let Some(b) = self.buckets.inst.get_mut(ix) {
                            b.tokens = w.full();
                        }
                    }
                }
            }
        }
    }

    fn set_params(&mut self, p: &RiskParams, scope: AccountScope) -> Result<(), RiskParamError> {
        self.rowcaps.check(p)?;
        if !params::merge_into(&self.params, p, scope, &mut self.scratch) {
            return Err(RiskParamError::Capacity);
        }
        self.scratch
            .groups
            .as_mut_slice()
            .sort_unstable_by_key(|r| (r.account, r.group));
        params::validate_rows_n(&self.scratch, self.n_accounts, &self.acct_venues)?;
        self.scratch_ix.rebuild(&self.scratch);
        params::check_exec_facts(
            &Lim {
                p: &self.scratch,
                ix: &self.scratch_ix,
            },
            &self.insts,
            &self.acct_venues,
            &self.exec,
        )?;
        core::mem::swap(&mut self.params, &mut self.scratch);
        core::mem::swap(&mut self.ix, &mut self.scratch_ix);
        let max = self.params.max_perp_leverage_x100;
        for a in 1..=self.n_accounts {
            let acc = AccountId(a as u8);
            for v in 0u8..64 {
                if self.venues(acc) & (1u64 << v) != 0 {
                    self.lev.recheck(acc, VenueId(v), max);
                }
            }
        }
        Ok(())
    }
}

/// The cap scope helpers re-exported for callers building inputs.
pub use mode::{account_group_scope, account_scope, account_venue_scope, cell_scope, venue_inst_scope};

/// `CmdScope` with every field "any".
pub fn scope_any() -> CmdScope {
    mode::scope_of(&[])
}

/// A `source = Risk` Note level helper kept public for tests of the Note table.
pub const fn note_level_of_margin(level: u8) -> NoteLevel {
    if level >= 3 { NoteLevel::P0 } else { NoteLevel::P1 }
}
