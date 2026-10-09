#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use SendStatus::*;
use common::*;
use hft_event::{ConnRole, ConnStateKind, GwOp, NoteLevel, OrderAction, SendStatus};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::OrderUpdateKind;
use hft_types::{Qty, RejectBits, Tif};

const NOT_WRITTEN: [(SendStatus, u32); 6] = [
    (KillBlocked, 2),
    (RateLimited, 3),
    (NoConn, 5),
    (EncodeError, 6),
    (RingFull, 7),
    (QueryUnsupported, 8),
];

#[test]
fn oms_070_send_report_place() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, Written, 1, 1);
    assert_eq!(h.stamps(s).sent_tsc, 1_000_000, "OMS-070.1");
    assert_eq!(h.state(s), OrderState::PendingNew);
    assert!(h.out.notes.is_empty());
    for (st, code) in NOT_WRITTEN {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, st, 1, 1);
        assert_eq!(
            (h.state(s), h.slot(s).reject_code),
            (OrderState::Rejected, code),
            "{st:?}"
        );
        assert_eq!(transitions(&h), vec![(1, 7, 17)], "{st:?}");
        assert_eq!(
            h.out.updates.iter().map(|u| u.kind).collect::<Vec<_>>(),
            vec![OrderUpdateKind::Rejected {
                bits: RejectBits::LOCAL_SEND_FAIL,
                venue_code: code
            }],
            "{st:?}"
        );
        let ib = h.o.inflight().pos(A1, X);
        assert_eq!((ib.open_buy, ib.imm_buy), (Qty(0), Qty(0)), "{st:?}");
        assert_eq!(h.legs(A1, g1()).view.leg[0].inflight_immediate, Qty(0), "{st:?}");
        assert_eq!(h.stamps(s).op, SlotStamps::OP_NONE, "{st:?}");
    }
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, WriteError, 1, 1);
    assert_eq!(h.state(s), OrderState::Unknown, "OMS-070.8");
    assert_eq!(transitions(&h)[0].2, 18);
    assert_eq!(h.o.inflight().open_buy(A1, X), q("0.01"));
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
}

