//! `ToyEngine`: the DST framework's own system under test. It is a deliberately small two-leg
//! engine (one taker strategy per group, an OMS slot table, OnFill hedging with residuals and
//! retries, a ledger, kill caps and modes, Broken auto-fix, L3 reduction, Drain) that fills every
//! observation row the checkers read. It exists so the framework's claims — a pass is credible, a
//! failure is reproducible and shrinkable — can be tested before and independently of the real
//! engine adapter, and it carries planted defects (`planted` names) that violate one invariant
//! each. It is not a model of the production engine.

use std::collections::{BTreeMap, HashMap};

use hft_event::{
    Bbo, CmdHdr, ControlCmd, EventKind, ExternalFill, GwCmd, GwOp, MarginReportHdr, OmsTimer, OrderAction,
    OrderDecision, OrderEvent, OrderEventKind, OrderFlags, RecordFlags, SendReport, SendStatus, Timer,
    TimerCode,
};
use hft_types::{
    AccountId, AssetId, GroupId, InstId, Px, Qty, RejectBits, RunMode, Side, SlotId, StrategyId, Tif, Tsc,
    VenueId,
};

use crate::dst::driver::MemJournal;
use crate::dst::obs::{
    AssetRow, AutoFixPhase, CapLevel, CapRow, CapScope, CellModeRow, DecClass, DecOrigin, DecisionInfo,
    DrainRow, Evidence, EvidenceKind, ExecCellRow, ExternalKey, ForceTarget, HedgeKind, InflightRow, InstRow,
    LegNetRow, LegRow, LimitRow, LinkRow, LinkState, MarginRow, NoteInfo, Obs, PlanKind, PosRow, RiskRow,
    SlotRow, SlotState, SlotTableRow, TombRow,
};
use crate::dst::sut::{BootCtx, BootFail, CrashPoint, InputRec, JRec, OutRing, StepOut, Sut};
use crate::dst::venue_behavior::ToyMatcher;
use crate::dst::world::{AccountKind, GroupSpecW, InstSpec, WorldSpec};
use crate::hash::ComponentHashes;

/// Planted defects understood by the toy engine (name → violated invariant).
pub const PLANTED: &[(&str, u8)] = &[
    ("oms-429-fails", 1),
    ("oms-drop-unknown-report", 2),
    ("risk-no-exposure-limit", 3),
    ("unknown-opens", 4),
    ("kill-no-risk-reducing", 5),
    ("snapshot-field", 6),
    ("slot-leak", 7),
    ("hedge-gross-qty", 8),
    ("hashmap-order", 9),
    ("mode-incremental", 11),
    ("draining-blocks-rr", 12),
    ("setmode-opens", 13),
    ("gate-counts", 14),
    ("hedge-plus-2-lots", 15),
    ("ignore-fee", 16),
    ("l3-overshoot", 17),
    ("drain-early-done", 18),
    ("double-hedge-external", 19),
    ("broken-never-autofix", 20),
    ("broken-repeat-autofix", 20),
];

