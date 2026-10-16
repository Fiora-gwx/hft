//! Property tests: EXE-020, 024, 050, 129, 133, 187, 188, 201, 261, 368, 417, 418, 433, 442, 462.
//!
//! The random world (generator "同 EXE-050"): groups g7 (S spot on A, P perp on B) and g8 (P), an
//! ungrouped P4 perp on B, account a1; every order gets a place result at once (Placed or a gated
//! reject); placed orders later fill fully, partially, not at all or go Unknown first; external
//! fills, link notices, risk events, force/flatten, gate releases, timers, clock and quote moves.
//! Mock views follow every fill (legs, group positions, inventory, ungrouped perp).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;
use common::*;

use hft_event::ExecDecision;
use hft_exec::decision::{action, autofix, engine, external, table, unpack};
use hft_exec::*;
use hft_strategy_sdk::{
    ExecSpec, ExecUpdateKind, ExternalCause, GroupPosView, IntentClass, InventoryView, LegPos, MarginLevel,
    OrderIntent, OrderRef, OrderUpdate, OrderUpdateKind, PerpPosView, TargetIntent, TargetOrigin,
    TargetState, Views,
};
use hft_types::{
    CapSource, GROUP_ALL, GroupId, InstId, Notional, Px, Qty, Rate, RejectBits, RunMode, Side, Tsc,
};
use proptest::prelude::*;

// ------------------------------------------------------------------------------------ world

#[derive(Copy, Clone, Debug)]
struct Open {
    o: ExecOrder,
    r: OrderRef,
    unknown: bool,
    cum: i64,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum Entry {
    Single,
    Multi,
}

/// Which checks run.
#[derive(Copy, Clone, Default)]
struct Checks {
    caps20: bool,
    inv50: bool,
    hedge201: bool,
    af261: bool,
    lock368: bool,
    done417: bool,
    cold418: bool,
    codes433: bool,
    timers442: bool,
}

struct World {
    fx: Fx,
    open: Vec<Open>,
    to_cancel: Vec<OrderRef>,
    timers: Vec<(u32, u32, Tsc)>,
    slot: u32,
    ms: u64,
    ck: Checks,
    trace: Rec,
    // trackers
    done_since_set: Vec<(u32, u32)>,
    af_terminal: Vec<(u32, u32)>,
    ext_timed: Vec<u32>,
    af_running: Vec<u32>,
    eng_unreached: Vec<u32>,
    restore_continuations: bool,
}

const GROUPS: [GroupId; 2] = [G7, G8];

fn leg_of(g: GroupId, i: InstId) -> usize {
    if g == G7 && i == S {
        0
    } else if g == G7 {
        1
    } else {
        0
    }
}

impl World {
    fn new(capped: bool, ck: Checks) -> World {
        let mut fx = Fx::new();
        if capped {
            fx.out = ExecOut::with_caps(OutCaps::README);
        }
        World {
            fx,
            open: Vec::new(),
            to_cancel: Vec::new(),
            timers: Vec::new(),
            slot: 0,
            ms: 0,
            ck,
            trace: Rec::default(),
            done_since_set: Vec::new(),
            af_terminal: Vec::new(),
            ext_timed: Vec::new(),
            af_running: Vec::new(),
            eng_unreached: Vec::new(),
            restore_continuations: false,
        }
    }

    fn net(&self, g: GroupId, leg: usize) -> i64 {
        if g == GNONE {
            return hft_strategy_sdk::PortfolioRead::perp(&Views::portfolio(&self.fx.v.m, A1), B, P4)
                .qty
                .0;
        }
        Views::legs(&self.fx.v.m, A1, g).leg[leg].net.0
    }

    fn set_net(&mut self, g: GroupId, leg: usize, x: i64) {
        let m = &mut self.fx.v.m;
        if g == GNONE {
            hft_strategy_sdk::testing::MockViews::portfolio(m, A1, |p| {
                p.perp(
                    B,
                    P4,
                    PerpPosView {
                        qty: Qty(x),
                        entry_px: Px(0),
                        upnl: Notional(0),
                        liq_px: Px(0),
                        adl_quantile: 0,
                    },
                );
            });
            return;
        }
        let mut l = *Views::legs(m, A1, g);
        l.leg[leg].net = Qty(x);
        hft_strategy_sdk::testing::MockViews::legs(m, A1, g, l);
        let mut gp = GroupPosView::EMPTY;
        gp.n_legs = l.n_legs;
        for k in 0..usize::from(l.n_legs) {
            gp.legs[k] = LegPos {
                qty: l.leg[k].net,
                entry_px: Px(0),
                upnl: Notional(0),
            };
        }
        hft_strategy_sdk::testing::MockViews::group_pos(m, A1, g, gp);
        if g == G7 && leg == 0 {
            let q = Qty(x.max(0));
            hft_strategy_sdk::testing::MockViews::portfolio(m, A1, |p| {
                p.inventory_of(A, G7, q);
                p.inventory(
                    A,
                    BASE,
                    InventoryView {
                        total: q,
                        assigned: q,
                        unassigned: Qty(0),
                        baseline: Qty(0),
                    },
                );
            });
        }
    }

