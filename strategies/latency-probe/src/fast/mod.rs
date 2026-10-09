//! Fast part of `LatencyProbe` (README "实现说明"): per-group state, event dispatch and the shared
//! helpers; quote probes in `quote`, the pair/target phase machine in `pair` and `target`, residual
//! flattening and the residual cap in `flatten`.
#![deny(clippy::float_arithmetic)]

pub mod flatten;
pub mod pair;
pub mod quote;
pub mod target;

use hft_strat_lib::{Throttle, ThrottleParams};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::{
    BboView, Ctx, EventRef, ExecUpdateKind, GIX_NONE, GroupSpecExt, GroupTable, IntentHandle, IntentOutcome,
    NoticeKind, Now, OrderRef, RestoreError, RiskEvent, SnapshotReader, SnapshotWriter, Views,
};

use crate::desc;
use crate::params::{Mode, ProbeFields, ProbeParams};

/// Timer tags.
pub const PAIR_TICK: u32 = 1;
pub const FLAT: u32 = 2;

/// Order-intent tag kinds (`tag = kind << 8 | leg`).
pub const K_PROBE: u32 = 1;
pub const K_OPEN: u32 = 2;
pub const K_CLOSE: u32 = 3;
pub const K_RESID: u32 = 4;
/// Pending target kinds (not order tags).
pub const K_T_OPEN: u8 = 5;
pub const K_T_EXIT: u8 = 6;

/// `CapSource::HALTED` wire code (hft-types; not in the SDK prelude).
pub const HALTED_CAP: u8 = 8;

/// Most legs of a group.
pub const MAX_LEGS: usize = 4;
/// Note cadence of the counting Notes (README "Note 码表与报表字段").
pub const NOTE_PERIOD_NS: i64 = 600_000 * 1_000_000;

// Leg flags.
pub const L_INFLIGHT: u8 = 1;
pub const L_REF: u8 = 2;
pub const L_ACKED: u8 = 4;
pub const L_CANCEL_SENT: u8 = 8;
pub const L_VERIFY: u8 = 16;
pub const L_UNKNOWN: u8 = 32;
pub const L_DELIST: u8 = 64;
pub const L_EXTERNAL: u8 = 128;

// Group flags.
pub const G_ACTIVE: u8 = 1;
pub const G_STARTED: u8 = 2;
pub const G_TICK: u8 = 4;
pub const G_STUCK_RETRY: u8 = 8;
pub const G_FILLED: u8 = 16;
pub const G_FLATTEN_NEXT: u8 = 32;
pub const G_RESTORED: u8 = 64;
/// `Blocked` that only a parameter change releases (target `LotMismatch`).
pub const G_STICKY: u8 = 128;

/// Pair/target phase.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum Phase {
    #[default]
    Idle = 0,
    Due = 1,
    Opening = 2,
    Holding = 3,
    Flattening = 4,
    Blocked = 5,
}

impl Phase {
    const fn of(v: u8) -> Option<Phase> {
        Some(match v {
            0 => Phase::Idle,
            1 => Phase::Due,
            2 => Phase::Opening,
            3 => Phase::Holding,
            4 => Phase::Flattening,
            5 => Phase::Blocked,
            _ => return None,
        })
    }
}

/// State of one probe point (leg). Times are engine_ns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct LegState {
    pub last_probe_ens: i64,
    /// `OrderRef.slot`/`gen` of the probe (or pair first leg) in flight; valid with `L_REF`.
    pub rest_slot: u32,
    pub rest_gen: u32,
    /// `L_*` bits.
    pub flags: u8,
    /// `alternate`: the next probe sells.
    pub next_sell: bool,
}

/// State of one group (≤ 256 B). Times are engine_ns, the maintenance window UTC ns; no TSC.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ProbeGroup {
    pub legs: [LegState; MAX_LEGS],
    pub thr: Throttle,
    pub next_pair_ens: i64,
    pub flat_at_ens: i64,
    pub note_due_ens: i64,
    /// Maintenance window (UTC ns) of the legs in `maint_mask`; (0, 0) = none.
    pub maint_from: i64,
    pub maint_to: i64,
    /// target: signed leg-0 quantity of the opening target and its slice `max_chunk_notional`.
    pub tgt_q: i64,
    pub tgt_chunk: i64,
    pub fp: u64,
    pub gid: u32,
    pub rejects: u16,
    pub gated: u16,
    pub nofill: u16,
    pub skip_spread: u16,
    pub skip_unhealthy: u16,
    pub rej_bits: u32,
    pub rej_code: u32,
    pub gated_bits: u32,
    pub phase: Phase,
    /// `Flattening(n)`.
    pub attempt: u8,
    /// 0 = the next `auto` opening of a perp/perp pair buys A.
    pub next_dir: u8,
    pub retry_tag: u8,
    pub maint_mask: u8,
    /// `G_*` bits.
    pub flags: u8,
}

