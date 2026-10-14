//! `classify`: the RISK_REDUCING / reduceOnly classification table (README "分类").

use hft_event::OrderFlags;
use hft_refdata::VenueCaps;
use hft_strategy_sdk::{IntentClass, LinkRole};
use hft_types::Qty;

use crate::types::{Classification, IntentDraft, Origin, PosView};

const fn c(class: IntentClass, risk_reducing: bool, reduce_only: bool) -> Classification {
    Classification {
        class,
        risk_reducing,
        reduce_only,
    }
}

/// `qty ≤ |pos| − pending` (i128, no overflow).
#[inline]
pub(crate) fn reduces(qty: Qty, pos: Qty, pending: Qty) -> bool {
    (qty.0 as i128) <= (pos.0 as i128).abs() - (pending.0 as i128)
}

/// Pure classification of one intent draft (README "分类" table).
#[inline]
pub fn classify(it: &IntentDraft, origin: Origin, pos: &impl PosView, caps: &VenueCaps) -> Classification {
    let ro = caps.reduce_only;
    if origin == Origin::Command && it.flags & OrderFlags::SMOKE.0 != 0 {
        return c(IntentClass::Open, false, false);
    }
    if origin == Origin::Strategy
        && it.link_role.is_none()
        && it.class == IntentClass::Hedge
        && it.qty.0 > 0
        && it.qty <= pos.hedge_cap_qty(it.side)
    {
        let closes_local = match it.side {
            hft_types::Side::Buy => pos.pos().0 < 0,
            hft_types::Side::Sell => pos.pos().0 > 0,
        } && reduces(it.qty, pos.pos(), pos.pending_reduce(it.side));
        return c(IntentClass::Hedge, true, ro && closes_local);
    }
    match it.link_role {
        Some(LinkRole::Second | LinkRole::SecondRetry) => {
            if it.simultaneous {
                if it.hedge_reduce_only {
                    c(
                        IntentClass::Reduce,
                        reduces(it.qty, pos.pos(), pos.pending_reduce(it.side)),
                        ro,
                    )
                } else {
                    c(IntentClass::Open, false, false)
                }
            } else {
                c(IntentClass::Hedge, true, it.hedge_reduce_only && ro)
            }
        }
        Some(LinkRole::Reverse | LinkRole::HedgeFix) => c(IntentClass::Hedge, true, ro),
        Some(LinkRole::First) | None => match it.class {
            IntentClass::Flatten => c(IntentClass::Flatten, true, ro),
            IntentClass::Reduce => c(
                IntentClass::Reduce,
                reduces(it.qty, pos.pos(), pos.pending_reduce(it.side)),
                ro,
            ),
            IntentClass::Open | IntentClass::Hedge => c(IntentClass::Open, false, false),
        },
    }
}

/// `Amend` keeps the original order's classification; it stays risk-reducing only when the
/// original was and the new quantity is not larger.
#[inline]
pub fn classify_amend(orig: Classification, orig_qty: Qty, new_qty: Qty) -> Classification {
    Classification {
        risk_reducing: orig.risk_reducing && new_qty.0 <= orig_qty.0,
        ..orig
    }
}