    /// Mirrors the open orders into the legs view (`inflight_immediate`, `has_unknown`).
    fn sync_infl(&mut self) {
        for g in GROUPS {
            let mut l = *Views::legs(&self.fx.v.m, A1, g);
            let mut unknown = false;
            for k in 0..4 {
                l.leg[k].inflight_immediate = Qty(0);
            }
            for x in &self.open {
                if x.o.intent.group != g {
                    continue;
                }
                let leg = leg_of(g, x.o.intent.inst);
                let rest = x.o.intent.qty.0 - x.cum;
                let sgn = if x.o.intent.side == Side::Buy { rest } else { -rest };
                l.leg[leg].inflight_immediate = Qty(l.leg[leg].inflight_immediate.0 + sgn);
                unknown |= x.unknown;
            }
            l.has_unknown = unknown;
            hft_strategy_sdk::testing::MockViews::legs(&mut self.fx.v.m, A1, g, l);
        }
    }

    fn now(&mut self) -> hft_strategy_sdk::Now {
        self.fx.now()
    }

    /// Takes the output of one call, places new orders at once, records and checks.
    fn take(&mut self, e: Entry, salt: u16) -> bool {
        let r = Rec::take(&mut self.fx.out);
        let cont = r
            .timers
            .iter()
            .any(|t| !t.cancel && t.kind() == Some(ExecTimerKind::Continue));
        self.check(e, &r);
        self.timers_apply(&r);
        let orders = r.orders.clone();
        let cancels = r.cancels.clone();
        self.trace.extend(r);
        self.to_cancel.extend(cancels);
        for o in &orders {
            // Keyed by the order (not its position in the batch: capacity stops split batches).
            let pick = (usize::from(salt) + usize::from(o.id.cix) * 7 + usize::from(o.id.n) * 3) % 10;
            let now = self.now();
            if pick == 0 {
                self.fx.ex.on_place_result(
                    o.id,
                    PlaceResult::Rejected(RejectBits::RATE),
                    now,
                    &mut self.fx.out,
                );
            } else if pick == 1 {
                self.fx.ex.on_place_result(
                    o.id,
                    PlaceResult::Rejected(RejectBits::PX_BAND),
                    now,
                    &mut self.fx.out,
                );
            } else {
                self.slot += 1;
                let r = OrderRef::new(o.intent.venue, o.intent.account, self.slot, 1);
                self.fx
                    .ex
                    .on_place_result(o.id, PlaceResult::Placed(r), now, &mut self.fx.out);
                self.open.push(Open {
                    o: *o,
                    r,
                    unknown: false,
                    cum: 0,
                });
                self.sync_infl();
            }
            let r = Rec::take(&mut self.fx.out);
            self.check(Entry::Single, &r);
            self.timers_apply(&r);
            self.to_cancel.extend(r.cancels.iter().copied());
            assert!(r.orders.is_empty());
            self.trace.extend(r);
        }
        cont
    }

    fn timers_apply(&mut self, r: &Rec) {
        for t in &r.timers {
            if self.ck.timers442 {
                assert_ne!(t.group, GROUP_ALL, "EXE-442");
                assert!(t.kind().is_some(), "EXE-442");
            }
            self.timers.retain(|x| !(x.0 == t.timer_id && x.1 == t.group.0));
            if !t.cancel {
                self.timers.push((t.timer_id, t.group.0, t.at));
            }
        }
    }

    fn advance(&mut self, salt: u16) {
        let now = self.now();
        self.fx.ex.advance(&self.fx.v, now, &mut self.fx.out);
        let mut cont = self.take(Entry::Multi, salt);
        // The Continue timer (capacity stop) fires before any other input.
        for _ in 0..64 {
            if !cont {
                break;
            }
            if self.restore_continuations {
                let snap = self.fx.snap();
                self.fx.ex = self.fx.restore(&snap);
                assert_eq!(self.fx.snap(), snap, "continuation state roundtrip");
            }
            self.timers.retain(|t| t.0 != 4);
            let now = self.now();
            self.fx.ex.on_timer(4, now, &mut self.fx.out);
            self.fx.ex.advance(&self.fx.v, now, &mut self.fx.out);
            cont = self.take(Entry::Multi, salt);
        }
        self.post_step();
    }

    fn target(&self, g: GroupId, x: u16) -> TargetIntent {
        let q = ["0", "1", "3", "5", "0.5"][usize::from(x % 5)];
        let neg = format!("-{q}");
        let mut t = if g == G7 {
            if x & 0x40 != 0 {
                taker(A1, G7, &[(0, q), (1, &neg)])
            } else {
                t0(q, &neg)
            }
        } else {
            let p = if x & 0x20 != 0 { q.to_string() } else { neg };
            t1(&p)
        };
        if x & 0x100 != 0 {
            t.urgency = hft_strategy_sdk::Urgency::Urgent;
        }
        t
    }

