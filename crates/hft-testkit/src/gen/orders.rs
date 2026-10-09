//! `order_lifecycle` (README "2. proptest 生成器"): report sequences legal under a venue's rules —
//! Ack → PartialFill* → Fill / CancelAck / Expired, with fill-before-ack, cancel-reject-because-
//! filled, amend races and fee-only reports.

use hft_event::OrderEventKind;
use proptest::prelude::*;

/// Report semantics of a venue (core venues use stub values).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueRules {
    /// Fills may arrive before the Ack.
    pub fill_before_ack: bool,
    /// Amends exist (and can race fills).
    pub amend: bool,
    /// Fees can arrive in a separate fee-only report.
    pub fee_only: bool,
}

impl VenueRules {
    /// Stub rules of the three core venues (index 0, 1, 2).
    pub const CORE: [VenueRules; 3] = [
        VenueRules {
            fill_before_ack: true,
            amend: true,
            fee_only: false,
        },
        VenueRules {
            fill_before_ack: false,
            amend: true,
            fee_only: true,
        },
        VenueRules {
            fill_before_ack: true,
            amend: false,
            fee_only: true,
        },
    ];
}

/// One report of a generated lifecycle (`qty` = this fill, `cum` = cumulative filled).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LifeEvent {
    pub what: OrderEventKind,
    pub qty: i64,
    pub cum: i64,
    pub fee: i64,
}

/// Lifecycle shapes (for coverage accounting).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Shape {
    Filled,
    Cancelled,
    Expired,
    FillBeforeAck,
    CancelRejectFilled,
    AmendRace,
    FeeOnly,
}

/// A generated order: its quantity, the shape and the report sequence.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Lifecycle {
    pub order_qty: i64,
    pub shape: Shape,
    pub events: Vec<LifeEvent>,
}

impl LifeEvent {
    const fn new(what: OrderEventKind, qty: i64, cum: i64, fee: i64) -> LifeEvent {
        LifeEvent { what, qty, cum, fee }
    }
}

/// Terminal report kinds.
pub fn is_terminal(e: &LifeEvent, order_qty: i64) -> bool {
    match e.what {
        OrderEventKind::Fill => e.cum == order_qty,
        OrderEventKind::CancelAck | OrderEventKind::Expired => true,
        _ => false,
    }
}

fn shapes(r: VenueRules) -> Vec<Shape> {
    let mut v = vec![
        Shape::Filled,
        Shape::Cancelled,
        Shape::Expired,
        Shape::CancelRejectFilled,
    ];
    if r.fill_before_ack {
        v.push(Shape::FillBeforeAck);
    }
    if r.amend {
        v.push(Shape::AmendRace);
    }
    if r.fee_only {
        v.push(Shape::FeeOnly);
    }
    v
}

/// Partial fills of `q` in `k` pieces (each ≥ 1).
fn split(q: i64, cuts: &[u32]) -> Vec<i64> {
    let k = (cuts.len() as i64).min(q - 1).max(0) as usize;
    let mut pts: Vec<i64> = cuts
        .iter()
        .take(k)
        .map(|c| 1 + i64::from(*c) % (q - 1).max(1))
        .collect();
    pts.sort_unstable();
    pts.dedup();
    let mut out = Vec::new();
    let mut last = 0;
    for p in pts {
        if p > last && p < q {
            out.push(p - last);
            last = p;
        }
    }
    out.push(q - last);
    out
}

fn fills(evs: &mut Vec<LifeEvent>, parts: &[i64], start_cum: i64, total: i64, fee_on_fills: bool) -> i64 {
    let mut cum = start_cum;
    for (i, p) in parts.iter().enumerate() {
        cum += p;
        let last = i + 1 == parts.len() && cum == total;
        let what = if last {
            OrderEventKind::Fill
        } else {
            OrderEventKind::PartialFill
        };
        evs.push(LifeEvent::new(
            what,
            *p,
            cum,
            if fee_on_fills { (p / 1_000).max(1) } else { 0 },
        ));
    }
    cum
}

/// Lifecycles of one order of 1–1000 lots (lot = 1) under `rules`.
pub fn order_lifecycle(rules: VenueRules) -> impl Strategy<Value = Lifecycle> {
    let sh = shapes(rules);
    (
        1i64..=1_000,
        0..sh.len(),
        proptest::collection::vec(any::<u32>(), 0..4),
    )
        .prop_map(move |(q, si, cuts)| {
            use OrderEventKind as K;
            let shape = sh[si];
            let mut ev = Vec::new();
            match shape {
                Shape::Filled => {
                    ev.push(LifeEvent::new(K::Ack, 0, 0, 0));
                    fills(&mut ev, &split(q, &cuts), 0, q, true);
                }
                Shape::FillBeforeAck => {
                    let parts = split(q, &cuts);
                    let (first, rest) = parts.split_at(1);
                    let c = fills(&mut ev, first, 0, q, true);
                    ev.push(LifeEvent::new(K::Ack, 0, c, 0));
                    fills(&mut ev, rest, c, q, true);
                }
                Shape::Cancelled => {
                    ev.push(LifeEvent::new(K::Ack, 0, 0, 0));
                    let part = if q > 1 {
                        split(q, &cuts).first().copied().unwrap_or(0).min(q - 1)
                    } else {
                        0
                    };
                    let c = if part > 0 {
                        fills(&mut ev, &[part], 0, q, true)
                    } else {
                        0
                    };
                    ev.push(LifeEvent::new(K::CancelAck, 0, c, 0));
                }
                Shape::Expired => {
                    ev.push(LifeEvent::new(K::Ack, 0, 0, 0));
                    let part = if q > 1 { (q / 2).max(1) } else { 0 };
                    let c = if part > 0 {
                        fills(&mut ev, &[part], 0, q, true)
                    } else {
                        0
                    };
                    ev.push(LifeEvent::new(K::Expired, 0, c, 0));
                }
                Shape::CancelRejectFilled => {
                    ev.push(LifeEvent::new(K::Ack, 0, 0, 0));
                    ev.push(LifeEvent::new(K::CancelReject, 0, 0, 0));
                    fills(&mut ev, &[q], 0, q, true);
                }
                Shape::AmendRace => {
                    ev.push(LifeEvent::new(K::Ack, 0, 0, 0));
                    let parts = split(q, &cuts);
                    let (first, rest) = parts.split_at(1);
                    let c = fills(&mut ev, first, 0, q, true);
                    ev.push(LifeEvent::new(
                        if c == q { K::AmendReject } else { K::AmendAck },
                        0,
                        c,
                        0,
                    ));
                    fills(&mut ev, rest, c, q, true);
                }
                Shape::FeeOnly => {
                    ev.push(LifeEvent::new(K::Ack, 0, 0, 0));
                    fills(&mut ev, &split(q, &cuts), 0, q, false);
                    ev.push(LifeEvent::new(K::FeeOnly, 0, q, (q / 1_000).max(1)));
                }
            }
            Lifecycle {
                order_qty: q,
                shape,
                events: ev,
            }
        })
}
