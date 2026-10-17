//! Cumulative cash facts are additive across orders, independent of report delivery order.

use hft_types::{AccountId, InstId, Qty, VenueId};
use zerocopy::FromZeros;

use crate::CashError;
use crate::types::ForeignCashReceipt;

pub(crate) fn normalize_execution(
    event: &hft_event::OrderEvent,
    previous: (Qty, Qty, Qty),
    spot: bool,
    strict: bool,
) -> Result<hft_event::OrderEvent, CashError> {
    let mut event = *event;
    if spot && (event.fill_cash_qty.0 != 0 || event.fill_cash_amount.0 != 0) {
        return Err(CashError::Identity);
    }
    if !strict || event.fill_cash_qty.0 != 0 || event.fill_cash_amount.0 != 0 {
        event.normalize_fill_cash(previous).map_err(|e| match e {
            hft_event::CashFactError::Coverage => CashError::Coverage,
            hft_event::CashFactError::Missing => CashError::Missing,
            hft_event::CashFactError::Overflow => CashError::Overflow,
        })?;
    }
    Ok(event)
}

impl crate::Oms {
    /// Preflight provenance and all distinct new order identities in one reconciliation.
    /// No mutation/allocation; budget: oms/reconcile_cash_guard.
    pub fn reconcile_cash_guard(&self, batch: &hft_event::ReconcileBatchView<'_>) -> Result<(), CashError> {
        let t = self
            .tix(batch.hdr.venue, batch.hdr.account)
            .and_then(|i| self.t(i))
            .ok_or(CashError::Identity)?;
        let free = t.foreign_cash.iter().filter(|r| r.exch_order_id == 0).count();
        let mut needed = 0;
        for (i, e) in batch.fills().enumerate() {
            if self.external_cash_guard(&e)? {
                return Err(CashError::ExternalOverlap);
            }
            if e.exch_order_id != 0
                && !self.has_foreign_cash(e.venue, e.account, e.inst, e.exch_order_id)
                && !batch
                    .fills()
                    .take(i)
                    .any(|r| r.inst == e.inst && r.exch_order_id == e.exch_order_id)
            {
                needed += 1;
            }
        }
        if needed > free {
            Err(CashError::Capacity)
        } else {
            Ok(())
        }
    }

    /// Read-only exchange-order provenance lookup; bounded by `foreign_fill_track`.
    /// Budget: oms/foreign_cash_seen.
    pub fn has_foreign_cash(&self, venue: VenueId, account: AccountId, inst: InstId, eid: u64) -> bool {
        eid != 0
            && self.tix(venue, account).and_then(|i| self.t(i)).is_some_and(|t| {
                t.foreign_cash
                    .iter()
                    .take_while(|r| r.exch_order_id != 0)
                    .any(|r| r.exch_order_id == eid && r.inst == inst)
            })
    }

    /// Validate provenance/capacity before booking an original external cash execution.
    /// `true` proves this same financial receipt was already booked, even after FIFO wrap.
    /// Budget: oms/external_cash_guard. No mutation or allocation.
    pub fn external_cash_guard(&self, e: &hft_event::ExternalFill) -> Result<bool, CashError> {
        if e.exch_order_id == 0 {
            return Ok(false);
        }
        // Only rows the section 12 restore accepts may be retained (a strict-cash perpetual of
        // the reporting venue); anything else is rejected before it can be stored.
        let inst = self.inst(e.inst);
        if !inst.valid || !inst.perp_cash || inst.venue != e.venue {
            return Err(CashError::Identity);
        }
        let t = self
            .tix(e.venue, e.account)
            .and_then(|i| self.t(i))
            .ok_or(CashError::Identity)?;
        if let Some((ix, r)) = t
            .foreign_cash
            .iter()
            .enumerate()
            .find(|(_, r)| r.exch_order_id == e.exch_order_id && r.inst == e.inst)
        {
            if r.side != 0 {
                return Err(CashError::ExternalOverlap);
            }
            if let Some(receipt) = t.foreign_receipts.get(ix)
                && receipt.exch_order_id != 0
                && receipt.same_key(e)
            {
                if e.cash_qty != e.qty || e._pad != [0; 6] {
                    return Err(CashError::Missing);
                }
                if *receipt != ForeignCashReceipt::from_external(e) {
                    return Err(CashError::Conflict);
                }
                return Ok(true);
            }
            return Ok(false);
        }
        if t.foreign_cash.iter().any(|r| r.exch_order_id == 0) {
            Ok(false)
        } else {
            Err(CashError::Capacity)
        }
    }