    fn fill(&mut self, x: u16) {
        if self.open.is_empty() {
            return;
        }
        let k = usize::from(x) % self.open.len();
        let mut op = self.open[k];
        let must_cancel = self.to_cancel.contains(&op.r);
        let lot = self.fx.rd.inst(op.o.intent.inst).lot.max(1);
        let rest = op.o.intent.qty.0 - op.cum;
        let mode = if must_cancel { 1 } else { (x >> 4) % 5 };
        let (d, done, unknown) = match mode {
            0 => (rest, true, false),
            1 => (0, true, false),
            2 => ((rest / 2 / lot) * lot, false, false),
            3 if !op.unknown => (0, false, true),
            _ => (rest, true, false),
        };
        let now = self.now();
        let g = op.o.intent.group;
        if unknown {
            op.unknown = true;
            self.open[k] = op;
            self.sync_infl();
            let u = OrderUpdate {
                r: op.r,
                group: g,
                tag: 0,
                kind: OrderUpdateKind::Unknown,
            };
            self.fx.ex.on_order_update(&u, &self.fx.v, now, &mut self.fx.out);
            self.take(Entry::Single, x);
            return;
        }
        if d > 0 {
            let leg = leg_of(g, op.o.intent.inst);
            let s = if op.o.intent.side == Side::Buy { d } else { -d };
            let n = self.net(g, leg);
            self.set_net(g, leg, n + s);
        }
        op.cum += d;
        let kind = if d > 0 || !done {
            OrderUpdateKind::Filled {
                d: Qty(d),
                px: op.o.intent.px,
                fee: 0,
                fee_asset: USDT,
                cum: Qty(op.cum),
                done,
            }
        } else {
            OrderUpdateKind::Cancelled
        };
        let u = OrderUpdate {
            r: op.r,
            group: g,
            tag: 0,
            kind,
        };
        if done {
            self.open.remove(k);
            self.to_cancel.retain(|r| *r != op.r);
        } else {
            self.open[k] = op;
        }
        self.sync_infl();
        self.fx.ex.on_order_update(&u, &self.fx.v, now, &mut self.fx.out);
        self.take(Entry::Single, x);
    }

    /// The sequencer fires every due timer (and advances) before the next input.
    fn fire_due(&mut self, x: u16) {
        for _ in 0..16 {
            let now = Views::now(&self.fx.v.m).tsc;
            let Some(k) = self.timers.iter().position(|t| t.2.0 <= now.0) else {
                return;
            };
            let (id, grp, _) = self.timers.remove(k);
            let now = self.now();
            self.fx
                .ex
                .on_timer(u64::from(id) | (u64::from(grp) << 32), now, &mut self.fx.out);
            let e = if grp == 0 { Entry::Multi } else { Entry::Single };
            self.take(e, x);
            self.advance(x);
        }
    }

    fn step(&mut self, op: u8, x: u16) {
        self.fire_due(x);
        let g = GROUPS[usize::from(x) % 2];
        match op {
            0 | 1 => {
                let t = self.target(g, x >> 1);
                let now = self.now();
                let _ = self
                    .fx
                    .ex
                    .set_target(I1, &t, false, &self.fx.v, now, &mut self.fx.out);
                self.take(Entry::Single, x);
            }
            2 => {
                let now = self.now();
                self.fx.ex.clear_target(I1, A1, g, now, &mut self.fx.out);
                self.take(Entry::Single, x);
            }
            3..=5 => self.fill(x),
            6 => {
                let first = self.open.iter().find(|o| o.o.intent.group == G7).map(|o| o.r);
                let mut ph =
                    [LinkPhase::Hedging, LinkPhase::Done, LinkPhase::Broken][usize::from(x >> 2) % 3];
                // A link is Done only once its first order is terminal.
                if ph == LinkPhase::Done && first.is_some() {
                    ph = LinkPhase::Hedging;
                }
                let mut l = link(u32::from(x % 4) + 1, A1, G7, ph, LinkBrokenCause::Deadline);
                if ph != LinkPhase::Broken {
                    l.broken = LinkBrokenCause::None;
                }
                if let Some(r) = first {
                    l.first = r;
                }
                let now = self.now();
                self.fx.ex.on_link(&l, now, &mut self.fx.out);
                self.take(Entry::Single, x);
            }
            7 => {
                let cause = [
                    ExternalCause::Liquidation,
                    ExternalCause::Adl,
                    ExternalCause::Manual,
                    ExternalCause::Unknown,
                ][usize::from(x >> 2) % 4];
                let (leg, inst, venue) = if g == G7 { (1u8, P, B) } else { (0u8, P, B) };
                let side = if x & 0x200 != 0 { Side::Buy } else { Side::Sell };
                let q = Qty(i64::from((x >> 4) % 5 + 1) * 10_000_000);
                let mut f = xfill(g, leg, inst, venue, side, "0.1", cause);
                f.qty = q;
                let n = self.net(g, usize::from(leg));
                self.set_net(
                    g,
                    usize::from(leg),
                    if side == Side::Buy { n + q.0 } else { n - q.0 },
                );
                let now = self.now();
                let _ = self.fx.ex.on_external(A1, &f, &self.fx.v, now, &mut self.fx.out);
                self.take(Entry::Single, x);
            }
            8 => {
                let e = match (x >> 2) % 9 {
                    0 => ms_margin(MarginLevel::L1),
                    1 => ms_margin(MarginLevel::L2),
                    2 => ms_margin(MarginLevel::L3),
                    3 => ms_margin(MarginLevel::None),
                    4 => cap_set(CapSource::BROKEN, A1, g),
                    5 => cap_clr(CapSource::BROKEN, A1, g),
                    6 => cap_set(CapSource::REORG, A1, g),
                    7 => cap_clr(CapSource::REORG, A1, g),
                    _ => cap_clr(CapSource::KILL, A1, GroupId(0)),
                };
                let now = self.now();
                self.fx.ex.on_risk(A1, &e, now);
                self.take(Entry::Multi, x);
            }
            9 => {
                let now = self.now();
                if x & 1 == 0 {
                    self.fx
                        .ex
                        .force_deleverage(A1, B, rate("0.2"), now, &mut self.fx.out);
                } else if x % 16 == 1 {
                    self.fx.ex.engine_flatten(A1, now, &mut self.fx.out);
                } else {
                    self.fx.ex.clear_account(A1, now, &mut self.fx.out);
                }
                self.take(Entry::Multi, x);
            }
            10 => {
                let now = self.now();
                let v = [A, B][usize::from(x >> 1) % 2];
                if x & 1 == 0 {
                    self.fx.ex.on_venue_gate_open(A1, v, now);
                } else {
                    self.fx.ex.on_gate_open(A1, now);
                }
                self.take(Entry::Multi, x);
            }
            11 => {
                // Early fire of the earliest pending timer (a stale registration after recovery).
                if let Some(&(id, grp, _)) = self.timers.iter().min_by_key(|t| t.2) {
                    let now = self.now();
                    self.fx
                        .ex
                        .on_timer(u64::from(id) | (u64::from(grp) << 32), now, &mut self.fx.out);
                    let e = if grp == 0 { Entry::Multi } else { Entry::Single };
                    self.take(e, x);
                }
            }
            12 => {
                self.ms += [100, 300, 1000, 5000, 30_000, 120_000][usize::from(x) % 6];
                let ms = self.ms;
                self.fx.at(ms);
                self.fx.touch(S);
                self.fx.touch(P);
                self.fx.touch(P4);
            }
            13 => {
                let i = [S, P][usize::from(x) % 2];
                let off = i64::from((x >> 2) % 7) - 3;
                let mid = 10_000_000_000 + off * 50_000_000;
                let spread = 10_000_000 * i64::from((x >> 5) % 4 + 1);
                self.fx.book(i, &fmt8(mid - spread), &fmt8(mid + spread), "50", 5);
                self.fx.touch(i);
            }
            14 => {
                let now = self.now();
                match (x >> 1) % 3 {
                    0 => self.fx.ex.note_strategy_action(A1, g, now, &mut self.fx.out),
                    1 => {
                        let _ = self.fx.ex.hedge_fix_cmd(A1, g, &self.fx.v, now, &mut self.fx.out);
                    }
                    _ => {
                        let m = if x & 0x80 != 0 {
                            RunMode::CloseOnly
                        } else {
                            RunMode::Live
                        };
                        self.fx.mode(A1, g, m);
                        let now = self.now();
                        self.fx.ex.on_mode(A1, g, RunMode::Live, m, now, &mut self.fx.out);
                    }
                }
                self.take(Entry::Single, x);
            }
            _ => {
                // Ungrouped position changes outside the engine (no own order in flight).
                if x & 1 == 0 && !self.open.iter().any(|o| o.o.intent.group == GNONE) {
                    self.fx
                        .perp(A1, B, P4, ["0", "-0.5", "0.3"][usize::from(x >> 1) % 3]);
                }
                self.fx.ex.on_leg_adjust(A1, g);
            }
        }
        self.advance(x);
    }

