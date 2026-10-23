//! Core slot mechanics: in-flight contributions, state transitions and their side effects, and the
//! output layer (direct emission, `pending_out` queues, `flush`).

use hft_event::{NoteLevel, OrderAction, OrderDecision};
use hft_strategy_sdk::{OrderRef, OrderUpdate, OrderUpdateKind};
use hft_types::{
    AccountId, GROUP_NONE, GroupId, InstId, Px, Qty, Rate, RejectBits, RoundMode, Side, SlotId, Tif, Tsc,
};

use crate::notes;
use crate::types::{
    OmsOut, OmsTimerKind, OmsTimerReq, OrderSlot, OrderState, Origin, OutItem, SlotExt, SlotFlags,
    SlotStamps, TransitionCause, link_note, out_kind,
};
use crate::{LinkAux, Oms};

/// What one slot contributes to the in-flight book, the LegTracker and the counters.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Contrib {
    pub(crate) on: bool,
    pub(crate) unknown: bool,
    pub(crate) pending: bool,
    pub(crate) account: AccountId,
    pub(crate) inst: InstId,
    pub(crate) sign: i64,
    pub(crate) imm: i64,
    pub(crate) rest: i64,
    pub(crate) notional: i64,
    pub(crate) open_class: bool,
    pub(crate) spot_buy: bool,
    pub(crate) perp: bool,
    pub(crate) lev: u16,
    pub(crate) group: GroupId,
    pub(crate) leg_ix: u8,
    pub(crate) link_id: u32,
    pub(crate) role: u8,
}

impl Contrib {
    const OFF: Contrib = Contrib {
        on: false,
        unknown: false,
        pending: false,
        account: AccountId(0),
        inst: InstId(0),
        sign: 0,
        imm: 0,
        rest: 0,
        notional: 0,
        open_class: false,
        spot_buy: false,
        perp: false,
        lev: 0,
        group: GROUP_NONE,
        leg_ix: SlotExt::NO_LEG,
        link_id: 0,
        role: 0,
    };
}

/// `IntentClass::Open` code.
pub(crate) const CLASS_OPEN: u8 = 1;

/// Whether a slot counts as immediate in-flight (README "在途的两类").
#[inline]
pub(crate) fn is_immediate(sl: &OrderSlot) -> bool {
    matches!(sl.tif, Tif::Ioc | Tif::Fok)
        || !sl.sflags.has(SlotFlags::ACKED)
        || sl.state == OrderState::Unknown
}

impl Oms {
    pub(crate) fn contrib(&self, ti: usize, s: u32) -> Contrib {
        let Some(t) = self.t(ti) else { return Contrib::OFF };
        let (Some(sl), Some(ex)) = (t.slots.get(s as usize), t.ext.get(s as usize)) else {
            return Contrib::OFF;
        };
        if !sl.state.open() {
            return Contrib::OFF;
        }
        let rem = sl.remaining().0;
        let imm_kind = is_immediate(sl);
        let info = self.inst(sl.inst);
        let notional = sl.px.mul_qty(Qty(rem), info.exp, RoundMode::HalfEven).0.0;
        Contrib {
            on: true,
            unknown: sl.state == OrderState::Unknown,
            pending: matches!(
                sl.state,
                OrderState::PendingNew | OrderState::PendingCancel | OrderState::PendingAmend
            ),
            account: sl.account,
            inst: sl.inst,
            sign: sl.side.sign(),
            imm: if imm_kind { rem } else { 0 },
            rest: if imm_kind { 0 } else { rem },
            notional,
            open_class: ex.class == CLASS_OPEN,
            spot_buy: info.spot && sl.side == Side::Buy,
            perp: !info.spot && info.valid,
            lev: ex.lev_x100,
            group: sl.group,
            leg_ix: ex.leg_ix,
            link_id: sl.link_id,
            role: ex.role,
        }
    }