const _: () = assert!(core::mem::size_of::<ProbeGroup>() <= 256);

/// Static per-group data derived from the GroupTable (not snapshotted).
#[derive(Clone, Copy, Debug)]
pub struct GroupCfg {
    pub gid: GroupId,
    pub n_legs: u8,
    pub insts: [InstId; MAX_LEGS],
    pub venues: [VenueId; MAX_LEGS],
    pub spot: [bool; MAX_LEGS],
    pub fp: u64,
}

impl GroupCfg {
    pub const EMPTY: GroupCfg = GroupCfg {
        gid: GroupId(0),
        n_legs: 0,
        insts: [InstId(0); MAX_LEGS],
        venues: [VenueId(0); MAX_LEGS],
        spot: [false; MAX_LEGS],
        fp: 0,
    };
    pub fn leg_of(&self, i: InstId) -> Option<usize> {
        self.insts
            .iter()
            .take(usize::from(self.n_legs))
            .position(|x| *x == i)
    }
    pub fn n(&self) -> usize {
        usize::from(self.n_legs).min(MAX_LEGS)
    }
}

/// One intent of the previous `on_event` whose outcome is read in the next one.
#[derive(Clone, Copy, Debug)]
pub struct Pend {
    pub gix: u16,
    pub kind: u8,
    pub leg: u8,
    pub h: IntentHandle,
}

pub const MAX_PEND: usize = 32;

/// Everything a handler reads besides the group state.
#[allow(missing_debug_implementations)]
pub struct Cx<'a, V: Views> {
    pub v: &'a V,
    pub now: Now,
    pub acct: AccountId,
    pub cfg: &'a GroupCfg,
    pub f: &'a ProbeFields,
    pub mode: Mode,
    pub gix: u16,
    pub size_margin: Rate,
    pub max_consec_fail: u8,
    /// `rx_tsc` of the previous `Bbo` of the instance (burst detection).
    pub last_rx: Tsc,
}

impl<V: Views> Cx<'_, V> {
    pub fn gid(&self) -> GroupId {
        self.cfg.gid
    }
    pub fn ens(&self) -> i64 {
        self.now.engine_ns
    }
    pub fn inst(&self, leg: usize) -> InstId {
        self.cfg.insts.get(leg).copied().unwrap_or(InstId(0))
    }
    pub fn venue(&self, leg: usize) -> VenueId {
        self.cfg.venues.get(leg).copied().unwrap_or(VenueId(0))
    }
    pub fn spot(&self, leg: usize) -> bool {
        self.cfg.spot.get(leg).copied().unwrap_or(false)
    }
    pub fn net(&self, leg: usize) -> Qty {
        self.v
            .legs(self.acct, self.gid())
            .leg
            .get(leg)
            .map_or(Qty(0), |l| l.net)
    }
    pub fn lot(&self, leg: usize) -> i64 {
        self.v.inst(self.inst(leg)).lot
    }
    /// `|net| < 1 lot` on every leg.
    pub fn flat(&self) -> bool {
        (0..self.cfg.n()).all(|k| self.net(k).0.unsigned_abs() < self.lot(k).max(1).unsigned_abs())
    }
    pub fn live(&self) -> bool {
        self.v.mode(self.acct, self.gid()) == RunMode::Live
    }
    pub fn order_ref(&self, leg: usize, l: &LegState) -> OrderRef {
        OrderRef::new(self.venue(leg), self.acct, l.rest_slot, l.rest_gen)
    }
    pub fn throttle_params(&self) -> hft_strat_lib::ThrottleParamsNs {
        ThrottleParams {
            cooldown_ms: 0,
            max_consec_fail: self.max_consec_fail,
            fallback_ms: 0,
            hard_rearm_ms: 0,
        }
        .ns()
    }
    pub fn note(&self, ctx: &mut Ctx<'_>, code: u32, v: [u64; 8]) {
        let level = desc::NOTES
            .iter()
            .find(|n| n.code == code)
            .map_or(NoteLevel::Info, |n| n.level);
        ctx.note(code, level, self.gid(), INST_NONE, v);
    }
    pub fn timer(&self, ctx: &mut Ctx<'_>, at_ens: i64, tag: u32) {
        let _ = ctx.timer_at(self.now.tsc_of(at_ens), tag, self.gid());
    }
}

// ------------------------------------------------------------------------------ fixed point

