//! `RiskParams` (versioned, replaced by `SetRisk`): structure, defaults builder, validation,
//! the start-up row capacities, account-scoped replacement, `loosens` and the
//! `ExecRiskMismatch` consistency check (README "配置", "引擎单与执行配置的一致性").

use hft_refdata::{LimitBucket, LimitScope, MarginData, RefData};
use hft_types::{AccountId, GroupId, InstId, MarketKind, Notional, Qty, Rate, VenueId};

use crate::types::{AccountScope, Capacities, ExecBounds, RiskParamError};
use crate::vec::{FixedVec, PreVec};

/// Price band of one risk class.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct PxBand {
    pub max_px_dev: Rate,
    pub ioc_max_cross: Rate,
    pub rr_max_cross: Rate,
}

/// inst → price-band class.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct InstClassRow {
    pub inst: InstId,
    pub class: u8,
}

/// Group-level limits.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GroupLimits {
    pub max_group_notional: Notional,
    pub max_pos_qty: Qty,
    pub naked_gate: Rate,
    pub naked_gate_min: Notional,
}

/// Limits of one (account, group).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GroupLimitsRow {
    pub account: AccountId,
    pub group: GroupId,
    pub l: GroupLimits,
}

/// Margin-tier thresholds (README "保证金分档").
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarginLimits {
    pub liq_dist_block_open: Rate,
    pub liq_dist_reduce: Rate,
    pub liq_dist_force: Rate,
    pub liq_dist_hyst: Rate,
    pub margin_ratio_crosscheck: Rate,
    pub margin_ratio_hyst: Rate,
    pub deleverage_deadline_ms: u32,
    pub force_step: Rate,
    pub force_step_interval_ms: u32,
    pub margin_stale_ms: u32,
    pub mark_stale_ms: u32,
}

/// Limits of one account.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct AcctLimits {
    pub account: AccountId,
    pub max_order_notional: Notional,
    pub default_group: GroupLimits,
    pub max_account_notional: Notional,
    pub account_notional_from_equity: Rate,
    pub max_hedge_notional: Notional,
    pub max_reduce_notional: Notional,
    pub daily_loss_kill: Rate,
    pub daily_loss_abs: Notional,
    pub daily_loss_max_resumes: u8,
    pub recon_closeonly_notional: Notional,
    pub unknown_blocks_open: bool,
    pub funds_buffer: Rate,
    pub margin: MarginLimits,
}

/// Limits of one (venue, account); window limits come from refdata `RateLimitTable`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueLimits {
    pub venue: VenueId,
    pub account: AccountId,
    pub max_venue_notional: Notional,
    pub inst_bucket_per_s: u32,
    pub backoff_base_ms: u32,
    pub backoff_max_ms: u32,
    pub rr_reserve: Rate,
}

/// Versioned risk parameters.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct RiskParams {
    pub version: u64,
    pub generated_at_ns: i64,
    pub author: [u8; 16],
    /// Share of each window limit kept unused (the only place of the headroom).
    pub rate_headroom: Rate,
    /// Quote-health connection silence limit (the only configuration place).
    pub conn_liveness_ms: u32,
    /// Effective-leverage cap of traded perp insts (hundredths).
    pub max_perp_leverage_x100: u32,
    /// Optional wider L3 cross cap; 0 = disabled.
    pub l3_max_cross: Rate,
    pub risk_class: PreVec<InstClassRow>,
    pub accounts: PreVec<AcctLimits>,
    pub venues: PreVec<VenueLimits>,
    pub classes: FixedVec<PxBand, 8>,
    pub groups: PreVec<GroupLimitsRow>,
}

// ------------------------------------------------------------------------------------ defaults

