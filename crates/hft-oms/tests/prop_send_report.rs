#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
//! Scenario B-1 (docs/tests/scenarios.md): a `SendReport` that was not written leaves the
//! target slot's state alone; only a new order's failure moves it to `Rejected`. The reference is
//! the naive `(op, status)` table of `model.rs` (`send_report_table`).
use crate::common;
use crate::model::{SendEffect, SendSlot, send_report_table};

use common::*;
use hft_event::{ConnRole, ConnStateKind, GwOp, OrderAction, SendStatus};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::{OrderRef, OrderUpdateKind};
use hft_types::{Qty, RejectBits};
use proptest::prelude::*;

const STATUSES: [SendStatus; 7] = [
    SendStatus::Written,
    SendStatus::KillBlocked,
    SendStatus::RateLimited,
    SendStatus::NoConn,
    SendStatus::EncodeError,
    SendStatus::RingFull,
    SendStatus::WriteError,
];

/// Initial state of a generated slot (reached by legal operations).
#[derive(Copy, Clone, Debug)]
enum Init {
    PendingNew,
    Live,
    PendingCancel,
    PendingAmend,
    Unknown,
}

fn init() -> impl Strategy<Value = Init> {
    prop_oneof![
        Just(Init::PendingNew),
        Just(Init::Live),
        Just(Init::PendingCancel),
        Just(Init::PendingAmend),
        Just(Init::Unknown),
    ]
}

/// One report: (slot choice, op choice, status choice, duplicate).
type Rep = (u8, u8, u8, bool);

fn model_of(h: &H, r: OrderRef) -> SendSlot {
    let sl = h.slot(r);
    SendSlot {
        state: sl.state,
        sflags: sl.sflags.0,
        query_count: sl.query_count,
    }
}

/// The ops slot `m` has in flight (README "SendReport 处理"; Query only for Unknown/VERIFY).
fn in_flight(m: &SendSlot) -> Vec<GwOp> {
    let mut v = Vec::new();
    match m.state {
        OrderState::PendingNew => v.push(GwOp::Place),
        OrderState::PendingCancel if m.sflags & SlotFlags::CANCEL_SENT != 0 => {
            v.push(GwOp::Cancel);
            v.push(GwOp::CancelAll);
        }
        OrderState::PendingAmend if m.sflags & SlotFlags::AMEND_SENT != 0 => v.push(GwOp::Amend),
        _ => {}
    }
    if m.state == OrderState::Unknown || (m.state.open() && m.sflags & SlotFlags::VERIFY != 0) {
        v.push(GwOp::Query);
    }
    v
}

/// In-flight quantity of account 1: X buys (VS) + Y sells (VX).
fn open_qty(h: &H) -> i64 {
    h.o.inflight().pos(A1, X).open_buy.0 + h.o.inflight().pos(A1, Y).open_sell.0
}