    // -------------------------------------------------------------------------- checks

    fn check(&mut self, e: Entry, r: &Rec) {
        let ck = self.ck;
        if ck.caps20 && e == Entry::Single {
            assert!(
                r.decisions.len() <= 2
                    && r.notify.len() <= 1
                    && r.orders.len() <= 1
                    && r.cancels.len() <= 4
                    && r.cancel_resting.is_empty()
                    && r.broken.len() <= 1
                    && r.timers.len() <= 2
                    && r.notes.len() <= 2,
                "EXE-020: {r:?}"
            );
        }
        let acct = *self.fx.ex.acct_state(A1).unwrap();
        for o in &r.orders {
            let g = o.intent.group;
            let fix = matches!(
                o.kind,
                ExecOrderKind::HedgeFix
                    | ExecOrderKind::ResidualTrim
                    | ExecOrderKind::ExternalDefault
                    | ExecOrderKind::AutoFix
            );
            let leg = leg_of(g, o.intent.inst);
            let mut p = self.net(g, leg);
            for x in &self.open {
                if x.o.intent.group == g && x.o.intent.inst == o.intent.inst {
                    let rest = x.o.intent.qty.0 - x.cum;
                    p += if x.o.intent.side == Side::Buy { rest } else { -rest };
                }
            }
            let reduces = |p: i64| {
                (o.intent.side == Side::Buy && p < 0 && o.intent.qty.0 <= -p)
                    || (o.intent.side == Side::Sell && p > 0 && o.intent.qty.0 <= p)
            };
            if ck.inv50 {
                if g == GNONE {
                    assert!(
                        o.intent.flags & OrderIntent::REDUCE_ONLY != 0
                            && o.intent.class == IntentClass::Flatten,
                        "EXE-050 ungrouped {o:?}"
                    );
                } else {
                    let c = *self.fx.ex.cell(A1, g).unwrap();
                    if fix {
                        assert!(c.hold & MANUAL_HOLD == 0, "EXE-050 hold {o:?}");
                        assert!(
                            acct.gate_open || acct.venue_open & (1 << o.intent.venue.0) != 0,
                            "EXE-050 venue {o:?}"
                        );
                    }
                    // A new slice only in Working/Stuck; Broken only engine-target or auto-fix orders
                    // (a slice already in flight when the cell went Idle/Broken ends by itself).
                    if o.origin == ExecOrigin::Target {
                        assert!(
                            matches!(c.view.state, TargetState::Working | TargetState::Stuck(_)),
                            "EXE-050 target order in {:?}",
                            c.view.state
                        );
                    }
                    if matches!(c.view.state, TargetState::Broken(_)) {
                        let engine_target = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
                        assert!(
                            engine_target || o.kind == ExecOrderKind::CmdHedgeFix || fix,
                            "EXE-050 order in Broken {o:?}"
                        );
                    }
                    if c.serial != 0 {
                        // No pair link (`role = First`); HedgeFix-role single orders are not pairs.
                        assert!(
                            o.intent
                                .link
                                .get()
                                .is_none_or(|l| l.role != hft_strategy_sdk::LinkRole::First),
                            "EXE-050 serial link {:?}",
                            o.kind
                        );
                    }
                }
                if o.origin == ExecOrigin::Engine && (o.kind == ExecOrderKind::ForceStep || fix) {
                    assert!(reduces(p), "EXE-050 engine increases: {o:?} pos {p}");
                }
            }
            if ck.hedge201 && o.kind.is_hedge() {
                assert!(o.intent.qty.0 <= o.hedge_cap_qty.0, "EXE-201 {o:?}");
            }
            if ck.af261 && o.kind == ExecOrderKind::AutoFix {
                assert!(
                    acct.gate_open || acct.venue_open & (1 << o.intent.venue.0) != 0,
                    "EXE-261 venue"
                );
            }
            if ck.lock368 {
                if acct.flatten_lock {
                    assert!(reduces(p), "EXE-368 lock increases: {o:?} pos {p}");
                }
                if acct.flatten_lock || acct.force_active {
                    assert_eq!(o.origin, ExecOrigin::Engine, "EXE-368 origin {o:?}");
                }
            }
        }
        for d in &r.decisions {
            let (t, c, a) = unpack(d.detail);
            let key = d.group.0;
            if ck.codes433 {
                assert!(t <= 6, "EXE-433");
                if t == table::AUTO_FIX && c == autofix::FAILED {
                    assert!(a != 3 && a != 5, "EXE-433");
                }
                if t == table::ENGINE && c == engine::SERIAL_FALLBACK {
                    assert!(a != 2 && a != 3, "EXE-433");
                }
            }
            if ck.af261 && d.action == action::AUTO_FIX {
                if c == autofix::START {
                    self.af_terminal.retain(|x| x.0 != key);
                    self.af_terminal.push((key, 0));
                } else if c == autofix::DONE || c == autofix::FAILED {
                    let e = self.af_terminal.iter_mut().find(|x| x.0 == key);
                    if let Some(e) = e {
                        e.1 += 1;
                        assert!(e.1 <= 1, "EXE-261 two terminals");
                    }
                }
            }
            let engine_set = d.action == action::FORCE_STEP
                && t == table::ENGINE
                && matches!(c, engine::STEP | engine::UNGROUPED | engine::HARD_ALIGN);
            if ck.done417
                && (engine_set
                    || matches!(
                        d.action,
                        action::TARGET_SET | action::TARGET_REPLACED | action::TARGET_CLEARED
                    ))
            {
                self.done_since_set.retain(|x| x.0 != key);
                self.done_since_set.push((key, 0));
            }
            if ck.cold418 {
                let set = |v: &mut Vec<u32>, on: bool| {
                    v.retain(|x| *x != key);
                    if on {
                        v.push(key);
                    }
                };
                if d.action == action::EXTERNAL_SEEN && t == table::EXTERNAL {
                    if a == external::TIMED {
                        set(&mut self.ext_timed, true);
                    } else if a == external::CLEARED {
                        set(&mut self.ext_timed, false);
                    }
                }
                if d.action == action::EXTERNAL_DEFAULT {
                    set(&mut self.ext_timed, false);
                }
                if d.action == action::AUTO_FIX {
                    if c == autofix::START {
                        set(&mut self.af_running, true);
                    } else if c == autofix::DONE || c == autofix::FAILED {
                        set(&mut self.af_running, false);
                    }
                }
                if d.action == action::FORCE_STEP
                    && t == table::ENGINE
                    && matches!(c, engine::STEP | engine::UNGROUPED | engine::HARD_ALIGN)
                {
                    set(&mut self.eng_unreached, true);
                }
            }
        }
        if ck.done417 {
            for n in &r.notify {
                if n.u.kind
                    == (ExecUpdateKind::Done {
                        cleared_by_engine: false,
                    })
                {
                    let key = n.u.group.0;
                    // One Done per arrival: a target re-reached after drifting out of band (Idle →
                    // Working → Idle) reports again.
                    let arrivals = r
                        .decisions
                        .iter()
                        .filter(|d| d.group.0 == key && d.action == action::STATE && d.state_to == 1)
                        .count();
                    let dones = r
                        .notify
                        .iter()
                        .filter(|m| m.u.group.0 == key && m.u.kind == n.u.kind)
                        .count();
                    assert!(
                        arrivals >= 1 && dones <= arrivals,
                        "EXE-417 Done without State→Idle"
                    );
                    if let Some(e) = self.done_since_set.iter_mut().find(|x| x.0 == key) {
                        e.1 += 1;
                    }
                }
            }
        }
    }