impl PxBand {
    pub const ZERO: PxBand = PxBand {
        max_px_dev: Rate(0),
        ioc_max_cross: Rate(0),
        rr_max_cross: Rate(0),
    };
}
impl Default for PxBand {
    fn default() -> Self {
        Self::ZERO
    }
}
impl GroupLimits {
    pub const ZERO: GroupLimits = GroupLimits {
        max_group_notional: Notional(0),
        max_pos_qty: Qty(0),
        naked_gate: Rate(0),
        naked_gate_min: Notional(0),
    };
}
impl Default for GroupLimits {
    fn default() -> Self {
        Self::ZERO
    }
}
impl Default for MarginLimits {
    fn default() -> Self {
        DEFAULT_MARGIN
    }
}
impl Default for BandSpec {
    fn default() -> Self {
        BandSpec {
            max_px_dev: Rate(0),
            ioc_max_cross: Rate(0),
            rr_max_cross: None,
        }
    }
}
impl Default for AcctSpec {
    fn default() -> Self {
        AcctSpec {
            max_order_notional: Notional(0),
            default_group: GroupLimits::ZERO,
            max_account_notional: Notional(0),
            account_notional_from_equity: Rate(0),
            max_hedge_notional: None,
            max_reduce_notional: None,
            daily_loss_kill: Rate(0),
            daily_loss_abs: Notional(0),
            daily_loss_max_resumes: None,
            recon_closeonly_notional: Notional(0),
            unknown_blocks_open: None,
            funds_buffer: None,
            margin: None,
        }
    }
}
impl Default for VenueSpec {
    fn default() -> Self {
        VenueSpec {
            max_venue_notional: Notional(0),
            inst_bucket_per_s: 0,
            backoff_base_ms: 0,
            backoff_max_ms: 0,
            rr_reserve: None,
        }
    }
}

/// Default `conn_liveness_ms`.
pub const DEFAULT_CONN_LIVENESS_MS: u32 = 2_000;
/// Default `max_perp_leverage_x100` (3×).
pub const DEFAULT_MAX_PERP_LEVERAGE_X100: u32 = 300;
/// Default `rr_reserve` (0.20).
pub const DEFAULT_RR_RESERVE: Rate = Rate(200_000_000);
/// Default `funds_buffer` (0.05).
pub const DEFAULT_FUNDS_BUFFER: Rate = Rate(50_000_000);
/// Default `daily_loss_max_resumes`.
pub const DEFAULT_DAILY_LOSS_MAX_RESUMES: u8 = 1;
/// `margin_stale_ms` of a `Push` venue.
pub const PUSH_MARGIN_STALE_MS: u32 = 15_000;

/// Default margin thresholds (to be calibrated).
pub const DEFAULT_MARGIN: MarginLimits = MarginLimits {
    liq_dist_block_open: Rate(250_000_000),
    liq_dist_reduce: Rate(150_000_000),
    liq_dist_force: Rate(100_000_000),
    liq_dist_hyst: Rate(20_000_000),
    margin_ratio_crosscheck: Rate(800_000_000),
    margin_ratio_hyst: Rate(50_000_000),
    deleverage_deadline_ms: 30_000,
    force_step: Rate(200_000_000),
    force_step_interval_ms: 5_000,
    margin_stale_ms: PUSH_MARGIN_STALE_MS,
    mark_stale_ms: 10_000,
};

/// Per-class input of the builder; unset fields take their default.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct BandSpec {
    pub max_px_dev: Rate,
    pub ioc_max_cross: Rate,
    /// Default `2 × ioc_max_cross`.
    pub rr_max_cross: Option<Rate>,
}

/// Per-account input of the builder; `None` fields take their default.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AcctSpec {
    pub max_order_notional: Notional,
    pub default_group: GroupLimits,
    pub max_account_notional: Notional,
    pub account_notional_from_equity: Rate,
    /// Default `2 × max_order_notional`.
    pub max_hedge_notional: Option<Notional>,
    /// Default `max_order_notional`.
    pub max_reduce_notional: Option<Notional>,
    pub daily_loss_kill: Rate,
    pub daily_loss_abs: Notional,
    pub daily_loss_max_resumes: Option<u8>,
    pub recon_closeonly_notional: Notional,
    pub unknown_blocks_open: Option<bool>,
    pub funds_buffer: Option<Rate>,
    /// Default `DEFAULT_MARGIN`, with `margin_stale_ms` from the account's perp venues.
    pub margin: Option<MarginLimits>,
}

/// Per-(venue, account) input of the builder.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct VenueSpec {
    pub max_venue_notional: Notional,
    pub inst_bucket_per_s: u32,
    pub backoff_base_ms: u32,
    pub backoff_max_ms: u32,
    pub rr_reserve: Option<Rate>,
}