#[test]
fn oms_071_send_report_cancel() {
    let setup = || {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        h.ack(s, 1, 1);
        h.cancel(s, 2).unwrap();
        (h, s)
    };
    let (mut h, s) = setup();
    assert_eq!(h.stamps(s).op, SlotStamps::OP_CANCEL);
    h.send(VS, A1, GwOp::Cancel, s.slot, s.r#gen, Written, 5, 5);
    assert_eq!(h.stamps(s).sent_tsc, 5_000_000, "OMS-071.1");
    for (st, _) in NOT_WRITTEN {
        let (mut h, s) = setup();
        h.send(VS, A1, GwOp::Cancel, s.slot, s.r#gen, st, 5, 5);
        assert_eq!(h.state(s), OrderState::PendingCancel, "{st:?}");
        assert!(h.flags(s) & SlotFlags::CANCEL_SENT == 0, "{st:?}");
        let n = h.notes(notes::CANCEL_NOT_SENT);
        assert_eq!((n.len(), n[0].level), (1, NoteLevel::P2), "{st:?}");
        assert_eq!(h.stamps(s).op, SlotStamps::OP_NONE, "{st:?}");
    }
    let (mut h, s) = setup();
    h.send(VS, A1, GwOp::Cancel, s.slot, s.r#gen, WriteError, 5, 5);
    assert_eq!(h.state(s), OrderState::PendingCancel, "OMS-071.8");
    assert!(h.flags(s) & SlotFlags::VERIFY != 0);
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
    // .9 the New sample is pending: the cancel was not stamped and keeps the New sent_tsc.
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, Written, 1, 1);
    h.cancel(s, 2).unwrap();
    h.send(VS, A1, GwOp::Cancel, s.slot, s.r#gen, Written, 5, 5);
    assert_eq!(h.stamps(s).sent_tsc, 1_000_000, "OMS-071.9");
}

#[test]
fn oms_072_send_report_amend() {
    let setup = || {
        let mut h = H::new();
        let t = h.place_ok(&sell_y(), 0);
        h.ack(t, 1, 1);
        h.o.test_stamps_mut(VX, A1, t.slot, |s| s.op = SlotStamps::OP_NONE);
        h.amend(t, p("60200.0"), q("0.008"), 2).unwrap();
        (h, t)
    };
    let (mut h, t) = setup();
    assert_eq!(h.stamps(t).op, SlotStamps::OP_AMEND);
    h.send(VX, A1, GwOp::Amend, t.slot, t.r#gen, Written, 5, 5);
    assert_eq!(h.stamps(t).sent_tsc, 5_000_000, "OMS-072.1");
    for st in [KillBlocked, RateLimited, NoConn, EncodeError, RingFull] {
        let (mut h, t) = setup();
        h.send(VX, A1, GwOp::Amend, t.slot, t.r#gen, st, 5, 5);
        assert_eq!(h.state(t), OrderState::PendingAmend, "{st:?}");
        assert!(h.flags(t) & SlotFlags::AMEND_SENT == 0, "{st:?}");
        assert_eq!(h.n_notes(notes::CANCEL_NOT_SENT), 1, "{st:?}");
        assert_eq!(h.stamps(t).op, SlotStamps::OP_NONE, "{st:?}");
    }
    let (mut h, t) = setup();
    h.send(VX, A1, GwOp::Amend, t.slot, t.r#gen, WriteError, 5, 5);
    assert!(h.flags(t) & SlotFlags::VERIFY != 0, "OMS-072.7");
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
}

#[test]
fn oms_073_send_report_query() {
    let setup = || {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, WriteError, 0, 0);
        h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.query_count = 2);
        (h, s)
    };
    let (mut h, s) = setup();
    h.send(VS, A1, GwOp::Query, s.slot, s.r#gen, Written, 1, 1);
    assert_eq!(h.slot(s).query_count, 3, "OMS-073.1");
    for st in [
        KillBlocked,
        RateLimited,
        NoConn,
        EncodeError,
        RingFull,
        WriteError,
    ] {
        let (mut h, s) = setup();
        h.send(VS, A1, GwOp::Query, s.slot, s.r#gen, st, 1, 1);
        assert_eq!(
            (h.state(s), h.slot(s).query_count),
            (OrderState::Unknown, 2),
            "{st:?}"
        );
        assert!(h.out.notes.is_empty(), "{st:?}");
    }
    let (mut h, s) = setup();
    h.send(VS, A1, GwOp::Query, s.slot, s.r#gen, QueryUnsupported, 1, 1);
    assert_eq!(
        (h.state(s), h.slot(s).query_count),
        (OrderState::Unknown, 2),
        "OMS-073.7"
    );
    let n = h.notes(notes::NEEDS_REST_QUERY);
    assert_eq!(
        (n.len(), n[0].v[0] >> 16 & 1, n[0].v[1]),
        (1, 0, u64::from(s.slot)),
        "OMS-073.7"
    );
}

#[test]
fn oms_074_send_report_cancel_all() {
    let setup = || {
        let mut h = H::new();
        let rs: Vec<_> = (0..3)
            .map(|i| {
                let t = h.place_ok(&sell_y(), 0);
                h.ack(t, i + 1, 1);
                t
            })
            .collect();
        h.out.clear();
        h.o.cancel_all(CancelScope::VenueAccount(VX, A1), false, ms(2), 1, &mut h.out);
        assert_eq!(h.decisions(OrderAction::CancelAll).len(), 1);
        (h, rs)
    };
    let (mut h, rs) = setup();
    h.send(VX, A1, GwOp::CancelAll, 0, 0, Written, 3, 3);
    assert!(h.out.is_empty(), "OMS-074.1");
    for r in &rs {
        assert!(h.flags(*r) & SlotFlags::CANCEL_SENT != 0);
    }
    for st in [KillBlocked, RateLimited, NoConn, EncodeError, RingFull] {
        let (mut h, rs) = setup();
        h.send(VX, A1, GwOp::CancelAll, 0, 0, st, 3, 3);
        for r in &rs {
            assert_eq!(h.state(*r), OrderState::PendingCancel, "{st:?}");
            assert!(h.flags(*r) & SlotFlags::CANCEL_SENT == 0, "{st:?}");
        }
        assert_eq!(h.n_notes(notes::CANCEL_NOT_SENT), 1, "{st:?}");
    }
    let (mut h, rs) = setup();
    h.send(VX, A1, GwOp::CancelAll, 0, 0, WriteError, 3, 3);
    for r in &rs {
        assert!(h.flags(*r) & SlotFlags::VERIFY != 0, "OMS-074.7");
    }
    assert_eq!(h.decisions(OrderAction::Query).len(), 3, "OMS-074.7");
}

/// L1 with the first leg filled 0.012 and the second leg placed; returns (h, h_slot, link).
fn hedged(now: u64) -> (H, hft_strategy_sdk::OrderRef, u32) {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), now);
    let req = one_hedge(&h);
    let hs = place_hedge(&mut h, &req, now).unwrap();
    (h, hs, l)
}

#[test]
fn oms_075_local_fail_classes() {
    for (row, st, retries, timer) in [
        ("OMS-075.1", KillBlocked, 0, true),
        ("OMS-075.2", RateLimited, 0, true),
        ("OMS-075.3", NoConn, 0, true),
        ("OMS-075.4", RingFull, 0, true),
        ("OMS-075.5", EncodeError, 1, true),
        ("OMS-075.6", WriteError, 0, false),
    ] {
        let (mut h, hs, l) = hedged(400);
        h.send(VX, A1, GwOp::Place, hs.slot, hs.r#gen, st, 500, 500);
        let want_state = if st == WriteError {
            OrderState::Unknown
        } else {
            OrderState::Rejected
        };
        assert_eq!(h.state(hs), want_state, "{row}");
        assert_eq!(h.link(l).retries, retries, "{row}");
        let t = h.timers(OmsTimerKind::HedgeRetry);
        if timer {
            assert_eq!((t.len(), t[0].id, t[0].deadline_tsc), (1, l, ms(600)), "{row}");
        } else {
            assert!(t.is_empty(), "{row}");
            assert_eq!(h.link(l).hedge_open, q("0.012"), "{row}");
        }
    }
}

#[test]
fn oms_090_report_decode_verify() {
    let mut h = H::new();
    let p_ = h.place_ok(&buy_x(), 0);
    let l = h.place_ok(&buy_x(), 0);
    h.ack(l, 1, 0);
    let f = h.place_ok(&buy_x(), 0);
    h.fill(f, q("0.01"), p("60000.00"), 0);
    h.conn_reason(VS, A1, ConnRole::UserStream, ConnStateKind::Degraded, 12, 1);
    for r in [p_, l] {
        assert!(h.flags(r) & SlotFlags::VERIFY != 0);
    }
    assert_eq!(
        (h.state(p_), h.state(l), h.state(f)),
        (OrderState::PendingNew, OrderState::Live, OrderState::Filled)
    );
    assert!(h.flags(f) & SlotFlags::VERIFY == 0);
    assert_eq!(h.decisions(OrderAction::Query).len(), 2);
    let tv = h.o.table_view(VS, A1).unwrap();
    assert_eq!((tv.unknown_count, tv.decode_fails), (0, 1));
}

#[test]
fn oms_091_decode_fail_threshold() {
    let setup = |n: u8| {
        let mut h = H::with(|c| c.decode_fail_down_n = n);
        let p_ = h.place_ok(&buy_x(), 0);
        let l = h.place_ok(&buy_x(), 0);
        h.ack(l, 1, 0);
        (h, p_, l)
    };
    let dec = |h: &mut H| h.conn_reason(VS, A1, ConnRole::UserStream, ConnStateKind::Degraded, 12, 1);
    let (mut h, p_, _l) = setup(3);
    dec(&mut h);
    dec(&mut h);
    assert_eq!(h.state(p_), OrderState::PendingNew, "OMS-091.1");
    assert!(h.flags(p_) & SlotFlags::VERIFY != 0);
    assert_eq!(h.o.table_view(VS, A1).unwrap().decode_fails, 2);
    dec(&mut h);
    assert_eq!(h.state(p_), OrderState::Unknown, "OMS-091.2");
    assert_eq!(transitions(&h)[0].2, 19);
    let tv = h.o.table_view(VS, A1).unwrap();
    assert_eq!((tv.conn_up, tv.decode_fails), (true, 0));
    let (mut h, p_, _) = setup(3);
    dec(&mut h);
    dec(&mut h);
    h.conn(VS, A1, ConnRole::UserStream, ConnStateKind::Up, 1);
    dec(&mut h);
    assert_eq!(h.state(p_), OrderState::PendingNew, "OMS-091.3");
    assert_eq!(h.o.table_view(VS, A1).unwrap().decode_fails, 1);
    let (mut h, p_, _) = setup(1);
    dec(&mut h);
    assert_eq!(h.state(p_), OrderState::Unknown, "OMS-091.4");
}