    /// Adds (`k = 1`) or removes (`k = −1`) a contribution.
    pub(crate) fn apply(&mut self, ti: usize, c: &Contrib, k: i64) {
        if !c.on {
            return;
        }
        let both = c.imm.saturating_add(c.rest).saturating_mul(k);
        let imm = c.imm.saturating_mul(k);
        if let Some(p) = self.inflight.pos_mut(c.account, c.inst) {
            if c.sign > 0 {
                p.open_buy = Qty(p.open_buy.0.saturating_add(both));
                p.imm_buy = Qty(p.imm_buy.0.saturating_add(imm));
            } else {
                p.open_sell = Qty(p.open_sell.0.saturating_add(both));
                p.imm_sell = Qty(p.imm_sell.0.saturating_add(imm));
            }
        }
        let n = c.notional.saturating_mul(k);
        if let Some(vb) = self.inflight.venue.get_mut(ti) {
            if c.open_class {
                vb.open_notional = vb.open_notional.saturating_add(n);
                if c.perp {
                    let lev = if c.lev == 0 { 100 } else { i128::from(c.lev) };
                    let m = (i128::from(c.notional) * 100 / lev) as i64;
                    vb.open_margin = vb.open_margin.saturating_add(m.saturating_mul(k));
                }
            }
            if c.spot_buy {
                vb.buy_notional = vb.buy_notional.saturating_add(n);
            }
        }
        if c.open_class && c.group != GROUP_NONE {
            self.inflight.add_group(c.account, c.group, n);
        }
        if let Some(gi) = self.group_ix(c.account, c.group) {
            if let Some(e) = self.groups.get_mut(gi) {
                if let Some(l) = e.st.view.leg.get_mut(usize::from(c.leg_ix)) {
                    l.inflight_immediate =
                        Qty(l.inflight_immediate.0.saturating_add(imm.saturating_mul(c.sign)));
                    l.inflight_resting = Qty(l
                        .inflight_resting
                        .0
                        .saturating_add(c.rest.saturating_mul(k).saturating_mul(c.sign)));
                }
                if c.pending {
                    e.st.pending_slots = add_u16(e.st.pending_slots, k);
                }
                if c.unknown {
                    e.st.unknown_slots = add_u16(e.st.unknown_slots, k);
                }
            }
            self.recompute_view(gi);
        }
        if c.link_id != 0
            && let Some(i) = (c.link_id as usize).checked_sub(1)
            && let Some(a) = self.aux.get_mut(i)
        {
            a.open = if k > 0 {
                a.open.saturating_add(1)
            } else {
                a.open.saturating_sub(1)
            };
            if c.unknown {
                a.unknown = add_u16(a.unknown, k);
            }
            if c.role == hft_strategy_sdk::LinkRole::Reverse as u8 {
                a.rev_open = add_u16(a.rev_open, k);
            }
        }
        if c.unknown
            && let Some(t) = self.tm(ti)
        {
            t.unknown_count = if k > 0 {
                t.unknown_count.saturating_add(1)
            } else {
                t.unknown_count.saturating_sub(1)
            };
        }
    }

    /// Recomputes the derived `LegView` fields of a group.
    pub(crate) fn recompute_view(&mut self, gi: usize) {
        let Some(e) = self.groups.get(gi) else { return };
        let exp = self.inst(e.insts[0]).exp;
        let Some(e) = self.groups.get_mut(gi) else { return };
        let n = usize::from(e.n_legs).min(4);
        let mut sum: i128 = 0;
        let mut maxnet: i128 = 0;
        for l in e.st.view.leg.iter().take(n) {
            sum += i128::from(l.net.0) + i128::from(l.inflight_immediate.0);
            maxnet = maxnet.max(i128::from(l.net.0).abs());
        }
        let naked = sum.unsigned_abs().min(i64::MAX as u128) as i64;
        let px = e.st.last_px();
        let v = &mut e.st.view;
        v.naked_qty = Qty(naked);
        v.naked_ratio = if naked == 0 {
            Rate(0)
        } else if maxnet == 0 {
            Rate::MAX
        } else {
            Rate((i128::from(naked) * 1_000_000_000 / maxnet).min(i128::from(i64::MAX)) as i64)
        };
        v.naked_notional = px.mul_qty(Qty(naked), exp, RoundMode::HalfEven).0;
        let (u, p) = (e.st.unknown_slots, e.st.pending_slots);
        let v = &mut e.st.view;
        v.has_unknown = u > 0;
        v.has_pending = p > 0;
    }