/// Builder applying the documented defaults (hft-cfg reuses it).
#[derive(Clone, Debug)]
pub struct ParamsBuilder<'a> {
    rd: &'a RefData,
    pub version: u64,
    pub rate_headroom: Rate,
    pub conn_liveness_ms: Option<u32>,
    pub max_perp_leverage_x100: Option<u32>,
    pub l3_max_cross: Option<Rate>,
    pub classes: Vec<BandSpec>,
    pub risk_class: Vec<InstClassRow>,
    pub accounts: Vec<(AccountId, AcctSpec)>,
    /// `(venue, account)` rows; an account venue without a row gets `VenueSpec::default()`.
    pub venues: Vec<(VenueId, AccountId, VenueSpec)>,
    pub groups: Vec<GroupLimitsRow>,
}

/// Default `margin_stale_ms` of an account: `3 × poll interval` per `Poll` perp venue, 15 000
/// for `Push`; the maximum over the account's perp venues (15 000 without any).
pub fn default_margin_stale_ms(rd: &RefData, a: AccountId) -> u32 {
    let mut best: Option<u32> = None;
    for v in venues_of(rd, a) {
        if VenueId(v).market() != Some(MarketKind::LinearPerp) {
            continue;
        }
        let ms = match rd.caps(VenueId(v)).margin_data {
            MarginData::Poll { interval_ms } => interval_ms.saturating_mul(3),
            MarginData::Push | MarginData::NotApplicable => PUSH_MARGIN_STALE_MS,
        };
        best = Some(best.map_or(ms, |b| b.max(ms)));
    }
    best.unwrap_or(PUSH_MARGIN_STALE_MS)
}

