//! Bounded integer signal and linked target execution. No statistical model or system time.
#![deny(clippy::float_arithmetic)]
use crate::{CarryFields, CarryParams};
use hft_strategy_sdk::prelude::*;

const ONE: i128 = 1_000_000_000;
const YEAR_S: i128 = 365 * 86_400;
const TICK: u32 = 1;
pub static NOTES: [NoteDesc; 1] = [NoteDesc {
    code: 1,
    name: "CarryTarget",
    level: NoteLevel::Info,
    v: &["signed leg zero quantity", "net annual edge", "reason"],
    doc: "linked target requested; zero quantity exits",
}];
pub static REPORT: [ReportFieldDesc; 2] = [
    ReportFieldDesc {
        name: "targets",
        unit: Unit::Count,
        agg: ReportAgg::Count,
        scope: ReportScope::Group,
        note: 1,
        v_ix: 0,
        doc: "accepted target requests",
    },
    ReportFieldDesc {
        name: "net_apr",
        unit: Unit::Rate,
        agg: ReportAgg::Last,
        scope: ReportScope::Group,
        note: 1,
        v_ix: 1,
        doc: "net annual edge at the latest target",
    },
];

#[derive(Clone, Debug)]
struct Group {
    spec: GroupSpec,
    f: CarryFields,
    quote_exp: i8,
    spot: bool,
    wake_ens: i64,
    attempt_ens: i64,
    // Safety exits stay latched through Broken/engine intervention until flat.
    exiting: bool,
}

#[derive(Debug)]
pub struct Fast {
    account: AccountId,
    groups: Vec<Group>,
}

/// Rate per period to annual rate, rounding income down and expense away from zero.
pub fn annual(rate: Rate, interval_s: u32) -> Option<i128> {
    if interval_s == 0 {
        return None;
    }
    Some((i128::from(rate.0) * YEAR_S).div_euclid(i128::from(interval_s)))
}

/// Net annual rate for short leg 0 / long leg 1 (`direction = -1`) or the reverse.
pub fn net_edge(
    a: &FundingView,
    b: Option<&FundingView>,
    fees: Rate,
    f: &CarryFields,
    direction: i64,
) -> Option<i64> {
    let diff = annual(a.rate, a.interval_s)?
        - match b {
            Some(b) => annual(b.rate, b.interval_s)?,
            None => 0,
        };
    let round_trip =
        i128::from(fees.0.max(0)) * 2 + i128::from(f.max_cross.0) * 4 + i128::from(f.expected_basis_cost.0);
    let horizon = i128::from(f.fee_horizon_ms);
    if horizon == 0 {
        return None;
    }
    let cost = (round_trip * YEAR_S * 1_000 + horizon - 1) / horizon;
    i64::try_from(diff * -i128::from(direction) - cost - i128::from(f.borrow_apr.0)).ok()
}

fn scale(value: i128, from: i32, to: i32) -> Option<i128> {
    let power = 10_i128.checked_pow(from.abs_diff(to))?;
    if from >= to {
        value.checked_mul(power)
    } else {
        Some(value.div_euclid(power))
    }
}
fn quantity(n: i64, px: Px, i: &InstHot) -> Option<i64> {
    if n <= 0 || px.0 <= 0 {
        return None;
    }
    let raw = scale(i128::from(n), -6, i32::from(i.qty_exp) + i32::from(i.px_exp))?;
    i64::try_from(raw / i128::from(px.0)).ok()
}
fn notional(q: i64, px: Px, i: &InstHot) -> i64 {
    let raw = i128::from(q).abs() * i128::from(px.0);
    scale(raw, i32::from(i.qty_exp) + i32::from(i.px_exp), -6)
        .and_then(|n| i64::try_from(n).ok())
        .unwrap_or(i64::MAX)
}
fn pair_lot(a: i64, b: i64) -> Option<i64> {
    if a <= 0 || b <= 0 {
        return None;
    }
    let (mut x, mut y) = (a, b);
    while y != 0 {
        let t = x % y;
        x = y;
        y = t;
    }
    (a / x).checked_mul(b)
}