fn run_case(inits: &[Init], reps: &[Rep]) -> Result<(), TestCaseError> {
    let mut h = H::new();
    // Unknown slots first: placed, then the order connection drops and comes back.
    let mut slots: Vec<OrderRef> = Vec::new();
    let unknown: Vec<usize> = (0..inits.len())
        .filter(|i| matches!(inits[*i], Init::Unknown))
        .collect();
    let mut by_ix: Vec<Option<OrderRef>> = vec![None; inits.len()];
    for &i in &unknown {
        by_ix[i] = Some(h.place_ok(&buy_x(), 1));
    }
    if !unknown.is_empty() {
        h.conn(VS, A1, ConnRole::Order, ConnStateKind::Down, 2);
        h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 3);
    }
    for (i, k) in inits.iter().enumerate() {
        if by_ix[i].is_some() {
            continue;
        }
        let r = if matches!(k, Init::PendingAmend) {
            OrderRef::new(VX, A1, 0, 0)
        } else {
            h.place_ok(&buy_x(), 4)
        };
        match k {
            Init::PendingNew | Init::Unknown => {}
            Init::Live => h.ack(r, 1_000 + u64::from(r.slot), 5),
            Init::PendingCancel => {
                h.ack(r, 1_000 + u64::from(r.slot), 5);
                h.cancel(r, 6).expect("cancel");
            }
            Init::PendingAmend => {
                // Amend needs an amend-capable venue (VX) and no pending New sample (OMS-072).
                let r = h.place_ok(&sell_y(), 4);
                h.ack(r, 2_000 + u64::from(r.slot), 5);
                h.o.test_stamps_mut(VX, A1, r.slot, |s| s.op = SlotStamps::OP_NONE);
                h.amend(r, p("60200.00"), q("0.008"), 6).expect("amend");
                by_ix[i] = Some(r);
                continue;
            }
        }
        by_ix[i] = Some(r);
    }
    slots.extend(by_ix.into_iter().flatten());
    let mut t = 10u64;
    for &(si, oi, sti, dup) in reps {
        let r = slots[usize::from(si) % slots.len()];
        let before: Vec<SendSlot> = slots.iter().map(|x| model_of(&h, *x)).collect();
        let me = model_of(&h, r);
        let ops = in_flight(&me);
        if ops.is_empty() {
            continue;
        }
        let op = ops[usize::from(oi) % ops.len()];
        let mut st = STATUSES[usize::from(sti) % STATUSES.len()];
        if op == GwOp::Query && sti % 8 == 7 {
            st = SendStatus::QueryUnsupported;
        }
        let ib0 = open_qty(&h);
        for k in 0..(1 + u32::from(dup)) {
            t += 1;
            let before_k: Vec<SendSlot> = slots.iter().map(|x| model_of(&h, *x)).collect();
            h.send(r.venue, r.account, op, r.slot, r.r#gen, st, t, t);
            // Every slot against the table: the target (or, for CancelAll, the table range).
            for (j, x) in slots.iter().enumerate() {
                let affected = *x == r || (op == GwOp::CancelAll && x.venue == r.venue);
                let want: SendEffect = if affected {
                    send_report_table(&before_k[j], op, st)
                } else {
                    SendEffect::unchanged(&before_k[j])
                };
                let got = model_of(&h, *x);
                prop_assert_eq!(
                    got.state,
                    want.state,
                    "slot {} op {:?} {:?} (k {})",
                    x.slot,
                    op,
                    st,
                    k
                );
                prop_assert_eq!(
                    got.sflags & want.flag_mask,
                    want.flags,
                    "flags of slot {}",
                    x.slot
                );
                prop_assert_eq!(
                    got.query_count,
                    want.query_count,
                    "query_count of slot {}",
                    x.slot
                );
            }
            let effects: Vec<SendEffect> = slots
                .iter()
                .enumerate()
                .filter(|(_, x)| **x == r || (op == GwOp::CancelAll && x.venue == r.venue))
                .map(|(j, _)| send_report_table(&before_k[j], op, st))
                .collect();
            let n_rejected = effects.iter().filter(|e| e.rejected).count();
            let upd: Vec<_> = h
                .out
                .updates
                .iter()
                .filter(|u| matches!(u.kind, OrderUpdateKind::Rejected { .. }))
                .collect();
            prop_assert_eq!(upd.len(), n_rejected);
            for u in upd {
                prop_assert_eq!(
                    u.kind,
                    OrderUpdateKind::Rejected {
                        bits: RejectBits::LOCAL_SEND_FAIL,
                        venue_code: u32::from(st as u8)
                    }
                );
                prop_assert_eq!(h.slot(u.r).reject_code, u32::from(st as u8));
            }
            let not_sent = effects.iter().any(|e| e.cancel_not_sent);
            prop_assert_eq!(h.n_notes(notes::CANCEL_NOT_SENT), usize::from(not_sent));
            let rest = effects.iter().filter(|e| e.needs_rest).count();
            prop_assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), rest);
            // Queries: emitted now or queued for the continuation (bounded fan-out, T10).
            let want_q: Vec<u32> = slots
                .iter()
                .enumerate()
                .filter(|(j, x)| {
                    (**x == r || (op == GwOp::CancelAll && x.venue == r.venue))
                        && send_report_table(&before_k[*j], op, st).query
                })
                .map(|(_, x)| x.slot)
                .collect();
            let mut got_q: Vec<u32> = h.decisions(OrderAction::Query).iter().map(|d| d.slot.0).collect();
            for _ in 0..16 {
                h.cont();
                let more: Vec<u32> = h.decisions(OrderAction::Query).iter().map(|d| d.slot.0).collect();
                if more.is_empty() && h.out.decisions.is_empty() {
                    break;
                }
                got_q.extend(more);
            }
            got_q.sort_unstable();
            got_q.dedup();
            for x in &got_q {
                prop_assert!(want_q.contains(x), "unexpected Query of slot {}", x);
            }
            for x in &want_q {
                let queued_before = slots
                    .iter()
                    .zip(before_k.iter())
                    .any(|(y, b)| y.slot == *x && b.sflags & SlotFlags::QUEUED != 0);
                prop_assert!(got_q.contains(x) || queued_before, "missing Query of slot {}", x);
            }
        }
        // In flight: a rejected Place releases its remaining quantity.
        let ib1 = open_qty(&h);
        let released: i64 = slots
            .iter()
            .zip(before.iter())
            .filter(|(x, b)| {
                **x == r && b.state == OrderState::PendingNew && h.state(**x) == OrderState::Rejected
            })
            .map(|(x, _)| h.slot(*x).qty.0 - h.slot(*x).filled.0)
            .sum();
        prop_assert_eq!(ib1, ib0 - released);
    }
    // Every slot accounted for: the in-flight book equals the open slots' remaining quantity.
    let open: i64 = slots
        .iter()
        .filter(|x| h.state(**x).open())
        .map(|x| h.slot(*x).qty.0 - h.slot(*x).filled.0)
        .sum();
    prop_assert_eq!(Qty(open_qty(&h)), Qty(open));
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2_000, ..ProptestConfig::default() })]
    #[test]
    fn b_1_unwritten_send_report_keeps_target_state(
        inits in proptest::collection::vec(init(), 1..=64),
        reps in proptest::collection::vec((any::<u8>(), any::<u8>(), any::<u8>(), proptest::bool::weighted(0.05)), 1..=256),
    ) {
        run_case(&inits, &reps)?;
    }
}
