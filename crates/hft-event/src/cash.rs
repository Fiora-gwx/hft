//! Representation conversion for a matched order's execution cash facts. Callers own identity,
//! perpetual validation and atomic booking; this never obtains cash from price or balances.

use hft_types::Qty;

use crate::{OrderEvent, OrderEventKind};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CashFactError {
    Coverage,
    Missing,
    Overflow,
}

impl OrderEvent {
    /// Fold this execution into an already booked `(filled, cash-covered, cash-sum)` prefix.
    /// Duplicate/older executions cannot book cash twice. A gap fails before any caller state
    /// changes; a non-fill at the same watermark carries forward the known prefix.
    pub fn normalize_fill_cash(&mut self, previous: (Qty, Qty, Qty)) -> Result<(), CashFactError> {
        let (filled, covered, sum) = previous;
        if self.fill_cash_qty.0 == 0 {
            if self.fill_cash_amount.0 != 0 {
                return Err(CashFactError::Coverage);
            }
            if covered.0 > 0 && self.cash_qty.0 == 0 && self.cash_sum.0 == 0 && self.cum_filled == filled {
                self.cash_qty = covered;
                self.cash_sum = sum;
            }
            return Ok(());
        }
        if self.fill_cash_qty.0 < 0
            || self.fill_cash_qty != self.qty
            || self.cum_filled.0 < self.qty.0
            || self.cash_qty.0 != 0
            || self.cash_sum.0 != 0
            || !matches!(self.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
        {
            return Err(CashFactError::Coverage);
        }
        if self.cum_filled < filled {
            // The amount is ignored below the booked watermark, as for cumulative facts.
            self.cash_qty = self.cum_filled;
            self.cash_sum = Qty(0);
        } else if self.cum_filled == filled {
            if covered != filled {
                return Err(CashFactError::Missing);
            }
            self.cash_qty = covered;
            self.cash_sum = sum;
        } else {
            if covered != filled || self.cum_filled.0.checked_sub(filled.0) != Some(self.fill_cash_qty.0) {
                return Err(CashFactError::Missing);
            }
            self.cash_sum = Qty(sum
                .0
                .checked_add(self.fill_cash_amount.0)
                .ok_or(CashFactError::Overflow)?);
            self.cash_qty = self.cum_filled;
        }
        Ok(())
    }
}
