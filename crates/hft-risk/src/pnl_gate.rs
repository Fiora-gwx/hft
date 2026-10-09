//! Daily loss (latched, equity-based quota), the exposure-gate leg summaries, reconciliation
//! grading with the recovery count and the `AUTO_EXTERNAL` user-stream precondition
//! (README "当日亏损", "对账分级").

use hft_event::{CapRecord, ReconcileReport};
use hft_types::{AccountId, CapSource, GroupId, InstId, Notional, Qty, VenueId};

use crate::mode::{ModeInput, ReconGateInput, ReconItemKey, account_scope, account_venue_scope, recon_flags};
use crate::types::LegSummary;
use crate::vec::{FixedVec, PreVec};

/// `RiskAcctRow.eq_flags`.
pub mod eq_flags {
    pub const SOD_VALID: u8 = 1;
}

/// Daily-loss state of one account (= `RiskAcctRow`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct AcctPnl {
    pub day_pnl_base: Notional,
    pub loss_trigger: Notional,
    pub loss_quota: Notional,
    pub equity_sod: Notional,
    pub resumes_today: u8,
    pub eq_flags: u8,
    /// Latched (derived from the `DailyLoss` cap on restore; not in the row).
    pub latched: bool,
}

impl Default for AcctPnl {
    fn default() -> Self {
        AcctPnl {
            day_pnl_base: Notional(0),
            loss_trigger: Notional(0),
            loss_quota: Notional(0),
            equity_sod: Notional(0),
            resumes_today: 0,
            eq_flags: 0,
            latched: false,
        }
    }
}

/// `DailyLoss` cap input of an account at trigger line `t`.
pub(crate) fn daily_loss_input(a: AccountId, t: Notional) -> ModeInput<'static> {
    ModeInput::Auto(CapRecord {
        source: CapSource::DAILY_LOSS,
        level: 0,
        cflags: 0,
        _pad: [0; 5],
        scope: account_scope(a),
        set_seq: 0,
        params: [t.0 as u64, 0],
    })
}

/// A group of `ReconcileReport` entries of one (account, venue) and one `report_id`.
#[derive(Copy, Clone, Debug)]
pub struct ReconcileGroup<'a> {
    pub items: &'a [ReconcileReport],
}

/// Result of `on_reconcile`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ReconVerdicts {
    /// `Auto(Recon{a, v})` when a difference repeated over two rounds above the notional line.
    pub recon: Option<ModeInput<'static>>,
    /// `None` = no `Summary` in the group (not a reconciliation round).
    pub gate: Option<ReconGateInput>,
    pub n_warn: u16,
}

/// Item codes of `ReconcileReport.item`.
pub(crate) mod item {
    pub const POSITION: u8 = 1;
    pub const BALANCE: u8 = 2;
    pub const OPEN_ORDER: u8 = 3;
    pub const SUMMARY: u8 = 5;
}

/// `ReconRow.inst` flag of a balance (asset) key.
pub const ASSET_KEY: u32 = 1 << 31;

/// Per-key grading row (`ReconRow` with `inst ≠ 0`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct ReconKeyRow {
    pub account: AccountId,
    pub venue: VenueId,
    pub key: u32,
    pub warn_streak: u8,
    pub last_diff: Qty,
    pub seen: bool,
}

/// Grading state.
#[derive(Clone, Debug)]
pub(crate) struct ReconState {
    /// account × 64 + venue → consecutive Ignore rounds.
    pub ignore_streak: Vec<u8>,
    pub keys: PreVec<ReconKeyRow>,
}

impl ReconState {
    pub fn new(n_accounts: usize, n_av: usize) -> ReconState {
        ReconState {
            ignore_streak: vec![0; (n_accounts + 1) * 64],
            keys: PreVec::with_capacity(n_av.max(1) * 64),
        }
    }
    pub fn streak(&self, a: AccountId, v: VenueId) -> u8 {
        self.ignore_streak
            .get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
            .copied()
            .unwrap_or(0)
    }
    pub fn set_streak(&mut self, a: AccountId, v: VenueId, n: u8) {
        if let Some(x) = self
            .ignore_streak
            .get_mut(usize::from(a.0) * 64 + usize::from(v.0 & 63))
        {
            *x = n;
        }
    }
    pub fn last_diff(&self, a: AccountId, v: VenueId, key: u32) -> Option<Qty> {
        self.keys
            .iter()
            .find(|r| r.account == a && r.venue == v && r.key == key)
            .map(|r| r.last_diff)
    }
}

/// Facts the grading reads per item.
pub(crate) trait GradeFacts {
    fn lot(&self, i: InstId) -> i64;
    fn dust(&self, asset: u16) -> i64;
    fn recon_line(&self, a: AccountId) -> Notional;
}

