//! Cumulative cash facts are additive across orders, independent of report delivery order.

use hft_types::Qty;

use crate::CashError;

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
