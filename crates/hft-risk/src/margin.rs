//! Margin tiers by distance to liquidation (README "保证金分档"): measure, ratio cross-check,
//! stale/incomplete data (report and mark price), hysteresis, the L2 deadline, L3 steps with
//! the restart confirmation, the `MarginStale`/`MarginDeadline`/`MarginForce` timers and the
//! account-level `MarginStress` cap input.

use hft_event::{CapRecord, CmdScope, Note, NoteLevel};
use hft_strategy_sdk::{MarginLevel, Now, RiskEvent};
use hft_types::{AccountId, CapSource, GROUP_ALL, InstId, Rate, StrategyId, Tsc, VenueId};

use crate::mode::ModeInput;
use crate::params::MarginLimits;
use crate::types::{N_MARGIN_LEVEL, note};
use crate::vec::FixedVec;

const NS_PER_MS: i64 = 1_000_000;

/// Margin facts of one (account, venue), projected by the sequencer from ledger `MarginState`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct MarginSample {
    /// By exchange `liq_px`: min over perp positions `|liq_px − mark| / mark` (`Rate::MAX` = none).
    pub rep_liq_dist: Rate,
    /// Ledger estimate (`Rate::MAX` = no perp position).
    pub est_liq_dist: Rate,
    pub reported_ratio: Rate,
    pub est_ratio: Rate,
    /// engine_ns of the latest `MarginReport`.
    pub report_ens: i64,
    /// Oldest mark-price time of the venue's perp positions (`i64::MAX` = no perp position).
    pub mark_ens: i64,
    pub has_report: bool,
    /// The report carried `ITEMS_TRUNCATED`.
    pub truncated: bool,
}

/// One default-reduction step for hft-exec.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ForceDeleverage {
    pub account: AccountId,
    pub venue: VenueId,
    /// `= force_step`.
    pub frac: Rate,
}

/// Timers this crate registers (`Timer{Risk, …}`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum RiskTimer {
    MarginStale { a: AccountId, v: VenueId },
    MarginDeadline { a: AccountId, v: VenueId },
    MarginForce { a: AccountId, v: VenueId },
}

impl RiskTimer {
    /// `timer_id = kind | account << 8 | venue << 16` (kind 1 Stale, 2 Deadline, 3 Force).
    pub const fn timer_id(self) -> u32 {
        let (k, a, v) = match self {
            RiskTimer::MarginStale { a, v } => (1u32, a, v),
            RiskTimer::MarginDeadline { a, v } => (2, a, v),
            RiskTimer::MarginForce { a, v } => (3, a, v),
        };
        k | (a.0 as u32) << 8 | (v.0 as u32) << 16
    }
    /// Inverse of `timer_id`; unknown kind or high bits set → `None`.
    pub const fn from_timer_id(id: u32) -> Option<RiskTimer> {
        let a = AccountId((id >> 8) as u8);
        let v = VenueId((id >> 16) as u8);
        if id >> 24 != 0 {
            return None;
        }
        Some(match id & 0xFF {
            1 => RiskTimer::MarginStale { a, v },
            2 => RiskTimer::MarginDeadline { a, v },
            3 => RiskTimer::MarginForce { a, v },
            _ => return None,
        })
    }
}

/// Outputs of one `on_margin`/`on_timer` call (one (account, venue); capacities are bounds).
#[derive(Clone, Debug, Default)]
pub struct MarginOut {
    pub mode: FixedVec<ModeInput<'static>, 4>,
    pub events: FixedVec<(AccountId, RiskEvent), 4>,
    pub force: Option<ForceDeleverage>,
    /// Deadlines in engine_ns (the sequencer converts with `Now::tsc_of`, rounding up).
    pub timers: FixedVec<(RiskTimer, i64), 4>,
    pub notes: FixedVec<Note, 4>,
}

impl MarginOut {
    pub fn clear(&mut self) {
        self.mode.clear();
        self.events.clear();
        self.force = None;
        self.timers.clear();
        self.notes.clear();
    }
}