const CAPACITY: u32 = 64;
const QUARANTINE_MS: i64 = 500;
const SWEEP_BATCH: u32 = 16;
const BROKEN_RETRIES: u32 = 3;
const TIMER_SWEEP: u32 = 1;
const TIMER_RETRY: u32 = 2;
const TIMER_AUTOFIX: u32 = 3;
const TIMER_PENDING: u32 = 4;
const TIMER_L3: u32 = 5;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Slot {
    venue: VenueId,
    account: AccountId,
    ix: u32,
    r#gen: u32,
    state: SlotState,
    group: GroupId,
    inst: InstId,
    side: Side,
    qty: i64,
    filled: i64,
    immediate: bool,
    rr: bool,
    link: u32,
    hedge: bool,
    terminal_ens: i64,
    sent_ens: i64,
    class: DecClass,
    autofix: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Link {
    id: u32,
    account: AccountId,
    group: GroupId,
    first_side: Side,
    hedge_inst: InstId,
    state: LinkState,
    first_net: i128,
    hedge_req: i128,
    hedge_filled: i128,
    retries: u32,
    owed: i128,
    hedge_in_flight: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
struct Cell {
    broken: bool,
    phase: u8,
    due_ens: i64,
    budget: i128,
    af_filled: i128,
    fail_streak: u32,
    starts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Cap {
    scope: CapScope,
    hard: bool,
    set_seq: u64,
}

/// The framework's self-test engine.
#[derive(Clone, Debug)]
pub struct ToyEngine {
    world: WorldSpec,
    planted: Vec<&'static str>,
    tsc_hz: u64,
    j: MemJournal,
    bbo: BTreeMap<u32, (i64, i64)>,
    slots: Vec<Slot>,
    tombs: Vec<TombRow>,
    gens: BTreeMap<(u8, u8), u32>,
    links: Vec<Link>,
    pos: BTreeMap<(u8, u8, u32), i128>,
    inv: BTreeMap<(u8, u8, u16), i128>,
    bal: BTreeMap<(u8, u8, u16), i128>,
    leg_net: BTreeMap<(u8, u32, u8), i128>,
    residual: BTreeMap<(u8, u32), i128>,
    fee_adj: BTreeMap<(u8, u32), i128>,
    /// Totals of finished links per group: (first_net, hedge_req, hedge_filled).
    retired: BTreeMap<(u8, u32), (i128, i128, i128)>,
    /// Hedge quantity cut by the naked cap (`OmsHedgeNakedCapped`).
    naked_capped: BTreeMap<(u8, u32), i128>,
    caps: Vec<Cap>,
    desired: BTreeMap<(u8, u32), RunMode>,
    effective: BTreeMap<(u8, u32), RunMode>,
    cells: BTreeMap<(u8, u32), Cell>,
    l3: BTreeMap<u8, bool>,
    ext_seen: Vec<(AccountId, VenueId, InstId, ExternalKey)>,
    oms_dup: u64,
    stale_gen: u64,
    timers: Vec<(u64, u32, u32)>,
    drain: DrainRow,
    draining: bool,
    next_link: u32,
    /// Planted I6: a counter outside the journal.
    shadow_counter: u64,
    now_ns: u64,
    /// Latest snapshot (every `SNAPSHOT_EVERY` Inputs): the state after Input seq `.0`.
    snap: Option<(u64, Box<ToyEngine>)>,
    inputs: u64,
}

/// Inputs between two toy snapshots.
const SNAPSHOT_EVERY: u64 = 1_000;

fn note(out: &mut StepOut, seq: u64, name: &'static str, account: AccountId, group: GroupId) {
    out.info.notes.push(NoteInfo {
        seq,
        name,
        account,
        group,
        inst: InstId(0),
        v: [0; 8],
    });
}

impl ToyEngine {
    fn has(&self, p: &str) -> bool {
        self.planted.contains(&p)
    }
    fn inst(&self, i: InstId) -> Option<&InstSpec> {
        self.world.inst(i)
    }
    fn group(&self, a: AccountId, g: GroupId) -> Option<&GroupSpecW> {
        self.world.groups.iter().find(|x| x.account == a && x.group == g)
    }
    fn is_sim(&self, a: AccountId) -> bool {
        self.world.account(a).is_some_and(|x| x.kind == AccountKind::Sim)
    }
    fn group_naked(&self, a: AccountId, g: GroupId) -> i128 {
        (0..2u8)
            .map(|l| self.leg_net.get(&(a.0, g.0, l)).copied().unwrap_or(0))
            .sum()
    }
    fn killed(&self, a: AccountId) -> (bool, bool) {
        let mut k = false;
        let mut h = false;
        for c in &self.caps {
            let hit = matches!(c.scope, CapScope::Global) || c.scope == CapScope::Account(a);
            if hit {
                k = true;
                h |= c.hard;
            }
        }
        (k, h)
    }
    fn recompute_modes(&mut self) {
        let keys: Vec<(u8, u32)> = self.desired.keys().copied().collect();
        for k in keys {
            let a = AccountId(k.0);
            let d = self.desired[&k];
            let killed = self.killed(a).0;
            let e = if d == RunMode::Shadow || killed {
                RunMode::CloseOnly
            } else {
                d
            };
            if self.has("mode-incremental") && e == d && self.effective.get(&k) == Some(&RunMode::CloseOnly) {
                continue;
            }
            self.effective.insert(k, e);
        }
    }

    fn alloc_slot(&mut self, venue: VenueId, account: AccountId) -> Option<(u32, u32)> {
        let used: Vec<u32> = self
            .slots
            .iter()
            .filter(|s| s.venue == venue && s.account == account)
            .map(|s| s.ix)
            .collect();
        let ix = (1..=CAPACITY).find(|i| !used.contains(i))?;
        let g = self.gens.entry((venue.0, account.0)).or_insert(0);
        *g += 1;
        Some((ix, *g))
    }

    /// Places one order: decision record, slot, frame.
    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        out: &mut StepOut,
        tsc: u64,
        trigger: u64,
        a: AccountId,
        g: GroupId,
        inst: InstId,
        side: Side,
        qty: i64,
        class: DecClass,
        origin: DecOrigin,
        hedge: Option<HedgeKind>,
        link: u32,
        basis: i128,
        margin_stress: bool,
    ) -> bool {
        if qty <= 0 {
            return false;
        }
        let Some(spec) = self.inst(inst).cloned() else {
            return false;
        };
        let venue = spec.venue;
        let Some((ix, r#gen)) = self.alloc_slot(venue, a) else {
            return false;
        };
        let (bid, ask) = self.bbo.get(&inst.0).copied().unwrap_or((spec.bid, spec.ask));
        let px = match side {
            Side::Buy => ask + 10 * spec.tick,
            Side::Sell => (bid - 10 * spec.tick).max(spec.tick),
        };
        let rr = matches!(class, DecClass::Hedge | DecClass::Flatten | DecClass::Close);
        let mut oflags = 0u16;
        if rr {
            oflags |= OrderFlags::RISK_REDUCING.0;
        }
        if link != 0 {
            oflags |= OrderFlags::LINKED.0;
        }
        let (killed, hard) = self.killed(a);
        let mut reject = 0u32;
        if hard {
            reject |= RejectBits::HARD.0;
        } else if killed && !rr && !self.has("kill-no-risk-reducing") {
            reject |= RejectBits::KILLED.0;
        }
        if self.has("draining-blocks-rr") && rr && killed {
            reject |= RejectBits::DRAINING.0;
        }
        let link_role = match hedge {
            Some(HedgeKind::Second) => 2,
            Some(HedgeKind::SecondRetry) => 3,
            Some(HedgeKind::HedgeFix) => 4,
            Some(HedgeKind::Reverse) => 5,
            _ if link != 0 => 1,
            _ => 0,
        };
        let d = OrderDecision {
            trigger_seq: trigger,
            decided_tsc: Tsc(tsc),
            px: Px(px),
            qty: Qty(qty),
            group: g,
            inst,
            slot: SlotId(ix),
            r#gen,
            target_slot: SlotId(0),
            reject,
            strategy: StrategyId(0),
            venue,
            account: a,
            action: OrderAction::New,
            side,
            tif: Tif::Ioc,
            oflags,
            reason: 0,
            intent_no: 0,
            req_px: Px(px),
            link_role,
            _pad: [0; 7],
        };
        let seq = self.j.append_body(RecordFlags::default(), tsc, &d);
        let pos = self.pos.get(&(a.0, venue.0, inst.0)).copied().unwrap_or(0);
        let s = if side == Side::Buy { 1 } else { -1 };
        let same_dir: i128 = self
            .slots
            .iter()
            .filter(|x| x.account == a && x.inst == inst && x.side == side && !x.state.is_terminal())
            .map(|x| i128::from(x.qty - x.filled))
            .sum();
        let naked = self.group_naked(a, g);
        let naked_dir = if s < 0 { naked.max(0) } else { (-naked).max(0) };
        let max_pos = self.group(a, g).map_or(0, |x| x.max_pos_qty);
        out.info.decisions.push(DecisionInfo {
            seq,
            class,
            origin,
            hedge,
            margin_stress,
            basis,
            naked_dir,
            pos,
            same_dir_inflight: same_dir,
            max_pos_qty: max_pos,
            notional: i128::from(qty) * i128::from(px) / 1_000_000,
            link,
            link_unknown: link != 0
                && self
                    .slots
                    .iter()
                    .any(|s| s.link == link && s.state == SlotState::Unknown),
            group_unknown: self
                .slots
                .iter()
                .any(|s| s.account == a && s.group == g && s.state == SlotState::Unknown),
        });
        if reject != 0 {
            return false;
        }
        self.slots.push(Slot {
            venue,
            account: a,
            ix,
            r#gen,
            state: SlotState::PendingNew,
            group: g,
            inst,
            side,
            qty,
            filled: 0,
            immediate: true,
            rr,
            link,
            hedge: hedge.is_some(),
            terminal_ens: 0,
            sent_ens: self.now_ns as i64,
            class,
            autofix: hedge == Some(HedgeKind::AutoFix),
        });
        let cmd = GwCmd {
            decision_seq: seq,
            decided_tsc: Tsc(tsc),
            px: Px(px),
            qty: Qty(qty),
            inst,
            slot: SlotId(ix),
            r#gen,
            target_slot: SlotId(0),
            op: GwOp::Place,
            account: a,
            side,
            tif: Tif::Ioc,
            oflags,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        };
        let ring = if self.is_sim(a) {
            OutRing::Sim
        } else {
            OutRing::Venue(venue)
        };
        out.frames.push(crate::dst::sut::FrameRec {
            ring,
            cmd,
            trigger_seq: trigger,
        });
        self.timers.push((
            self.now_ns + 3_000_000_000,
            TIMER_PENDING,
            ix | u32::from(a.0) << 16,
        ));
        true
    }

    fn strategy(&mut self, out: &mut StepOut, tsc: u64, trigger: u64, inst: InstId) {
        let mut groups: Vec<GroupSpecW> = self
            .world
            .groups
            .iter()
            .filter(|g| g.first == inst)
            .cloned()
            .collect();
        if self.has("hashmap-order") {
            let m: HashMap<u32, GroupSpecW> = groups.drain(..).map(|g| (g.group.0, g)).collect();
            groups = m.into_values().collect();
        }
        for g in groups {
            let a = g.account;
            let key = (a.0, g.group.0);
            let mode = self.effective.get(&key).copied().unwrap_or(RunMode::Live);
            let live = mode == RunMode::Live || self.has("kill-no-risk-reducing");
            if mode == RunMode::Shadow || self.draining {
                continue;
            }
            if self.cells.get(&key).is_some_and(|c| c.broken) || self.l3.get(&a.0).copied().unwrap_or(false) {
                continue;
            }
            let busy = self.links.iter().any(|l| {
                l.account == a
                    && l.group == g.group
                    && matches!(l.state, LinkState::Open | LinkState::Hedging)
            });
            let unknown = self
                .slots
                .iter()
                .any(|s| s.account == a && s.group == g.group && s.state == SlotState::Unknown);
            if busy || (unknown && !self.has("unknown-opens")) {
                continue;
            }
            let Some(spec) = self.inst(g.first).cloned() else {
                continue;
            };
            let pos = self
                .pos
                .get(&(a.0, spec.venue.0, g.first.0))
                .copied()
                .unwrap_or(0);
            let hedge_lot = self.inst(g.second).map_or(1, |x| x.lot);
            let open = pos <= 0 || self.has("risk-no-exposure-limit");
            if open && !live {
                continue;
            }
            let (side, qty, class) = if open {
                if !self.has("risk-no-exposure-limit") && pos + i128::from(hedge_lot) > g.max_pos_qty {
                    continue;
                }
                (Side::Buy, hedge_lot, DecClass::Open)
            } else {
                (Side::Sell, (pos as i64).min(hedge_lot * 4), DecClass::Close)
            };
            self.next_link += 1;
            let id = self.next_link;
            self.links.push(Link {
                id,
                account: a,
                group: g.group,
                first_side: side,
                hedge_inst: g.second,
                state: LinkState::Open,
                first_net: 0,
                hedge_req: 0,
                hedge_filled: 0,
                retries: 0,
                owed: 0,
                hedge_in_flight: false,
            });
            if !self.place(
                out,
                tsc,
                trigger,
                a,
                g.group,
                g.first,
                side,
                qty,
                class,
                DecOrigin::Strategy,
                None,
                id,
                0,
                false,
            ) && let Some(l) = self.links.last_mut()
            {
                l.state = LinkState::Expired;
            }
        }
    }

    fn book_fill(&mut self, out: &mut StepOut, e: &OrderEvent, d: i64) {
        let a = e.account;
        let v = e.venue;
        let s: i128 = if self.slot_side(e) == Side::Buy { 1 } else { -1 };
        let spec = self.inst(e.inst).cloned();
        let perp = spec.as_ref().is_some_and(|x| x.perp);
        let base = spec.as_ref().map_or(AssetId(0), |x| x.base);
        let quote = spec.as_ref().map_or(AssetId(0), |x| x.quote);
        let delta = s * i128::from(d);
        *self.pos.entry((a.0, v.0, e.inst.0)).or_default() += delta;
        out.info.evidence.push(Evidence {
            kind: EvidenceKind::Fill,
            account: a,
            venue: v,
            inst: e.inst,
            delta,
            ext_key: None,
        });
        let notional = i128::from(d) * i128::from(e.px.0) / 1_000_000;
        let fee = i128::from(e.fee.0);
        let ignore_fee = self.has("ignore-fee");
        if !perp {
            *self.inv.entry((a.0, v.0, base.0)).or_default() += delta;
            *self.bal.entry((a.0, v.0, quote.0)).or_default() -= s * notional;
        }
        if !ignore_fee && fee != 0 {
            if e.fee_asset == base && !perp {
                *self.inv.entry((a.0, v.0, base.0)).or_default() -= fee;
                *self.pos.entry((a.0, v.0, e.inst.0)).or_default() -= fee;
                out.info.evidence.push(Evidence {
                    kind: EvidenceKind::FeeNet,
                    account: a,
                    venue: v,
                    inst: e.inst,
                    delta: -fee,
                    ext_key: None,
                });
            } else {
                *self.bal.entry((a.0, v.0, e.fee_asset.0)).or_default() -= fee;
            }
        }
    }

    fn slot_side(&self, e: &OrderEvent) -> Side {
        self.slots
            .iter()
            .find(|s| s.venue == e.venue && s.account == e.account && s.ix == e.slot.0)
            .map_or(Side::Buy, |s| s.side)
    }

    fn on_order_event(&mut self, out: &mut StepOut, seq: u64, tsc: u64, e: OrderEvent) {
        if e.what == OrderEventKind::FeeOnly {
            self.on_fee_only(out, seq, e);
            return;
        }
        let now = self.now_ns as i64;
        let ix = self.slots.iter().position(|s| {
            s.venue == e.venue && s.account == e.account && s.ix == e.slot.0 && s.r#gen == e.r#gen
        });
        let Some(ix) = ix else {
            if self.has("oms-drop-unknown-report") {
                return;
            }
            let tomb = self
                .tombs
                .iter()
                .find(|t| {
                    t.venue == e.venue && t.account == e.account && t.slot == e.slot.0 && t.r#gen == e.r#gen
                })
                .copied();
            let fill =
                matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill) || e.cum_filled.0 > 0;
            if e.slot == SlotId::UNRESOLVED {
                note(
                    out,
                    seq,
                    if fill {
                        "OmsForeignFillViaOrderEvent"
                    } else {
                        "OmsForeignOrder"
                    },
                    e.account,
                    GroupId(0),
                );
            } else if let Some(t) = tomb {
                if i128::from(e.cum_filled.0) > t.filled {
                    note(out, seq, "OmsLateFill", e.account, GroupId(0));
                } else {
                    self.oms_dup += 1;
                }
            } else if fill {
                note(out, seq, "OmsStaleGenFill", e.account, GroupId(0));
            } else {
                self.stale_gen += 1;
            }
            // The framework model follows the same external-evidence path. Distinct stale
            // orders must be booked; copies of an already observed receipt stay deduplicated.
            if fill && tomb.is_none() && e.exch_order_id != 0 && matches!(e.detail, 1 | 2) {
                self.on_external(
                    out,
                    seq,
                    tsc,
                    ExternalFill {
                        venue: e.venue,
                        account: e.account,
                        side: if e.detail == 1 { Side::Buy } else { Side::Sell },
                        cause: hft_event::ExternalCause::Unknown,
                        inst: e.inst,
                        qty: e.cum_filled,
                        px: e.px,
                        fee: e.fee,
                        fee_asset: e.fee_asset,
                        _pad: [0; 6],
                        exch_order_id: e.exch_order_id,
                        trade_id: e.trade_id,
                        ex_ts_ns: e.ex_ts_ns,
                        rx_tsc: Tsc(e.rx_tsc),
                        fill_seq: if e.trade_id == 0 { e.cum_filled.0 as u64 } else { 0 },
                        cash_qty: e.cash_qty,
                        cash_amount: e.cash_sum,
                    },
                );
            }
            return;
        };
        let s = self.slots[ix].clone();
        let mut transitioned = true;
        match e.what {
            OrderEventKind::Ack => {
                if matches!(s.state, SlotState::PendingNew | SlotState::Unknown) {
                    self.slots[ix].state = SlotState::Live;
                } else {
                    transitioned = false;
                }
            }
            OrderEventKind::PartialFill | OrderEventKind::Fill => {
                let d = e.cum_filled.0 - s.filled;
                if d <= 0 {
                    transitioned = false;
                } else {
                    self.slots[ix].filled = e.cum_filled.0;
                    self.book_fill(out, &e, d);
                    let leg = u8::from(self.group(s.account, s.group).is_some_and(|g| g.second == s.inst));
                    let sgn: i128 = if s.side == Side::Buy { 1 } else { -1 };
                    let fee_base = if !self.inst(e.inst).is_some_and(|x| x.perp)
                        && e.fee_asset == self.inst(e.inst).map_or(AssetId(0), |x| x.base)
                        && !self.has("ignore-fee")
                    {
                        i128::from(e.fee.0)
                    } else {
                        0
                    };
                    *self.leg_net.entry((s.account.0, s.group.0, leg)).or_default() +=
                        sgn * i128::from(d) - fee_base;
                    if s.autofix
                        && let Some(c) = self.cells.get_mut(&(s.account.0, s.group.0))
                    {
                        c.af_filled += i128::from(d);
                    }
                    if e.what == OrderEventKind::Fill || self.slots[ix].filled >= s.qty {
                        self.terminal(ix, SlotState::Filled, now);
                    }
                    if s.link != 0 {
                        let net = i128::from(d) - fee_base;
                        self.on_link_fill(out, seq, tsc, s.link, s.hedge, net, i128::from(d));
                    }
                    if s.autofix && self.slots.get(ix).is_none_or(|x| x.state.is_terminal()) {
                        note(out, seq, "ExecAutoFixDone", s.account, s.group);
                        if let Some(c) = self.cells.get_mut(&(s.account.0, s.group.0)) {
                            c.phase = 4;
                        }
                    }
                }
            }
            OrderEventKind::Reject => {
                self.terminal(ix, SlotState::Rejected, now);
                if self.has("oms-429-fails") && e.reject_code == 429 {
                    let sg: i128 = if s.side == Side::Buy { 1 } else { -1 };
                    *self.pos.entry((s.account.0, s.venue.0, s.inst.0)).or_default() +=
                        sg * i128::from(s.qty);
                }
                if s.link != 0 {
                    self.on_link_reject(out, seq, tsc, s.link, s.hedge);
                }
            }
            OrderEventKind::Expired | OrderEventKind::CancelAck => {
                self.terminal(
                    ix,
                    if e.what == OrderEventKind::Expired {
                        SlotState::Expired
                    } else {
                        SlotState::Cancelled
                    },
                    now,
                );
                if s.link != 0 && !s.hedge {
                    self.close_link_if_done(s.link);
                }
                if s.link != 0 && s.hedge {
                    self.on_link_reject(out, seq, tsc, s.link, true);
                }
            }
            OrderEventKind::OutcomeUnknown => {
                if !s.state.is_terminal() {
                    self.slots[ix].state = SlotState::Unknown;
                } else {
                    transitioned = false;
                }
            }
            _ => transitioned = false,
        }
        if transitioned {
            note(out, seq, "OmsTransition", s.account, s.group);
        } else {
            self.oms_dup += 1;
        }
    }

    fn terminal(&mut self, ix: usize, st: SlotState, now: i64) {
        if !self.slots[ix].state.is_terminal() {
            self.slots[ix].state = st;
            self.slots[ix].terminal_ens = now.max(1);
        }
    }

    fn on_fee_only(&mut self, out: &mut StepOut, seq: u64, mut e: OrderEvent) {
        let cur = self.slots.iter().find(|s| {
            s.venue == e.venue
                && s.account == e.account
                && s.ix == e.slot.0
                && s.r#gen == e.r#gen
                && e.epoch == 0
        });
        let tomb = self.tombs.iter().find(|s| {
            s.venue == e.venue
                && s.account == e.account
                && s.slot == e.slot.0
                && s.r#gen == e.r#gen
                && s.epoch == e.epoch
        });
        let owner = cur
            .map(|s| (s.inst, s.group))
            .or_else(|| tomb.map(|s| (s.inst, s.group)));
        let group = if let Some((inst, group)) = owner {
            e.inst = inst;
            group
        } else {
            note(out, seq, "OmsFeeUnattributed", e.account, GroupId(0));
            GroupId(0)
        };
        // A zero fill applies only its fee and produces zero Fill evidence.
        self.book_fill(out, &e, 0);
        if group.0 != 0
            && !self.has("ignore-fee")
            && e.fee_asset.0 != 0
            && self
                .inst(e.inst)
                .is_some_and(|i| !i.perp && i.base == e.fee_asset)
        {
            let leg = u8::from(self.group(e.account, group).is_some_and(|g| g.second == e.inst));
            let delta = -i128::from(e.fee.0);
            *self.leg_net.entry((e.account.0, group.0, leg)).or_default() += delta;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_link_fill(
        &mut self,
        out: &mut StepOut,
        seq: u64,
        tsc: u64,
        id: u32,
        hedge: bool,
        net: i128,
        gross: i128,
    ) {
        let Some(li) = self.links.iter().position(|l| l.id == id) else {
            return;
        };
        if hedge {
            self.links[li].hedge_filled += gross;
            self.links[li].owed -= gross;
            if self.links[li].owed <= 0 {
                self.links[li].hedge_in_flight = false;
            }
            self.close_link_if_done(id);
            return;
        }
        let l = self.links[li].clone();
        let lot = self.inst(l.hedge_inst).map_or(1, |x| i128::from(x.lot));
        let key = (l.account.0, l.group.0);
        let res = self.residual.get(&key).copied().unwrap_or(0);
        let basis = net + res;
        let mut q = basis / lot * lot;
        if self.has("hedge-gross-qty") {
            q = (gross + res) / lot * lot;
        }
        // The hedge never exceeds the group's naked quantity in its direction (naked cap).
        let naked = self.group_naked(l.account, l.group);
        let naked_dir = match l.first_side {
            Side::Buy => naked.max(0),
            Side::Sell => (-naked).max(0),
        };
        let cap = naked_dir / lot * lot;
        self.residual.insert(key, basis - q);
        if q > cap && !self.has("hedge-gross-qty") {
            *self.naked_capped.entry(key).or_default() += q - cap;
            q = cap;
        }
        self.links[li].first_net += net;
        self.links[li].hedge_req += q;
        self.links[li].state = LinkState::Hedging;
        self.links[li].owed += q;
        let side = match l.first_side {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        };
        if q > 0 {
            self.links[li].hedge_in_flight = true;
            self.place(
                out,
                tsc,
                seq,
                l.account,
                l.group,
                l.hedge_inst,
                side,
                q as i64,
                DecClass::Hedge,
                DecOrigin::Oms,
                Some(HedgeKind::Second),
                id,
                basis,
                false,
            );
        } else {
            self.close_link_if_done(id);
        }
    }

    fn on_link_reject(&mut self, out: &mut StepOut, seq: u64, _tsc: u64, id: u32, hedge: bool) {
        let Some(li) = self.links.iter().position(|l| l.id == id) else {
            return;
        };
        if !hedge {
            self.close_link_if_done(id);
            return;
        }
        self.links[li].retries += 1;
        let l = self.links[li].clone();
        if l.retries >= BROKEN_RETRIES {
            let due = self.now_ns as i64 + self.world.risk.broken_auto_fix_ms * 1_000_000;
            let c = self.cells.entry((l.account.0, l.group.0)).or_default();
            if !c.broken {
                c.broken = true;
                c.phase = 1;
                c.due_ens = due;
                c.starts = 0;
                note(out, seq, "ExecBroken", l.account, l.group);
                self.timers.push((
                    due as u64,
                    TIMER_AUTOFIX,
                    u32::from(l.account.0) << 16 | (l.group.0 & 0xFFFF),
                ));
            }
            self.links[li].state = LinkState::Broken;
            return;
        }
        let at = self.now_ns + self.world.risk.hedge_retry_ms as u64 * 1_000_000;
        self.timers.push((at, TIMER_RETRY, id));
    }

    fn close_link_if_done(&mut self, id: u32) {
        let first_open = self
            .slots
            .iter()
            .any(|s| s.link == id && !s.hedge && !s.state.is_terminal());
        if let Some(ix) = self.links.iter().position(|l| l.id == id)
            && !first_open
            && self.links[ix].owed <= 0
            && matches!(self.links[ix].state, LinkState::Open | LinkState::Hedging)
        {
            // A finished link is folded into its group's totals (bounded state).
            let l = self.links.remove(ix);
            let t = self.retired.entry((l.account.0, l.group.0)).or_default();
            t.0 += l.first_net;
            t.1 += l.hedge_req;
            t.2 += l.hedge_filled;
        }
    }

    fn on_timer(&mut self, out: &mut StepOut, seq: u64, tsc: u64, kind: u32, arg: u32) {
        let now = self.now_ns as i64;
        match kind {
            TIMER_SWEEP => {
                if !self.has("slot-leak") {
                    let q = QUARANTINE_MS * 1_000_000;
                    let mut freed = Vec::new();
                    self.slots.retain(|s| {
                        let free = s.state.is_terminal() && now - s.terminal_ens >= q;
                        if free {
                            freed.push(TombRow {
                                venue: s.venue,
                                account: s.account,
                                slot: s.ix,
                                r#gen: s.r#gen,
                                epoch: 0,
                                inst: s.inst,
                                group: s.group,
                                filled: i128::from(s.filled),
                            });
                        }
                        !free
                    });
                    self.tombs.extend(freed);
                    let n = self.tombs.len();
                    if n > 256 {
                        self.tombs.drain(..n - 256);
                    }
                }
                self.timers.push((self.now_ns + 1_000_000_000, TIMER_SWEEP, 0));
            }
            TIMER_RETRY => {
                let Some(l) = self.links.iter().find(|l| l.id == arg).cloned() else {
                    return;
                };
                if l.state != LinkState::Hedging || l.owed <= 0 {
                    return;
                }
                let side = match l.first_side {
                    Side::Buy => Side::Sell,
                    Side::Sell => Side::Buy,
                };
                let lot = self.inst(l.hedge_inst).map_or(1, |x| i128::from(x.lot));
                let mut q = l.owed;
                if self.has("hedge-plus-2-lots") {
                    q += 2 * lot;
                }
                self.place(
                    out,
                    tsc,
                    seq,
                    l.account,
                    l.group,
                    l.hedge_inst,
                    side,
                    q as i64,
                    DecClass::Hedge,
                    DecOrigin::Oms,
                    Some(HedgeKind::SecondRetry),
                    l.id,
                    l.owed,
                    false,
                );
            }
            TIMER_AUTOFIX => {
                let a = AccountId((arg >> 16) as u8);
                let g = self
                    .world
                    .groups
                    .iter()
                    .find(|x| x.account == a && (x.group.0 & 0xFFFF) == (arg & 0xFFFF))
                    .cloned();
                if let Some(g) = g {
                    let primed = self
                        .cells
                        .get(&(a.0, g.group.0))
                        .is_some_and(|c| c.broken && c.phase == 1);
                    if primed || self.has("broken-repeat-autofix") {
                        self.autofix(out, tsc, seq, &g);
                    }
                    if self.has("broken-repeat-autofix") {
                        self.timers
                            .push((self.now_ns + 1_000_000_000, TIMER_AUTOFIX, arg));
                    }
                }
            }
            TIMER_PENDING => {
                let a = AccountId((arg >> 16) as u8);
                let ix = arg & 0xFFFF;
                for s in self.slots.iter_mut() {
                    if s.account == a
                        && s.ix == ix
                        && s.state == SlotState::PendingNew
                        && now - s.sent_ens >= 3_000_000_000
                    {
                        s.state = SlotState::Unknown;
                        if let Some(l) = self.links.iter_mut().find(|l| l.id == s.link && !s.hedge) {
                            l.state = LinkState::Expired;
                        }
                    }
                }
            }
            TIMER_L3 => {
                let a = AccountId(arg as u8);
                if !self.l3.get(&a.0).copied().unwrap_or(false) {
                    return;
                }
                let groups: Vec<GroupSpecW> = self
                    .world
                    .groups
                    .iter()
                    .filter(|g| g.account == a)
                    .cloned()
                    .collect();
                let mut any = false;
                for g in groups {
                    let Some(spec) = self.inst(g.second).cloned() else {
                        continue;
                    };
                    let pos = self
                        .pos
                        .get(&(a.0, spec.venue.0, g.second.0))
                        .copied()
                        .unwrap_or(0);
                    let inflight = self
                        .slots
                        .iter()
                        .any(|s| s.account == a && s.inst == g.second && !s.state.is_terminal());
                    if pos == 0 || inflight {
                        any |= pos != 0;
                        continue;
                    }
                    any = true;
                    let lot = i128::from(spec.lot);
                    let step = pos.abs() * i128::from(self.world.risk.force_step_bp) / 10_000;
                    let target_abs = ((pos.abs() - step.max(lot)) / lot * lot).max(0);
                    let mut q = pos.abs() - target_abs;
                    if self.has("l3-overshoot") {
                        q += lot;
                    }
                    out.info.force_targets.push(ForceTarget {
                        account: a,
                        group: g.group,
                        inst: g.second,
                        base: pos,
                        target: pos.signum() * target_abs,
                        strategy_target: None,
                    });
                    let side = if pos > 0 { Side::Sell } else { Side::Buy };
                    self.place(
                        out,
                        tsc,
                        seq,
                        a,
                        g.group,
                        g.second,
                        side,
                        q as i64,
                        DecClass::Flatten,
                        DecOrigin::Exec,
                        None,
                        0,
                        0,
                        true,
                    );
                }
                if any {
                    self.timers.push((self.now_ns + 1_000_000_000, TIMER_L3, arg));
                } else {
                    self.l3.insert(a.0, false);
                }
            }
            _ => {}
        }
    }

    /// Starts the bounded auto-fix of a Broken cell (once per Broken period).
    fn autofix(&mut self, out: &mut StepOut, tsc: u64, seq: u64, g: &GroupSpecW) {
        let a = g.account;
        let key = (a.0, g.group.0);
        let Some(c) = self.cells.get(&key).cloned() else {
            return;
        };
        if !c.broken || self.has("broken-never-autofix") {
            return;
        }
        if c.starts > 0 && !self.has("broken-repeat-autofix") {
            return;
        }
        if self.killed(a).1 {
            // HARD pauses the auto-fix (it starts once HARD is lifted).
            return;
        }
        let naked = self.group_naked(a, g.group);
        if naked == 0 && !self.has("broken-repeat-autofix") {
            // Nothing naked any more: the Broken period ends without an auto-fix.
            if let Some(c) = self.cells.get_mut(&key) {
                c.broken = false;
                c.phase = 0;
            }
            return;
        }
        out.info.autofix_starts.push((a, g.group));
        let lot = self.inst(g.first).map_or(1, |x| i128::from(x.lot));
        let q = naked.abs() / lot * lot;
        if let Some(c) = self.cells.get_mut(&key) {
            c.phase = 2;
            c.starts += 1;
            c.budget = naked.abs();
        }
        let side = if naked > 0 { Side::Sell } else { Side::Buy };
        self.place(
            out,
            tsc,
            seq,
            a,
            g.group,
            g.first,
            side,
            q as i64,
            DecClass::Hedge,
            DecOrigin::Exec,
            Some(HedgeKind::AutoFix),
            0,
            naked.abs(),
            false,
        );
    }

    /// A Drain is quiescent: no Open/Hedging link, no pending or immediate live slot, no hedge
    /// retry timer.
    fn quiescent(&self) -> bool {
        !self
            .links
            .iter()
            .any(|l| matches!(l.state, LinkState::Open | LinkState::Hedging))
            && !self.slots.iter().any(|s| {
                matches!(
                    s.state,
                    SlotState::PendingNew | SlotState::PendingCancel | SlotState::PendingAmend
                ) || (s.immediate && !s.state.is_terminal())
            })
            && !self.timers.iter().any(|t| t.1 == TIMER_RETRY)
    }

    fn finish_drain(&mut self) {
        self.drain.done = true;
        self.drain.exec_drain_ready = true;
        self.drain.hedge_retry_timers = self.timers.iter().filter(|t| t.1 == TIMER_RETRY).count() as u32;
        self.draining = false;
    }

    /// Per-step `advance`: due auto-fixes start in the step that reaches their due time.
    fn advance(&mut self, out: &mut StepOut, tsc: u64, seq: u64) {
        let now = self.now_ns as i64;
        let due: Vec<GroupSpecW> = self
            .world
            .groups
            .iter()
            .filter(|g| {
                self.cells
                    .get(&(g.account.0, g.group.0))
                    .is_some_and(|c| c.broken && c.phase == 1 && now >= c.due_ens)
            })
            .cloned()
            .collect();
        for g in due {
            self.autofix(out, tsc, seq, &g);
        }
    }

    fn on_cmd(&mut self, out: &mut StepOut, seq: u64, tsc: u64, body: &[u8]) {
        let Ok((h, c)) = ControlCmd::decode(body) else {
            return;
        };
        let _: CmdHdr = h;
        match c {
            ControlCmd::Kill { scope, hard, .. } => {
                let sc = if scope.account.0 == 0 {
                    CapScope::Global
                } else {
                    CapScope::Account(scope.account)
                };
                self.caps.push(Cap {
                    scope: sc,
                    hard,
                    set_seq: seq,
                });
                self.recompute_modes();
            }
            ControlCmd::SetMode { mode, .. } => {
                let lowered = mode != RunMode::Live;
                for v in self.desired.values_mut() {
                    *v = mode;
                }
                self.recompute_modes();
                if lowered && self.has("setmode-opens") {
                    let g = self.world.groups.first().cloned();
                    if let Some(g) = g {
                        self.next_link += 1;
                        let lot = self.inst(g.second).map_or(1, |x| x.lot);
                        self.place(
                            out,
                            tsc,
                            seq,
                            g.account,
                            g.group,
                            g.first,
                            Side::Buy,
                            lot,
                            DecClass::Open,
                            DecOrigin::Strategy,
                            None,
                            0,
                            0,
                            false,
                        );
                    }
                }
            }
            ControlCmd::Resume { .. } => {
                self.caps.retain(|c| c.hard);
                self.recompute_modes();
            }
            ControlCmd::Drain { .. } => {
                self.draining = true;
                if self.has("drain-early-done") {
                    self.finish_drain();
                }
            }
            _ => {}
        }
    }

    fn on_external(&mut self, out: &mut StepOut, seq: u64, tsc: u64, x: ExternalFill) {
        let key = ExternalKey::from_fill(&x);
        if let Some(key) = key {
            let full = (x.account, x.venue, x.inst, key);
            if self.ext_seen.contains(&full) {
                return;
            }
            self.ext_seen.push(full);
        }
        let s: i128 = if x.side == Side::Buy { 1 } else { -1 };
        let d = s * i128::from(x.qty.0);
        *self.pos.entry((x.account.0, x.venue.0, x.inst.0)).or_default() += d;
        if let Some(sp) = self.inst(x.inst).cloned()
            && !sp.perp
        {
            *self.inv.entry((x.account.0, x.venue.0, sp.base.0)).or_default() += d;
        }
        out.info.evidence.push(Evidence {
            kind: EvidenceKind::External,
            account: x.account,
            venue: x.venue,
            inst: x.inst,
            delta: d,
            ext_key: key,
        });
        note(out, seq, "ExternalFill", x.account, GroupId(0));
        if self.has("double-hedge-external") {
            let g = self
                .world
                .groups
                .iter()
                .find(|g| g.account == x.account && (g.first == x.inst || g.second == x.inst))
                .cloned();
            if let Some(g) = g {
                let other = if g.first == x.inst { g.second } else { g.first };
                let lot = self.inst(other).map_or(1, |i| i128::from(i.lot));
                let q = i128::from(x.qty.0) / lot * lot;
                let side = x.side;
                for _ in 0..2 {
                    self.place(
                        out,
                        tsc,
                        seq,
                        x.account,
                        g.group,
                        other,
                        side,
                        q as i64,
                        DecClass::Hedge,
                        DecOrigin::Exec,
                        Some(HedgeKind::ExternalDefault),
                        0,
                        i128::from(x.qty.0),
                        false,
                    );
                }
            }
        }
    }

    /// Processes one Input record (also used by replay).
    fn apply(&mut self, out: &mut StepOut, seq: u64, tsc: u64, kind: EventKind, body: &[u8]) {
        self.advance(out, tsc, seq);
        // A completed Drain is visible for the step that completed it only.
        if self.drain.done && !self.has("drain-early-done") {
            self.drain = DrainRow::default();
        }
        let rec = JRec {
            seq,
            end_seq: seq,
            tsc,
            kind,
            flags: RecordFlags::default(),
            body: body.to_vec(),
        };
        match kind {
            EventKind::BBO => {
                if let Some(b) = rec.read::<Bbo>() {
                    self.bbo.insert(b.h.inst.0, (b.bid_px.0, b.ask_px.0));
                    self.strategy(out, tsc, seq, b.h.inst);
                }
            }
            EventKind::ORDER_EVENT => {
                if let Some(e) = rec.try_read::<OrderEvent>() {
                    self.on_order_event(out, seq, tsc, e);
                }
            }
            EventKind::SEND_REPORT => {
                if let Some(s) = rec.try_read::<SendReport>()
                    && s.status != SendStatus::Written
                {
                    let now = self.now_ns as i64;
                    if let Some(ix) = self.slots.iter().position(|x| {
                        x.venue == s.venue && x.account == s.account && x.ix == s.slot.0 && x.r#gen == s.r#gen
                    }) {
                        let link = self.slots[ix].link;
                        let hedge = self.slots[ix].hedge;
                        if self.slots[ix].state == SlotState::PendingNew {
                            self.terminal(ix, SlotState::Rejected, now);
                            if self.has("gate-counts")
                                && let Some(l) = self.links.iter_mut().find(|l| l.id == link)
                            {
                                l.retries += 1;
                            }
                            if link != 0 && hedge {
                                let at = self.now_ns + self.world.risk.hedge_retry_ms as u64 * 1_000_000;
                                self.timers.push((at, TIMER_RETRY, link));
                            } else if link != 0 {
                                self.close_link_if_done(link);
                            }
                        }
                    }
                }
            }
            EventKind::EXTERNAL_FILL => {
                if let Some(x) = rec.try_read::<ExternalFill>() {
                    self.on_external(out, seq, tsc, x);
                }
            }
            EventKind::CONTROL_CMD => self.on_cmd(out, seq, tsc, body),
            EventKind::MARGIN_REPORT => {
                if let Some(m) = rec.read::<MarginReportHdr>()
                    && m.margin_ratio.0 >= 8_000_000
                    && !self.l3.get(&m.account.0).copied().unwrap_or(false)
                {
                    self.l3.insert(m.account.0, true);
                    self.timers.push((self.now_ns, TIMER_L3, u32::from(m.account.0)));
                }
            }
            EventKind::TIMER => {
                if let Some(t) = rec.read::<Timer>() {
                    self.on_timer(out, seq, tsc, t.owner as u32 >> 8, t.timer_id);
                }
            }
            _ => {}
        }
        if self.draining && self.quiescent() {
            self.finish_drain();
        }
    }

    fn fresh(world: &WorldSpec, planted: &[&'static str], tsc_hz: u64) -> ToyEngine {
        let mut bal = BTreeMap::new();
        for a in &world.accounts {
            for (v, asset, amt) in &a.balances {
                bal.insert((a.id.0, v.0, asset.0), *amt);
            }
        }
        let mut desired = BTreeMap::new();
        let mut effective = BTreeMap::new();
        for g in &world.groups {
            desired.insert((g.account.0, g.group.0), RunMode::Live);
            effective.insert((g.account.0, g.group.0), RunMode::Live);
        }
        ToyEngine {
            world: world.clone(),
            planted: planted.to_vec(),
            tsc_hz,
            j: MemJournal::default(),
            bbo: BTreeMap::new(),
            slots: Vec::new(),
            tombs: Vec::new(),
            gens: BTreeMap::new(),
            links: Vec::new(),
            pos: BTreeMap::new(),
            inv: BTreeMap::new(),
            bal,
            leg_net: BTreeMap::new(),
            residual: BTreeMap::new(),
            fee_adj: BTreeMap::new(),
            naked_capped: BTreeMap::new(),
            retired: BTreeMap::new(),
            caps: Vec::new(),
            desired,
            effective,
            cells: BTreeMap::new(),
            l3: BTreeMap::new(),
            ext_seen: Vec::new(),
            oms_dup: 0,
            stale_gen: 0,
            timers: vec![(1_000_000_000, TIMER_SWEEP, 0)],
            drain: DrainRow::default(),
            draining: false,
            next_link: 0,
            shadow_counter: 0,
            now_ns: 0,
            snap: None,
            inputs: 0,
        }
    }

    /// The snapshot of the current state (journal and nested snapshot excluded; the planted
    /// `snapshot-field` counter is not part of it).
    fn snapshot_state(&self) -> ToyEngine {
        let mut c = self.clone();
        c.j = MemJournal::default();
        c.snap = None;
        c.shadow_counter = 0;
        c
    }

    /// Engine time of a journal TSC.
    fn ns_of(&self, tsc: u64) -> u64 {
        (u128::from(tsc) * 1_000_000_000 / u128::from(self.tsc_hz.max(1))) as u64
    }

    /// Rebuilds from the latest snapshot plus the journal's later Input records (from scratch
    /// when there is no snapshot).
    fn replay(&self) -> ToyEngine {
        let (from, mut e) = match &self.snap {
            Some((seq, st)) => (*seq, (**st).clone()),
            None => (0, ToyEngine::fresh(&self.world, &self.planted, self.tsc_hz)),
        };
        let mut out = StepOut::default();
        for r in self.j.records() {
            if r.seq <= from || !is_input(r.kind, &r.body) {
                continue;
            }
            e.now_ns = e.ns_of(r.tsc);
            e.j.append(r.kind, r.flags, r.tsc, &r.body);
            out.clear();
            e.apply(&mut out, r.seq, r.tsc, r.kind, &r.body);
            e.j.end_event();
            if r.kind == EventKind::TIMER {
                e.pop_due_timer(r);
            }
        }
        e
    }

    fn pop_due_timer(&mut self, r: &JRec) {
        if let Some(t) = r.read::<Timer>()
            && let Some(p) = self
                .timers
                .iter()
                .position(|x| x.0 == t.deadline_tsc && x.1 == (t.owner as u32 >> 8) && x.2 == t.timer_id)
        {
            self.timers.remove(p);
        }
    }

    fn hashes_of(&self) -> ComponentHashes {
        let h = |s: String| xxhash_rust::xxh3::xxh3_64(s.as_bytes());
        let mut c = ComponentHashes {
            ledger: h(format!(
                "{:?}{:?}{:?}{:?}{:?}",
                self.pos, self.inv, self.bal, self.leg_net, self.ext_seen
            )),
            oms: h(format!(
                "{:?}{:?}{:?}{:?}{:?}{}{}{:?}{:?}",
                self.slots,
                self.tombs,
                self.links,
                self.residual,
                self.gens,
                self.oms_dup,
                self.stale_gen,
                self.naked_capped,
                self.retired
            )),
            risk: h(format!(
                "{:?}{:?}{:?}{:?}",
                self.caps, self.desired, self.effective, self.l3
            )),
            strategies: h(format!("{}", self.next_link)),
            book: h(format!("{:?}", self.bbo)),
            exec: h(format!("{:?}{:?}", self.cells, self.drain)),
            all: 0,
        };
        if self.has("snapshot-field") {
            c.exec ^= self.shadow_counter;
        }
        c.all = u128::from(h(format!("{c:?}{:?}", self.timers)));
        c
    }
}

/// Input records of the toy's journal (everything it journals first in a step).
fn is_input(kind: EventKind, body: &[u8]) -> bool {
    hft_event::classify(kind, body).is_ok_and(|r| r == hft_event::Role::Input)
        || kind == EventKind::CLOCK_STATUS
        || kind == EventKind::ADVICE
}

impl Sut for ToyEngine {
    type Matcher = ToyMatcher;

    fn boot(ctx: &BootCtx<'_>) -> Result<Self, BootFail> {
        for p in ctx.planted {
            if !PLANTED.iter().any(|(n, _)| n == p) {
                return Err(BootFail(format!("toy engine has no planted defect {p}")));
            }
        }
        Ok(ToyEngine::fresh(ctx.world, ctx.planted, ctx.tsc_hz))
    }

    fn process(&mut self, now_ns: u64, tsc: u64, input: &InputRec, out: &mut StepOut) {
        self.now_ns = now_ns;
        let seq = self
            .j
            .append(input.kind, RecordFlags::default(), tsc, &input.body);
        self.shadow_counter += 1;
        self.apply(out, seq, tsc, input.kind, &input.body);
        self.j.end_event();
        let from = self.j.records().iter().position(|r| r.seq == seq).unwrap_or(0);
        out.records.extend_from_slice(&self.j.records()[from..]);
        self.inputs += 1;
        if self.inputs.is_multiple_of(SNAPSHOT_EVERY) {
            let last = self.j.last_seq();
            self.snap = Some((last, Box::new(self.snapshot_state())));
        }
    }

    fn next_deadline(&self) -> Option<u64> {
        self.timers.iter().map(|t| t.0).min()
    }

    fn fire_timers(&mut self, now_ns: u64, tsc: u64, out: &mut StepOut) -> bool {
        let Some(p) = self
            .timers
            .iter()
            .enumerate()
            .filter(|(_, t)| t.0 <= now_ns)
            .min_by_key(|(i, t)| (t.0, *i))
            .map(|(i, _)| i)
        else {
            return false;
        };
        let (deadline, kind, arg) = self.timers.remove(p);
        let code = TimerCode::Oms(OmsTimer::Sweep);
        let (_, _, group) = code.encode().unwrap_or((3, 0, GroupId(0)));
        let t = Timer {
            timer_id: arg,
            owner: (kind << 8) as u16,
            _pad: 0,
            group,
            _pad2: 0,
            deadline_tsc: deadline,
        };
        let input = InputRec::of(&t, crate::dst::sut::Source::Engine);
        self.process(now_ns, tsc, &input, out);
        true
    }

    fn observe(&self, o: &mut Obs) {
        for ((a, v, i), q) in &self.pos {
            let asset = self.inst(InstId(*i)).map_or(AssetId(0), |x| x.base);
            o.positions.push(PosRow {
                account: AccountId(*a),
                venue: VenueId(*v),
                inst: InstId(*i),
                asset,
                qty: *q,
            });
        }
        for ((a, v, x), q) in &self.inv {
            o.inventory.push(AssetRow {
                account: AccountId(*a),
                venue: VenueId(*v),
                asset: AssetId(*x),
                qty: *q,
            });
        }
        for ((a, v, x), q) in &self.bal {
            o.balances.push(AssetRow {
                account: AccountId(*a),
                venue: VenueId(*v),
                asset: AssetId(*x),
                qty: *q,
            });
        }
        for ((a, g, l), n) in &self.leg_net {
            o.leg_net.push(LegNetRow {
                account: AccountId(*a),
                group: GroupId(*g),
                leg: *l,
                net: *n,
            });
        }
        for s in &self.slots {
            o.slots.push(SlotRow {
                venue: s.venue,
                account: s.account,
                slot: s.ix,
                r#gen: s.r#gen,
                epoch: 0,
                state: s.state,
                group: s.group,
                inst: s.inst,
                side: s.side,
                qty: i128::from(s.qty),
                filled: i128::from(s.filled),
                immediate: s.immediate,
                risk_reducing: s.rr,
                terminal_ens: s.terminal_ens,
            });
        }
        for a in &self.world.accounts {
            for v in &a.venues {
                let mine = self.slots.iter().filter(|s| s.account == a.id && s.venue == *v);
                let n = mine.clone().count() as u32;
                o.tables.push(SlotTableRow {
                    venue: *v,
                    account: a.id,
                    capacity: CAPACITY,
                    free: CAPACITY - n,
                    unknown_count: mine.filter(|s| s.state == SlotState::Unknown).count() as u32,
                    free_list_consistent: true,
                });
            }
        }
        o.tombs = self.tombs.clone();
        for ((a, gid), (net, req, filled)) in &self.retired {
            let g = self.group(AccountId(*a), GroupId(*gid));
            o.links.push(LinkRow {
                id: 0x8000_0000 | (*gid & 0xFFFF) | u32::from(*a) << 16,
                key: None,
                account: AccountId(*a),
                group: GroupId(*gid),
                plan: PlanKind::OnFill,
                state: LinkState::Done,
                hedge_leg: 1,
                hedge_inst: g.map_or(InstId(0), |g| g.second),
                ratio_num: g.map_or(1, |g| g.ratio_num),
                ratio_den: g.map_or(1, |g| g.ratio_den),
                first_net: *net,
                hedge_req: *req,
                hedge_filled: *filled,
                reversed: false,
                retries: 0,
                has_unknown: false,
            });
        }
        for l in &self.links {
            let g = self.group(l.account, l.group);
            o.links.push(LinkRow {
                id: l.id,
                key: None,
                account: l.account,
                group: l.group,
                plan: PlanKind::OnFill,
                state: l.state,
                hedge_leg: 1,
                hedge_inst: l.hedge_inst,
                ratio_num: g.map_or(1, |g| g.ratio_num),
                ratio_den: g.map_or(1, |g| g.ratio_den),
                first_net: l.first_net,
                hedge_req: l.hedge_req,
                hedge_filled: l.hedge_filled,
                reversed: false,
                retries: l.retries,
                has_unknown: self
                    .slots
                    .iter()
                    .any(|s| s.link == l.id && s.state == SlotState::Unknown),
            });
        }
        for g in &self.world.groups {
            let naked = self.group_naked(g.account, g.group);
            let mut imm = [0i128; 4];
            for s in self.slots.iter().filter(|s| {
                s.account == g.account && s.group == g.group && s.immediate && !s.state.is_terminal()
            }) {
                let leg = usize::from(s.inst == g.second);
                imm[leg] += i128::from(s.qty - s.filled);
            }
            o.legs.push(LegRow {
                account: g.account,
                group: g.group,
                unknown_slots: self
                    .slots
                    .iter()
                    .filter(|s| s.account == g.account && s.group == g.group && s.state == SlotState::Unknown)
                    .count() as u32,
                naked_qty: naked.abs(),
                naked_buy: (-naked).max(0),
                naked_sell: naked.max(0),
                max_pos_qty: g.max_pos_qty,
                residual: self.residual.get(&(g.account.0, g.group.0)).copied().unwrap_or(0),
                fee_adjust: self.fee_adj.get(&(g.account.0, g.group.0)).copied().unwrap_or(0),
                naked_capped: self
                    .naked_capped
                    .get(&(g.account.0, g.group.0))
                    .copied()
                    .unwrap_or(0),
                inflight_immediate: imm,
            });
            o.limits.push(LimitRow {
                account: g.account,
                asset: self.inst(g.first).map_or(AssetId(0), |x| x.base),
                max_pos_qty: g.max_pos_qty * 2,
            });
            let key = (g.account.0, g.group.0);
            o.cells.push(CellModeRow {
                account: g.account,
                strategy: g.instance,
                group: g.group,
                desired: self.desired.get(&key).copied().unwrap_or(RunMode::Live),
                effective: self.effective.get(&key).copied().unwrap_or(RunMode::Live),
                deferred_shadow: false,
                flat: naked == 0,
            });
            let c = self.cells.get(&key).cloned().unwrap_or_default();
            o.exec.push(ExecCellRow {
                account: g.account,
                group: g.group,
                broken: c.broken,
                serial_fallback: false,
                manual_hold: false,
                phase: match c.phase {
                    0 => AutoFixPhase::Idle,
                    1 => AutoFixPhase::Primed,
                    2 => AutoFixPhase::Running,
                    _ => AutoFixPhase::Done,
                },
                due_ens: c.due_ens,
                due_ens_recomputed: c.due_ens,
                budget: c.budget,
                autofix_filled: c.af_filled,
                fail_streak: c.fail_streak,
                fix_count: c.starts,
                engine_target_pending: false,
            });
            for side in [Side::Buy, Side::Sell] {
                for inst in [g.first, g.second] {
                    let q: i128 = self
                        .slots
                        .iter()
                        .filter(|s| {
                            s.account == g.account
                                && s.inst == inst
                                && s.side == side
                                && !s.state.is_terminal()
                        })
                        .map(|s| i128::from(s.qty - s.filled))
                        .sum();
                    if q != 0 {
                        o.inflight.push(InflightRow {
                            account: g.account,
                            inst,
                            side,
                            immediate: q,
                            resting: 0,
                        });
                    }
                }
            }
        }
        o.limits.dedup_by_key(|l| (l.account.0, l.asset.0));
        for c in &self.caps {
            o.caps.push(CapRow {
                source: 1,
                scope: c.scope,
                level: if c.hard {
                    CapLevel::Hard
                } else {
                    CapLevel::CloseOnly
                },
                venue_level: false,
                kill: true,
                hard: c.hard,
                set_seq: c.set_seq,
            });
        }
        for v in &self.world.venues {
            o.kill.push(crate::dst::invariants::kill_from_caps(o, v.id));
        }
        for a in &self.world.accounts {
            o.margin.push(MarginRow {
                account: a.id,
                level: if self.l3.get(&a.id.0).copied().unwrap_or(false) {
                    3
                } else {
                    0
                },
                engine_flatten: false,
                hard: self.killed(a.id).1,
            });
        }
        o.risk = RiskRow {
            max_hedge_notional: self.world.risk.max_hedge_notional,
            max_reduce_notional: self.world.risk.max_reduce_notional,
            force_step_bp: self.world.risk.force_step_bp,
            broken_auto_fix_ms: self.world.risk.broken_auto_fix_ms,
            reuse_quarantine_ms: QUARANTINE_MS,
            sweep_batch: SWEEP_BATCH,
        };
        for i in &self.world.insts {
            o.insts.push(InstRow {
                inst: i.id,
                venue: i.venue,
                asset: i.base,
                perp: i.perp,
                lot: i128::from(i.lot),
                min_qty: i128::from(i.min_qty),
            });
        }
        o.drain = self.drain;
        o.oms_dup = self.oms_dup;
        o.oms_stale_gen = self.stale_gen;
        o.ext_seen = self.ext_seen.clone();
    }

    fn hashes(&self) -> ComponentHashes {
        self.hashes_of()
    }

    fn rebuilt_hashes(&self) -> Result<Option<ComponentHashes>, String> {
        let mut r = self.replay();
        r.shadow_counter = 0;
        Ok(Some(r.hashes_of()))
    }

    fn seq(&self) -> u64 {
        self.j.last_seq()
    }

    fn journal(&self) -> &[JRec] {
        self.j.records()
    }

    fn crash_and_recover(&mut self, _point: CrashPoint, _input: Option<&InputRec>) -> Result<(), BootFail> {
        let mut j = self.j.clone();
        j.truncate_to_event_end();
        let keep = std::mem::replace(&mut self.j, j);
        let mut r = self.replay();
        r.j = self.j.clone();
        r.now_ns = self.now_ns;
        r.shadow_counter = self.shadow_counter;
        r.snap = self.snap.take();
        r.inputs = self.inputs;
        let _ = keep;
        *self = r;
        Ok(())
    }

    fn clean_restart(&mut self) -> Result<(), BootFail> {
        self.crash_and_recover(CrashPoint::SeqInput, None)
    }

    fn is_sim(&self, a: AccountId) -> bool {
        ToyEngine::is_sim(self, a)
    }
}