impl Fast {
    pub fn new(account: AccountId, p: &CarryParams, g: &GroupTable, rd: &RefView<'_>) -> Self {
        let mut s = Self {
            account,
            groups: Vec::with_capacity(usize::from(g.capacity())),
        };
        s.update(p, g, rd);
        s
    }
    pub fn update(&mut self, p: &CarryParams, g: &GroupTable, rd: &RefView<'_>) {
        self.groups = g
            .iter()
            .map(|(ix, spec)| {
                let old = self
                    .groups
                    .iter()
                    .find(|x| x.spec.id == spec.id && x.spec.fingerprint() == spec.fingerprint());
                Group {
                    spec: *spec,
                    f: *p.at(ix),
                    spot: rd.caps(spec.legs[1].venue).market == MarketKind::Spot,
                    quote_exp: rd.refdata().asset(rd.inst(spec.legs[0].inst).quote).exp.qty_exp,
                    wake_ens: old.map_or(0, |x| x.wake_ens),
                    attempt_ens: old.map_or(0, |x| x.attempt_ens),
                    exiting: old.is_some_and(|x| x.exiting),
                }
            })
            .collect();
    }

    pub fn subscriptions(&self, out: &mut SubsBuilder<'_>) {
        for (ix, g) in self.groups.iter().enumerate() {
            for (leg, l) in g.spec.legs.iter().take(2).enumerate() {
                let mask = if leg == 1 && g.spot {
                    EvMask::BBO
                } else {
                    EvMask::BBO | EvMask::FUNDING | EvMask::MARK
                };
                let _ = out.market(l.inst, ix as u16, mask, Part::Fast);
            }
        }
    }

    pub fn on_event<V: Views>(&mut self, e: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        let ix = match e {
            EventRef::Bbo { gix, .. }
            | EventRef::Funding { gix, .. }
            | EventRef::Mark { gix, .. }
            | EventRef::Order { gix, .. }
            | EventRef::Exec { gix, .. }
            | EventRef::Timer { gix, .. }
            | EventRef::Mode { gix, .. }
            | EventRef::External { gix, .. }
            | EventRef::Notice { gix, .. } => Some(*gix),
            EventRef::Risk { .. } | EventRef::Conn { .. } => None,
            _ => return,
        };
        // Account-wide safety events only latch exits. Per-group timers bound Ctx usage.
        if ix.is_none() || ix == Some(GIX_NONE) {
            for g in &mut self.groups {
                if matches!(
                    e,
                    EventRef::External { .. }
                        | EventRef::Risk {
                            e: RiskEvent::MarginStress {
                                level: MarginLevel::L1 | MarginLevel::L2 | MarginLevel::L3,
                                ..
                            },
                            ..
                        }
                ) {
                    g.exiting = true;
                }
            }
            return;
        }
        let k = usize::from(ix.unwrap_or(GIX_NONE));
        let Some(g) = self.groups.get_mut(k) else { return };
        if matches!(e, EventRef::External { .. } | EventRef::Notice { .. }) {
            g.exiting = true;
        }
        if matches!(e, EventRef::Timer { tag: TICK, .. }) {
            g.wake_ens = 0;
        }
        let now = v.now();
        if g.wake_ens == 0 {
            let at = now
                .engine_ns
                .saturating_add(hft_strat_lib::units::ms_to_ns(g.f.retry_ms));
            if ctx.timer_at(now.tsc_of(at), TICK, g.spec.id).is_ok() {
                g.wake_ens = at;
            }
        }
        let g = g.clone(); // fixed-size row, no heap members
        self.evaluate(k, &g, v, ctx);
    }