/// `MarginRow.mflags` bits.
pub mod mflags {
    pub const TRUNCATED: u8 = 1;
    pub const RATIO_L2: u8 = 2;
    pub const DEADLINE_L3: u8 = 4;
    /// A `MarginReport` has been received.
    pub const HAS_REPORT: u8 = 8;
}

/// Margin state of one (account, venue) (= `MarginRow` fields).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct MState {
    pub present: bool,
    pub level: u8,
    pub mflags: u8,
    pub report_ens: i64,
    pub reported_ratio: Rate,
    pub est_ratio: Rate,
    pub rep_liq_dist: Rate,
    pub est_liq_dist: Rate,
    pub l2_since_ens: i64,
    pub deadline_ens: i64,
    pub next_force_ens: i64,
    pub mark_ens: i64,
    /// Deadline of the `MarginStale` timer in the heap (0 = none).
    pub stale_due_ens: i64,
}

impl Default for MState {
    fn default() -> Self {
        MState {
            present: false,
            level: 0,
            mflags: 0,
            report_ens: 0,
            reported_ratio: Rate(0),
            est_ratio: Rate(0),
            rep_liq_dist: Rate::MAX,
            est_liq_dist: Rate::MAX,
            l2_since_ens: 0,
            deadline_ens: 0,
            next_force_ens: 0,
            mark_ens: i64::MAX,
            stale_due_ens: 0,
        }
    }
}

impl MState {
    /// This venue's share of the account tier: its level when present, else 0.
    pub(crate) fn contribution(&self) -> u8 {
        if self.present { self.level } else { 0 }
    }

    pub fn liq(&self) -> Rate {
        self.rep_liq_dist.min(self.est_liq_dist)
    }
    pub fn ratio(&self) -> Rate {
        self.reported_ratio.max(self.est_ratio)
    }
    fn has_report(&self) -> bool {
        self.mflags & mflags::HAS_REPORT != 0
    }
    fn set_flag(&mut self, f: u8, on: bool) {
        if on {
            self.mflags |= f;
        } else {
            self.mflags &= !f;
        }
    }
    fn store(&mut self, s: &MarginSample) {
        self.present = true;
        self.rep_liq_dist = s.rep_liq_dist;
        self.est_liq_dist = s.est_liq_dist;
        self.reported_ratio = s.reported_ratio;
        self.est_ratio = s.est_ratio;
        self.report_ens = s.report_ens;
        self.mark_ens = s.mark_ens;
        self.set_flag(mflags::HAS_REPORT, s.has_report);
        self.set_flag(mflags::TRUNCATED, s.truncated);
    }
    /// Data stale or incomplete at `now` (README "数据过期或不完整").
    fn stale(&self, now: i64, run_start: i64, m: &MarginLimits) -> bool {
        let rep_old = i128::from(now) - i128::from(self.report_ens) > i128::from(ms(m.margin_stale_ms));
        let mark_old = self.mark_ens != i64::MAX
            && (self.mark_ens < run_start
                || i128::from(now) - i128::from(self.mark_ens) > i128::from(ms(m.mark_stale_ms)));
        !self.has_report()
            || self.report_ens < run_start
            || rep_old
            || self.mflags & mflags::TRUNCATED != 0
            || mark_old
    }
    /// `MarginStale` deadline: `min(report + margin_stale, mark + mark_stale) + 1 ns`.
    pub(crate) fn stale_deadline(&self, m: &MarginLimits) -> i64 {
        let r = self.report_ens.saturating_add(ms(m.margin_stale_ms));
        let k = if self.mark_ens == i64::MAX {
            i64::MAX
        } else {
            self.mark_ens.saturating_add(ms(m.mark_stale_ms))
        };
        r.min(k).saturating_add(1)
    }
    /// L3 confirmed by this run's data (report or mark at/after `run_start`).
    fn confirmed(&self, run_start: i64) -> bool {
        self.report_ens >= run_start || self.mark_ens >= run_start
    }
}

const fn ms(x: u32) -> i64 {
    (x as i64) * NS_PER_MS
}

fn lvl(l: u8) -> MarginLevel {
    match l {
        1 => MarginLevel::L1,
        2 => MarginLevel::L2,
        3 => MarginLevel::L3,
        _ => MarginLevel::None,
    }
}

