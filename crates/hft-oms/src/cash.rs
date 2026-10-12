//! Cumulative cash facts are additive across orders, independent of report delivery order.

use hft_types::{AccountId, InstId, Qty, VenueId};

use crate::CashError;

impl crate::Oms {
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
    /// Budget: oms/external_cash_guard. No mutation or allocation.
    pub fn external_cash_guard(&self, e: &hft_event::ExternalFill) -> Result<(), CashError> {
        if e.exch_order_id == 0 {
            return Ok(());
        }
        let t = self
            .tix(e.venue, e.account)
            .and_then(|i| self.t(i))
            .ok_or(CashError::Identity)?;
        if let Some(r) = t
            .foreign_cash
            .iter()
            .find(|r| r.exch_order_id == e.exch_order_id && r.inst == e.inst)
        {
            return if r.side == 0 {
                Ok(())
            } else {
                Err(CashError::ExternalOverlap)
            };
        }
        if t.foreign_cash.iter().any(|r| r.exch_order_id == 0) {
            Ok(())
        } else {
            Err(CashError::Capacity)
        }
    }

    /// Retain the original-source identity after successful ledger validation. The caller
    /// first calls external_cash_guard; this never evicts another identity.
    /// Budget: oms/remember_external_cash.
    pub fn remember_external_cash(&mut self, e: &hft_event::ExternalFill) -> Result<(), CashError> {
        self.external_cash_guard(e)?;
        if e.exch_order_id == 0 {
            return Ok(());
        }
        let t = self
            .tix(e.venue, e.account)
            .and_then(|i| self.tm(i))
            .ok_or(CashError::Identity)?;
        if t.foreign_cash
            .iter()
            .any(|r| r.exch_order_id == e.exch_order_id && r.inst == e.inst)
        {
            return Ok(());
        }
        if let Some(r) = t.foreign_cash.iter_mut().find(|r| r.exch_order_id == 0) {
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
        if t.ext_held.get(e.exch_order_id).is_some_and(|q| q.0 > 0) {
            return Err(CashError::ExternalOverlap);
        }
        let ix = t
            .foreign_cash
            .iter()
            .position(|r| r.exch_order_id == e.exch_order_id && r.inst == e.inst);
        let (filled, sum) = if let Some(ix) = ix {
            let r = t.foreign_cash.get(ix).ok_or(CashError::Identity)?;
            if r.side == 0 {
                return Err(CashError::ExternalOverlap);
            }
            if r.side != e.detail {
                return Err(CashError::Identity);
            }
            (r.filled, r.cash_sum)
        } else {
            (Qty(0), Qty(0))
        };
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