    fn evaluate<V: Views>(&mut self, k: usize, g: &Group, v: &V, ctx: &mut Ctx<'_>) {
        let a = self.account;
        let now = v.now();
        let legs = v.legs(a, g.spec.id);
        let pos = v.group_pos(a, g.spec.id);
        let held = pos.legs.iter().take(2).any(|p| p.qty.0 != 0);
        let target = v.target(a, g.spec.id);
        if target.is_some_and(|t| {
            matches!(t.state, TargetState::Broken(_))
                || (t.origin != TargetOrigin::Strategy && !matches!(t.state, TargetState::Idle))
        }) || legs.has_unknown
            || legs.open_links != 0
            || legs.has_pending
        {
            return;
        }
        let f = &g.f;
        let insts = [g.spec.legs[0].inst, g.spec.legs[1].inst];
        let spot = v.caps(g.spec.legs[1].venue).market == MarketKind::Spot;
        let fa = v.funding(insts[0]);
        let fb = if spot { None } else { Some(v.funding(insts[1])) };
        let fresh = |x: &FundingView| {
            now.clock_ok
                && x.interval_s != 0
                && x.upd_tsc.0 != 0
                && x.upd_tsc <= now.tsc
                && now.tsc.0 - x.upd_tsc.0 <= u64::from(f.funding_max_age_ms).saturating_mul(now.tsc_per_ms)
                && x.next_settle_ns > now.utc_ns
        };
        let data_ok = fresh(fa) && fb.is_none_or(fresh);
        let fees = Rate(
            v.fee(a, insts[0])
                .taker
                .0
                .max(0)
                .saturating_add(v.fee(a, insts[1]).taker.0.max(0)),
        );
        // Validation permits only short leg 0 / long spot leg 1. Legacy opposite
        // inventory can be reduced, never used to select a reverse opening target.
        let direction = -1;
        let edge = net_edge(fa, fb, fees, f, direction).unwrap_or(i64::MIN);
        let opening_target = target.is_some_and(|t| t.intent.legs.iter().take(2).any(|l| l.qty.0 != 0));
        let close = pos.legs[0].qty.0 > 0
            || pos.legs[1].qty.0 < 0
            || g.exiting
            || !f.enabled
            || g.spec.open_allowed == 0
            || v.mode(a, g.spec.id) == RunMode::CloseOnly
            || (data_ok && edge < f.exit_apr.0)
            || (opening_target && !held && !data_ok);
        if close {
            if held || opening_target {
                self.submit(k, g, v, ctx, 0, edge, TargetReason::Exit);
            } else if let Some(state) = self.groups.get_mut(k) {
                state.exiting = false;
            }
            return;
        }
        // Hold during missing/stale funding, never extrapolate it into a new position.
        if held || opening_target || !data_ok || edge < f.enter_apr.0 {
            return;
        }
        if !insts
            .iter()
            .all(|i| v.quote_ok(a, *i) && v.venue_ready(a, v.inst(*i).venue))
        {
            return;
        }
        let next = if direction < 0 {
            fa.next_settle_ns
        } else {
            fb.map_or(0, |b| b.next_settle_ns)
        };
        let lead = next.saturating_sub(now.utc_ns);
        if lead < hft_strat_lib::units::ms_to_ns(f.min_lead_ms)
            || lead > hft_strat_lib::units::ms_to_ns(f.entry_window_ms)
        {
            return;
        }
        // Each group's share is disjoint, including commitments that have not filled yet.
        let mut qty = i64::MAX;
        for i in insts {
            let meta = v.inst(i);
            let shares = self
                .groups
                .iter()
                .filter(|x| x.spec.legs.iter().take(2).any(|l| l.venue == meta.venue))
                .count()
                .max(1);
            let port = v.portfolio(a);
            let perp = v.caps(meta.venue).market == MarketKind::LinearPerp;
            let capital = if perp {
                let m = port.margin(meta.venue);
                let Some(l) = v.leverage(a, i) else { return };
                if l == 0 || l > f.max_leverage_x100 || m.stale || m.margin_balance.0 <= 0 {
                    return;
                }
                // Nominal hard cap is independent of leverage. Available margin caps the increment.
                m.margin_balance.0.min(m.avail_margin.0.max(0))
            } else {
                let cash = port.cash(meta.venue, meta.quote).free.0.max(0);
                scale(i128::from(cash), i32::from(g.quote_exp), -6)
                    .and_then(|n| i64::try_from(n).ok())
                    .unwrap_or(0)
            };
            let cap = ((i128::from(capital) * i128::from(f.capital_fraction.0) / ONE) / shares as i128)
                .min(i128::from(f.max_notional.0));
            let b = v.bbo(i);
            if b.bid_px.0 <= 0 || b.ask_px.0 < b.bid_px.0 {
                return;
            }
            let px = Px(
                ((i128::from(b.ask_px.0.max(v.mark(i).0)) * (ONE + i128::from(f.max_cross.0)) + ONE - 1)
                    / ONE)
                    .min(i128::from(i64::MAX)) as i64,
            );
            let Some(q) = quantity(cap as i64, px, meta) else {
                return;
            };
            qty = qty.min(q).min(meta.max_qty.0);
        }
        let (m0, m1) = (v.inst(insts[0]), v.inst(insts[1]));
        let Some(lot) = pair_lot(m0.lot, m1.lot) else {
            return;
        };
        qty = qty / lot * lot;
        if qty <= 0
            || qty < m0.min_qty.0
            || qty < m1.min_qty.0
            || insts
                .iter()
                .any(|i| notional(qty, v.bbo(*i).bid_px, v.inst(*i)) < v.inst(*i).min_notional.0)
        {
            return;
        }
        self.submit(k, g, v, ctx, qty * direction, edge, TargetReason::Rebalance);
    }