impl<'a> ParamsBuilder<'a> {
    pub fn new(rd: &'a RefData) -> Self {
        ParamsBuilder {
            rd,
            version: 1,
            rate_headroom: Rate(0),
            conn_liveness_ms: None,
            max_perp_leverage_x100: None,
            l3_max_cross: None,
            classes: Vec::new(),
            risk_class: Vec::new(),
            accounts: Vec::new(),
            venues: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn build(&self) -> RiskParams {
        let mut classes = FixedVec::new();
        for b in self.classes.iter().take(8) {
            classes.push(PxBand {
                max_px_dev: b.max_px_dev,
                ioc_max_cross: b.ioc_max_cross,
                rr_max_cross: b
                    .rr_max_cross
                    .unwrap_or(Rate(b.ioc_max_cross.0.saturating_mul(2))),
            });
        }
        let accounts: Vec<AcctLimits> = self
            .accounts
            .iter()
            .map(|(a, s)| {
                let mut margin = s.margin.unwrap_or(DEFAULT_MARGIN);
                if s.margin.is_none() {
                    margin.margin_stale_ms = default_margin_stale_ms(self.rd, *a);
                }
                AcctLimits {
                    account: *a,
                    max_order_notional: s.max_order_notional,
                    default_group: s.default_group,
                    max_account_notional: s.max_account_notional,
                    account_notional_from_equity: s.account_notional_from_equity,
                    max_hedge_notional: s
                        .max_hedge_notional
                        .unwrap_or(Notional(s.max_order_notional.0.saturating_mul(2))),
                    max_reduce_notional: s.max_reduce_notional.unwrap_or(s.max_order_notional),
                    daily_loss_kill: s.daily_loss_kill,
                    daily_loss_abs: s.daily_loss_abs,
                    daily_loss_max_resumes: s
                        .daily_loss_max_resumes
                        .unwrap_or(DEFAULT_DAILY_LOSS_MAX_RESUMES),
                    recon_closeonly_notional: s.recon_closeonly_notional,
                    unknown_blocks_open: s.unknown_blocks_open.unwrap_or(true),
                    funds_buffer: s.funds_buffer.unwrap_or(DEFAULT_FUNDS_BUFFER),
                    margin,
                }
            })
            .collect();
        let mut venues = Vec::new();
        for (a, _) in &self.accounts {
            for v in venues_of(self.rd, *a) {
                let spec = self
                    .venues
                    .iter()
                    .find(|(x, y, _)| x.0 == v && y == a)
                    .map(|(_, _, s)| *s)
                    .unwrap_or_default();
                venues.push(VenueLimits {
                    venue: VenueId(v),
                    account: *a,
                    max_venue_notional: spec.max_venue_notional,
                    inst_bucket_per_s: spec.inst_bucket_per_s,
                    backoff_base_ms: spec.backoff_base_ms,
                    backoff_max_ms: spec.backoff_max_ms,
                    rr_reserve: spec.rr_reserve.unwrap_or(DEFAULT_RR_RESERVE),
                });
            }
        }
        RiskParams {
            version: self.version,
            generated_at_ns: 0,
            author: [0; 16],
            rate_headroom: self.rate_headroom,
            conn_liveness_ms: self.conn_liveness_ms.unwrap_or(DEFAULT_CONN_LIVENESS_MS),
            max_perp_leverage_x100: self
                .max_perp_leverage_x100
                .unwrap_or(DEFAULT_MAX_PERP_LEVERAGE_X100),
            l3_max_cross: self.l3_max_cross.unwrap_or(Rate(0)),
            risk_class: PreVec::from_vec(self.risk_class.clone()),
            accounts: PreVec::from_vec(accounts),
            venues: PreVec::from_vec(venues),
            classes,
            groups: PreVec::from_vec(self.groups.clone()),
        }
    }
}

// ------------------------------------------------------------------------------------ refdata helpers

/// Venue ids of an account (ascending).
pub(crate) fn venues_of(rd: &RefData, a: AccountId) -> impl Iterator<Item = u8> {
    let bits = rd.account(a).venues;
    (0u8..64).filter(move |v| bits & (1u64 << v) != 0)
}

/// Number of accounts (dense ids `1..=n`).
pub(crate) fn account_count(rd: &RefData) -> usize {
    let mut n = 0usize;
    for a in 1..=u8::MAX {
        if rd.account(AccountId(a)).id != AccountId(a) {
            break;
        }
        n += 1;
    }
    n
}

/// (account, venue) pairs of refdata.
pub(crate) fn account_venue_count(rd: &RefData) -> usize {
    (1..=account_count(rd))
        .map(|a| venues_of(rd, AccountId(a as u8)).count())
        .sum()
}

/// `(limit_10s, limit_min)` order limits per (venue, account) from `RateLimitTable`
/// (`scope ∈ {Account, Uid}`, `bucket = Orders`, windows 10 s / 60 s); absent → `None`.
pub(crate) fn window_limits(rd: &RefData, v: VenueId) -> (Option<u32>, Option<u32>) {
    let mut s10 = None;
    let mut s60 = None;
    for r in rd.rate_limits(v) {
        if !matches!(r.scope, LimitScope::Account | LimitScope::Uid) || r.bucket != LimitBucket::Orders {
            continue;
        }
        match r.window_ms {
            10_000 => s10 = Some(r.limit),
            60_000 => s60 = Some(r.limit),
            _ => {}
        }
    }
    (s10, s60)
}

// ------------------------------------------------------------------------------------ index

const NONE16: u16 = u16::MAX;

/// Dense lookup tables over the current `RiskParams` (rebuilt by `set_params`, no allocation).
#[derive(Clone, Debug)]
pub(crate) struct ParamIx {
    /// inst → class index.
    pub inst_class: Vec<u8>,
    /// account → row of `accounts`.
    pub acct_row: Vec<u16>,
    /// account × 64 + venue → row of `venues`.
    pub venue_row: Vec<u16>,
}

impl ParamIx {
    pub fn new(n_accounts: usize, n_insts: usize) -> ParamIx {
        ParamIx {
            inst_class: vec![0; n_insts + 1],
            acct_row: vec![NONE16; n_accounts + 1],
            venue_row: vec![NONE16; (n_accounts + 1) * 64],
        }
    }

    /// Rebuilds from `p` (rows already validated).
    pub fn rebuild(&mut self, p: &RiskParams) {
        self.inst_class.iter_mut().for_each(|c| *c = 0);
        for r in &p.risk_class {
            if let Some(c) = self.inst_class.get_mut(r.inst.0 as usize) {
                *c = r.class;
            }
        }
        self.acct_row.iter_mut().for_each(|c| *c = NONE16);
        for (i, r) in p.accounts.iter().enumerate() {
            if let Some(c) = self.acct_row.get_mut(usize::from(r.account.0)) {
                *c = i as u16;
            }
        }
        self.venue_row.iter_mut().for_each(|c| *c = NONE16);
        for (i, r) in p.venues.iter().enumerate() {
            if let Some(c) = self
                .venue_row
                .get_mut(usize::from(r.account.0) * 64 + usize::from(r.venue.0 & 63))
            {
                *c = i as u16;
            }
        }
    }
}

/// Read access to the current params through the index.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Lim<'a> {
    pub p: &'a RiskParams,
    pub ix: &'a ParamIx,
}

const ZERO_ACCT: AcctLimits = AcctLimits {
    account: AccountId(0),
    max_order_notional: Notional(0),
    default_group: GroupLimits {
        max_group_notional: Notional(0),
        max_pos_qty: Qty(0),
        naked_gate: Rate(0),
        naked_gate_min: Notional(0),
    },
    max_account_notional: Notional(0),
    account_notional_from_equity: Rate(0),
    max_hedge_notional: Notional(0),
    max_reduce_notional: Notional(0),
    daily_loss_kill: Rate(0),
    daily_loss_abs: Notional(0),
    daily_loss_max_resumes: 0,
    recon_closeonly_notional: Notional(0),
    unknown_blocks_open: true,
    funds_buffer: Rate(0),
    margin: DEFAULT_MARGIN,
};

const ZERO_VENUE: VenueLimits = VenueLimits {
    venue: VenueId(0),
    account: AccountId(0),
    max_venue_notional: Notional(0),
    inst_bucket_per_s: 0,
    backoff_base_ms: 0,
    backoff_max_ms: 0,
    rr_reserve: Rate(0),
};

impl<'a> Lim<'a> {
    #[inline]
    pub fn acct(&self, a: AccountId) -> &'a AcctLimits {
        let r = self.ix.acct_row.get(usize::from(a.0)).copied().unwrap_or(NONE16);
        self.p.accounts.get(usize::from(r)).unwrap_or(&ZERO_ACCT)
    }
    #[inline]
    pub fn venue(&self, a: AccountId, v: VenueId) -> &'a VenueLimits {
        let r = self
            .ix
            .venue_row
            .get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
            .copied()
            .unwrap_or(NONE16);
        self.p.venues.get(usize::from(r)).unwrap_or(&ZERO_VENUE)
    }
    /// Limits of (account, group); missing row or `GROUP_NONE` → the account's `default_group`.
    #[inline]
    pub fn group(&self, a: AccountId, g: GroupId) -> &'a GroupLimits {
        let rows = self.p.groups.as_slice();
        match rows.binary_search_by(|r| (r.account, r.group).cmp(&(a, g))) {
            Ok(i) => rows.get(i).map_or(&self.acct(a).default_group, |r| &r.l),
            Err(_) => &self.acct(a).default_group,
        }
    }
    #[inline]
    pub fn band(&self, i: InstId) -> PxBand {
        let c = self.ix.inst_class.get(i.0 as usize).copied().unwrap_or(0);
        self.p.classes.get(usize::from(c)).copied().unwrap_or_default()
    }
}