    fn post_step(&mut self) {
        let acct = *self.fx.ex.acct_state(A1).unwrap();
        for g in [G7, G8, GNONE] {
            let Some(c) = self.fx.ex.cell(A1, g).copied() else {
                continue;
            };
            let engine_target = c.has_target && matches!(c.view.origin, TargetOrigin::Engine(_));
            if self.ck.cold418 {
                if c.ext_deadline.0 != 0 {
                    assert!(self.ext_timed.contains(&g.0), "EXE-418 ext timing not visible");
                }
                if c.auto_fix.phase() == AutoFixPhase::Running {
                    assert!(self.af_running.contains(&g.0), "EXE-418 auto-fix not visible");
                }
                if g != GNONE
                    && engine_target
                    && !matches!(c.view.state, TargetState::Idle | TargetState::Broken(_))
                {
                    assert!(
                        self.eng_unreached.contains(&g.0) || acct.flatten_lock,
                        "EXE-418 engine target not visible"
                    );
                }
            }
        }
    }
}

fn ops() -> impl Strategy<Value = Vec<(u8, u16)>> {
    prop::collection::vec((0u8..16, any::<u16>()), 1..80)
}

fn run(ops: &[(u8, u16)], ck: Checks) -> World {
    let mut w = World::new(false, ck);
    // Some starting positions.
    w.set_net(G7, 0, 100_000_000);
    w.set_net(G7, 1, -100_000_000);
    w.fx.spot(A1, A, G7, "1", "1");
    for (op, x) in ops {
        w.step(*op, *x);
    }
    w
}

#[test]
fn exe_513_manual_hold_survives_other_external_events() {
    run(
        &[(0, 6402), (7, 15018), (7, 56772), (7, 35176)],
        Checks {
            inv50: true,
            ..Checks::default()
        },
    );
}

#[test]
fn exe_050_external_deadline_survives_force_mode_changes() {
    // CI's reduced sequence: target, liquidation, own fill, force request, L1, deadline.
    // Changing the displayed Stuck reason must not resume the suspended strategy target.
    let w = run(
        &[
            (0, 52419),
            (7, 30147),
            (3, 25184),
            (9, 6836),
            (8, 54756),
            (12, 9441),
        ],
        Checks {
            inv50: true,
            ..Checks::default()
        },
    );
    let c = w.fx.ex.cell(A1, G8).unwrap();
    assert!(!c.has_target);
    assert_eq!(c.ext_deadline, Tsc(0));
    assert_eq!(c.view.state, TargetState::Idle);
}

fn strip(d: &[ExecDecision]) -> Vec<ExecDecision> {
    d.iter()
        .map(|x| {
            let mut y = *x;
            y.trigger_seq = 0;
            y.set_seq = 0;
            y
        })
        .collect()
}

// ------------------------------------------------------------------------------------ props

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn exe_020_single_cell_entry_caps(o in ops()) {
        run(&o, Checks { caps20: true, ..Checks::default() });
    }