/// Context of one evaluation.
pub(crate) struct Ctx<'a> {
    pub a: AccountId,
    pub v: VenueId,
    pub now: &'a Now,
    pub run_start: i64,
    pub m: &'a MarginLimits,
}

/// Tier the measures alone demand ("上行" conditions).
fn up_level(st: &MState, stale: bool, m: &MarginLimits) -> u8 {
    let (liq, r) = (st.liq(), st.ratio());
    let mut up = 0;
    if stale || liq <= m.liq_dist_block_open {
        up = 1;
    }
    if liq <= m.liq_dist_reduce || r >= m.margin_ratio_crosscheck {
        up = 2;
    }
    if liq <= m.liq_dist_force {
        up = 3;
    }
    up
}

fn add_rate(a: Rate, b: Rate) -> Rate {
    Rate(a.0.saturating_add(b.0))
}

/// Re-evaluates the tier of `st` at `now`; `from_sample` = called from `on_margin`.
pub(crate) fn evaluate(st: &mut MState, c: &Ctx<'_>, from_sample: bool, out: &mut MarginOut) {
    let now = c.now.engine_ns;
    let m = c.m;
    let stale = st.stale(now, c.run_start, m);
    let up = up_level(st, stale, m);
    let cur = st.level;
    let (liq, r) = (st.liq(), st.ratio());
    let mut l = cur;
    if up > cur {
        l = up;
    } else if up < cur && !stale {
        while l > up {
            let leave = match l {
                3 => {
                    liq > add_rate(m.liq_dist_force, m.liq_dist_hyst)
                        && (st.mflags & mflags::DEADLINE_L3 == 0
                            || liq > add_rate(m.liq_dist_reduce, m.liq_dist_hyst))
                }
                2 => {
                    liq > add_rate(m.liq_dist_reduce, m.liq_dist_hyst)
                        && (st.mflags & mflags::RATIO_L2 == 0
                            || r < Rate(m.margin_ratio_crosscheck.0.saturating_sub(m.margin_ratio_hyst.0)))
                }
                1 => liq > add_rate(m.liq_dist_block_open, m.liq_dist_hyst),
                _ => false,
            };
            if !leave {
                break;
            }
            l -= 1;
        }
    }
    let mut deadline_hit = false;
    if cur == 2 && l == 2 && st.deadline_ens != 0 && now >= st.deadline_ens {
        l = 3;
        deadline_hit = true;
    }
    transition(st, c, l, deadline_hit, out);
    if from_sample && st.level == 3 && cur == 3 && st.next_force_ens == 0 && st.confirmed(c.run_start) {
        force_step(st, c, out);
    }
}

/// Applies the new tier `l` (entry/exit actions, flags, notes, events).
fn transition(st: &mut MState, c: &Ctx<'_>, l: u8, deadline_hit: bool, out: &mut MarginOut) {
    let now = c.now.engine_ns;
    let cur = st.level;
    if l == 2 && st.ratio() >= c.m.margin_ratio_crosscheck {
        st.mflags |= mflags::RATIO_L2;
    }
    if l < 2 {
        st.mflags &= !mflags::RATIO_L2;
    }
    if l < 3 {
        st.mflags &= !mflags::DEADLINE_L3;
    }
    if l == cur {
        return;
    }
    st.level = l;
    if deadline_hit {
        st.mflags |= mflags::DEADLINE_L3;
    }
    if l == 2 {
        st.deadline_ens = now.saturating_add(ms(c.m.deleverage_deadline_ms));
        st.l2_since_ens = now;
        out.timers
            .push((RiskTimer::MarginDeadline { a: c.a, v: c.v }, st.deadline_ens));
    }
    if l < 2 {
        st.deadline_ens = 0;
        st.l2_since_ens = 0;
    }
    if l < 3 {
        st.next_force_ens = 0;
    }
    if l == 3 {
        st.next_force_ens = 0;
        if st.confirmed(c.run_start) {
            force_step(st, c, out);
        }
    }
    if l >= 2 {
        let level = if l == 3 { NoteLevel::P0 } else { NoteLevel::P1 };
        out.notes.push(note(
            N_MARGIN_LEVEL,
            level,
            [
                u64::from(c.a.0) | u64::from(c.v.0) << 8,
                u64::from(l),
                st.liq().0 as u64,
                st.ratio().0 as u64,
                st.deadline_ens as u64,
                0,
                0,
                0,
            ],
        ));
    }
    out.events.push((
        c.a,
        RiskEvent::MarginStress {
            venue: c.v,
            level: lvl(l),
            liq_dist: st.liq(),
            ratio: st.ratio(),
            deadline_tsc: if st.deadline_ens != 0 && l >= 2 {
                c.now.tsc_of(st.deadline_ens)
            } else {
                Tsc(0)
            },
        },
    ));
}