/// `10^k` (k ≤ 38).
const fn pow10(k: u32) -> i128 {
    let mut v = 1i128;
    let mut i = 0;
    while i < k {
        v *= 10;
        i += 1;
    }
    v
}

/// `⌈q × px⌉` as `Notional` (USDT 1e-6) at the inst's exponents; saturating.
pub fn notional_ceil(q: Qty, px: Px, h: &InstHot) -> i64 {
    let prod = i128::from(q.0.unsigned_abs().min(i64::MAX as u64) as i64) * i128::from(px.0.max(0));
    let e = i32::from(h.px_exp) + i32::from(h.qty_exp) + 6;
    let v = if e >= 0 {
        prod.saturating_mul(pow10(e.unsigned_abs().min(38)))
    } else {
        let d = pow10(e.unsigned_abs().min(38));
        prod / d + i128::from(prod % d != 0)
    };
    v.min(i128::from(i64::MAX)) as i64
}

/// Smallest `q`: multiple of `lot`, ≥ `min_qty`, `q × px ≥ min_notional × (1 + margin)`
/// (README "意图"); `None` when `px ≤ 0`, `lot ≤ 0` or out of range.
pub fn min_qty(h: &InstHot, px: Px, margin: Rate, step: i64) -> Option<Qty> {
    if px.0 <= 0 || step <= 0 {
        return None;
    }
    let need = i128::from(h.min_notional.0) * (1_000_000_000 + i128::from(margin.0));
    let need = need / 1_000_000_000 + i128::from(need % 1_000_000_000 != 0);
    // q × px × 10^e ≥ need, e = px_exp + qty_exp + 6.
    let e = i32::from(h.px_exp) + i32::from(h.qty_exp) + 6;
    let (num, den) = if e >= 0 {
        (need, i128::from(px.0) * pow10(e.unsigned_abs().min(38)))
    } else {
        (need * pow10(e.unsigned_abs().min(38)), i128::from(px.0))
    };
    let q = num / den + i128::from(num % den != 0);
    let q = q.max(i128::from(h.min_qty.0));
    let s = i128::from(step);
    let q = (q + s - 1) / s * s;
    i64::try_from(q).ok().map(Qty)
}

/// `(ask − bid) × 10^9 / mid` (HalfEven) ≤ `max`; any side ≤ 0 → false.
pub fn spread_ok(b: &BboView, max: Rate) -> bool {
    if b.bid_px.0 <= 0 || b.ask_px.0 <= 0 {
        return false;
    }
    let mid = Px::mid_floor(b.bid_px, b.ask_px);
    if mid.0 <= 0 {
        return false;
    }
    let n = (i128::from(b.ask_px.0) - i128::from(b.bid_px.0)) * 1_000_000_000;
    let d = i128::from(mid.0);
    let (q, r) = (n.div_euclid(d), n.rem_euclid(d));
    let up = match (2 * r).cmp(&d) {
        core::cmp::Ordering::Greater => true,
        core::cmp::Ordering::Equal => q & 1 == 1,
        core::cmp::Ordering::Less => false,
    };
    q + i128::from(up) <= i128::from(max.0)
}

/// IOC price crossing the touch by `cross` (buy: ask × (1 + cross), sell: bid × (1 − cross)),
/// rounded to the tick by `Side::price_round`.
pub fn cross_px(side: Side, b: &BboView, cross: Rate, tick: i64) -> Px {
    match side {
        Side::Buy => b
            .ask_px
            .mul_rate(cross, Side::Buy.price_round())
            .round_to_tick(tick, Side::Buy.price_round()),
        Side::Sell => b
            .bid_px
            .mul_rate(Rate(-cross.0), Side::Sell.price_round())
            .round_to_tick(tick, Side::Sell.price_round()),
    }
}

/// Smallest quantity that is a multiple of both lots (pair lot of a 1:1 pair); `None` on lots ≤ 0.
pub fn pair_lot(a: i64, b: i64) -> Option<i64> {
    if a <= 0 || b <= 0 {
        return None;
    }
    let (mut x, mut y) = (a.unsigned_abs(), b.unsigned_abs());
    while y != 0 {
        (x, y) = (y, x % y);
    }
    let l = u128::from(a.unsigned_abs() / x) * u128::from(b.unsigned_abs());
    i64::try_from(l).ok()
}

pub const fn opposite(s: Side) -> Side {
    match s {
        Side::Buy => Side::Sell,
        Side::Sell => Side::Buy,
    }
}

// ----------------------------------------------------------------------------------- strategy