    #[test]
    fn exe_050_cell_invariants(o in ops()) {
        run(&o, Checks { inv50: true, ..Checks::default() });
    }

    #[test]
    fn exe_201_hedge_qty_le_cap(o in ops()) {
        run(&o, Checks { hedge201: true, ..Checks::default() });
    }

    #[test]
    fn exe_261_one_attempt_per_broken(o in ops()) {
        run(&o, Checks { af261: true, ..Checks::default() });
    }

    #[test]
    fn exe_368_lock_no_increase(o in ops()) {
        run(&o, Checks { lock368: true, ..Checks::default() });
    }

    #[test]
    fn exe_417_done_once(o in ops()) {
        run(&o, Checks { done417: true, ..Checks::default() });
    }

    #[test]
    fn exe_418_cold_reader_reconstruction(o in ops()) {
        run(&o, Checks { cold418: true, ..Checks::default() });
    }

    #[test]
    fn exe_433_reserved_codes_unused(o in ops()) {
        run(&o, Checks { codes433: true, ..Checks::default() });
    }

    #[test]
    fn exe_442_timer_keys(o in ops()) {
        run(&o, Checks { timers442: true, ..Checks::default() });
    }
}

#[test]
fn exe_516_new_fix_waits_for_wake_under_capacity_and_restore() {
    // CI seed 1b34924f66b9b633b256751eb7ad6eee445ccd6e8b1279e459094a35677c2100.
    const OPS: [(u8, u16); 4] = [(7, 15590), (9, 65457), (12, 112), (3, 34016)];
    let run_case = |capped, cut| {
        let mut w = World::new(capped, Checks::default());
        w.set_net(G7, 0, 100_000_000);
        w.set_net(G7, 1, -100_000_000);
        w.fx.spot(A1, A, G7, "1", "1");
        for (k, (op, x)) in OPS.iter().enumerate() {
            if k == cut {
                let snapshot = w.fx.snap();
                w.fx.ex = w.fx.restore(&snapshot);
            }
            w.step(*op, *x);
            if k == 1 {
                let c = w.fx.cell(A1, G7);
                assert_eq!(c.gated_bits, RejectBits::RATE);
                assert!(c.wake_at > Views::now(&w.fx.v.m).tsc);
                assert_eq!(c.fix.first_tsc, Tsc(0));
                assert!(w.open.is_empty(), "first force order was rejected");
            }
        }
        assert_eq!(w.trace.orders.len(), 2, "rejected force slice and its retry");
        assert!(w.trace.broken.is_empty(), "capacity must not cause FixExhausted");
        assert_eq!(
            w.trace
                .timers
                .iter()
                .any(|t| t.kind() == Some(ExecTimerKind::Continue)),
            capped,
            "exercise the capacity continuation"
        );
        w
    };
    let mut baselines = Vec::new();
    for capped in [false, true] {
        let baseline = run_case(capped, OPS.len());
        for cut in 0..OPS.len() {
            let restored = run_case(capped, cut);
            assert_eq!(baseline.trace.decisions, restored.trace.decisions, "cut {cut}");
            assert_eq!(baseline.trace.orders, restored.trace.orders, "cut {cut}");
            assert_eq!(baseline.trace.notify, restored.trace.notify, "cut {cut}");
            assert_eq!(baseline.trace.timers, restored.trace.timers, "cut {cut}");
            assert_eq!(baseline.trace.notes, restored.trace.notes, "cut {cut}");
        }
        let mut decisions = strip(&baseline.trace.decisions);
        decisions.sort_by_key(|d| d.group.0);
        let mut orders = baseline.trace.orders;
        orders.sort_by_key(|o| o.intent.group.0);
        let mut notify = baseline.trace.notify;
        notify.sort_by_key(|n| n.u.group.0);
        baselines.push((decisions, orders, notify));
    }
    assert_eq!(baselines[0], baselines[1]);
}

