//! Foreign reports (defensive path), `StaleGen`, tombstone late fills, `FeeOnly`, and the
//! LegTracker adjustments from outside (`external_adjust`, `net_adjust`, `adopt_adjust`).

use hft_event::{ExternalCause, OrderEvent, OrderEventKind};
use hft_types::{AccountId, GROUP_NONE, GroupId, InstId, Qty, Side, Tsc, VenueId};

use crate::Oms;
use crate::fsm::net_of;
use crate::notes;
use crate::types::{FeeOut, FillOut, ForeignFill, OmsOut, OrderState, anomaly, fill_flags};

/// `OrderEvent.detail` of a fill report = `Side` code (0 = not provided).
fn side_of(detail: u8) -> Option<Side> {
    match detail {
        1 => Some(Side::Buy),
        2 => Some(Side::Sell),
        _ => None,
    }
}

impl Oms {
    /// `OrderEvent{slot = SLOT_UNRESOLVED}`.
    pub(crate) fn foreign_report(
        &mut self,
        ti: usize,
        ev: &OrderEvent,
        _now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        let Some(t) = self.t(ti) else { return };
        let (venue, account) = (t.venue, t.account);
        let st = self.strategy_of(account);
        if ev.what == OrderEventKind::FeeOnly {
            self.fee_unattributed(ti, ev, out);
            return;
        }
        if ev.cum_filled.0 <= 0 {
            self.stats.foreign_orders += 1;
            let v0 = u64::from(venue.0) | u64::from(account.0) << 8;
            let n = notes::note(
                notes::FOREIGN_ORDER,
                st,
                GROUP_NONE,
                ev.inst,
                [
                    v0,
                    ev.exch_order_id,
                    ev.what as u64,
                    u64::from(ev.reject_code),
                    0,
                    0,
                    0,
                    0,
                ],
            );
            self.note(n, out);
            return;
        }
        let Some(d) = self.foreign_increment(ti, ev, out) else {
            return;
        };
        let v0 = u64::from(venue.0) | u64::from(account.0) << 8;
        let n = notes::note(
            notes::FOREIGN_FILL_VIA_ORDER_EVENT,
            st,
            GROUP_NONE,
            ev.inst,
            [
                v0,
                ev.exch_order_id,
                d.0 as u64,
                ev.cum_filled.0 as u64,
                0,
                0,
                0,
                0,
            ],
        );
        self.note(n, out);
        self.book_foreign(ti, ev, d, 1, seq, out);
    }

    /// Increment of a foreign order by its cumulative quantity (`foreign` table).
    fn foreign_increment(&mut self, ti: usize, ev: &OrderEvent, out: &mut OmsOut) -> Option<Qty> {
        if self.inst(ev.inst).perp_cash {
            out.cash_error = Some(crate::CashError::Untracked);
            return None;
        }
        if ev.exch_order_id == 0 {
            return None;
        }
        let t = self.t(ti)?;
        let prev = t.foreign.get(ev.exch_order_id).unwrap_or(Qty(0));
        let d = ev.cum_filled.0.saturating_sub(prev.0);
        if d <= 0 {
            return None;
        }
        let evicted = self
            .tm(ti)
            .is_some_and(|t| t.foreign.set(ev.exch_order_id, ev.cum_filled));
        if evicted {
            self.evict_note(ti, ev.exch_order_id, out);
        }
        Some(Qty(d))
    }

    pub(crate) fn evict_note(&mut self, ti: usize, eid: u64, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let v0 = u64::from(t.venue.0) | u64::from(t.account.0) << 8;
        let st = self.strategy_of(t.account);
        let n = notes::note(
            notes::ANOMALY,
            st,
            GROUP_NONE,
            InstId(0),
            [anomaly::FOREIGN_EVICT, v0, eid, 0, 0, 0, 0, 0],
        );
        self.note(n, out);
    }