// ------------------------------------------------------------------------------------ validation

/// Row validation (accounts and venues exist, class indexes, group order, headroom, margin and
/// daily-loss rules of every account row).
pub(crate) fn validate_rows_n(
    p: &RiskParams,
    n_acc: usize,
    acct_venues: &[u64],
) -> Result<(), RiskParamError> {
    for r in &p.accounts {
        if r.account.0 == 0 || usize::from(r.account.0) > n_acc {
            return Err(RiskParamError::Invalid("accounts.account"));
        }
        validate_acct(r)?;
    }
    for r in &p.venues {
        let vb = acct_venues.get(usize::from(r.account.0)).copied().unwrap_or(0);
        if r.account.0 == 0 || usize::from(r.account.0) > n_acc || vb & (1u64 << (r.venue.0 & 63)) == 0 {
            return Err(RiskParamError::Invalid("venues.venue"));
        }
    }
    for r in &p.risk_class {
        if usize::from(r.class) >= p.classes.len() {
            return Err(RiskParamError::Invalid("risk_class.class"));
        }
    }
    for w in p.groups.as_slice().windows(2) {
        if let [x, y] = w
            && (x.account, x.group) >= (y.account, y.group)
        {
            return Err(RiskParamError::Invalid("groups.order"));
        }
    }
    if p.rate_headroom.0 < 0 || p.rate_headroom.0 >= 1_000_000_000 {
        return Err(RiskParamError::Invalid("rate_headroom"));
    }
    Ok(())
}