/// Grades one group (README "对账分级" items 1–8).
pub(crate) fn grade(
    st: &mut ReconState,
    f: &impl GradeFacts,
    grp: &ReconcileGroup<'_>,
) -> (ReconVerdicts, Option<(AccountId, VenueId, u64, bool)>) {
    let none = ReconVerdicts {
        recon: None,
        gate: None,
        n_warn: 0,
    };
    let Some(sum) = grp.items.iter().find(|r| r.item == item::SUMMARY) else {
        return (none, None);
    };
    let (a, v) = (sum.account, sum.venue);
    let us_down = sum.sflags & ReconcileReport::USERSTREAM_DOWN != 0;
    let mut keys: FixedVec<ReconItemKey, 64> = FixedVec::new();
    let mut n_items: u16 = 0;
    let mut n_warn: u16 = 0;
    let mut recon = None;
    for r in st.keys.as_mut_slice().iter_mut() {
        if r.account == a && r.venue == v {
            r.seen = false;
        }
    }
    let line = f.recon_line(a);
    for r in grp.items {
        if r.account != a || r.venue != v {
            continue;
        }
        let (k, thr, key_item) = match r.item {
            item::POSITION => (r.inst.0, f.lot(r.inst), ReconItemKey::POSITION),
            item::BALANCE => (
                u32::from(r.asset.0) | ASSET_KEY,
                f.dust(r.asset.0),
                ReconItemKey::BALANCE,
            ),
            item::OPEN_ORDER => {
                n_items = n_items.saturating_add(1);
                keys.push(ReconItemKey::new(ReconItemKey::OPEN_ORDER, r.inst.0));
                continue;
            }
            _ => continue,
        };
        let diff = i128::from(r.exch_qty.0) - i128::from(r.engine_qty.0);
        if diff.abs() < i128::from(thr) {
            continue;
        }
        n_warn = n_warn.saturating_add(1);
        n_items = n_items.saturating_add(1);
        let id = if key_item == ReconItemKey::BALANCE {
            u32::from(r.asset.0)
        } else {
            r.inst.0
        };
        keys.push(ReconItemKey::new(key_item, id));
        let d = Qty(i64::try_from(diff).unwrap_or(if diff < 0 { i64::MIN } else { i64::MAX }));
        let row = st
            .keys
            .as_mut_slice()
            .iter_mut()
            .find(|x| x.account == a && x.venue == v && x.key == k);
        let streak = match row {
            Some(x) => {
                let same = (x.last_diff.0 < 0) == (d.0 < 0)
                    && (i128::from(d.0) - i128::from(x.last_diff.0)).abs() < i128::from(thr)
                    && !x.seen;
                x.warn_streak = if same { x.warn_streak.saturating_add(1) } else { 1 };
                x.last_diff = d;
                x.seen = true;
                x.warn_streak
            }
            None => {
                let at = st
                    .keys
                    .iter()
                    .position(|x| (x.account, x.venue, x.key) > (a, v, k))
                    .unwrap_or(st.keys.len());
                st.keys.insert(
                    at,
                    ReconKeyRow {
                        account: a,
                        venue: v,
                        key: k,
                        warn_streak: 1,
                        last_diff: d,
                        seen: true,
                    },
                );
                1
            }
        };
        if streak >= 2 && r.diff.0.unsigned_abs() >= line.0.unsigned_abs() {
            recon = Some(ModeInput::Auto(CapRecord {
                source: CapSource::RECON,
                level: 0,
                cflags: 0,
                _pad: [0; 5],
                scope: account_venue_scope(a, v),
                set_seq: 0,
                params: [0, 0],
            }));
        }
    }
    st.keys.retain(|x| x.account != a || x.venue != v || x.seen);
    if sum.skipped_inflight == 0 {
        let s = st.streak(a, v);
        let n = if us_down {
            0
        } else if n_items == 0 {
            s.saturating_add(1)
        } else {
            0
        };
        st.set_streak(a, v, n);
    }
    let mut flags = 0u16;
    if sum.sflags & ReconcileReport::TRUNCATED != 0 {
        flags |= recon_flags::TRUNCATED;
    }
    if sum.skipped_inflight > 0 {
        flags |= recon_flags::INFLIGHT;
    }
    if us_down {
        flags |= recon_flags::USERSTREAM_DOWN;
    }
    if n_items > 64 {
        flags |= recon_flags::OVERFLOW;
    }
    let gate = ReconGateInput {
        account: a,
        venue: v,
        report_id: sum.report_id,
        view_seq: sum.view_seq,
        keys,
        n_items,
        flags,
    };
    (
        ReconVerdicts {
            recon,
            gate: Some(gate),
            n_warn,
        },
        Some((a, v, sum.view_seq, us_down)),
    )
}

/// Sorted per-(account, group) leg summaries (derived cache, not in the snapshot).
#[derive(Clone, Debug)]
pub(crate) struct Legs {
    pub rows: PreVec<(AccountId, GroupId, LegSummary)>,
}

impl Legs {
    pub fn get(&self, a: AccountId, g: GroupId) -> Option<&LegSummary> {
        let rows = self.rows.as_slice();
        rows.binary_search_by(|r| (r.0, r.1).cmp(&(a, g)))
            .ok()
            .and_then(|i| rows.get(i))
            .map(|r| &r.2)
    }
    pub fn put(&mut self, a: AccountId, g: GroupId, s: LegSummary) {
        match self.rows.as_slice().binary_search_by(|r| (r.0, r.1).cmp(&(a, g))) {
            Ok(i) => {
                if let Some(r) = self.rows.get_mut(i) {
                    r.2 = s;
                }
            }
            Err(i) => {
                let _ = self.rows.insert(i, (a, g, s));
            }
        }
    }
}