    /// Deducts `ext_held` and emits `ForeignFill` + `Note{OmsForeignFill}`; returns whether booked.
    fn book_foreign(
        &mut self,
        ti: usize,
        ev: &OrderEvent,
        d: Qty,
        path: u64,
        seq: u64,
        out: &mut OmsOut,
    ) -> bool {
        let Some(t) = self.t(ti) else { return false };
        let (venue, account) = (t.venue, t.account);
        let st = self.strategy_of(account);
        let mut booked = d.0;
        if let Some(b) = t.ext_held.get(ev.exch_order_id)
            && b.0 > 0
        {
            let m = booked.min(b.0);
            booked -= m;
            if let Some(t) = self.tm(ti) {
                t.ext_held.set(ev.exch_order_id, Qty(b.0 - m));
            }
            let n = notes::note(
                notes::EXT_DEDUP,
                st,
                GROUP_NONE,
                ev.inst,
                [ev.exch_order_id, d.0 as u64, m as u64, 0, 0, 0, 0, 0],
            );
            self.note(n, out);
        }
        let Some(side) = side_of(ev.detail) else {
            return false;
        };
        if booked <= 0 {
            return false;
        }
        let px = if ev.avg_px.0 != 0 { ev.avg_px } else { ev.px };
        let ff = ForeignFill {
            venue,
            account,
            inst: ev.inst,
            side,
            qty: Qty(booked),
            px,
            fee: ev.fee.0,
            fee_asset: ev.fee_asset,
            exch_order_id: ev.exch_order_id,
            seq,
            trade_id: 0,
            cum_filled: ev.cum_filled,
        };
        let r = out.foreign_fills.push(ff);
        debug_assert!(r.is_ok(), "OmsOut.foreign_fills overflow");
        let v0 = u64::from(venue.0)
            | u64::from(account.0) << 8
            | u64::from(side as u8) << 16
            | path << 24
            | u64::from(ev.fee_asset.0) << 32;
        let n = notes::note(
            notes::FOREIGN_FILL,
            st,
            GROUP_NONE,
            ev.inst,
            [
                v0,
                booked as u64,
                px.0 as u64,
                ev.fee.0 as u64,
                ev.exch_order_id,
                0,
                ev.cum_filled.0 as u64,
                ev.cum_filled.0 as u64,
            ],
        );
        self.note(n, out);
        true
    }