    /// Retain provenance after ledger validation. `booked` is true only when this Input
    /// actually booked the full execution; a duplicate/confirmation cannot create new proof.
    /// The caller first calls external_cash_guard; this never evicts another identity.
    /// Budget: oms/remember_external_cash.
    pub fn remember_external_cash(
        &mut self,
        e: &hft_event::ExternalFill,
        booked: bool,
    ) -> Result<(), CashError> {
        self.external_cash_guard(e)?;
        if e.exch_order_id == 0 {
            return Ok(());
        }
        let proof = booked
            && self.inst(e.inst).perp_cash
            && e.cause == hft_event::ExternalCause::Unknown
            && e.qty.0 > 0
            && e.cash_qty == e.qty
            && e.px.0 > 0
            && e._pad == [0; 6]
            && ((e.trade_id > 0 && e.fill_seq == 0) || (e.trade_id == 0 && e.fill_seq == e.qty.0 as u64));
        let t = self
            .tix(e.venue, e.account)
            .and_then(|i| self.tm(i))
            .ok_or(CashError::Identity)?;
        if let Some(ix) = t
            .foreign_cash
            .iter()
            .position(|r| r.exch_order_id == e.exch_order_id && r.inst == e.inst)
        {
            // A later distinct execution makes this a multi-fill source. Retain the opaque
            // order identity but discard the single-execution proof permanently.
            if let Some(r) = t.foreign_receipts.get_mut(ix)
                && r.exch_order_id != 0
                && !r.same_key(e)
            {
                *r = ForeignCashReceipt::new_zeroed();
            }
            return Ok(());
        }
        if let Some((ix, r)) = t
            .foreign_cash
            .iter_mut()
            .enumerate()
            .find(|(_, r)| r.exch_order_id == 0)
        {
            *r = crate::types::ForeignCashRow {
                venue: e.venue,
                account: e.account,
                side: 0,
                _pad: 0,
                inst: e.inst,
                exch_order_id: e.exch_order_id,
                filled: Qty(0),
                cash_sum: Qty(0),
            };
            if proof && let Some(r) = t.foreign_receipts.get_mut(ix) {
                *r = ForeignCashReceipt::from_external(e);
            }
        }
        Ok(())
    }