fn check_placement_callbacks_wait_for_next_pass() {
    // Persisted CI seed 5bf7fb15af4ed9f8d98cff7f381029ea91e3e5b8acf9eeb64cf7fea51ff1da7c.
    let ops = [
        (0, 57830),
        (7, 699),
        (6, 156),
        (3, 1344),
        (7, 27008),
        (0, 0),
        (9, 33763),
        (3, 54631),
        (9, 8558),
        (6, 47804),
        (7, 9788),
        (8, 36812),
        (0, 25592),
    ];
    let run = |capped, restore| {
        let mut w = World::new(capped, Checks::default());
        w.restore_continuations = restore;
        w.set_net(G7, 0, 100_000_000);
        w.set_net(G7, 1, -100_000_000);
        w.fx.spot(A1, A, G7, "1", "1");
        for (op, x) in ops {
            w.step(op, x);
        }
        assert_eq!(
            w.trace
                .timers
                .iter()
                .any(|t| t.kind() == Some(ExecTimerKind::Continue)),
            capped
        );
        let mut decisions = strip(&w.trace.decisions);
        decisions.sort_by_key(|d| d.group.0);
        w.trace.orders.sort_by_key(|o| o.intent.group.0);
        w.trace.notify.sort_by_key(|n| n.u.group.0);
        (decisions, w.trace.orders, w.trace.notify)
    };
    let baseline = run(false, false);
    assert_eq!(run(true, false), baseline, "capacity only");
    assert_eq!(run(true, true), baseline, "restore every continuation");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    #[test]
    fn exe_024_no_output_lost_under_caps(o in ops()) {
    check_placement_callbacks_wait_for_next_pass();
        let mut a = World::new(true, Checks::default());
        let mut b = World::new(false, Checks::default());
        for w in [&mut a, &mut b] {
            w.set_net(G7, 0, 100_000_000);
            w.set_net(G7, 1, -100_000_000);
            w.fx.spot(A1, A, G7, "1", "1");
            for (op, x) in &o {
                w.step(*op, *x);
            }
        }
        // Place results follow each output batch, so records of different cells may interleave
        // differently; each cell's own sequence is identical.
        let by_cell = |d: &[ExecDecision]| {
            let mut v = strip(d);
            v.sort_by_key(|x| x.group.0);
            v
        };
        prop_assert_eq!(by_cell(&a.trace.decisions), by_cell(&b.trace.decisions));
        let mut oa = a.trace.orders.clone();
        let mut ob = b.trace.orders.clone();
        oa.sort_by_key(|o| o.intent.group.0);
        ob.sort_by_key(|o| o.intent.group.0);
        prop_assert_eq!(oa, ob);
        let mut na = a.trace.notify.clone();
        let mut nb = b.trace.notify.clone();
        na.sort_by_key(|n| n.u.group.0);
        nb.sort_by_key(|n| n.u.group.0);
        prop_assert_eq!(na, nb);
    }

    #[test]
    fn exe_462_deterministic(o in ops(), cut in 0usize..80) {
        let a = run(&o, Checks::default());
        let b = run(&o, Checks::default());
        prop_assert_eq!(&a.trace.decisions, &b.trace.decisions, "second run");
        prop_assert_eq!(&a.trace.orders, &b.trace.orders);
        prop_assert_eq!(&a.trace.notify, &b.trace.notify);
        prop_assert_eq!(&a.trace.timers, &b.trace.timers);
        prop_assert_eq!(&a.trace.notes, &b.trace.notes);
        // Snapshot → restore inserted after step `cut`.
        let mut c = World::new(false, Checks::default());
        c.set_net(G7, 0, 100_000_000);
        c.set_net(G7, 1, -100_000_000);
        c.fx.spot(A1, A, G7, "1", "1");
        for (k, (op, x)) in o.iter().enumerate() {
            if k == cut.min(o.len() - 1) {
                let s = c.fx.snap();
                c.fx.ex = c.fx.restore(&s);
            }
            c.step(*op, *x);
        }
        prop_assert_eq!(&a.trace.decisions, &c.trace.decisions, "restored run");
        prop_assert_eq!(&a.trace.orders, &c.trace.orders, "restored run");
        prop_assert_eq!(&a.trace.notify, &c.trace.notify, "restored run");
    }
}

// ------------------------------------------------------------------- pure-function properties

fn hot() -> hft_refdata::InstHot {
    use std::sync::OnceLock;
    static H: OnceLock<hft_refdata::InstHot> = OnceLock::new();
    *H.get_or_init(|| {
        let fx = Fx::new();
        *fx.rd.inst(P)
    })
}

