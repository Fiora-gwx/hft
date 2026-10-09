//! Row-by-row reference of the README "分类" table (RSK-013).

use hft_strategy_sdk::{IntentClass, LinkRole};

/// Kinds of intent listed in the classification table.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Kind {
    /// OMS second leg / retry (`hedge_reduce_only`), not simultaneous.
    Second {
        retry: bool,
        reduce_only: bool,
    },
    /// Second leg of a `Simultaneous` link.
    SimultaneousSecond,
    Reverse,
    HedgeFix,
    Flatten,
    Reduce,
    Smoke,
    Open,
}

/// `(class, risk_reducing, reduce_only)` from the table; `venue_ro` = `VenueCaps.reduce_only`.
pub fn reference(kind: Kind, venue_ro: bool, qty: i64, pos: i64, pending: i64) -> (IntentClass, bool, bool) {
    match kind {
        Kind::Second { reduce_only, .. } => (IntentClass::Hedge, true, reduce_only && venue_ro),
        Kind::SimultaneousSecond => (IntentClass::Open, false, false),
        Kind::Reverse | Kind::HedgeFix => (IntentClass::Hedge, true, venue_ro),
        Kind::Flatten => (IntentClass::Flatten, true, venue_ro),
        Kind::Reduce => {
            let rr = i128::from(qty) <= i128::from(pos).abs() - i128::from(pending);
            (IntentClass::Reduce, rr, venue_ro)
        }
        Kind::Smoke | Kind::Open => (IntentClass::Open, false, false),
    }
}

/// The draft's `(link_role, simultaneous, hedge_reduce_only, class hint)` of a kind.
pub fn draft_parts(kind: Kind) -> (Option<LinkRole>, bool, bool, IntentClass) {
    match kind {
        Kind::Second { retry, reduce_only } => (
            Some(if retry {
                LinkRole::SecondRetry
            } else {
                LinkRole::Second
            }),
            false,
            reduce_only,
            IntentClass::Hedge,
        ),
        Kind::SimultaneousSecond => (Some(LinkRole::Second), true, false, IntentClass::Hedge),
        Kind::Reverse => (Some(LinkRole::Reverse), false, false, IntentClass::Hedge),
        Kind::HedgeFix => (Some(LinkRole::HedgeFix), false, false, IntentClass::Open),
        Kind::Flatten => (None, false, false, IntentClass::Flatten),
        Kind::Reduce => (None, false, false, IntentClass::Reduce),
        Kind::Smoke | Kind::Open => (None, false, false, IntentClass::Open),
    }
}