    pub(crate) fn foreign_cash_increment(
        &mut self,
        ti: usize,
        e: &hft_event::OrderEvent,
    ) -> Result<(Qty, Qty), CashError> {
        let t = self.t(ti).ok_or(CashError::Identity)?;
        if e.exch_order_id == 0
            || !matches!(
                e.what,
                hft_event::OrderEventKind::PartialFill | hft_event::OrderEventKind::Fill
            )
        {
            return Err(CashError::Untracked);
        }
        // A wrong epoch/slot must never reclassify a known system order as new foreign cash.
        if (e.slot.0 != 0
            && !self.has_foreign_cash(e.venue, e.account, e.inst, e.exch_order_id)
            && t.slot(e.slot.0).is_some_and(|s| s.r#gen >= e.r#gen))
            || t.slots.iter().any(|s| {
                s.state != crate::OrderState::Free && s.inst == e.inst && s.exch_order_id == e.exch_order_id
            })
            || t.tombs
                .iter()
                .any(|s| s.r#gen != 0 && s.inst == e.inst && s.exch_order_id == e.exch_order_id)
        {
            return Err(CashError::Untracked);
        }
        if self.inst(e.inst).venue != e.venue
            || !matches!(e.detail, 1 | 2)
            || (if e.avg_px.0 != 0 { e.avg_px.0 } else { e.px.0 }) <= 0
        {
            return Err(CashError::Identity);
        }
        let ix = t
            .foreign_cash
            .iter()
            .position(|r| r.exch_order_id == e.exch_order_id && r.inst == e.inst);
        let (filled, sum) = if let Some(ix) = ix {
            let r = t.foreign_cash.get(ix).ok_or(CashError::Identity)?;
            if r.side == 0 {
                let proof = t.foreign_receipts.get(ix).ok_or(CashError::ExternalOverlap)?;
                proof.validate_first_duplicate(e)?;
                return Ok((Qty(0), Qty(0)));
            }
            if r.side != e.detail {
                return Err(CashError::Identity);
            }
            (r.filled, r.cash_sum)
        } else {
            (Qty(0), Qty(0))
        };
        if t.ext_held.get(e.exch_order_id).is_some_and(|q| q.0 > 0) {
            return Err(CashError::ExternalOverlap);
        }
        let cash = delta(
            true,
            (filled, filled, sum),
            (e.cum_filled, e.cash_qty, e.cash_sum),
        )?
        .ok_or(CashError::Missing)?;
        if e.cum_filled.0 <= filled.0 {
            return Ok((Qty(0), Qty(0)));
        }
        let ix = ix
            .or_else(|| t.foreign_cash.iter().position(|r| r.exch_order_id == 0))
            .ok_or(CashError::Capacity)?;
        let row = self
            .tm(ti)
            .and_then(|t| t.foreign_cash.get_mut(ix))
            .ok_or(CashError::Identity)?;
        *row = crate::types::ForeignCashRow {
            venue: e.venue,
            account: e.account,
            side: e.detail,
            _pad: 0,
            inst: e.inst,
            exch_order_id: e.exch_order_id,
            filled: e.cum_filled,
            cash_sum: e.cash_sum,
        };
        Ok((Qty(e.cum_filled.0 - filled.0), cash))
    }
}

impl ForeignCashReceipt {
    fn from_external(e: &hft_event::ExternalFill) -> Self {
        Self {
            venue: e.venue,
            account: e.account,
            side: e.side as u8,
            _pad: 0,
            inst: e.inst,
            exch_order_id: e.exch_order_id,
            trade_id: e.trade_id,
            fill_seq: e.fill_seq,
            qty: e.qty,
            px: e.px,
            cash: e.cash_amount,
            fee: e.fee,
            fee_asset: e.fee_asset,
            _pad2: [0; 6],
        }
    }

    fn same_key(&self, e: &hft_event::ExternalFill) -> bool {
        self.trade_id == e.trade_id && (self.trade_id != 0 || self.fill_seq == e.fill_seq)
    }

    fn validate_first_duplicate(&self, e: &hft_event::OrderEvent) -> Result<(), CashError> {
        if self.exch_order_id == 0
            || self.trade_id != e.trade_id
            || (self.trade_id == 0 && self.fill_seq != e.cum_filled.0 as u64)
            || self.qty != e.qty
            || self.qty != e.cum_filled
        {
            return Err(CashError::ExternalOverlap);
        }
        if e.cash_qty.0 < 0 || e.cash_qty > e.cum_filled {
            return Err(CashError::Coverage);
        }
        if e.cash_qty != e.cum_filled {
            return Err(CashError::Missing);
        }
        if self.side != e.detail {
            return Err(CashError::Identity);
        }
        let px = if e.avg_px.0 != 0 { e.avg_px } else { e.px };
        if self.cash != e.cash_sum || self.px != px || self.fee != e.fee || self.fee_asset != e.fee_asset {
            return Err(CashError::Conflict);
        }
        Ok(())
    }
}

/// `(filled, covered, sum)` is scoped to one full order identity. An older report does not
/// rewind it; an equal watermark must agree exactly, including an explicit zero amount.
pub(crate) fn delta(
    required: bool,
    before: (Qty, Qty, Qty),
    after: (Qty, Qty, Qty),
) -> Result<Option<Qty>, CashError> {
    if !required {
        return Ok(None);
    }
    let (filled, covered, sum) = before;
    let (next, next_covered, next_sum) = after;
    if next.0 < 0 || next_covered.0 < 0 || next_covered.0 > next.0 || (next_covered.0 == 0 && next_sum.0 != 0)
    {
        return Err(CashError::Coverage);
    }
    if next.0 < filled.0 {
        return Ok(Some(Qty(0)));
    }
    if next_covered != next || covered != filled {
        return Err(CashError::Missing);
    }
    if next == filled && next_sum != sum {
        return Err(CashError::Conflict);
    }
    next_sum
        .0
        .checked_sub(sum.0)
        .map(|x| Some(Qty(x)))
        .ok_or(CashError::Overflow)
}

pub(crate) fn valid_cursor(required: bool, filled: Qty, covered: Qty, sum: Qty) -> bool {
    filled.0 >= 0
        && covered.0 >= 0
        && covered.0 <= filled.0
        && (covered.0 != 0 || sum.0 == 0)
        && (!required || covered == filled)
}
