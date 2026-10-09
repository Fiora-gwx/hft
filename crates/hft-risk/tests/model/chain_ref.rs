//! Column-by-column reference of the README check-chain table over R0 (RSK-045). Shares no
//! product code: the R0 facts and P0 limits are restated here.

use crate::common::{A, A1, A2, B, B1, B2, C, C1, G1, G2, G3, GS, Mv};
use hft_event::OrderFlags;
use hft_risk::{IntentView, Origin, RiskView, Scope};
use hft_strategy_sdk::IntentClass;
use hft_types::{AccountId, GROUP_NONE, GroupId, InstId, RejectBits, Side, Tif, VenueId};

/// A cap-producing option applied to the world (product side applies the matching input).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Opt {
    KillGlobal {
        hard: bool,
    },
    KillAcct {
        a: u8,
        hard: bool,
        flatten: bool,
    },
    KillVenue {
        v: VenueId,
        a: u8,
    },
    Draining,
    Daily {
        a: u8,
    },
    Recon {
        a: u8,
        v: VenueId,
    },
    Unknown {
        a: u8,
        v: VenueId,
    },
    /// `Broken{a, g}` with the account's `HedgeFailure{a}`.
    Broken {
        a: u8,
        g: GroupId,
    },
    Margin {
        a: u8,
    },
    Storage,
    Cfg {
        a: u8,
        v: VenueId,
    },
    Recovery {
        a: u8,
    },
    Halted {
        s: u8,
        a: u8,
        g: GroupId,
    },
    /// inst 0 = the whole venue.
    Delist {
        v: VenueId,
        i: InstId,
    },
    ClockBad,
    /// Desired mode CloseOnly of a cell.
    CloseCell {
        s: u8,
        a: u8,
        g: GroupId,
    },
}

/// Environment of one evaluation besides the caps.
#[derive(Clone, Debug)]
pub struct Env<'a> {
    pub mv: &'a Mv,
    pub caps: &'a [Opt],
    pub now_tsc: u64,
    /// Tokens committed on (venue, account) in the current 10 s window.
    pub used: &'a [((VenueId, u8), i64)],
    pub backoff: &'a [(VenueId, u8)],
    /// Latest `on_legs` of (account, group): (naked_ratio raw, naked_notional raw).
    pub legs: &'a [((u8, GroupId), (i64, i64))],
    /// Known leverage raw of (account, inst).
    pub lev: &'a [((u8, InstId), i64)],
}

struct Inst {
    venue: VenueId,
    tick: i64,
    lot: i64,
    class: usize,
    perp: bool,
}

fn inst(i: InstId) -> Option<Inst> {
    let mk = |venue, tick, lot, class, perp| {
        Some(Inst {
            venue,
            tick,
            lot,
            class,
            perp,
        })
    };
    match i {
        x if x == A1 => mk(A, 1, 10_000, 0, false),
        x if x == A2 => mk(A, 1, 100_000, 1, false),
        x if x == B1 => mk(B, 10, 100_000, 0, true),
        x if x == B2 => mk(B, 1, 1_000_000, 1, true),
        x if x == C1 => mk(C, 10, 100_000, 0, true),
        _ => None,
    }
}

/// `(max_px_dev, ioc_max_cross, rr_max_cross)` of P0's classes.
const BANDS: [(i64, i64, i64); 2] = [
    (30_000_000, 5_000_000, 10_000_000),
    (50_000_000, 10_000_000, 20_000_000),
];
const ONE: i128 = 1_000_000_000;
const NOTIONAL: i64 = 1_000_000;
const BTC: i64 = 100_000_000;