/// Outputs one `ForceDeleverage` step and registers the next `MarginForce`.
fn force_step(st: &mut MState, c: &Ctx<'_>, out: &mut MarginOut) {
    let now = c.now.engine_ns;
    out.force = Some(ForceDeleverage {
        account: c.a,
        venue: c.v,
        frac: c.m.force_step,
    });
    st.next_force_ens = now.saturating_add(ms(c.m.force_step_interval_ms));
    out.timers
        .push((RiskTimer::MarginForce { a: c.a, v: c.v }, st.next_force_ens));
}

/// `on_margin`: stores the sample, re-evaluates, keeps one `MarginStale` timer in the heap.
pub(crate) fn on_sample(st: &mut MState, s: &MarginSample, c: &Ctx<'_>, out: &mut MarginOut) {
    st.store(s);
    evaluate(st, c, true, out);
    let now = c.now.engine_ns;
    if !st.stale(now, c.run_start, c.m) {
        let d = st.stale_deadline(c.m);
        if st.stale_due_ens == 0 || d < st.stale_due_ens {
            st.stale_due_ens = d;
            out.timers.push((RiskTimer::MarginStale { a: c.a, v: c.v }, d));
        }
    }
}

/// One `Timer{Risk, …}` for this (account, venue).
pub(crate) fn on_timer(st: &mut MState, t: RiskTimer, c: &Ctx<'_>, out: &mut MarginOut) {
    let now = c.now.engine_ns;
    match t {
        RiskTimer::MarginStale { .. } => {
            if st.stale_due_ens != 0 && now < st.stale_due_ens {
                out.timers.push((t, st.stale_due_ens));
                return;
            }
            st.stale_due_ens = 0;
            evaluate(st, c, false, out);
            if !st.stale(now, c.run_start, c.m) {
                let d = st.stale_deadline(c.m);
                st.stale_due_ens = d;
                out.timers.push((t, d));
            }
        }
        RiskTimer::MarginDeadline { .. } => {
            if st.level != 2 || st.deadline_ens == 0 {
                return;
            }
            if now < st.deadline_ens {
                out.timers.push((t, st.deadline_ens));
                return;
            }
            transition(st, c, 3, true, out);
        }
        RiskTimer::MarginForce { .. } => {
            if st.level != 3 || st.next_force_ens == 0 {
                return;
            }
            if now < st.next_force_ens {
                out.timers.push((t, st.next_force_ens));
                return;
            }
            if st.confirmed(c.run_start) {
                force_step(st, c, out);
            } else {
                st.next_force_ens = 0;
            }
        }
    }
}

/// `MarginStress` cap input for an account whose (level, venue bitmap) changed.
pub(crate) fn cap_input(a: AccountId, level: u8, bitmap: u64) -> ModeInput<'static> {
    let scope = CmdScope {
        strategy: StrategyId(0),
        venue: VenueId(0),
        account: a,
        _pad: 0,
        group: GROUP_ALL,
        inst: InstId(0),
        _pad2: 0,
    };
    if level == 0 {
        ModeInput::AutoClear {
            source: CapSource::MARGIN_STRESS,
            scope,
        }
    } else {
        ModeInput::Auto(CapRecord {
            source: CapSource::MARGIN_STRESS,
            level,
            cflags: CapRecord::CONDITIONAL,
            _pad: [0; 5],
            scope,
            set_seq: 0,
            params: [bitmap, 0],
        })
    }
}