    #[allow(clippy::too_many_arguments)]
    fn submit<V: Views>(
        &mut self,
        k: usize,
        g: &Group,
        v: &V,
        ctx: &mut Ctx<'_>,
        q: i64,
        edge: i64,
        reason: TargetReason,
    ) {
        let now = v.now();
        if g.attempt_ens != 0
            && now.engine_ns
                < g.attempt_ens
                    .saturating_add(hft_strat_lib::units::ms_to_ns(g.f.retry_ms))
        {
            return;
        }
        let old = v.target(self.account, g.spec.id);
        if old.is_some_and(|t| {
            t.intent.legs[0].qty.0 == q
                && t.intent.legs[1].qty.0 == -q
                && matches!(
                    t.state,
                    TargetState::Working | TargetState::Idle | TargetState::Stuck(StuckReason::MinOrder)
                )
        }) {
            return;
        }
        let f = &g.f;
        let t = TargetIntent {
            account: self.account,
            _pad0: [0; 3],
            group: g.spec.id,
            legs: [
                LegTarget::new(0, Qty(q)),
                LegTarget::new(1, Qty(-q)),
                LegTarget::ZERO,
                LegTarget::ZERO,
            ],
            n_legs: 2,
            reason,
            urgency: Urgency::Normal,
            _pad1: [0; 5],
            band: Rate(0),
            exec: ExecSpec::Simultaneous(SimulParams {
                slice: TakerSliceParams {
                    depth_frac: Rate(1_000_000_000),
                    depth_levels: 1,
                    _pad0: [0; 7],
                    max_cross: f.max_cross,
                    max_chunk_notional: f.max_notional,
                    min_chunk_notional: Notional(0),
                    cooldown_ms: f.retry_ms,
                    _pad1: 0,
                },
                on_fail: SimulOnFail::Reverse,
                _pad0: [0; 7],
                fix_tolerance: Rate(0),
                residual: ResidualPolicy::TrimExcess,
                _pad1: [0; 7],
            }),
            tag: old.map_or(0, |t| t.intent.tag.wrapping_add(1)),
            _pad2: 0,
        };
        if ctx.target(t).is_ok() {
            if let Some(s) = self.groups.get_mut(k) {
                s.attempt_ens = now.engine_ns;
                s.exiting = q == 0;
            }
            ctx.note(
                1,
                NoteLevel::Info,
                g.spec.id,
                INST_NONE,
                [q as u64, edge as u64, reason as u64, 0, 0, 0, 0, 0],
            );
        }
    }

    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u16(self.groups.len() as u16);
        for g in &self.groups {
            w.put_u32(g.spec.id.0);
            w.put_u64(g.spec.fingerprint());
            w.put_i64(g.wake_ens);
            w.put_i64(g.attempt_ens);
            w.put_u8(u8::from(g.exiting));
        }
    }
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        if !matches!(r.desc_version(), 1 | 2) {
            return Err(RestoreError::VersionUnsupported);
        }
        let n = r.get_u16()?;
        if usize::from(n) != self.groups.len() {
            return Err(RestoreError::Capacity);
        }
        let mut restored = self.groups.clone(); // restore is a cold, atomic operation
        for g in &mut restored {
            if r.get_u32()? != g.spec.id.0 || r.get_u64()? != g.spec.fingerprint() {
                return Err(RestoreError::BadValue);
            }
            g.wake_ens = r.get_i64()?;
            g.attempt_ens = r.get_i64()?;
            g.exiting = match r.get_u8()? {
                0 => false,
                1 => true,
                _ => return Err(RestoreError::BadValue),
            };
        }
        self.groups = restored;
        Ok(())
    }
}