/// The strategy instance.
#[derive(Debug)]
pub struct Fast {
    pub acct: AccountId,
    pub p: ProbeParams,
    pub cfg: Box<[GroupCfg]>,
    pub groups: Box<[ProbeGroup]>,
    pub n: u16,
    /// `rx_tsc` of the previous `Bbo` (burst detection; not snapshotted).
    pub last_bbo_rx_tsc: Tsc,
    pub pend: [Option<Pend>; MAX_PEND],
}

/// Group fingerprint and per-leg data of a GroupTable.
pub fn build_cfg(groups: &GroupTable, rd: &hft_strategy_sdk::RefView<'_>) -> Box<[GroupCfg]> {
    let mut out = vec![GroupCfg::EMPTY; usize::from(groups.capacity().max(groups.len()))];
    for (gix, g) in groups.iter() {
        let Some(c) = out.get_mut(usize::from(gix)) else {
            continue;
        };
        c.gid = g.id;
        c.n_legs = g.n_legs.min(MAX_LEGS as u8);
        c.fp = g.fingerprint();
        for (k, l) in g.legs.iter().take(c.n()).enumerate() {
            if let (Some(i), Some(v), Some(s)) = (c.insts.get_mut(k), c.venues.get_mut(k), c.spot.get_mut(k))
            {
                *i = l.inst;
                *v = rd.inst(l.inst).venue;
                *s = crate::params::is_spot(rd, l.inst);
            }
        }
    }
    out.into_boxed_slice()
}

impl Fast {
    pub fn new(
        acct: AccountId,
        p: &ProbeParams,
        groups: &GroupTable,
        rd: &hft_strategy_sdk::RefView<'_>,
    ) -> Fast {
        let cap = usize::from(groups.capacity().max(groups.len()));
        let mut f = Fast {
            acct,
            p: p.clone(),
            cfg: build_cfg(groups, rd),
            groups: vec![ProbeGroup::default(); cap].into_boxed_slice(),
            n: groups.len(),
            last_bbo_rx_tsc: Tsc(0),
            pend: [None; MAX_PEND],
        };
        for k in 0..usize::from(f.n) {
            if let (Some(g), Some(c)) = (f.groups.get_mut(k), f.cfg.get(k)) {
                *g = fresh(c);
            }
        }
        f
    }

    /// Aligns the state by GroupId: same GroupId and fingerprint keep their state (a `mode` change
    /// restarts the group after flattening its residual); new or changed groups start empty.
    pub fn on_params(&mut self, p: &ProbeParams, groups: &GroupTable, rd: &hft_strategy_sdk::RefView<'_>) {
        let cfg = build_cfg(groups, rd);
        let mut next = vec![ProbeGroup::default(); cfg.len().max(self.groups.len())].into_boxed_slice();
        for gix in 0..groups.len() {
            let Some(c) = cfg.get(usize::from(gix)) else {
                continue;
            };
            let mut g = fresh(c);
            let old = (0..self.n).find(|&o| self.cfg.get(usize::from(o)).is_some_and(|x| x.gid == c.gid));
            if let Some(o) = old {
                let og = self.groups.get(usize::from(o)).copied().unwrap_or_default();
                if og.fp == c.fp {
                    if self.p.at(o).mode == p.at(gix).mode {
                        g = og;
                        g.flags &= !G_STICKY;
                    } else {
                        g.flags |= G_FLATTEN_NEXT;
                    }
                }
            }
            if let Some(s) = next.get_mut(usize::from(gix)) {
                *s = g;
            }
        }
        self.p = p.clone();
        self.cfg = cfg;
        self.groups = next;
        self.n = groups.len();
        self.pend = [None; MAX_PEND];
    }

    fn push_pend(&mut self, p: Pend) {
        if let Some(s) = self.pend.iter_mut().find(|s| s.is_none()) {
            *s = Some(p);
        }
    }