    /// Changes slot `s` of table `ti` to `to` (in-flight, flags, counters, Notify output and the
    /// state-dependent side effects).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn transition(
        &mut self,
        ti: usize,
        s: u32,
        to: OrderState,
        cause: TransitionCause,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let was_ready = self.t(ti).is_some_and(|t| t.ready());
        let before = self.contrib(ti, s);
        let Some(t) = self.tm(ti) else { return };
        let Some(sl) = t.slots.get_mut(s as usize) else {
            return;
        };
        let from = sl.state;
        if to == OrderState::Unknown && from != OrderState::Unknown {
            sl.prev_state = from;
            sl.query_count = 0;
        }
        sl.state = to;
        sl.t_state = now;
        sl.seq_last = seq;
        let f = &mut sl.sflags.0;
        if !matches!(to, OrderState::PendingCancel | OrderState::Unknown) {
            *f &= !SlotFlags::CANCEL_SENT;
        }
        // A held cancel waits in PendingNew/Unknown; Live sends it (below), any other state drops it.
        let release = to == OrderState::Live && *f & SlotFlags::CANCEL_HELD != 0;
        if !matches!(to, OrderState::PendingNew | OrderState::Unknown) {
            *f &= !SlotFlags::CANCEL_HELD;
        }
        if !matches!(
            to,
            OrderState::PendingAmend | OrderState::PendingCancel | OrderState::Unknown
        ) {
            *f &= !SlotFlags::AMEND_SENT;
        }
        if to != OrderState::Unknown {
            *f &= !(SlotFlags::HIST_WAIT | SlotFlags::ALARMED | SlotFlags::REST_QUERY);
        }
        if to == OrderState::Live {
            *f |= SlotFlags::ACKED;
            *f &= !SlotFlags::AGED;
        }
        if to.terminal() {
            *f &= !SlotFlags::VERIFY;
        }
        let after = self.contrib(ti, s);
        self.apply(ti, &before, -1);
        self.apply(ti, &after, 1);
        let handle = self
            .t(ti)
            .and_then(|t| t.slots.get(s as usize))
            .map(OrderSlot::handle);
        if let Some(h) = handle {
            self.push_notify(h, from, to, cause, seq, out);
        }
        self.after_transition(ti, s, from, to, now, seq, out);
        self.check_ready(ti, was_ready, out);
        if release {
            self.cancel_slot(ti, s, TransitionCause::CancelReq, None, now, seq, out);
        }
    }

    /// Emits `OmsVenueReady` when the table turned ready.
    pub(crate) fn check_ready(&mut self, ti: usize, was_ready: bool, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        if !was_ready && t.ready() {
            let v = u64::from(t.venue.0) | u64::from(t.account.0) << 8;
            let st = self.acct_strategy.get(t.aix).copied().unwrap_or_default_sid();
            self.note(
                notes::note(
                    notes::VENUE_READY,
                    st,
                    GROUP_NONE,
                    InstId(0),
                    [v, 0, 0, 0, 0, 0, 0, 0],
                ),
                out,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn after_transition(
        &mut self,
        ti: usize,
        s: u32,
        from: OrderState,
        to: OrderState,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        if to == OrderState::Unknown && from != OrderState::Unknown {
            self.unknown_entered(ti, s, now, seq, out);
        }
        if to.terminal() {
            self.slot_terminal(ti, s, now, seq, out);
        }
        if from == OrderState::Unknown && to != OrderState::Unknown {
            self.unknown_left(ti, s, to, now, seq, out);
        }
    }

    /// Side effects of a slot reaching a terminal state.
    fn slot_terminal(&mut self, ti: usize, s: u32, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let (venue, account) = (t.venue, t.account);
        if t.smoke_slot == s {
            if let Some(t) = self.tm(ti) {
                t.smoke_slot = 0;
            }
            self.push_timer(
                OmsTimerKind::SmokeTtl,
                OmsTimerReq::smoke_id(account, venue),
                Tsc(0),
                true,
                out,
            );
        }
        self.link_slot_terminal(ti, s, now, seq, out);
    }

    /// The slot left Unknown: deferred smoke cancel, link resumption.
    fn unknown_left(&mut self, ti: usize, s: u32, to: OrderState, now: Tsc, seq: u64, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let Some(sl) = t.slots.get(s as usize) else { return };
        let link_id = sl.link_id;
        if to == OrderState::Live
            && sl.sflags.has(SlotFlags::SMOKE_DUE)
            && !sl.sflags.has(SlotFlags::CANCEL_SENT)
        {
            self.cancel_slot(ti, s, TransitionCause::SmokeTtl, None, now, seq, out);
        }
        if link_id != 0 {
            self.link_unknown_converged(link_id, now, seq, out);
        }
    }

    // ---------------------------------------------------------------------- outputs

    /// Pushes a direct Note (single-slot paths; capacity is sized for them).
    pub(crate) fn note(&mut self, n: hft_event::Note, out: &mut OmsOut) {
        let r = out.notes.push(n);
        debug_assert!(r.is_ok(), "OmsOut.notes overflow on a direct path");
    }

    pub(crate) fn push_notify(
        &mut self,
        h: OrderRef,
        from: OrderState,
        to: OrderState,
        cause: TransitionCause,
        seq: u64,
        out: &mut OmsOut,
    ) {
        self.push_item(
            OutItem {
                kind: out_kind::NOTIFY,
                action: to as u8,
                from: from as u8,
                cause: cause as u8,
                link_id: 0,
                cause_seq: seq,
                slot: h,
                _pad: 0,
            },
            out,
        );
    }

    pub(crate) fn push_timer(
        &mut self,
        kind: OmsTimerKind,
        id: u32,
        deadline: Tsc,
        cancel: bool,
        out: &mut OmsOut,
    ) {
        let k = match kind {
            OmsTimerKind::UnknownQuery => 1,
            OmsTimerKind::HedgeRetry => 2,
            OmsTimerKind::SmokeTtl => 3,
        };
        self.push_item(
            OutItem {
                kind: out_kind::TIMER,
                action: k,
                from: u8::from(cancel),
                cause: 0,
                link_id: id,
                cause_seq: deadline.0,
                slot: OrderRef::new(hft_types::VenueId(0), AccountId(0), 0, 0),
                _pad: 0,
            },
            out,
        );
    }

    pub(crate) fn push_link(&mut self, link_id: u32, note: u8, reason: u8, seq: u64, out: &mut OmsOut) {
        self.push_item(
            OutItem {
                kind: out_kind::LINK,
                action: note,
                from: reason,
                cause: 0,
                link_id,
                cause_seq: seq,
                slot: OrderRef::new(hft_types::VenueId(0), AccountId(0), 0, 0),
                _pad: 0,
            },
            out,
        );
    }

    pub(crate) fn push_slot_item(&mut self, kind: u8, h: OrderRef, seq: u64, out: &mut OmsOut) {
        self.push_item(
            OutItem {
                kind,
                action: 0,
                from: 0,
                cause: 0,
                link_id: 0,
                cause_seq: seq,
                slot: h,
                _pad: 0,
            },
            out,
        );
    }

    /// Global FIFO: emit now when nothing waits and the vectors have room, else queue.
    pub(crate) fn push_item(&mut self, it: OutItem, out: &mut OmsOut) {
        if self.gq.is_empty() && self.emit_item(&it, out) {
            return;
        }
        let ok = self.gq.push(it);
        debug_assert!(ok, "pending_out full");
    }

    /// Emits one global item; `false` (nothing emitted) when the vectors lack room.
    pub(crate) fn emit_item(&mut self, it: &OutItem, out: &mut OmsOut) -> bool {
        match it.kind {
            out_kind::NOTIFY => {
                if out.notes.room() < 2 || out.updates.room() < 1 {
                    return false;
                }
                self.emit_notify(it, out);
                true
            }
            out_kind::TIMER => {
                if out.timers.room() < 1 {
                    return false;
                }
                let kind = match it.action {
                    1 => OmsTimerKind::UnknownQuery,
                    2 => OmsTimerKind::HedgeRetry,
                    _ => OmsTimerKind::SmokeTtl,
                };
                let _ = out.timers.push(OmsTimerReq {
                    kind,
                    id: it.link_id,
                    deadline_tsc: Tsc(it.cause_seq),
                    cancel: it.from != 0,
                });
                true
            }
            out_kind::LINK => {
                if out.links.room() < 1 || out.notes.room() < 1 {
                    return false;
                }
                self.emit_link(it, out);
                true
            }
            out_kind::HEDGE => {
                if out.hedges.room() < 1
                    || out.notes.room() < 3
                    || out.links.room() < 1
                    || out.timers.room() < 1
                {
                    return false;
                }
                self.emit_hedge_item(it, out);
                true
            }
            out_kind::ALARM => {
                if out.notes.room() < 1 {
                    return false;
                }
                self.emit_alarm(it, out);
                true
            }
            out_kind::AGED => {
                if out.updates.room() < 1 {
                    return false;
                }
                self.emit_aged(it, out);
                true
            }
            _ => true,
        }
    }

    fn slot_by_ref(&self, h: OrderRef) -> Option<(&OrderSlot, &SlotExt)> {
        let t = self.table(h.venue, h.account)?;
        let sl = t.slot(h.slot)?;
        if sl.r#gen != h.r#gen {
            return None;
        }
        Some((sl, t.ext.get(h.slot as usize)?))
    }

    fn emit_notify(&mut self, it: &OutItem, out: &mut OmsOut) {
        let to = OrderState::try_from(it.action).unwrap_or(OrderState::Free);
        let from = OrderState::try_from(it.from).unwrap_or(OrderState::Free);
        let (fields, strategy, group, inst, tag, smoke, prev, reject) = match self.slot_by_ref(it.slot) {
            Some((sl, ex)) => (
                [
                    sl.filled.0 as u64,
                    sl.qty.0 as u64,
                    sl.px.0 as u64,
                    sl.exch_order_id,
                ],
                sl.strategy,
                sl.group,
                sl.inst,
                ex.tag,
                ex.origin == Origin::Command as u8,
                sl.prev_state as u64,
                sl.reject_code,
            ),
            None => (
                [0; 4],
                hft_types::StrategyId(0),
                GROUP_NONE,
                InstId(0),
                0,
                false,
                0,
                0,
            ),
        };
        let h = it.slot;
        let v0 = u64::from(h.venue.0)
            | u64::from(h.account.0) << 8
            | u64::from(it.from) << 16
            | u64::from(it.action) << 24
            | u64::from(it.cause) << 32;
        let v = [
            v0,
            u64::from(h.slot),
            u64::from(h.r#gen),
            it.cause_seq,
            fields[0],
            fields[1],
            fields[2],
            fields[3],
        ];
        let _ = out
            .notes
            .push(notes::note(notes::TRANSITION, strategy, group, inst, v));
        if to == OrderState::Unknown && from != OrderState::Unknown {
            let u0 = u64::from(h.venue.0) | u64::from(h.account.0) << 8 | u64::from(it.cause) << 16;
            let _ = out.notes.push(notes::note(
                notes::UNKNOWN,
                strategy,
                group,
                inst,
                [u0, u64::from(h.slot), u64::from(h.r#gen), prev, 0, 0, 0, 0],
            ));
        }
        if smoke {
            return;
        }
        let kind = match to {
            OrderState::Live if matches!(from, OrderState::PendingNew | OrderState::Unknown) => {
                Some(OrderUpdateKind::Acked)
            }
            OrderState::Cancelled if from != to => Some(OrderUpdateKind::Cancelled),
            OrderState::Rejected if from != to => {
                let local = it.cause == TransitionCause::LocalSendFail as u8;
                Some(OrderUpdateKind::Rejected {
                    bits: if local {
                        RejectBits::LOCAL_SEND_FAIL
                    } else {
                        RejectBits(0)
                    },
                    venue_code: reject,
                })
            }
            OrderState::Unknown if from != to => Some(OrderUpdateKind::Unknown),
            _ => None,
        };
        if let Some(kind) = kind {
            let _ = out.updates.push(OrderUpdate {
                r: h,
                group,
                tag,
                kind,
            });
        }
    }

    fn emit_alarm(&mut self, it: &OutItem, out: &mut OmsOut) {
        let h = it.slot;
        let (strategy, group, inst, prev) = match self.slot_by_ref(h) {
            Some((sl, _)) => (sl.strategy, sl.group, sl.inst, sl.prev_state as u64),
            None => (hft_types::StrategyId(0), GROUP_NONE, InstId(0), 0),
        };
        let u0 = u64::from(h.venue.0) | u64::from(h.account.0) << 8 | u64::from(it.cause) << 16;
        let _ = out.notes.push(notes::note_at(
            notes::UNKNOWN,
            NoteLevel::P0,
            strategy,
            group,
            inst,
            [u0, u64::from(h.slot), u64::from(h.r#gen), prev, 0, 0, 0, 0],
        ));
    }

    fn emit_aged(&mut self, it: &OutItem, out: &mut OmsOut) {
        if let Some((sl, ex)) = self.slot_by_ref(it.slot) {
            let u = OrderUpdate {
                r: it.slot,
                group: sl.group,
                tag: ex.tag,
                kind: OrderUpdateKind::Aged,
            };
            let _ = out.updates.push(u);
        }
    }

    // ------------------------------------------------------------------ decisions

    /// Effective K of this Input (`max_out_per_event`, bounded by the vector).
    #[inline]
    fn k(&self, out: &OmsOut) -> usize {
        usize::from(self.p.k).min(out.decisions.capacity())
    }

    fn decisions_of(out: &OmsOut, v: hft_types::VenueId, a: AccountId) -> usize {
        out.decisions
            .iter()
            .filter(|d| d.venue == v && d.account == a)
            .count()
    }

    fn decision_room(&self, ti: usize, out: &OmsOut) -> bool {
        let Some(t) = self.t(ti) else { return false };
        out.decisions.len() < self.k(out)
            && Self::decisions_of(out, t.venue, t.account) < usize::from(t.out_quota)
    }

    /// Queues or emits an order-class decision of table `ti`.
    pub(crate) fn push_decision(
        &mut self,
        ti: usize,
        action: OrderAction,
        s: u32,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(t) = self.t(ti) else { return };
        let h = match t.slot(s) {
            Some(sl) => sl.handle(),
            None => OrderRef::new(t.venue, t.account, 0, 0),
        };
        let it = OutItem {
            kind: out_kind::DECISION,
            action: action as u8,
            from: 0,
            cause: 0,
            link_id: 0,
            cause_seq: seq,
            slot: h,
            _pad: 0,
        };
        let empty = t.q_rr.is_empty() && t.q_open.is_empty();
        // A slot with a queued decision does not queue another Query (the queue stays bounded by
        // the slot count; the next UnknownQuery timer asks again).
        if action == OrderAction::Query && t.slot(s).is_some_and(|x| x.sflags.has(SlotFlags::QUEUED)) {
            return;
        }
        if empty && self.decision_room(ti, out) {
            self.emit_decision(ti, &it, out);
            return;
        }
        self.enqueue_decision(ti, it);
    }

    pub(crate) fn enqueue_decision(&mut self, ti: usize, it: OutItem) {
        let rr = self.decision_is_rr(ti, &it);
        let Some(t) = self.tm(ti) else { return };
        if let Some(sl) = t.slots.get_mut(it.slot.slot as usize)
            && it.slot.slot != 0
            && sl.r#gen == it.slot.r#gen
        {
            sl.sflags.0 |= SlotFlags::QUEUED;
        }
        let ok = if rr { t.q_rr.push(it) } else { t.q_open.push(it) };
        debug_assert!(ok, "decision queue full");
        if ok {
            self.queued_decisions += 1;
        }
    }

    /// Risk-reducing decisions (cancels, queries, risk-reducing orders) go before Open ones.
    pub(crate) fn decision_is_rr(&self, ti: usize, it: &OutItem) -> bool {
        let new_or_amend = it.action == OrderAction::New as u8 || it.action == OrderAction::Amend as u8;
        if !new_or_amend {
            return true;
        }
        self.t(ti)
            .and_then(|t| t.slot(it.slot.slot))
            .is_some_and(|sl| sl.oflags & hft_event::OrderFlags::RISK_REDUCING.0 != 0)
    }

    /// Builds and pushes the `OrderDecision` of a decision item (drops stale cancels/queries).
    fn emit_decision(&mut self, ti: usize, it: &OutItem, out: &mut OmsOut) -> bool {
        let Some(t) = self.t(ti) else { return false };
        let (venue, account) = (t.venue, t.account);
        let action = match it.action {
            1 => OrderAction::New,
            2 => OrderAction::Cancel,
            3 => OrderAction::Amend,
            4 => OrderAction::CancelAll,
            _ => OrderAction::Query,
        };
        let d = if action == OrderAction::CancelAll {
            OrderDecision {
                trigger_seq: it.cause_seq,
                decided_tsc: Tsc(0),
                px: Px(0),
                qty: Qty(0),
                group: GROUP_NONE,
                inst: InstId(0),
                slot: SlotId(0),
                r#gen: 0,
                target_slot: SlotId(0),
                reject: 0,
                strategy: self.acct_strategy.get(t.aix).copied().unwrap_or_default_sid(),
                venue,
                account,
                action,
                side: Side::Buy,
                tif: Tif::Gtc,
                oflags: 0,
                reason: 0,
                intent_no: 0,
                req_px: Px(0),
                link_role: 0,
                _pad: [0; 7],
            }
        } else {
            let s = it.slot.slot;
            let (Some(sl), Some(ex)) = (t.slot(s), t.ext.get(s as usize)) else {
                return false;
            };
            if sl.r#gen != it.slot.r#gen || sl.state == OrderState::Free {
                return false;
            }
            let stale = match action {
                OrderAction::Cancel => !sl.state.open(),
                OrderAction::Amend => {
                    sl.state != OrderState::PendingAmend && sl.state != OrderState::PendingCancel
                }
                OrderAction::Query => {
                    !(sl.state == OrderState::Unknown || sl.sflags.has(SlotFlags::VERIFY))
                        || sl.sflags.has(SlotFlags::REST_QUERY | SlotFlags::HIST_WAIT)
                        || sl.query_count > self.p.unknown_max_queries
                }
                _ => false,
            };
            if stale {
                return false;
            }
            let (px, qty) = if action == OrderAction::Amend {
                (sl.pending_px, sl.pending_qty)
            } else {
                (sl.px, sl.qty)
            };
            let target = match action {
                OrderAction::New => self.first_leg_slot(sl.link_id, ex.role),
                _ => s,
            };
            OrderDecision {
                trigger_seq: it.cause_seq,
                decided_tsc: Tsc(0),
                px,
                qty,
                group: sl.group,
                inst: sl.inst,
                slot: SlotId(s),
                r#gen: sl.r#gen,
                target_slot: SlotId(target),
                reject: 0,
                strategy: sl.strategy,
                venue,
                account,
                action,
                side: sl.side,
                tif: sl.tif,
                oflags: sl.oflags,
                reason: if action == OrderAction::New { ex.reason } else { 0 },
                intent_no: if action == OrderAction::New {
                    ex.intent_no
                } else {
                    0
                },
                req_px: px,
                link_role: if action == OrderAction::New { ex.role } else { 0 },
                _pad: [0; 7],
            }
        };
        let _ = out.decisions.push(d);
        true
    }

    /// Slot of the first leg for a linked second leg / retry / reverse; 0 otherwise.
    fn first_leg_slot(&self, link_id: u32, role: u8) -> u32 {
        use hft_strategy_sdk::LinkRole;
        if link_id == 0
            || !(role == LinkRole::Second as u8
                || role == LinkRole::SecondRetry as u8
                || role == LinkRole::Reverse as u8)
        {
            return 0;
        }
        (link_id as usize)
            .checked_sub(1)
            .and_then(|i| self.links.get(i))
            .map_or(0, |e| e.first.slot)
    }

    /// Hands out queued work within this Input's budget: the global FIFO first, then the decision
    /// queues round-robin over tables (one per table per round, risk-reducing first).
    pub(crate) fn flush(&mut self, out: &mut OmsOut) {
        while let Some(it) = self.gq.front() {
            if !self.emit_item(&it, out) {
                break;
            }
            self.gq.pop();
        }
        if self.queued_decisions == 0 {
            return;
        }
        loop {
            let mut progressed = false;
            for ti in 0..self.tables.len() {
                if out.decisions.len() >= self.k(out) {
                    return;
                }
                let Some(t) = self.t(ti) else { continue };
                if t.q_rr.is_empty() && t.q_open.is_empty() {
                    continue;
                }
                if Self::decisions_of(out, t.venue, t.account) >= usize::from(t.out_quota) {
                    continue;
                }
                // Pop the next live item of this table (stale ones are dropped without using budget).
                while let Some(it) = self.pop_decision(ti) {
                    progressed = true;
                    if self.emit_decision(ti, &it, out) {
                        break;
                    }
                }
            }
            if !progressed {
                return;
            }
        }
    }

    fn pop_decision(&mut self, ti: usize) -> Option<OutItem> {
        let t = self.tm(ti)?;
        let it = match t.q_rr.pop() {
            Some(x) => x,
            None => t.q_open.pop()?,
        };
        if let Some(sl) = t.slots.get_mut(it.slot.slot as usize)
            && it.slot.slot != 0
            && sl.r#gen == it.slot.r#gen
        {
            sl.sflags.0 &= !SlotFlags::QUEUED;
        }
        self.queued_decisions = self.queued_decisions.saturating_sub(1);
        Some(it)
    }

    /// `true` while `pending_out` holds work (the live driver then writes `Timer{OmsContinue}`).
    pub fn wants_continue(&self) -> bool {
        !self.gq.is_empty() || self.queued_decisions > 0
    }

    /// `Timer{OmsContinue}`: hands out up to K more decisions and the other queued outputs.
    pub fn on_continue(&mut self, _now: Tsc, _cause_seq: u64, out: &mut OmsOut) {
        self.flush(out);
    }

    // ------------------------------------------------------------------ small helpers

    /// Strategy instance of an account (exec/smoke owner).
    pub(crate) fn strategy_of(&self, a: AccountId) -> hft_types::StrategyId {
        self.aix(a)
            .and_then(|i| self.acct_strategy.get(i))
            .copied()
            .unwrap_or_default_sid()
    }

    /// Clears the latency stamp of a slot.
    pub(crate) fn clear_stamp(&mut self, ti: usize, s: u32) {
        if let Some(st) = self.tm(ti).and_then(|t| t.stamps.get_mut(s as usize)) {
            st.op = SlotStamps::OP_NONE;
        }
    }

    pub(crate) fn link_aux(&self, link_id: u32) -> LinkAux {
        (link_id as usize)
            .checked_sub(1)
            .and_then(|i| self.aux.get(i))
            .copied()
            .unwrap_or_default()
    }

    /// Emits a `Link` item's `LinkUpdate` (and its riding Note).
    fn emit_link(&mut self, it: &OutItem, out: &mut OmsOut) {
        let Some(i) = (it.link_id as usize).checked_sub(1) else {
            return;
        };
        let (Some(e), Some((tol, plan))) = (self.links.get(i).copied(), self.plans.get(i).copied()) else {
            return;
        };
        let u = crate::types::LinkUpdate {
            link_id: it.link_id,
            account: e.account,
            group: e.group,
            first: e.first,
            state: e.state,
            broken_reason: self.broken_reason(i),
            flags: e.flags & crate::types::link_flags::RESUME_HOLD,
            first_net: e.first_net,
            hedge_filled: e.hedge_filled,
            owed: e.owed(),
            tolerance: tol,
            residual_policy: plan.residual,
        };
        let _ = out.links.push(u);
        let st = self.strategy_of(e.account);
        let inst = self.first_inst(i);
        match it.action {
            link_note::HEDGE_BROKEN => {
                let n = notes::note(
                    notes::HEDGE_BROKEN,
                    st,
                    e.group,
                    inst,
                    [
                        u64::from(it.link_id),
                        u64::from(self.broken_reason(i) as u8),
                        e.first_net.0 as u64,
                        e.hedge_filled.0 as u64,
                        e.owed().0 as u64,
                        u64::from(e.retries),
                        0,
                        0,
                    ],
                );
                let _ = out.notes.push(n);
            }
            link_note::RESUME_TIMEOUT_P0 | link_note::RESUME_TIMEOUT_P1 => {
                let lvl = if it.action == link_note::RESUME_TIMEOUT_P0 {
                    NoteLevel::P0
                } else {
                    NoteLevel::P1
                };
                let (rv, ra) = self.resume_venue(i);
                let n = notes::note_at(
                    notes::RESUME_TIMEOUT,
                    lvl,
                    st,
                    e.group,
                    inst,
                    [
                        u64::from(it.link_id),
                        u64::from(ra.0) | u64::from(rv.0) << 8,
                        u64::from(e.held_ms),
                        e.owed().0 as u64,
                        u64::from(it.from),
                        e.first_net.0 as u64,
                        0,
                        0,
                    ],
                );
                let _ = out.notes.push(n);
            }
            _ => {}
        }
    }
}

#[inline]
fn add_u16(x: u16, k: i64) -> u16 {
    if k > 0 {
        x.saturating_add(1)
    } else {
        x.saturating_sub(1)
    }
}

/// `unwrap_or(StrategyId(0))` for options of `StrategyId`.
pub(crate) trait OrZeroSid {
    fn unwrap_or_default_sid(self) -> hft_types::StrategyId;
}
impl OrZeroSid for Option<hft_types::StrategyId> {
    #[inline]
    fn unwrap_or_default_sid(self) -> hft_types::StrategyId {
        self.unwrap_or(hft_types::StrategyId(0))
    }
}