fn venues_of(a: u8) -> &'static [VenueId] {
    match a {
        1 => &[A, B, C],
        2 | 3 => &[A, B],
        _ => &[],
    }
}
fn live(a: u8) -> bool {
    a == 1 || a == 2
}
fn max_pos(a: u8, g: GroupId) -> i64 {
    if a == 1 && g == G3 { 20 * BTC } else { BTC }
}
fn known_group(a: u8, g: GroupId) -> bool {
    matches!((a, g), (1, x) if x == G1 || x == G3) || (a == 2 && g == G2) || (a == 3 && g == GS)
}

const DRAIN: u8 = 0;
const KILL: u8 = 1;
const DAILY: u8 = 2;
const RECON: u8 = 3;
const OTHER: u8 = 4;

/// Sources (as above) covering an Open of (s, a, g) on (v, i).
fn cell_sources(caps: &[Opt], s: u8, a: u8, g: GroupId, v: VenueId, i: InstId) -> Vec<u8> {
    let mut out = Vec::new();
    for c in caps {
        let src = match *c {
            Opt::KillGlobal { .. } => Some(KILL),
            Opt::KillAcct { a: x, .. } if x == a => Some(KILL),
            Opt::KillVenue { v: x, a: y } if x == v && (y == a || y == 0) => Some(KILL),
            Opt::Draining => Some(DRAIN),
            Opt::Daily { a: x } if x == a => Some(DAILY),
            Opt::Recon { a: x, v: y } if x == a && y == v => Some(RECON),
            Opt::Unknown { a: x, v: y } | Opt::Cfg { a: x, v: y } if x == a && y == v => Some(OTHER),
            Opt::Broken { a: x, g: y } if x == a && (y == g || g == GROUP_NONE) => Some(OTHER),
            // HedgeFailure of a Broken account.
            Opt::Broken { a: x, .. } if x == a => Some(OTHER),
            Opt::Margin { a: x } | Opt::Recovery { a: x } if x == a => Some(OTHER),
            Opt::Storage | Opt::ClockBad => Some(OTHER),
            Opt::Halted { s: x, a: y, g: z } | Opt::CloseCell { s: x, a: y, g: z }
                if x == s && y == a && z == g =>
            {
                Some(OTHER)
            }
            Opt::Delist { v: x, i: y } if x == v && (y.0 == 0 || y == i) => Some(OTHER),
            _ => None,
        };
        out.extend(src);
    }
    out
}

/// Sources of `venue_level(a, v)` and `inst_blocked(v, i)` (link leg).
fn link_sources(caps: &[Opt], a: u8, v: VenueId, i: InstId) -> Vec<u8> {
    let mut out = Vec::new();
    for c in caps {
        let src = match *c {
            Opt::KillGlobal { .. } => Some(KILL),
            Opt::KillAcct { a: x, .. } if x == a => Some(KILL),
            Opt::KillVenue { v: x, a: y } if x == v && (y == a || y == 0) => Some(KILL),
            Opt::Draining => Some(DRAIN),
            Opt::Daily { a: x } if x == a => Some(DAILY),
            Opt::Recon { a: x, v: y } if x == a && y == v => Some(RECON),
            Opt::Unknown { a: x, v: y } | Opt::Cfg { a: x, v: y } if x == a && y == v => Some(OTHER),
            Opt::Broken { a: x, .. } if x == a => Some(OTHER),
            Opt::Margin { a: x } | Opt::Recovery { a: x } if x == a => Some(OTHER),
            Opt::Storage | Opt::ClockBad => Some(OTHER),
            Opt::Delist { v: x, i: y } if x == v && (y.0 == 0 || y == i) => Some(OTHER),
            _ => None,
        };
        out.extend(src);
    }
    out
}

fn bit_of(srcs: &[u8]) -> Option<RejectBits> {
    let m = srcs.iter().copied().min()?;
    Some(match m {
        DRAIN => RejectBits::DRAINING,
        KILL => RejectBits::KILLED,
        DAILY => RejectBits::DAILY_LOSS,
        RECON => RejectBits::RECON_DEVIATION,
        _ => RejectBits::CLOSE_ONLY,
    })
}