    /// One event (README "事件处理").
    pub fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>) {
        let now = v.now();
        self.take_outcomes(v, now, ctx);
        match *ev {
            EventRef::Bbo { inst, gix, b } => {
                self.group_event(gix, v, now, ctx, |g, cx, ctx, out| {
                    let Some(leg) = cx.cfg.leg_of(inst) else { return };
                    match cx.mode {
                        Mode::Quote => quote::on_bbo(g, cx, ctx, out, leg, &b),
                        _ if leg == 0 => pair::on_a_bbo(g, cx, ctx, out),
                        _ => {}
                    }
                });
                self.last_bbo_rx_tsc = b.rx_tsc;
            }
            EventRef::Order { gix, u } => {
                let u = *u;
                self.group_event(gix, v, now, ctx, |g, cx, ctx, out| match cx.mode {
                    Mode::Quote => quote::on_order(g, cx, ctx, out, &u),
                    _ => pair::on_order(g, cx, ctx, out, &u),
                });
            }
            EventRef::Exec { gix, u } => {
                let k = u.kind;
                self.group_event(gix, v, now, ctx, |g, cx, ctx, out| match cx.mode {
                    Mode::Quote => {}
                    Mode::Pair => pair::on_exec(g, cx, ctx, out, k),
                    Mode::Target => target::on_exec(g, cx, ctx, out, k),
                });
            }
            EventRef::External { gix, .. } => {
                if gix != GIX_NONE {
                    self.group_event(gix, v, now, ctx, flatten::on_external);
                }
            }
            EventRef::Risk { a, e } => {
                if a != self.acct {
                    return;
                }
                let e = *e;
                for gix in 0..self.n {
                    let gid = self.cfg.get(usize::from(gix)).map_or(GroupId(0), |c| c.gid);
                    let hit = match e {
                        RiskEvent::HedgeBroken { group } => group == gid,
                        RiskEvent::CapSet { source, scope } | RiskEvent::CapCleared { source, scope } => {
                            source.0 == HALTED_CAP && self.scope_hits(gix, &scope)
                        }
                        _ => false,
                    };
                    if hit {
                        self.group_event(gix, v, now, ctx, |g, cx, ctx, out| match e {
                            RiskEvent::HedgeBroken { .. } => pair::on_broken(g, cx, ctx, out),
                            RiskEvent::CapSet { .. } => g.thr.set_halted(true),
                            RiskEvent::CapCleared { .. } => g.thr.set_halted(false),
                            _ => {}
                        });
                    }
                }
            }
            EventRef::Notice { gix, n, .. } => {
                let n = *n;
                let range = if gix == GIX_NONE {
                    0..self.n
                } else {
                    gix..gix.saturating_add(1)
                };
                for x in range {
                    self.group_event(x, v, now, ctx, |g, cx, _, _| on_notice(g, cx, &n));
                }
            }
            EventRef::Timer { tag, gix, .. } => {
                self.group_event(gix, v, now, ctx, |g, cx, ctx, out| {
                    pair::on_timer(g, cx, ctx, out, tag)
                });
            }
            EventRef::Mode { gix, old, new } => {
                self.group_event(gix, v, now, ctx, |g, cx, ctx, out| {
                    on_mode(g, cx, ctx, out, old, new)
                });
            }
            _ => {}
        }
    }

    fn scope_hits(&self, gix: u16, s: &hft_strategy_sdk::CapScopeView) -> bool {
        let Some(c) = self.cfg.get(usize::from(gix)) else {
            return false;
        };
        (s.account.0 == 0 || s.account == self.acct)
            && (s.group.0 == 0 || s.group == c.gid)
            && (s.inst.0 == 0 || c.leg_of(s.inst).is_some())
            && (s.venue.0 == 0 || c.venues.iter().take(c.n()).any(|v| *v == s.venue))
    }

    /// Runs `f` for group `gix` between the common pre- and post-processing.
    fn group_event<V: Views>(
        &mut self,
        gix: u16,
        v: &V,
        now: Now,
        ctx: &mut Ctx<'_>,
        f: impl FnOnce(&mut ProbeGroup, &Cx<'_, V>, &mut Ctx<'_>, &mut Out),
    ) {
        if gix >= self.n {
            return;
        }
        let (Some(cfg), Some(g)) = (
            self.cfg.get(usize::from(gix)),
            self.groups.get_mut(usize::from(gix)),
        ) else {
            return;
        };
        let fp = self.p.at(gix);
        let cx = Cx {
            v,
            now,
            acct: self.acct,
            cfg,
            f: fp,
            mode: Mode::of(fp.mode),
            gix,
            size_margin: self.p.defaults().size_margin,
            max_consec_fail: u8::try_from(self.p.defaults().max_consec_fail).unwrap_or(u8::MAX),
            last_rx: self.last_bbo_rx_tsc,
        };
        let mut out = Out::default();
        begin(g, &cx, ctx, &mut out);
        f(g, &cx, ctx, &mut out);
        end(g, &cx, ctx);
        for p in out.pend.iter().take(usize::from(out.n)).flatten() {
            let p = *p;
            self.push_pend(p);
        }
    }

    /// Results of the previous call's intents (README "事件处理", last paragraph).
    fn take_outcomes<V: Views>(&mut self, v: &V, now: Now, ctx: &mut Ctx<'_>) {
        let pend = core::mem::replace(&mut self.pend, [None; MAX_PEND]);
        for p in pend.iter().flatten() {
            let Some(o) = ctx.outcome(p.h) else { continue };
            let p = *p;
            self.group_event(p.gix, v, now, ctx, |g, cx, ctx, out| match p.kind as u32 {
                K_PROBE => quote::on_outcome(g, cx, p.leg, o),
                K_OPEN => pair::on_open_outcome(g, cx, o),
                k if k == u32::from(K_T_OPEN) || k == u32::from(K_T_EXIT) => {
                    target::on_outcome(g, cx, ctx, out, p.kind, o);
                }
                _ => {}
            });
        }
    }

    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_u16(self.n);
        for k in 0..usize::from(self.n) {
            let (Some(g), Some(c)) = (self.groups.get(k), self.cfg.get(k)) else {
                continue;
            };
            put_group(w, g, c.n());
        }
    }

    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        if r.desc_version() > crate::DESC_VERSION {
            return Err(RestoreError::VersionUnsupported);
        }
        let n = r.get_u16()?;
        let mut got: Vec<ProbeGroup> = Vec::with_capacity(usize::from(n));
        for _ in 0..n {
            got.push(get_group(r)?);
        }
        for k in 0..usize::from(self.n) {
            let (Some(c), Some(slot)) = (self.cfg.get(k), self.groups.get_mut(k)) else {
                continue;
            };
            if let Some(g) = got.iter().find(|g| g.gid == c.gid.0 && g.fp == c.fp) {
                let mut g = *g;
                g.flags |= G_RESTORED | G_ACTIVE;
                for l in g.legs.iter_mut() {
                    if l.flags & L_INFLIGHT != 0 {
                        l.flags |= L_VERIFY;
                    }
                }
                *slot = g;
            }
        }
        self.last_bbo_rx_tsc = Tsc(0);
        self.pend = [None; MAX_PEND];
        Ok(())
    }
}