/// `block_open > reduce > force > 0`, `liq_dist_hyst < reduce − force`, `daily_loss_abs > 0`.
pub(crate) fn validate_acct(r: &AcctLimits) -> Result<(), RiskParamError> {
    let m = &r.margin;
    if m.liq_dist_force.0 <= 0 {
        return Err(RiskParamError::Invalid("margin.liq_dist_force"));
    }
    if m.liq_dist_reduce.0 <= m.liq_dist_force.0 {
        return Err(RiskParamError::Invalid("margin.liq_dist_reduce"));
    }
    if m.liq_dist_block_open.0 <= m.liq_dist_reduce.0 {
        return Err(RiskParamError::Invalid("margin.liq_dist_block_open"));
    }
    if i128::from(m.liq_dist_hyst.0) >= i128::from(m.liq_dist_reduce.0) - i128::from(m.liq_dist_force.0) {
        return Err(RiskParamError::Invalid("margin.liq_dist_hyst"));
    }
    if r.daily_loss_abs.0 <= 0 {
        return Err(RiskParamError::Invalid("daily_loss_abs"));
    }
    Ok(())
}

/// Row-count capacities fixed at start-up.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct RowCaps {
    pub accounts: usize,
    pub venues: usize,
    pub groups: usize,
    pub risk_class: usize,
}

impl RowCaps {
    pub fn of(rd: &RefData, caps: &Capacities) -> RowCaps {
        RowCaps {
            accounts: account_count(rd),
            venues: account_venue_count(rd),
            groups: caps.max_cells_total as usize,
            risk_class: caps.max_insts as usize,
        }
    }
    pub fn check(&self, p: &RiskParams) -> Result<(), RiskParamError> {
        if p.accounts.len() > self.accounts
            || p.venues.len() > self.venues
            || p.groups.len() > self.groups
            || p.risk_class.len() > self.risk_class
        {
            return Err(RiskParamError::Capacity);
        }
        Ok(())
    }
}

