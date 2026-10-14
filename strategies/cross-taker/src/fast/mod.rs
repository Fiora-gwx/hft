//! Event-driven trading with no local hedge state machine.
#![deny(clippy::float_arithmetic)]
pub mod signal;
use crate::{desc, params::XtParams};
use hft_strategy_sdk::prelude::*;
use signal::LegInput;

const TAG: u32 = 1;
#[derive(Clone, Copy, Debug, Default)]
struct State {
    next_ens: i64,
    failures: u32,
    killed: bool,
    broken: bool,
    halt_sent: bool,
    notice_until: i64,
    first_venue: u8,
    tracking: bool,
    filled: bool,
    failed_this_pair: bool,
    last_failed: Option<OrderRef>,
}
const _: () = assert!(core::mem::size_of::<State>() <= 128);

#[derive(Debug)]
pub struct Fast {
    account: AccountId,
    p: XtParams,
    groups: GroupTable,
    states: Box<[State]>,
}
impl Fast {
    pub fn new(account: AccountId, p: &XtParams, groups: &GroupTable) -> Self {
        Self {
            account,
            p: p.clone(),
            groups: groups.clone(),
            states: vec![State::default(); usize::from(groups.capacity())].into_boxed_slice(),
        }
    }
    pub fn on_params(&mut self, p: &XtParams, groups: &GroupTable) {
        let mut states = vec![State::default(); usize::from(groups.capacity())].into_boxed_slice();
        for (ix, g) in groups.iter() {
            if let Some(old) = self
                .groups
                .gix_of(g.id)
                .filter(|old| self.groups.get(*old).fingerprint() == g.fingerprint())
                && let (Some(dst), Some(src)) =
                    (states.get_mut(usize::from(ix)), self.states.get(usize::from(old)))
            {
                *dst = *src;
            }
        }
        self.states = states;
        self.groups = groups.clone();
        self.p = p.clone();
    }
    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        match ev {
            EventRef::Bbo { gix, .. } => self.trade(*gix, v, ctx),
            EventRef::Order { gix, u } => {
                let f = self.p.at(*gix);
                let g = self.groups.get(*gix);
                let Some(s) = self.states.get_mut(usize::from(*gix)) else {
                    return;
                };
                // OMS-generated second/reverse legs use tag 0. Ownership is the account/group;
                // one active pair is allowed per group, and any leg may fail first.
                if u.r.account != self.account || u.group != g.id {
                    return;
                }
                if matches!(u.kind, OrderUpdateKind::Filled { .. }) {
                    s.filled = true;
                }
                let failed = match u.kind {
                    OrderUpdateKind::Rejected { .. } => true,
                    OrderUpdateKind::Cancelled | OrderUpdateKind::Aged => {
                        v.order(u.r).is_some_and(|o| o.filled.0 == 0)
                    }
                    _ => false,
                };
                if s.tracking && !s.broken && !s.failed_this_pair && failed && s.last_failed != Some(u.r) {
                    s.last_failed = Some(u.r);
                    failure(s, g.id, f.max_hedge_failures, ctx);
                }
                settled(s, v.legs(self.account, g.id), v.risk(self.account).dust_notional);
                maybe_halt(s, g.id, ctx);
            }
            EventRef::Risk {
                a,
                e: RiskEvent::HedgeBroken { group },
            } if *a == self.account => {
                if let Some(ix) = self.groups.gix_of(*group) {
                    self.broken(ix, ctx);
                }
            }
            EventRef::Exec { gix, u } => match u.kind {
                ExecUpdateKind::Broken(_) => self.broken(*gix, ctx),
                ExecUpdateKind::Done {
                    cleared_by_engine: true,
                } => {
                    if let Some(s) = self.states.get_mut(usize::from(*gix)) {
                        s.broken = false;
                    }
                }
                _ => {}
            },
            EventRef::Mode { gix, old, new } if *old != *new && *new == RunMode::Live => {
                let g = self.groups.get(*gix);
                let l = v.legs(self.account, g.id);
                if idle(l)
                    && l.naked_qty.0 == 0
                    && !v.risk(self.account).open_blocked
                    && let Some(s) = self.states.get_mut(usize::from(*gix))
                {
                    s.killed = false;
                    s.halt_sent = false;
                    s.failures = 0;
                }
            }
            EventRef::Notice { gix, n, .. } => {
                if let Some(s) = self.states.get_mut(usize::from(*gix)) {
                    s.notice_until = match n.kind {
                        NoticeKind::Delisting { .. } => i64::MAX,
                        NoticeKind::Maintenance { to_ns, .. } => s.notice_until.max(to_ns),
                        _ => s.notice_until,
                    };
                }
            }
            // External positions, cap changes and connection state are read from Views on every BBO.
            _ => {}
        }
    }
    fn broken(&mut self, ix: u16, ctx: &mut Ctx<'_>) {
        let Some(s) = self.states.get_mut(usize::from(ix)) else {
            return;
        };
        let group = self.groups.get(ix).id;
        if !s.broken {
            s.broken = true;
            if !s.failed_this_pair {
                failure(s, group, self.p.at(ix).max_hedge_failures, ctx);
            }
        }
        maybe_halt(s, group, ctx);
    }
    fn trade<V: Views>(&mut self, ix: u16, v: &V, ctx: &mut Ctx<'_>) {
        if ix >= self.groups.len() {
            return;
        }
        let g = self.groups.get(ix);
        let f = self.p.at(ix);
        let Some(s) = self.states.get_mut(usize::from(ix)) else {
            return;
        };
        let now = v.now();
        let legs = v.legs(self.account, g.id);
        let risk = v.risk(self.account);
        settled(s, legs, risk.dust_notional);
        maybe_halt(s, g.id, ctx);
        if !f.enabled
            || !now.clock_ok
            || now.tsc_per_ms == 0
            || s.broken
            || now.engine_ns < s.next_ens
            || now.utc_ns < s.notice_until
            || !idle(legs)
            || !hedged(legs, risk.dust_notional)
        {
            return;
        }
        let [a, b, ..] = g.legs;
        if risk.hard {
            return;
        }
        let (a_book, b_book) = (v.bbo(a.inst), v.bbo(b.inst));
        // Signal requires buy_limit >= ask and sell_limit <= bid and never credits
        // rebates. With a positive required edge, uncrossed touches cannot qualify.
        // Keep settlement and halt delivery above this read-only rejection.
        if f.min_edge.0 > 0 && a_book.bid_px <= b_book.ask_px && b_book.bid_px <= a_book.ask_px {
            return;
        }
        let input = |l: LegDef, k: usize, book: BboView| -> Option<LegInput> {
            if !v.quote_ok(self.account, l.inst)
                || !v.venue_ready(self.account, l.venue)
                || book.rx_tsc.0 == 0
                || book.rx_tsc > now.tsc
                || now.tsc.0 - book.rx_tsc.0 >= u64::from(f.max_quote_age_ms).saturating_mul(now.tsc_per_ms)
            {
                return None;
            }
            let hot = *v.inst(l.inst);
            let spot = v.caps(l.venue).market == MarketKind::Spot;
            let position = v.group_pos(self.account, g.id).legs.get(k)?.qty;
            let inv = v.portfolio(self.account).inventory(l.venue, hot.asset);
            let spot_free = Qty(inv.total.0.saturating_sub(inv.baseline.0).max(0));
            Some(LegInput {
                book,
                hot,
                fee: v.fee(self.account, l.inst).taker,
                position,
                spot,
                spot_free,
            })
        };
        let Some(la) = input(a, 0, a_book) else {
            return;
        };
        let Some(lb) = input(b, 1, b_book) else {
            return;
        };
        let allowed = |x: &signal::Signal| {
            x.reducing
                || (!s.killed
                    && !risk.open_blocked
                    && !risk.flatten_lock
                    && matches!(v.mode(self.account, g.id), RunMode::Live | RunMode::Shadow))
        };
        let ab = signal::signal(la, lb, f).filter(allowed);
        let ba = signal::signal(lb, la, f).filter(allowed);
        let (sig, buy, sell, buy_input, buy_ix) = match (ab, ba) {
            (Some(x), Some(y)) if y.edge > x.edge => (y, b, a, lb, 1),
            (Some(x), _) => (x, a, b, la, 0),
            (_, Some(y)) => (y, b, a, lb, 1),
            _ => return,
        };
        let reduce_buy = sig.reducing && !buy_input.spot;
        let it = OrderIntent {
            venue: buy.venue,
            account: self.account,
            side: Side::Buy,
            tif: Tif::Ioc,
            class: if sig.reducing {
                IntentClass::Reduce
            } else {
                IntentClass::Open
            },
            flags: if reduce_buy { OrderIntent::REDUCE_ONLY } else { 0 },
            _pad: [0; 2],
            inst: buy.inst,
            group: g.id,
            px: sig.buy_px,
            qty: sig.qty,
            link: LegLinkOpt::some(LegLink {
                group: g.id,
                role: LinkRole::First,
                _pad: [0; 3],
                tolerance: Rate(0),
                hedge: HedgePlan {
                    kind: HedgeKind::Simultaneous,
                    px_mode: PxMode::Limit,
                    on_fail: OnFail::Reverse,
                    tif: Tif::Ioc,
                    venue: sell.venue,
                    account: self.account,
                    side: Side::Sell,
                    // Semantic reduction hint, including spot; the engine selects venue flags.
                    reduce_only: u8::from(sig.reducing),
                    inst: sell.inst,
                    ratio_num: 1,
                    ratio_den: 1,
                    residual: ResidualPolicy::TrimExcess,
                    _pad: [0; 3],
                    px_param: f.slip,
                    max_slip: f.slip,
                    limit_px: sig.sell_px,
                },
            }),
            tag: TAG,
            max_rest_ms: 0,
        };
        if ctx.place(it).is_ok() {
            s.next_ens = now.engine_ns.saturating_add(time::ms(f.cooldown_ms));
            s.first_venue = buy.venue.0;
            s.tracking = true;
            s.filled = false;
            s.failed_this_pair = false;
            s.last_failed = None;
            ctx.note(
                desc::FIRED,
                NoteLevel::Info,
                g.id,
                buy.inst,
                [
                    buy_ix,
                    sig.qty.0 as u64,
                    sig.edge.0 as u64,
                    u64::from(sig.reducing),
                    0,
                    0,
                    0,
                    0,
                ],
            );
        }
    }
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u16(self.groups.len());
        for (ix, g) in self.groups.iter() {
            let s = self.states.get(usize::from(ix)).copied().unwrap_or_default();
            w.put_u32(g.id.0);
            w.put_u64(g.fingerprint());
            w.put_i64(s.next_ens);
            w.put_u32(s.failures);
            w.put_u8(u8::from(s.killed));
            w.put_u8(u8::from(s.broken));
            w.put_u8(u8::from(s.halt_sent));
            w.put_i64(s.notice_until);
            w.put_u8(s.first_venue);
            w.put_u8(u8::from(s.tracking));
            w.put_u8(u8::from(s.filled));
            w.put_u8(u8::from(s.failed_this_pair));
            w.put_u8(u8::from(s.last_failed.is_some()));
            let r = s
                .last_failed
                .unwrap_or(OrderRef::new(VenueId(0), AccountId(0), 0, 0));
            w.put_u8(r.venue.0);
            w.put_u8(r.account.0);
            w.put_u32(r.slot);
            w.put_u32(r.r#gen);
        }
    }
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        // Validate the entire bounded section before changing any live state.
        self.read_snapshot(&mut r.clone(), false)?;
        self.read_snapshot(r, true)
    }
    fn read_snapshot(&mut self, r: &mut SnapshotReader<'_>, apply: bool) -> Result<(), RestoreError> {
        if r.desc_version() != 1 {
            return Err(RestoreError::VersionUnsupported);
        }
        let n = r.get_u16()?;
        if n != self.groups.len() {
            return Err(RestoreError::BadValue);
        }
        for (ix, g) in self.groups.iter() {
            if r.get_u32()? != g.id.0 || r.get_u64()? != g.fingerprint() {
                return Err(RestoreError::BadValue);
            }
            let mut s = State {
                next_ens: r.get_i64()?,
                failures: r.get_u32()?,
                killed: boolean(r)?,
                broken: boolean(r)?,
                halt_sent: boolean(r)?,
                notice_until: r.get_i64()?,
                first_venue: r.get_u8()?,
                tracking: boolean(r)?,
                filled: boolean(r)?,
                failed_this_pair: boolean(r)?,
                last_failed: None,
            };
            let has_ref = boolean(r)?;
            let order = OrderRef::new(
                VenueId(r.get_u8()?),
                AccountId(r.get_u8()?),
                r.get_u32()?,
                r.get_u32()?,
            );
            if has_ref {
                s.last_failed = Some(order);
            }
            if apply && let Some(dst) = self.states.get_mut(usize::from(ix)) {
                *dst = s;
            }
        }
        if r.remaining() != 0 {
            return Err(RestoreError::BadValue);
        }
        Ok(())
    }
}
fn boolean(r: &mut SnapshotReader<'_>) -> Result<bool, RestoreError> {
    match r.get_u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RestoreError::BadValue),
    }
}
fn idle(l: &LegView) -> bool {
    l.open_links == 0
        && !l.has_unknown
        && !l.has_pending
        && l.leg
            .iter()
            .all(|x| x.inflight_immediate.0 == 0 && x.inflight_resting.0 == 0)
}
fn hedged(l: &LegView, dust: Notional) -> bool {
    l.naked_qty.0 == 0 || (l.naked_notional.0 > 0 && l.naked_notional.0 <= dust.0)
}
fn settled(s: &mut State, l: &LegView, dust: Notional) {
    if s.tracking && idle(l) && hedged(l, dust) {
        if s.filled && !s.failed_this_pair {
            s.failures = 0;
        }
        s.tracking = false;
    }
}
fn failure(s: &mut State, group: GroupId, max: u32, ctx: &mut Ctx<'_>) {
    s.failed_this_pair = true;
    s.failures = s.failures.saturating_add(1);
    s.killed |= s.failures >= max;
    ctx.note(
        desc::HEDGE_FAILED,
        NoteLevel::P2,
        group,
        INST_NONE,
        [u64::from(s.failures), 0, 0, 0, 0, 0, 0, 0],
    );
    maybe_halt(s, group, ctx);
}
fn maybe_halt(s: &mut State, group: GroupId, ctx: &mut Ctx<'_>) {
    if s.killed && !s.halt_sent && ctx.request_halt(group, desc::HALTED as u16).is_ok() {
        s.halt_sent = true;
        ctx.note(
            desc::HALTED,
            NoteLevel::P1,
            group,
            INST_NONE,
            [u64::from(s.failures), 0, 0, 0, 0, 0, 0, 0],
        );
    }
}