/// Intents placed while handling one group (their handles are read in the next call).
#[derive(Debug, Default)]
pub struct Out {
    pub pend: [Option<Pend>; 8],
    pub n: u8,
    /// Legs that already got a residual-flatten order in this event.
    pub flattened: u8,
}

impl Out {
    pub fn push(&mut self, gix: u16, kind: u8, leg: u8, h: IntentHandle) {
        if let Some(s) = self.pend.get_mut(usize::from(self.n)) {
            *s = Some(Pend { gix, kind, leg, h });
            self.n += 1;
        }
    }
}

/// A group's state when it first appears.
pub fn fresh(c: &GroupCfg) -> ProbeGroup {
    ProbeGroup {
        gid: c.gid.0,
        fp: c.fp,
        flags: G_ACTIVE,
        ..ProbeGroup::default()
    }
}

/// Common pre-processing of every event of a group.
fn begin<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>, out: &mut Out) {
    let ens = cx.ens();
    if g.flags & G_STARTED == 0 {
        g.flags |= G_STARTED;
        g.note_due_ens = ens.saturating_add(NOTE_PERIOD_NS);
        g.next_pair_ens = ens;
    } else if ens >= g.note_due_ens {
        flush_counts(g, cx, ctx);
        g.note_due_ens = ens.saturating_add(NOTE_PERIOD_NS);
    }
    // Probes in flight across a restart: cancel the resting ones, forget the finished ones.
    for k in 0..cx.cfg.n() {
        let Some(l) = g.legs.get_mut(k) else { continue };
        if l.flags & L_VERIFY == 0 {
            continue;
        }
        l.flags &= !L_VERIFY;
        if l.flags & L_REF == 0 {
            // Placed in the last call before the snapshot: the ref arrives with its reports.
            continue;
        }
        let st = if l.flags & L_REF != 0 {
            cx.v.order(cx.order_ref(k, l)).map(|o| o.state)
        } else {
            None
        };
        use hft_strategy_sdk::OrderStateView as S;
        match st {
            Some(S::PendingNew | S::Live | S::PendingAmend) => {
                if l.flags & L_CANCEL_SENT == 0 {
                    let _ = ctx.cancel(cx.order_ref(k, l));
                    l.flags |= L_CANCEL_SENT;
                }
            }
            Some(S::PendingCancel | S::Unknown) => {}
            _ => l.flags &= !(L_INFLIGHT | L_REF | L_ACKED | L_CANCEL_SENT | L_UNKNOWN),
        }
    }
    if g.flags & G_RESTORED != 0 {
        g.flags &= !G_RESTORED;
        if cx.mode != Mode::Quote {
            pair::after_restore(g, cx, ctx);
        }
    }
    if g.flags & G_FLATTEN_NEXT != 0 {
        g.flags &= !G_FLATTEN_NEXT;
        flatten::all_legs(g, cx, ctx, out);
    }
    // External pause lasts until every leg is flat.
    if g.legs.iter().any(|l| l.flags & L_EXTERNAL != 0) && cx.flat() {
        for l in g.legs.iter_mut() {
            l.flags &= !L_EXTERNAL;
        }
    }
    if g.phase == Phase::Blocked && g.flags & G_STICKY == 0 && cx.flat() {
        g.phase = Phase::Idle;
    }
}