/// `ExecRiskMismatch` check of every account with an `ExecBounds` row (indexed by `AccountId`):
/// `force_max_cross ≤` the smallest `rr_max_cross` of the account's tradable insts and
/// `force_max_chunk ≤ max_order_notional`.
pub(crate) fn check_exec_facts(
    lim: &Lim<'_>,
    insts: &[crate::InstFacts],
    acct_venues: &[u64],
    exec: &[ExecBounds],
) -> Result<(), RiskParamError> {
    for (a, b) in exec.iter().enumerate().skip(1) {
        let Some(vb) = acct_venues.get(a).copied() else {
            break;
        };
        let acct = AccountId(a as u8);
        let mut min_rr: Option<Rate> = None;
        for (i, f) in insts.iter().enumerate().skip(1) {
            if f.venue.0 == 0 || vb & (1u64 << (f.venue.0 & 63)) == 0 {
                continue;
            }
            let rr = lim.band(InstId(i as u32)).rr_max_cross;
            min_rr = Some(min_rr.map_or(rr, |m| m.min(rr)));
        }
        if let Some(m) = min_rr
            && b.force_max_cross > m
        {
            return Err(RiskParamError::ExecRiskMismatch);
        }
        if b.force_max_chunk > lim.acct(acct).max_order_notional {
            return Err(RiskParamError::ExecRiskMismatch);
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------ scope merge

/// `cur` with `p` applied under `scope` written into `out` (no allocation: `out` has the
/// start-up capacities). `Account(a)` replaces only that account's rows; global fields keep `cur`.
pub(crate) fn merge_into(
    cur: &RiskParams,
    p: &RiskParams,
    scope: AccountScope,
    out: &mut RiskParams,
) -> bool {
    match scope {
        AccountScope::All => copy_params(p, out),
        AccountScope::Account(a) => {
            out.version = p.version;
            out.generated_at_ns = p.generated_at_ns;
            out.author = p.author;
            out.rate_headroom = cur.rate_headroom;
            out.conn_liveness_ms = cur.conn_liveness_ms;
            out.max_perp_leverage_x100 = cur.max_perp_leverage_x100;
            out.l3_max_cross = cur.l3_max_cross;
            out.classes = cur.classes;
            if !out.risk_class.assign(cur.risk_class.as_slice()) {
                return false;
            }
            out.accounts.clear();
            for r in cur.accounts.iter().filter(|r| r.account != a) {
                if !out.accounts.push(*r) {
                    return false;
                }
            }
            for r in p.accounts.iter().filter(|r| r.account == a) {
                if !out.accounts.push(*r) {
                    return false;
                }
            }
            out.accounts.as_mut_slice().sort_unstable_by_key(|r| r.account);
            out.venues.clear();
            for r in cur.venues.iter().filter(|r| r.account != a) {
                if !out.venues.push(*r) {
                    return false;
                }
            }
            for r in p.venues.iter().filter(|r| r.account == a) {
                if !out.venues.push(*r) {
                    return false;
                }
            }
            out.venues
                .as_mut_slice()
                .sort_unstable_by_key(|r| (r.account, r.venue));
            out.groups.clear();
            for r in cur.groups.iter().filter(|r| r.account != a) {
                if !out.groups.push(*r) {
                    return false;
                }
            }
            for r in p.groups.iter().filter(|r| r.account == a) {
                if !out.groups.push(*r) {
                    return false;
                }
            }
            out.groups
                .as_mut_slice()
                .sort_unstable_by_key(|r| (r.account, r.group));
            true
        }
    }
}

/// Copies `p` into `out`'s storage; `false` when a vector exceeds `out`'s capacity.
pub(crate) fn copy_params(p: &RiskParams, out: &mut RiskParams) -> bool {
    out.version = p.version;
    out.generated_at_ns = p.generated_at_ns;
    out.author = p.author;
    out.rate_headroom = p.rate_headroom;
    out.conn_liveness_ms = p.conn_liveness_ms;
    out.max_perp_leverage_x100 = p.max_perp_leverage_x100;
    out.l3_max_cross = p.l3_max_cross;
    out.classes = p.classes;
    out.risk_class.assign(p.risk_class.as_slice())
        && out.accounts.assign(p.accounts.as_slice())
        && out.venues.assign(p.venues.as_slice())
        && out.groups.assign(p.groups.as_slice())
}

/// Empty params with the start-up capacities.
pub(crate) fn with_caps(c: &RowCaps) -> RiskParams {
    RiskParams {
        version: 0,
        generated_at_ns: 0,
        author: [0; 16],
        rate_headroom: Rate(0),
        conn_liveness_ms: 0,
        max_perp_leverage_x100: 0,
        l3_max_cross: Rate(0),
        risk_class: PreVec::with_capacity(c.risk_class),
        accounts: PreVec::with_capacity(c.accounts),
        venues: PreVec::with_capacity(c.venues),
        classes: FixedVec::new(),
        groups: PreVec::with_capacity(c.groups),
    }
}

// ------------------------------------------------------------------------------------ loosens

fn up<T: PartialOrd>(old: T, new: T) -> bool {
    new > old
}
fn down<T: PartialOrd>(old: T, new: T) -> bool {
    new < old
}

fn group_loosens(o: &GroupLimits, n: &GroupLimits) -> bool {
    up(o.max_group_notional, n.max_group_notional)
        || up(o.max_pos_qty, n.max_pos_qty)
        || up(o.naked_gate, n.naked_gate)
        || up(o.naked_gate_min, n.naked_gate_min)
}

fn acct_loosens(o: &AcctLimits, n: &AcctLimits) -> bool {
    let (om, nm) = (&o.margin, &n.margin);
    up(o.max_order_notional, n.max_order_notional)
        || group_loosens(&o.default_group, &n.default_group)
        || up(o.max_account_notional, n.max_account_notional)
        || up(o.account_notional_from_equity, n.account_notional_from_equity)
        || up(o.max_hedge_notional, n.max_hedge_notional)
        || up(o.max_reduce_notional, n.max_reduce_notional)
        || up(o.daily_loss_kill, n.daily_loss_kill)
        || up(o.daily_loss_abs, n.daily_loss_abs)
        || up(o.daily_loss_max_resumes, n.daily_loss_max_resumes)
        || up(o.recon_closeonly_notional, n.recon_closeonly_notional)
        || (o.unknown_blocks_open && !n.unknown_blocks_open)
        || down(o.funds_buffer, n.funds_buffer)
        || down(om.liq_dist_block_open, nm.liq_dist_block_open)
        || down(om.liq_dist_reduce, nm.liq_dist_reduce)
        || down(om.liq_dist_force, nm.liq_dist_force)
        || down(om.liq_dist_hyst, nm.liq_dist_hyst)
        || up(om.margin_ratio_crosscheck, nm.margin_ratio_crosscheck)
        || down(om.margin_ratio_hyst, nm.margin_ratio_hyst)
        || up(om.deleverage_deadline_ms, nm.deleverage_deadline_ms)
        || down(om.force_step, nm.force_step)
        || up(om.force_step_interval_ms, nm.force_step_interval_ms)
        || up(om.margin_stale_ms, nm.margin_stale_ms)
        || up(om.mark_stale_ms, nm.mark_stale_ms)
}

fn venue_loosens(o: &VenueLimits, n: &VenueLimits) -> bool {
    up(o.max_venue_notional, n.max_venue_notional)
        || up(o.inst_bucket_per_s, n.inst_bucket_per_s)
        || down(o.backoff_base_ms, n.backoff_base_ms)
        || down(o.backoff_max_ms, n.backoff_max_ms)
        || down(o.rr_reserve, n.rr_reserve)
}

fn band_loosens(o: &PxBand, n: &PxBand) -> bool {
    up(o.max_px_dev, n.max_px_dev)
        || up(o.ioc_max_cross, n.ioc_max_cross)
        || up(o.rr_max_cross, n.rr_max_cross)
}

fn class_of(p: &RiskParams, i: InstId) -> PxBand {
    let c = p.risk_class.iter().find(|r| r.inst == i).map_or(0, |r| r.class);
    p.classes.get(usize::from(c)).copied().unwrap_or_default()
}

/// Whether `n` loosens `o` under `scope` (README "配置" field directions).
pub(crate) fn loosens(o: &RiskParams, n: &RiskParams, scope: AccountScope) -> bool {
    let in_scope = |a: AccountId| match scope {
        AccountScope::All => true,
        AccountScope::Account(x) => x == a,
    };
    for nr in n.accounts.iter().filter(|r| in_scope(r.account)) {
        match o.accounts.iter().find(|r| r.account == nr.account) {
            Some(or) if acct_loosens(or, nr) => return true,
            None => return true,
            _ => {}
        }
    }
    for nr in n.venues.iter().filter(|r| in_scope(r.account)) {
        match o
            .venues
            .iter()
            .find(|r| r.account == nr.account && r.venue == nr.venue)
        {
            Some(or) if venue_loosens(or, nr) => return true,
            None => return true,
            _ => {}
        }
    }
    for nr in n.groups.iter().filter(|r| in_scope(r.account)) {
        let ol = o
            .groups
            .iter()
            .find(|r| r.account == nr.account && r.group == nr.group)
            .map(|r| r.l)
            .or_else(|| {
                o.accounts
                    .iter()
                    .find(|r| r.account == nr.account)
                    .map(|r| r.default_group)
            });
        match ol {
            Some(ol) if group_loosens(&ol, &nr.l) => return true,
            None => return true,
            _ => {}
        }
    }
    if scope == AccountScope::All {
        if down(o.rate_headroom, n.rate_headroom)
            || up(o.conn_liveness_ms, n.conn_liveness_ms)
            || up(o.max_perp_leverage_x100, n.max_perp_leverage_x100)
            || up(o.l3_max_cross, n.l3_max_cross)
        {
            return true;
        }
        for (k, nb) in n.classes.iter().enumerate() {
            match o.classes.get(k) {
                Some(ob) if band_loosens(ob, nb) => return true,
                None => return true,
                _ => {}
            }
        }
        for r in n.risk_class.iter().chain(o.risk_class.iter()) {
            if band_loosens(&class_of(o, r.inst), &class_of(n, r.inst)) {
                return true;
            }
        }
    }
    false
}