fn hard(caps: &[Opt], a: u8) -> bool {
    caps.iter().any(|c| match *c {
        Opt::KillGlobal { hard } => hard,
        Opt::KillAcct { a: x, hard, .. } => hard && x == a,
        _ => false,
    })
}

fn quote_live(env: &Env<'_>, i: InstId) -> bool {
    let b = env.mv.bbo(i);
    b.conn_up
        && !b.resync
        && b.conn_live_tsc.0 != 0
        && env.now_tsc.saturating_sub(b.conn_live_tsc.0) <= 2_000 * 1_000_000
}

/// Adverse cross of a price relative to a base (raw), compared with `cap` (Rate raw):
/// `(d / base) ≤ cap`.
fn le_rate(d: i128, base: i128, cap: i64) -> bool {
    d * ONE <= i128::from(cap) * base
}

fn tokens_left(env: &Env<'_>, v: VenueId, a: u8) -> i64 {
    let used = env.used.iter().find(|(k, _)| *k == (v, a)).map_or(0, |(_, u)| *u);
    100 - used
}

/// The first failing bit of the README table, or `None` when every check passes.
pub fn first_fail(it: &IntentView, env: &Env<'_>) -> Option<RejectBits> {
    let rr = it.flags & OrderFlags::RISK_REDUCING.0 != 0;
    let smoke = it.origin == Origin::Command && it.flags & OrderFlags::SMOKE.0 != 0;
    let gated = !rr;
    let linked = it.link_venue.0 != 0;
    let a = it.account.0;
    let la = it.link_account.0;
    let mv = env.mv;

    // #0
    let Some(f) = inst(it.inst) else {
        return Some(RejectBits::INVALID);
    };
    if it.qty.0 <= 0 || it.px.0 <= 0 || f.venue != it.venue || !(1..=3).contains(&a) {
        return Some(RejectBits::INVALID);
    }
    if it.qty.0 % f.lot != 0 || it.px.0 % f.tick != 0 || (smoke && !live(a)) {
        return Some(RejectBits::INVALID);
    }
    let issuer = match it.origin {
        Origin::Strategy | Origin::Command => it.strategy.0,
        Origin::Exec | Origin::Oms => {
            if it.group == GROUP_NONE {
                it.strategy.0
            } else {
                it.group.strategy().0
            }
        }
    };
    if issuer != a {
        return Some(RejectBits::NOT_BOUND);
    }
    // #1
    if hard(env.caps, a) || (linked && hard(env.caps, la)) {
        return Some(RejectBits::HARD);
    }
    // #2
    if gated && env.caps.contains(&Opt::ClockBad) {
        return Some(RejectBits::CLOCK);
    }
    // #3
    if gated {
        let srcs = if smoke {
            let mut s = Vec::new();
            for c in env.caps {
                match *c {
                    Opt::Draining => s.push(DRAIN),
                    Opt::KillGlobal { .. } => s.push(KILL),
                    Opt::KillAcct { a: x, .. } if x == a => s.push(KILL),
                    Opt::KillVenue { v, a: y } if v == it.venue && (y == a || y == 0) => s.push(KILL),
                    Opt::Delist { v, i } if v == it.venue && (i.0 == 0 || i == it.inst) => s.push(OTHER),
                    _ => {}
                }
            }
            s
        } else {
            let mut s = cell_sources(env.caps, it.strategy.0, a, it.group, it.venue, it.inst);
            if linked {
                s.extend(link_sources(env.caps, la, it.link_venue, it.link_inst));
            }
            s
        };
        if let Some(b) = bit_of(&srcs) {
            return Some(b);
        }
    }
    // #4
    if gated {
        if !mv.venue_ready(it.venue, it.account)
            || (linked && !mv.venue_ready(it.link_venue, it.link_account))
        {
            return Some(RejectBits::VENUE_NOT_READY);
        }
        if mv.group_has_unknown(it.account, it.group) {
            return Some(RejectBits::UNKNOWN_PENDING);
        }
    }
    // #5
    let b = mv.bbo(it.inst);
    if b.bid_px.0 <= 0 || b.ask_px.0 <= 0 {
        return Some(RejectBits::NO_MID);
    }
    if !quote_live(env, it.inst) {
        return Some(RejectBits::STALE_MID);
    }
    if !smoke {
        let (dev, ioc, rrc) = BANDS[f.class];
        let mid2 = i128::from(b.bid_px.0) + i128::from(b.ask_px.0);
        let px2 = 2 * i128::from(it.px.0);
        let ok = if gated {
            let band = le_rate((px2 - mid2).abs(), mid2, dev);
            let ioc_ok = it.tif != Tif::Ioc
                || match it.side {
                    Side::Buy => le_rate(
                        i128::from(it.px.0) - i128::from(b.ask_px.0),
                        i128::from(b.ask_px.0),
                        ioc,
                    ),
                    Side::Sell => le_rate(
                        i128::from(b.bid_px.0) - i128::from(it.px.0),
                        i128::from(b.bid_px.0),
                        ioc,
                    ),
                };
            band && ioc_ok
        } else {
            let adverse = match it.side {
                Side::Buy => px2 - mid2,
                Side::Sell => mid2 - px2,
            };
            le_rate(adverse, mid2, rrc)
        };
        if !ok {
            return Some(RejectBits::PX_BAND);
        }
    }
    if linked && !quote_live(env, it.link_inst) {
        return Some(RejectBits::LINK_STALE);
    }
    // #6
    let nt = i128::from(it.notional.0);
    match it.class {
        IntentClass::Open => {
            if nt > i128::from(10_000 * NOTIONAL) {
                return Some(RejectBits::NOTIONAL_ORDER);
            }
            let sum = |s: Scope| {
                i128::from(mv.held_notional(it.account, s).0)
                    + i128::from(mv.open_notional(it.account, s).0)
                    + nt
            };
            if sum(Scope::Group(it.group)) > i128::from(50_000 * NOTIONAL) {
                return Some(RejectBits::NOTIONAL_GROUP);
            }
            let eq = mv.equity(it.account);
            let mut lim = i128::from(100_000 * NOTIONAL);
            if eq.valid {
                lim = lim.min(i128::from(eq.total.0));
            }
            if sum(Scope::Account) > lim {
                return Some(RejectBits::NOTIONAL_ACCOUNT);
            }
            if sum(Scope::Venue(it.venue)) > i128::from(80_000 * NOTIONAL) {
                return Some(RejectBits::NOTIONAL_VENUE);
            }
        }
        IntentClass::Hedge => {
            if it.qty.0 > it.hedge_cap_qty.0 || nt > i128::from(20_000 * NOTIONAL) {
                return Some(RejectBits::HEDGE_LIMIT);
            }
        }
        IntentClass::Reduce | IntentClass::Flatten => {
            if nt > i128::from(10_000 * NOTIONAL) {
                return Some(RejectBits::NOTIONAL_ORDER);
            }
        }
    }
    // #7
    let pos = i128::from(mv.pos(it.account, it.inst).0);
    let q = i128::from(it.qty.0);
    let gmax = |acct: u8, g: GroupId| {
        if known_group(acct, g) {
            i128::from(max_pos(acct, g))
        } else {
            i128::from(BTC)
        }
    };
    let dir_ok = |pos: i128, open: i128, q: i128, side: Side, max: i128| match side {
        Side::Buy => pos + open + q <= max,
        Side::Sell => open + q - pos <= max,
    };
    match it.class {
        IntentClass::Open => {
            let open = i128::from(mv.open_qty(it.account, it.inst, it.side).0);
            if !dir_ok(pos, open, q, it.side, gmax(a, it.group)) {
                return Some(RejectBits::POSITION);
            }
            if linked {
                let lp = i128::from(mv.pos(it.link_account, it.link_inst).0);
                let lo = i128::from(mv.open_qty(it.link_account, it.link_inst, it.link_side).0);
                if !dir_ok(
                    lp,
                    lo,
                    i128::from(it.link_qty.0),
                    it.link_side,
                    gmax(la, it.group),
                ) {
                    return Some(RejectBits::POSITION);
                }
            }
        }
        IntentClass::Hedge => {
            if it.flags & OrderFlags::REDUCE_ONLY.0 != 0 {
                let pend = i128::from(mv.pending_reduce_immediate(it.account, it.inst, it.side).0);
                if q > pos.abs() - pend {
                    return Some(RejectBits::REDUCE_INVALID);
                }
            } else {
                let open = i128::from(mv.open_qty_immediate(it.account, it.inst, it.side).0);
                if !dir_ok(pos, open, q, it.side, gmax(a, it.group)) {
                    return Some(RejectBits::POSITION);
                }
            }
        }
        IntentClass::Flatten => {
            let pend = i128::from(mv.pending_reduce_immediate(it.account, it.inst, it.side).0);
            if q > pos.abs() - pend {
                return Some(RejectBits::REDUCE_INVALID);
            }
        }
        IntentClass::Reduce => {
            let pend = i128::from(mv.pending_reduce(it.account, it.inst, it.side).0);
            if q > pos.abs() - pend {
                return Some(RejectBits::REDUCE_INVALID);
            }
        }
    }
    // #7b
    if linked && it.class == IntentClass::Open {
        let ln = i128::from(it.link_notional.0);
        if ln > i128::from(20_000 * NOTIONAL) {
            return Some(RejectBits::HEDGE_LIMIT);
        }
        let buf = ONE + 50_000_000;
        let Some(lf) = inst(it.link_inst) else {
            return Some(RejectBits::LINK_FUNDS);
        };
        let ok = if lf.perp {
            let lev = env
                .lev
                .iter()
                .find(|(k, _)| *k == (la, it.link_inst))
                .map(|(_, l)| *l);
            match (lev, mv.avail_margin(it.link_account, it.link_venue)) {
                (Some(l), Some(av)) if l > 0 => i128::from(av.0) * i128::from(l) >= ln * buf,
                _ => false,
            }
        } else {
            match it.link_side {
                Side::Buy => mv
                    .avail_quote(it.link_account, it.link_venue, hft_types::AssetId(2))
                    .is_some_and(|av| i128::from(av.0) * ONE >= ln * buf),
                Side::Sell => {
                    i128::from(mv.pos(it.link_account, it.link_inst).0)
                        - i128::from(mv.pending_reduce(it.link_account, it.link_inst, Side::Sell).0)
                        >= i128::from(it.link_qty.0)
                }
            }
        };
        if !ok {
            return Some(RejectBits::LINK_FUNDS);
        }
    }
    // #8 (inst buckets are never near empty in the generated states)
    let need = if rr { 1 } else { 21 };
    if env.backoff.contains(&(it.venue, a)) {
        return Some(RejectBits::RATE_BACKOFF);
    }
    if tokens_left(env, it.venue, a) < need {
        return Some(RejectBits::RATE);
    }
    if linked {
        if env.backoff.contains(&(it.link_venue, la)) {
            return Some(RejectBits::RATE_BACKOFF);
        }
        if tokens_left(env, it.link_venue, la) < 21 {
            return Some(RejectBits::RATE);
        }
    }
    // #9
    if gated
        && !smoke
        && let Some((_, (ratio, nn))) = env.legs.iter().find(|(k, _)| *k == (a, it.group))
        && *ratio > 500_000_000
        && *nn >= 100 * NOTIONAL
    {
        return Some(RejectBits::EXPOSURE_GATE);
    }
    let _ = (venues_of(a), AccountId(0), B2);
    None
}