/// Common post-processing: an Idle pair/target group without a pending PAIR_TICK registers one.
fn end<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>) {
    if cx.mode != Mode::Quote && g.phase == Phase::Idle && g.flags & G_TICK == 0 {
        cx.timer(ctx, g.next_pair_ens.max(cx.ens()), PAIR_TICK);
        g.flags |= G_TICK;
    }
}

/// Writes the non-zero counting Notes and clears the counters.
fn flush_counts<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, ctx: &mut Ctx<'_>) {
    if g.skip_spread != 0 || g.skip_unhealthy != 0 {
        cx.note(
            ctx,
            desc::SKIPPED,
            [
                u64::from(g.skip_spread),
                u64::from(g.skip_unhealthy),
                0,
                0,
                0,
                0,
                0,
                0,
            ],
        );
    }
    if g.gated != 0 {
        cx.note(
            ctx,
            desc::PROBE_GATED,
            [u64::from(g.gated), u64::from(g.gated_bits), 0, 0, 0, 0, 0, 0],
        );
    }
    if g.rejects != 0 {
        cx.note(
            ctx,
            desc::PROBE_REJECTED,
            [
                u64::from(g.rejects),
                u64::from(g.rej_bits),
                u64::from(g.rej_code),
                0,
                0,
                0,
                0,
                0,
            ],
        );
    }
    if g.nofill != 0 {
        cx.note(ctx, desc::PAIR_NOFILL, [u64::from(g.nofill), 0, 0, 0, 0, 0, 0, 0]);
    }
    g.skip_spread = 0;
    g.skip_unhealthy = 0;
    g.gated = 0;
    g.rejects = 0;
    g.nofill = 0;
}

/// Counts a gate reject.
pub fn count_gated(g: &mut ProbeGroup, bits: RejectBits) {
    g.gated = g.gated.saturating_add(1);
    g.gated_bits = bits.0;
}

/// Counts an exchange reject and applies the consecutive-failure breaker.
pub fn count_reject<V: Views>(
    g: &mut ProbeGroup,
    cx: &Cx<'_, V>,
    ctx: &mut Ctx<'_>,
    bits: RejectBits,
    code: u32,
) {
    g.rejects = g.rejects.saturating_add(1);
    g.rej_bits = bits.0;
    g.rej_code = code;
    let a = g
        .thr
        .on_outcome(hft_strat_lib::Outcome::Failed, cx.ens(), &cx.throttle_params());
    if a == hft_strat_lib::OutcomeAction::RequestHalt {
        let _ = ctx.request_halt(cx.gid(), 1);
        cx.note(
            ctx,
            desc::PROBE_HALTED,
            [1, u64::from(g.thr.consec_fail()), 0, 0, 0, 0, 0, 0],
        );
        g.thr.set_halted(true);
    }
}

/// Whether leg `k` may probe/open now (not Unknown, delisted, externally paused or in maintenance).
pub fn leg_paused<V: Views>(g: &ProbeGroup, cx: &Cx<'_, V>, k: usize) -> bool {
    let flags = g.legs.get(k).map_or(0, |l| l.flags);
    if flags & (L_UNKNOWN | L_DELIST | L_EXTERNAL) != 0 {
        return true;
    }
    g.maint_mask & (1 << k) != 0 && (g.maint_from..=g.maint_to).contains(&cx.now.utc_ns)
}

fn on_notice<V: Views>(g: &mut ProbeGroup, cx: &Cx<'_, V>, n: &hft_strategy_sdk::VenueNoticeView) {
    for k in 0..cx.cfg.n() {
        let hit = n.inst == cx.inst(k) || (n.inst == INST_ALL && n.venue == cx.venue(k));
        if !hit {
            continue;
        }
        match n.kind {
            NoticeKind::Delisting { .. } => {
                if let Some(l) = g.legs.get_mut(k) {
                    l.flags |= L_DELIST;
                }
            }
            NoticeKind::Maintenance { from_ns, to_ns } => {
                if (g.maint_from, g.maint_to) != (from_ns, to_ns) {
                    g.maint_from = from_ns;
                    g.maint_to = to_ns;
                    g.maint_mask = 0;
                }
                g.maint_mask |= 1 << k;
            }
            _ => {}
        }
    }
}