    /// Neither the slot nor a tombstone matches.
    pub(crate) fn stale_gen_report(
        &mut self,
        ti: usize,
        ev: &OrderEvent,
        _now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) {
        if ev.what == OrderEventKind::FeeOnly {
            self.fee_unattributed(ti, ev, out);
            return;
        }
        if ev.cum_filled.0 <= 0 {
            self.stats.stale_gen += 1;
            return;
        }
        let Some(t) = self.t(ti) else { return };
        let (venue, account) = (t.venue, t.account);
        let st = self.strategy_of(account);
        let mut booked = false;
        if ev.exch_order_id != 0
            && let Some(d) = self.foreign_increment(ti, ev, out)
        {
            booked = self.book_foreign(ti, ev, d, 2, seq, out);
        }
        let v0 = u64::from(venue.0) | u64::from(account.0) << 8;
        let n = notes::note(
            notes::STALE_GEN_FILL,
            st,
            GROUP_NONE,
            ev.inst,
            [
                v0,
                u64::from(ev.slot.0),
                u64::from(booked),
                u64::from(ev.r#gen),
                u64::from(ev.epoch),
                ev.exch_order_id,
                ev.cum_filled.0 as u64,
                0,
            ],
        );
        self.note(n, out);
        let n = notes::note(
            notes::NEEDS_REST_QUERY,
            st,
            GROUP_NONE,
            ev.inst,
            [
                v0 | 1 << 16,
                u64::from(ev.slot.0),
                u64::from(ev.r#gen),
                u64::from(ev.epoch),
                ev.exch_order_id,
                0,
                0,
                0,
            ],
        );
        self.note(n, out);
    }

    /// A report matching a tombstone: late fill booked, nothing reopened. `false` = no tombstone.
    pub(crate) fn tomb_report(
        &mut self,
        ti: usize,
        ev: &OrderEvent,
        now: Tsc,
        seq: u64,
        out: &mut OmsOut,
    ) -> bool {
        let tg = self.p.tomb_gens;
        let Some(t) = self.t(ti) else { return false };
        let s = ev.slot.0;
        if s == 0 || s > t.n {
            return false;
        }
        let ix = t.tomb_ix(s, ev.r#gen, tg);
        let Some(tb) = t.tombs.get(ix).copied() else {
            return false;
        };
        if tb.r#gen == 0 || tb.r#gen != ev.r#gen || tb.epoch != ev.epoch {
            return false;
        }
        let (venue, account) = (t.venue, t.account);
        if ev.inst != tb.inst {
            out.cash_error = Some(crate::CashError::Identity);
            return true;
        }
        let cash = match crate::cash::delta(
            self.inst(tb.inst).perp_cash,
            (tb.filled, tb.cash_qty, tb.cash_sum),
            (ev.cum_filled, ev.cash_qty, ev.cash_sum),
        ) {
            Ok(x) => x,
            Err(e) => {
                out.cash_error = Some(e);
                return true;
            }
        };
        if ev.what == OrderEventKind::FeeOnly {
            let fo = FeeOut {
                account,
                venue,
                inst: tb.inst,
                group: tb.group,
                side: tb.side as u8,
                fee: ev.fee.0,
                fee_asset: ev.fee_asset,
            };
            let _ = out.fees.push(fo);
            return true;
        }
        let d = ev.cum_filled.0.saturating_sub(tb.filled.0);
        if d <= 0 {
            self.stats.dup += 1;
            return true;
        }
        let info = *self.inst(tb.inst);
        let net = net_of(Qty(d), ev.fee, ev.fee_asset, info.asset, info.spot, tb.side);
        let signed = net.0.saturating_mul(tb.side.sign());
        let px = if ev.avg_px.0 != 0 && ev.px.0 == 0 {
            ev.avg_px
        } else if ev.px.0 != 0 {
            ev.px
        } else {
            ev.avg_px
        };
        let fo = FillOut {
            slot: hft_strategy_sdk::OrderRef::new(venue, account, s, tb.r#gen),
            account,
            venue,
            inst: tb.inst,
            group: tb.group,
            leg_ix: tb.leg_ix,
            side: tb.side,
            class: 0,
            flags: fill_flags::LATE,
            qty: Qty(d),
            net_qty: Qty(signed),
            px,
            fee: ev.fee.0,
            fee_asset: ev.fee_asset,
            seq,
            cash,
        };
        let _ = out.fills.push(fo);
        self.leg_fill(account, tb.group, tb.leg_ix, Qty(signed), px, now);
        if let Some(t) = self.tm(ti)
            && let Some(x) = t.tombs.get_mut(ix)
        {
            x.filled = ev.cum_filled;
            if cash.is_some() {
                x.cash_qty = ev.cash_qty;
                x.cash_sum = ev.cash_sum;
            }
            x.net_filled = Qty(x.net_filled.0.saturating_add(net.0));
        }
        let v0 = u64::from(venue.0) | u64::from(account.0) << 8;
        let n = notes::note(
            notes::LATE_FILL,
            tb.strategy,
            tb.group,
            tb.inst,
            [
                v0,
                u64::from(s),
                u64::from(tb.r#gen),
                d as u64,
                ev.cum_filled.0 as u64,
                u64::from(tb.link_id),
                0,
                0,
            ],
        );
        self.note(n, out);
        true
    }

    /// `FeeOnly` of a matched slot: `FeeOut`, no transition; remembered for `net_adjust`.
    pub(crate) fn fee_only_slot(&mut self, ti: usize, s: u32, ev: &OrderEvent, out: &mut OmsOut) {
        let Some(sl) = self.t(ti).and_then(|t| t.slot(s)).copied() else {
            return;
        };
        let fo = FeeOut {
            account: sl.account,
            venue: sl.venue,
            inst: sl.inst,
            group: sl.group,
            side: sl.side as u8,
            fee: ev.fee.0,
            fee_asset: ev.fee_asset,
        };
        let _ = out.fees.push(fo);
        if let Some(x) = self.tm(ti).and_then(|t| t.slots.get_mut(s as usize)) {
            x.fee_accum = x.fee_accum.saturating_add(ev.fee.0);
        }
        self.fee_ctx = Some((ti, s));
    }

    fn fee_unattributed(&mut self, ti: usize, ev: &OrderEvent, out: &mut OmsOut) {
        let Some(t) = self.t(ti) else { return };
        let (venue, account) = (t.venue, t.account);
        let fo = FeeOut {
            account,
            venue,
            inst: ev.inst,
            group: GROUP_NONE,
            side: 0,
            fee: ev.fee.0,
            fee_asset: ev.fee_asset,
        };
        let _ = out.fees.push(fo);
        let st = self.strategy_of(account);
        let v0 = u64::from(venue.0) | u64::from(account.0) << 8;
        let n = notes::note(
            notes::FEE_UNATTRIBUTED,
            st,
            GROUP_NONE,
            ev.inst,
            [
                v0,
                u64::from(ev.slot.0),
                u64::from(ev.r#gen),
                ev.fee.0 as u64,
                u64::from(ev.fee_asset.0),
                0,
                0,
                0,
            ],
        );
        self.note(n, out);
    }

    /// External fill attributed by hft-ledger: signed net into the leg; `cause = Unknown` with an
    /// exchange id is remembered in `ext_held` for de-duplication.
    #[allow(clippy::too_many_arguments)]
    pub fn external_adjust(
        &mut self,
        a: AccountId,
        v: VenueId,
        g: GroupId,
        inst: InstId,
        net: Qty,
        exch_order_id: u64,
        cause: ExternalCause,
        out: &mut OmsOut,
    ) {
        if g != GROUP_NONE {
            self.leg_add(a, g, inst, net);
        }
        if cause == ExternalCause::Unknown
            && exch_order_id != 0
            && let Some(ti) = self.tix(v, a)
        {
            let prev = self
                .t(ti)
                .and_then(|t| t.ext_held.get(exch_order_id))
                .unwrap_or(Qty(0));
            let evicted = self.tm(ti).is_some_and(|t| {
                t.ext_held
                    .set(exch_order_id, Qty(prev.0.saturating_add(net.0.saturating_abs())))
            });
            if evicted {
                self.evict_note(ti, exch_order_id, out);
            }
        }
    }

    /// `NetAdjust` from hft-ledger `apply_fee` (same event as the `FeeOnly`).
    pub fn net_adjust(&mut self, a: AccountId, g: GroupId, inst: InstId, delta: Qty) {
        self.leg_add(a, g, inst, delta);
        if let Some((ti, s)) = self.fee_ctx
            && let Some(t) = self.tm(ti)
            && let Some(sl) = t.slots.get(s as usize).copied()
            && sl.account == a
            && sl.inst == inst
            && sl.state != OrderState::Free
            && let Some(e) = t.ext.get_mut(s as usize)
        {
            e.net_filled = Qty(e
                .net_filled
                .0
                .saturating_add(delta.0.saturating_mul(sl.side.sign())));
        }
        self.residual_from_fee(a, g, inst, delta);
    }

    /// AdoptExchange: same-quantity LegTracker adjustment (no slot).
    pub fn adopt_adjust(&mut self, a: AccountId, g: GroupId, inst: InstId, qty: Qty) {
        if g != GROUP_NONE {
            self.leg_add(a, g, inst, qty);
        }
    }
}