/// Reference: `⌈q × px × r⌉` in USDT micro (exponents −8 / −8, rate 1e-9) via exact i128.
fn usdt_ref(q: i64, px: i64, r: i64) -> i128 {
    let n = i128::from(q) * i128::from(px) * i128::from(r);
    let d: i128 = 10_000_000_000 * 1_000_000_000;
    (n + d - 1) / d
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn exe_129_qty_max_under_cap(
        px in 10_000i64..=100_000_000_000_000,
        r in 500_000_000i64..=2_000_000_000,
        cap in 1_000_000i64..=1_000_000_000_000,
        lot in 1i64..=10_000_000_000,
    ) {
        let mut h = hot();
        h.lot = lot;
        let q = calc::qty_under_cap(Notional(cap), Px(px), Rate(r), &h);
        prop_assert_eq!(q.0 % lot, 0);
        prop_assert!(usdt_ref(q.0, px, r) <= i128::from(cap));
        prop_assert!(usdt_ref(q.0 + lot, px, r) > i128::from(cap));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn exe_133_price_is_cap_cross_px(
        mid in 100_000_000i64..=100_000_000_000_000,
        spread_bp in 0i64..=500,
        rr in 1_000_000i64..=50_000_000,
        mc in prop_oneof![Just(0i64), 5_000_000i64..=100_000_000],
        buy in any::<bool>(),
    ) {
        let h = hot();
        let half = mid * spread_bp / 20_000;
        let mut b = hft_strategy_sdk::BboView::EMPTY;
        b.bid_px = Px(mid - half);
        b.ask_px = Px(mid + half);
        let side = if buy { Side::Buy } else { Side::Sell };
        let got = calc::limit_px(side, &b, Rate(mc), Rate(rr), 2000, &h);
        let lim = hft_types::HedgePxLimits { conn_liveness_ms: 2000, rr_max_cross: Rate(rr) };
        let touch = if buy { b.ask_px } else { b.bid_px };
        let want = hft_types::cap_cross_px(side, touch, Px::mid_floor(b.bid_px, b.ask_px), Rate(mc), &lim, h.tick);
        prop_assert_eq!(got, want);
    }

    #[test]
    fn exe_187_pair_lot_shared_impl(lf in 0usize..5, lh in 0usize..5, num in 1u32..=100, den in 1u32..=100) {
        let lots = ["0.001", "0.002", "0.003", "0.01", "0.1"];
        let mut insts = default_insts();
        insts.retain(|i| i.id != S && i.id != P);
        insts.push(inst_p(S, A, false, lots[lf]));
        insts.push(inst_p(P, B, true, lots[lh]));
        let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
        let mut pp = pair0();
        pp.ratio_num = num;
        pp.ratio_den = den;
        let t = target(A1, G7, &[(0, q("1")), (1, q("-1"))], ExecSpec::PairOnFill(pp));
        inject(&mut fx, &t);
        let c = fx.cell(A1, G7);
        let want = hft_types::pair_lot(fx.rd.inst(S).lot, fx.rd.inst(P).lot, num, den);
        prop_assert_eq!(c.pair_lot, want.unwrap_or(Qty(0)));
    }

    #[test]
    fn exe_188_second_leg_lot_multiple(lf in 0usize..5, lh in 0usize..5, ratio in 0usize..3, depth in 1u32..=1000, mid_k in 1i64..=200_000) {
        let lots = ["0.001", "0.002", "0.003", "0.01", "0.1"];
        let (num, den) = [(1u32, 1u32), (2, 1), (1, 3)][ratio];
        let mut insts = default_insts();
        insts.retain(|i| i.id != S && i.id != P);
        insts.push(inst_p(S, A, false, lots[lf]));
        insts.push(inst_p(P, B, true, lots[lh]));
        let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
        let m = mid_k * 100_000_000;
        let dq = format!("{depth}");
        fx.book(S, &fmt8(m - m / 1000), &fmt8(m + m / 1000), &dq, 5);
        fx.book(P, &fmt8(m - m / 1000), &fmt8(m + m / 1000), &dq, 5);
        fx.mark(S, &fmt8(m));
        fx.mark(P, &fmt8(m));
        fx.spot(A1, A, G7, "0", "1000000000");
        let mut pp = pair0();
        pp.ratio_num = num;
        pp.ratio_den = den;
        pp.first_leg = hft_strategy_sdk::FirstLeg::Fixed;
        pp.first_leg_ix = 0;
        pp.slice.max_chunk_notional = usd("1000000");
        let t = target(A1, G7, &[(0, q("50")), (1, q("-50"))], ExecSpec::PairOnFill(pp));
        let (res, mut r) = fx.set(I1, &t);
        if res.is_ok() {
            r.extend(fx.adv());
        }
        let pl = fx.cell(A1, G7).pair_lot.0;
        let lot_h = fx.rd.inst(P).lot;
        let min_h = fx.rd.inst(P).min_qty.0;
        for o in r.ord(ExecOrderKind::PairFirst) {
            prop_assert!(pl > 0 && o.intent.qty.0 % pl == 0);
            let h = i128::from(o.intent.qty.0) * i128::from(num);
            prop_assert_eq!(h % i128::from(den), 0);
            let hq = h / i128::from(den);
            prop_assert_eq!(hq % i128::from(lot_h), 0);
            prop_assert!(hq >= i128::from(min_h));
        }
    }
}