fn on_mode<V: Views>(
    g: &mut ProbeGroup,
    cx: &Cx<'_, V>,
    ctx: &mut Ctx<'_>,
    out: &mut Out,
    old: RunMode,
    new: RunMode,
) {
    let _ = out;
    if new != RunMode::Live {
        match (cx.mode, g.phase) {
            (_, Phase::Due) => g.phase = Phase::Idle,
            (Mode::Target, Phase::Opening) => {
                let _ = ctx.clear_target(cx.acct, cx.gid());
                let _ = g
                    .thr
                    .on_outcome(hft_strat_lib::Outcome::Gated, cx.ens(), &cx.throttle_params());
                g.phase = Phase::Holding;
                pair::register_flat(g, cx, ctx);
            }
            _ => {}
        }
        g.thr.reset_open_state();
    } else if old != RunMode::Live {
        for l in g.legs.iter_mut() {
            // A leg still in flight keeps its time (the orphan check of `quote` measures from it).
            if l.flags & L_INFLIGHT == 0 {
                l.last_probe_ens = 0;
            }
        }
        if g.phase == Phase::Idle && cx.mode != Mode::Quote {
            g.next_pair_ens = cx.ens();
            cx.timer(ctx, g.next_pair_ens, PAIR_TICK);
            g.flags |= G_TICK;
        }
    }
}

// --------------------------------------------------------------------------------- snapshot

fn put_group(w: &mut SnapshotWriter, g: &ProbeGroup, n_legs: usize) {
    w.put_u32(g.gid);
    w.put_u64(g.fp);
    g.thr.snapshot(w);
    for x in [
        g.next_pair_ens,
        g.flat_at_ens,
        g.note_due_ens,
        g.maint_from,
        g.maint_to,
        g.tgt_q,
        g.tgt_chunk,
    ] {
        w.put_i64(x);
    }
    for x in [g.rejects, g.gated, g.nofill, g.skip_spread, g.skip_unhealthy] {
        w.put_u16(x);
    }
    for x in [g.rej_bits, g.rej_code, g.gated_bits] {
        w.put_u32(x);
    }
    for x in [
        g.phase as u8,
        g.attempt,
        g.next_dir,
        g.retry_tag,
        g.maint_mask,
        g.flags & !G_RESTORED,
    ] {
        w.put_u8(x);
    }
    w.put_u8(n_legs as u8);
    for l in g.legs.iter().take(n_legs) {
        w.put_i64(l.last_probe_ens);
        w.put_u32(l.rest_slot);
        w.put_u32(l.rest_gen);
        w.put_u8(l.flags);
        w.put_u8(u8::from(l.next_sell));
    }
}

fn get_group(r: &mut SnapshotReader<'_>) -> Result<ProbeGroup, RestoreError> {
    let mut g = ProbeGroup {
        gid: r.get_u32()?,
        fp: r.get_u64()?,
        ..ProbeGroup::default()
    };
    g.thr.restore(r)?;
    g.next_pair_ens = r.get_i64()?;
    g.flat_at_ens = r.get_i64()?;
    g.note_due_ens = r.get_i64()?;
    g.maint_from = r.get_i64()?;
    g.maint_to = r.get_i64()?;
    g.tgt_q = r.get_i64()?;
    g.tgt_chunk = r.get_i64()?;
    g.rejects = r.get_u16()?;
    g.gated = r.get_u16()?;
    g.nofill = r.get_u16()?;
    g.skip_spread = r.get_u16()?;
    g.skip_unhealthy = r.get_u16()?;
    g.rej_bits = r.get_u32()?;
    g.rej_code = r.get_u32()?;
    g.gated_bits = r.get_u32()?;
    g.phase = Phase::of(r.get_u8()?).ok_or(RestoreError::BadValue)?;
    g.attempt = r.get_u8()?;
    g.next_dir = r.get_u8()?;
    g.retry_tag = r.get_u8()?;
    g.maint_mask = r.get_u8()?;
    g.flags = r.get_u8()?;
    let n = usize::from(r.get_u8()?);
    if n > MAX_LEGS {
        return Err(RestoreError::BadValue);
    }
    for l in g.legs.iter_mut().take(n) {
        l.last_probe_ens = r.get_i64()?;
        l.rest_slot = r.get_u32()?;
        l.rest_gen = r.get_u32()?;
        l.flags = r.get_u8()?;
        l.next_sell = r.get_u8()? != 0;
    }
    Ok(g)
}

/// Exec update kinds re-exported for the handlers.
pub type ExecKind = ExecUpdateKind;
/// Outcome type re-exported for the handlers.
pub type Outc = IntentOutcome;
