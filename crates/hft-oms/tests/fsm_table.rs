#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
//! The report table, one test per row group (README "回报处理表").
use crate::common;

use OrderState::*;
use common::*;
use hft_event::{NoteLevel, OrderAction, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::{OrderRef, OrderUpdateKind};
use hft_types::{AssetId, Qty, RejectBits};

/// Slot s (buy X 0.01 GTC, unlinked) or t (sell Y on VX) in `st`.
fn mk(on_y: bool, st: OrderState, acked: bool, prev: OrderState, filled: &str) -> (H, OrderRef) {
    let mut h = H::new();
    let it = if on_y { sell_y() } else { buy_x() };
    let r = h.place_ok(&it, 0);
    let f = q(filled);
    h.o.test_slot_mut(r.venue, r.account, r.slot, |sl, _| {
        sl.state = st;
        if st == Unknown {
            sl.prev_state = prev;
        }
        if acked {
            sl.sflags.0 |= SlotFlags::ACKED;
        }
        sl.filled = f;
        sl.filled_notional = i128::from(f.0) * 6_000_000;
        if st == Unknown || prev != Free {
            sl.prev_state = prev;
        }
    });
    if st == Unknown {
        // Keep the table counters consistent with a real entry.
    }
    h.o.test_stamps_mut(r.venue, r.account, r.slot, |s| s.op = SlotStamps::OP_NONE);
    h.out.clear();
    (h, r)
}

fn ev(h: &mut H, r: OrderRef, what: OrderEventKind, f: impl FnOnce(&mut hft_event::OrderEvent)) {
    let mut e = h.oe(r, what);
    e.cum_filled = h.slot(r).filled;
    f(&mut e);
    h.ev(&e, 10);
}

fn dup(h: &H, before: u64) {
    assert!(h.out.is_empty());
    assert_eq!(h.o.stats().dup, before + 1);
}

fn updates(h: &H) -> Vec<OrderUpdateKind> {
    h.out.updates.iter().map(|u| u.kind).collect()
}

fn anomaly(h: &H) -> usize {
    h.notes(notes::ANOMALY).iter().filter(|n| n.v[0] == 1).count()
}

#[test]
fn oms_055_ack_row() {
    let ack = |h: &mut H, r| ev(h, r, OrderEventKind::Ack, |e| e.exch_order_id = 777);
    // .1
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    let ib = h.o.inflight().pos(A1, X);
    ack(&mut h, s);
    assert_eq!(h.state(s), Live, "OMS-055.1");
    assert_eq!(h.slot(s).exch_order_id, 777);
    assert!(h.flags(s) & SlotFlags::ACKED != 0);
    assert_eq!(transitions(&h), vec![(1, 2, 4)]);
    assert_eq!(updates(&h), vec![OrderUpdateKind::Acked]);
    let ib2 = h.o.inflight().pos(A1, X);
    assert_eq!(
        (ib2.imm_buy, ib2.open_buy),
        (Qty(ib.imm_buy.0 - 1_000_000), ib.open_buy)
    );
    let l = h.legs(A1, g1()).view.leg[0];
    assert_eq!((l.inflight_immediate, l.inflight_resting), (Qty(0), q("0.01")));
    // .2
    let (mut h, s) = mk(false, Live, true, Free, "0");
    let d = h.o.stats().dup;
    ack(&mut h, s);
    dup(&h, d);
    // .3
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.cancel(s, 1).unwrap();
    ack(&mut h, s);
    assert_eq!(h.state(s), PendingCancel, "OMS-055.3");
    assert_eq!(h.slot(s).exch_order_id, 777);
    assert!(h.flags(s) & SlotFlags::ACKED != 0);
    assert!(transitions(&h).is_empty());
    let l = h.legs(A1, g1()).view.leg[0];
    assert_eq!((l.inflight_immediate, l.inflight_resting), (Qty(0), q("0.01")));
    // .4
    let (mut h, s) = mk(false, PendingAmend, true, Free, "0");
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.exch_order_id = 777);
    ack(&mut h, s);
    assert_eq!(h.state(s), PendingAmend, "OMS-055.4");
    assert!(h.out.is_empty(), "OMS-055.4");
    // .5
    let (mut h, s) = mk(false, Unknown, false, PendingNew, "0");
    let u = h.o.table_view(VS, A1).unwrap().unknown_count;
    ack(&mut h, s);
    assert_eq!(h.state(s), Live, "OMS-055.5");
    assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, u - 1);
    assert_eq!(transitions(&h), vec![(8, 2, 4)]);
    assert_eq!(updates(&h), vec![OrderUpdateKind::Acked]);
    let l = h.legs(A1, g1()).view.leg[0];
    assert_eq!((l.inflight_immediate, l.inflight_resting), (Qty(0), q("0.01")));
    for (row, st) in [
        ("OMS-055.6", Filled),
        ("OMS-055.7", Cancelled),
        ("OMS-055.8", Rejected),
    ] {
        let (mut h, s) = mk(false, st, true, Free, if st == Filled { "0.01" } else { "0" });
        let d = h.o.stats().dup;
        ack(&mut h, s);
        assert!(h.out.is_empty(), "{row}");
        assert_eq!(h.o.stats().dup, d + 1, "{row}");
    }
}

#[test]
fn oms_056_reject_row() {
    let rej = |h: &mut H, r| ev(h, r, OrderEventKind::Reject, |e| e.reject_code = 2010);
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    rej(&mut h, s);
    assert_eq!((h.state(s), h.slot(s).reject_code), (Rejected, 2010), "OMS-056.1");
    assert_eq!(transitions(&h), vec![(1, 7, 5)]);
    assert_eq!(
        updates(&h),
        vec![OrderUpdateKind::Rejected {
            bits: RejectBits(0),
            venue_code: 2010
        }]
    );
    let ib = h.o.inflight().pos(A1, X);
    assert_eq!((ib.open_buy, ib.imm_buy), (Qty(0), Qty(0)));
    assert_eq!(h.legs(A1, g1()).view.leg[0].inflight_immediate, Qty(0));
    for (row, st) in [
        ("OMS-056.2", Live),
        ("OMS-056.3", PendingCancel),
        ("OMS-056.4", PendingAmend),
    ] {
        let (mut h, s) = mk(false, st, true, Free, "0");
        rej(&mut h, s);
        assert_eq!(h.state(s), st, "{row}");
        let a = h.notes(notes::ANOMALY);
        assert_eq!((a.len(), a[0].v[0], a[0].level), (1, 1, NoteLevel::P1), "{row}");
        assert!(transitions(&h).is_empty(), "{row}");
    }
    let (mut h, s) = mk(false, Unknown, false, PendingNew, "0");
    rej(&mut h, s);
    assert_eq!(h.state(s), Rejected, "OMS-056.5");
    assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 0);
    assert_eq!(
        updates(&h),
        vec![OrderUpdateKind::Rejected {
            bits: RejectBits(0),
            venue_code: 2010
        }]
    );
    for (row, st) in [
        ("OMS-056.6", Filled),
        ("OMS-056.7", Cancelled),
        ("OMS-056.8", Rejected),
    ] {
        let (mut h, s) = mk(false, st, true, Free, "0");
        let d = h.o.stats().dup;
        rej(&mut h, s);
        assert!(h.out.is_empty(), "{row}");
        assert_eq!(h.o.stats().dup, d + 1, "{row}");
    }
}

fn pfill(h: &mut H, r: OrderRef, cum: &str) {
    ev(h, r, OrderEventKind::PartialFill, |e| {
        e.cum_filled = q(cum);
        e.avg_px = p("60000.00");
        e.fee = Qty(240_000);
        e.fee_asset = USDT;
    });
}

#[test]
fn oms_057_partial_fill_row() {
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    pfill(&mut h, s, "0.004");
    assert_eq!(h.state(s), Live, "OMS-057.1");
    let f = h.flags(s);
    assert!(f & SlotFlags::FILL_BEFORE_ACK != 0 && f & SlotFlags::ACKED != 0);
    let fo = h.out.fills[0];
    assert_eq!(
        (fo.qty, fo.net_qty, fo.px),
        (q("0.004"), q("0.004"), p("60000.00"))
    );
    let u = updates(&h);
    assert_eq!(u[0], OrderUpdateKind::Acked);
    assert!(
        matches!(u[1], OrderUpdateKind::Filled { d, cum, done: false, .. } if d == q("0.004") && cum == q("0.004"))
    );
    assert_eq!(transitions(&h), vec![(1, 2, 6)]);
    let (mut h, s) = mk(false, Live, true, Free, "0");
    pfill(&mut h, s, "0.004");
    assert_eq!((h.state(s), h.slot(s).filled), (Live, q("0.004")), "OMS-057.2");
    assert_eq!(h.out.fills.len(), 1);
    assert_eq!(transitions(&h), vec![(2, 2, 6)]);
    assert_eq!(h.notes(notes::TRANSITION)[0].v[4], 400_000);
    for (row, st) in [("OMS-057.3", PendingCancel), ("OMS-057.4", PendingAmend)] {
        let (mut h, s) = mk(false, st, true, Free, "0");
        pfill(&mut h, s, "0.004");
        assert_eq!(h.state(s), st, "{row}");
        assert_eq!(h.out.fills.len(), 1, "{row}");
    }
    for (row, prev, want) in [
        ("OMS-057.5", PendingNew, Live),
        ("OMS-057.6", Live, Live),
        ("OMS-057.7", PendingCancel, PendingCancel),
        ("OMS-057.8", PendingAmend, PendingAmend),
    ] {
        let (mut h, s) = mk(false, Unknown, true, prev, "0");
        pfill(&mut h, s, "0.004");
        assert_eq!(h.state(s), want, "{row}");
        assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 0, "{row}");
        assert_eq!(h.out.fills.len(), 1, "{row}");
    }
    for (row, st) in [("OMS-057.9", Cancelled), ("OMS-057.10", Rejected)] {
        let (mut h, s) = mk(false, st, true, Free, "0");
        pfill(&mut h, s, "0.004");
        assert_eq!((h.state(s), h.slot(s).filled), (st, q("0.004")), "{row}");
        assert_eq!(h.out.fills.len(), 1, "{row}");
        assert_eq!(anomaly(&h), 1, "{row}");
        let nr = h.notes(notes::NEEDS_REST_QUERY);
        assert_eq!(nr.len(), 1, "{row}");
        assert_eq!(
            (nr[0].v[0] >> 16 & 1, nr[0].v[1]),
            (1, u64::from(s.slot)),
            "{row}"
        );
    }
    let (mut h, s) = mk(false, Filled, true, Free, "0.01");
    let d = h.o.stats().dup;
    pfill(&mut h, s, "0.004");
    dup(&h, d);
}

#[test]
fn oms_058_fill_row() {
    let fill = |h: &mut H, r| {
        ev(h, r, OrderEventKind::Fill, |e| {
            e.cum_filled = q("0.01");
            e.avg_px = p("60000.00");
        })
    };
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    fill(&mut h, s);
    assert_eq!(h.state(s), Filled, "OMS-058.1");
    assert!(h.flags(s) & SlotFlags::FILL_BEFORE_ACK != 0);
    assert_eq!(h.out.fills[0].qty, q("0.01"));
    assert!(
        updates(&h)
            .iter()
            .any(|u| matches!(u, OrderUpdateKind::Filled { done: true, .. }))
    );
    assert_eq!(h.o.inflight().open_buy(A1, X), Qty(0));
    let (mut h, s) = mk(false, Live, true, Free, "0.004");
    fill(&mut h, s);
    assert_eq!(
        (h.state(s), h.out.fills[0].qty),
        (Filled, q("0.006")),
        "OMS-058.2"
    );
    let (mut h, s) = mk(false, PendingCancel, true, Free, "0.004");
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.sflags.0 |= SlotFlags::CANCEL_SENT);
    fill(&mut h, s);
    assert_eq!(
        (h.state(s), h.out.fills[0].qty),
        (Filled, q("0.006")),
        "OMS-058.3"
    );
    assert!(h.flags(s) & SlotFlags::CANCEL_SENT == 0);
    let (mut h, s) = mk(false, PendingAmend, true, Free, "0.004");
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.sflags.0 |= SlotFlags::AMEND_SENT);
    fill(&mut h, s);
    assert_eq!(h.state(s), Filled, "OMS-058.4");
    assert!(h.flags(s) & SlotFlags::AMEND_SENT == 0);
    let (mut h, s) = mk(false, Unknown, true, PendingNew, "0");
    fill(&mut h, s);
    assert_eq!(h.state(s), Filled, "OMS-058.5");
    assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 0);
    for (row, st, f0, d) in [
        ("OMS-058.6", Cancelled, "0.004", "0.006"),
        ("OMS-058.7", Rejected, "0", "0.01"),
    ] {
        let (mut h, s) = mk(false, st, true, Free, f0);
        fill(&mut h, s);
        assert_eq!((h.state(s), h.out.fills[0].qty), (Filled, q(d)), "{row}");
        assert_eq!(anomaly(&h), 1, "{row}");
        assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 1, "{row}");
    }
    let (mut h, s) = mk(false, Filled, true, Free, "0.01");
    let d = h.o.stats().dup;
    fill(&mut h, s);
    dup(&h, d);
}

#[test]
fn oms_059_cancel_ack_row() {
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    ev(&mut h, s, OrderEventKind::CancelAck, |_| {});
    assert_eq!(h.state(s), Cancelled, "OMS-059.1");
    assert_eq!(transitions(&h), vec![(1, 6, 7)]);
    assert_eq!(updates(&h), vec![OrderUpdateKind::Cancelled]);
    let ib = h.o.inflight().pos(A1, X);
    assert_eq!((ib.open_buy, ib.imm_buy), (Qty(0), Qty(0)));
    let (mut h, s) = mk(false, Live, true, Free, "0.004");
    assert_eq!(h.o.inflight().open_buy(A1, X), q("0.006"));
    ev(&mut h, s, OrderEventKind::CancelAck, |_| {});
    assert_eq!(
        (h.state(s), h.o.inflight().open_buy(A1, X)),
        (Cancelled, Qty(0)),
        "OMS-059.2"
    );
    for (row, st) in [
        ("OMS-059.3", PendingCancel),
        ("OMS-059.4", PendingAmend),
        ("OMS-059.5", Unknown),
    ] {
        let (mut h, s) = mk(false, st, true, PendingNew, "0");
        ev(&mut h, s, OrderEventKind::CancelAck, |_| {});
        assert_eq!(h.state(s), Cancelled, "{row}");
        assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 0, "{row}");
    }
    for (row, st) in [
        ("OMS-059.6", Filled),
        ("OMS-059.7", Cancelled),
        ("OMS-059.8", Rejected),
    ] {
        let (mut h, s) = mk(false, st, true, Free, if st == Filled { "0.01" } else { "0" });
        let d = h.o.stats().dup;
        ev(&mut h, s, OrderEventKind::CancelAck, |_| {});
        dup(&h, d);
        let _ = row;
    }
}

#[test]
fn oms_060_expired_row() {
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    ev(&mut h, s, OrderEventKind::Expired, |_| {});
    assert_eq!(h.state(s), Cancelled, "OMS-060.1");
    assert_eq!(transitions(&h)[0].2, 8);
    let mut h = H::new();
    let mut it = buy_x();
    it.intent.tif = hft_types::Tif::Ioc;
    let s = h.place_ok(&it, 0);
    h.ack(s, 1, 1);
    h.fill(s, q("0.004"), p("60000.00"), 2);
    assert_eq!(h.legs(A1, g1()).view.leg[0].inflight_immediate, q("0.006"));
    ev(&mut h, s, OrderEventKind::Expired, |_| {});
    assert_eq!(h.state(s), Cancelled, "OMS-060.2");
    assert_eq!(h.legs(A1, g1()).view.leg[0].inflight_immediate, Qty(0));
    for (row, st) in [
        ("OMS-060.3", PendingCancel),
        ("OMS-060.4", PendingAmend),
        ("OMS-060.5", Unknown),
    ] {
        let (mut h, s) = mk(false, st, true, PendingNew, "0");
        ev(&mut h, s, OrderEventKind::Expired, |_| {});
        assert_eq!(h.state(s), Cancelled, "{row}");
    }
    let (mut h, s) = mk(false, Cancelled, true, Free, "0");
    let d = h.o.stats().dup;
    ev(&mut h, s, OrderEventKind::Expired, |_| {});
    dup(&h, d);
}

#[test]
fn oms_061_cancel_reject_gone_row() {
    let gone = |h: &mut H, r| ev(h, r, OrderEventKind::CancelReject, |e| e.detail = 1);
    for (row, st) in [("OMS-061.1", PendingNew), ("OMS-061.2", Live)] {
        let (mut h, s) = mk(false, st, st == Live, Free, "0");
        gone(&mut h, s);
        assert_eq!(h.state(s), st, "{row}");
        assert_eq!(anomaly(&h), 1, "{row}");
    }
    let (mut h, s) = mk(false, PendingCancel, true, Free, "0.01");
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.sflags.0 |= SlotFlags::CANCEL_SENT);
    gone(&mut h, s);
    assert_eq!(h.state(s), Filled, "OMS-061.3");
    assert!(h.flags(s) & SlotFlags::CANCEL_SENT == 0);
    for (row, st) in [("OMS-061.4", PendingCancel), ("OMS-061.6", PendingAmend)] {
        let (mut h, s) = mk(false, st, true, Free, "0.004");
        gone(&mut h, s);
        assert_eq!(h.state(s), Unknown, "{row}");
        assert_eq!(h.slot(s).prev_state, st, "{row}");
        assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 1, "{row}");
        assert_eq!(h.notes(notes::UNKNOWN).len(), 1, "{row}");
        assert_eq!(h.notes(notes::UNKNOWN)[0].level, NoteLevel::P1, "{row}");
        assert!(updates(&h).contains(&OrderUpdateKind::Unknown), "{row}");
        let qd = h.decisions(OrderAction::Query);
        assert_eq!((qd.len(), qd[0].target_slot.0), (1, s.slot), "{row}");
        let t = h.timers(OmsTimerKind::UnknownQuery);
        assert_eq!(t[0].deadline_tsc, ms(1010), "{row}");
        assert!(h.decisions(OrderAction::Cancel).is_empty(), "{row}");
    }
    let (mut h, s) = mk(false, PendingAmend, true, Free, "0.01");
    gone(&mut h, s);
    assert_eq!(h.state(s), Filled, "OMS-061.5");
    let (mut h, s) = mk(false, Unknown, true, PendingCancel, "0");
    gone(&mut h, s);
    assert_eq!(h.state(s), Unknown, "OMS-061.7");
    assert!(h.out.decisions.is_empty(), "OMS-061.7");
    let (mut h, s) = mk(false, Cancelled, true, Free, "0");
    let d = h.o.stats().dup;
    gone(&mut h, s);
    dup(&h, d);
}

#[test]
fn oms_062_cancel_reject_other_row() {
    let other = |h: &mut H, r| {
        ev(h, r, OrderEventKind::CancelReject, |e| {
            e.detail = 0;
            e.reject_code = 1015;
        })
    };
    for (row, st) in [
        ("OMS-062.1", PendingNew),
        ("OMS-062.2", Live),
        ("OMS-062.5", PendingAmend),
    ] {
        let (mut h, s) = mk(false, st, st != PendingNew, Free, "0");
        other(&mut h, s);
        assert_eq!(h.state(s), st, "{row}");
        assert_eq!(anomaly(&h), 1, "{row}");
    }
    let (mut h, s) = mk(false, PendingCancel, true, Free, "0");
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.sflags.0 |= SlotFlags::CANCEL_SENT);
    other(&mut h, s);
    assert_eq!(h.state(s), Live, "OMS-062.3");
    assert!(h.flags(s) & SlotFlags::CANCEL_SENT == 0);
    assert_eq!(transitions(&h), vec![(3, 2, 9)]);
    assert!(h.out.updates.is_empty());
    assert!(h.decisions(OrderAction::Cancel).is_empty());
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.cancel(s, 1).unwrap();
    other(&mut h, s);
    assert_eq!(h.state(s), Live, "OMS-062.4");
    assert!(h.flags(s) & SlotFlags::ACKED != 0);
    let l = h.legs(A1, g1()).view.leg[0];
    assert_eq!((l.inflight_immediate, l.inflight_resting), (Qty(0), q("0.01")));
    let (mut h, s) = mk(false, Unknown, true, PendingCancel, "0");
    other(&mut h, s);
    assert_eq!(h.state(s), Unknown, "OMS-062.6");
    assert!(h.out.is_empty(), "OMS-062.6");
    let (mut h, s) = mk(false, Filled, true, Free, "0.01");
    let d = h.o.stats().dup;
    other(&mut h, s);
    dup(&h, d);
}

fn amend_ev(h: &mut H, r: OrderRef, what: OrderEventKind, detail: u8) {
    ev(h, r, what, |e| {
        e.px = p("60200.0");
        e.qty = q("0.008");
        e.detail = detail;
    });
}

fn mk_t(st: OrderState, amend_sent: bool, prev: OrderState, filled: &str) -> (H, OrderRef) {
    let (mut h, t) = mk(true, st, true, prev, filled);
    h.o.test_slot_mut(VX, A1, t.slot, |sl, _| {
        sl.px = p("60100.0");
        sl.pending_px = p("60200.0");
        sl.pending_qty = q("0.008");
        if amend_sent {
            sl.sflags.0 |= SlotFlags::AMEND_SENT;
        }
    });
    (h, t)
}

#[test]
fn oms_063_amend_ack_row() {
    for (row, st) in [("OMS-063.1", PendingNew), ("OMS-063.2", Live)] {
        let (mut h, t) = mk_t(st, false, Free, "0");
        amend_ev(&mut h, t, OrderEventKind::AmendAck, 0);
        assert_eq!(h.state(t), st, "{row}");
        assert_eq!((h.slot(t).px, h.slot(t).qty), (p("60100.0"), q("0.01")), "{row}");
        assert_eq!(anomaly(&h), 1, "{row}");
    }
    let (mut h, t) = mk_t(PendingCancel, true, Free, "0");
    amend_ev(&mut h, t, OrderEventKind::AmendAck, 0);
    assert_eq!(h.state(t), PendingCancel, "OMS-063.3");
    assert_eq!((h.slot(t).px, h.slot(t).qty), (p("60200.0"), q("0.008")));
    assert!(h.flags(t) & SlotFlags::AMEND_SENT == 0);
    assert!(transitions(&h).is_empty());
    assert_eq!(h.legs(A1, g1()).view.leg[1].inflight_resting, Qty(-800_000));
    let (mut h, t) = mk_t(PendingCancel, false, Free, "0");
    amend_ev(&mut h, t, OrderEventKind::AmendAck, 0);
    assert_eq!(anomaly(&h), 1, "OMS-063.4");
    assert_eq!((h.slot(t).px, h.slot(t).qty), (p("60100.0"), q("0.01")));
    let (mut h, t) = mk_t(PendingAmend, true, Free, "0");
    amend_ev(&mut h, t, OrderEventKind::AmendAck, 0);
    assert_eq!(h.state(t), Live, "OMS-063.5");
    assert_eq!((h.slot(t).px, h.slot(t).qty), (p("60200.0"), q("0.008")));
    assert!(h.flags(t) & SlotFlags::AMEND_SENT == 0);
    assert_eq!(transitions(&h), vec![(4, 2, 10)]);
    assert!(h.out.updates.is_empty());
    assert_eq!(h.o.inflight().open_sell(A1, Y), q("0.008"));
    let (mut h, t) = mk_t(Unknown, false, PendingAmend, "0");
    h.o.test_slot_mut(VX, A1, t.slot, |sl, _| sl.pending_qty = q("0.005"));
    amend_ev(&mut h, t, OrderEventKind::AmendAck, 0);
    assert_eq!(h.state(t), Live, "OMS-063.6");
    assert_eq!((h.slot(t).px, h.slot(t).qty), (p("60200.0"), q("0.008")));
    assert_eq!(h.o.table_view(VX, A1).unwrap().unknown_count, 0);
    let (mut h, t) = mk_t(Filled, false, Free, "0.01");
    let d = h.o.stats().dup;
    amend_ev(&mut h, t, OrderEventKind::AmendAck, 0);
    dup(&h, d);
}

#[test]
fn oms_064_amend_reject_row() {
    for (row, st) in [("OMS-064.1", PendingNew), ("OMS-064.2", Live)] {
        let (mut h, t) = mk_t(st, false, Free, "0");
        amend_ev(&mut h, t, OrderEventKind::AmendReject, 0);
        assert_eq!(anomaly(&h), 1, "{row}");
    }
    let (mut h, t) = mk_t(PendingCancel, true, Free, "0");
    amend_ev(&mut h, t, OrderEventKind::AmendReject, 0);
    assert_eq!(h.state(t), PendingCancel, "OMS-064.3");
    assert_eq!((h.slot(t).px, h.slot(t).qty), (p("60100.0"), q("0.01")));
    assert!(h.flags(t) & SlotFlags::AMEND_SENT == 0);
    let (mut h, t) = mk_t(PendingAmend, true, Free, "0");
    amend_ev(&mut h, t, OrderEventKind::AmendReject, 0);
    assert_eq!(h.state(t), Live, "OMS-064.4");
    assert_eq!((h.slot(t).px, h.slot(t).qty), (p("60100.0"), q("0.01")));
    assert!(h.flags(t) & SlotFlags::AMEND_SENT == 0);
    assert_eq!(transitions(&h), vec![(4, 2, 11)]);
    let (mut h, t) = mk_t(PendingAmend, true, Free, "0.01");
    amend_ev(&mut h, t, OrderEventKind::AmendReject, 1);
    assert_eq!(h.state(t), Filled, "OMS-064.5");
    let (mut h, t) = mk_t(PendingAmend, true, Free, "0.004");
    amend_ev(&mut h, t, OrderEventKind::AmendReject, 1);
    assert_eq!(
        (h.state(t), h.slot(t).prev_state),
        (Unknown, PendingAmend),
        "OMS-064.6"
    );
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
    let (mut h, t) = mk_t(Unknown, false, PendingAmend, "0");
    amend_ev(&mut h, t, OrderEventKind::AmendReject, 0);
    assert_eq!(h.state(t), Unknown, "OMS-064.7");
    assert!(h.out.is_empty());
    let (mut h, t) = mk_t(Cancelled, false, Free, "0");
    let d = h.o.stats().dup;
    amend_ev(&mut h, t, OrderEventKind::AmendReject, 0);
    dup(&h, d);
}

fn qr(h: &mut H, r: OrderRef, status: u8, cum: &str, avg: &str, px: &str) {
    ev(h, r, OrderEventKind::QueryResult, |e| {
        e.detail = status;
        e.cum_filled = q(cum);
        e.avg_px = p(avg);
        e.px = p(px);
        e.qty = q("0.01");
    });
}

#[test]
fn oms_065_query_result_row() {
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    qr(&mut h, s, 1, "0", "0", "60000.00");
    assert_eq!(h.state(s), Live, "OMS-065.1");
    assert!(h.flags(s) & SlotFlags::ACKED != 0);
    assert_eq!(updates(&h), vec![OrderUpdateKind::Acked]);
    assert_eq!(transitions(&h)[0].2, 12);
    let (mut h, s) = mk(false, Live, true, Free, "0");
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.sflags.0 |= SlotFlags::VERIFY);
    qr(&mut h, s, 2, "0.004", "60000.00", "60000.00");
    assert_eq!(h.state(s), Live, "OMS-065.2");
    assert_eq!(h.out.fills[0].qty, q("0.004"));
    assert!(h.flags(s) & SlotFlags::VERIFY == 0);
    let (mut h, s) = mk(false, PendingCancel, true, Free, "0");
    qr(&mut h, s, 4, "0", "0", "60000.00");
    assert_eq!(h.state(s), Cancelled, "OMS-065.3");
    let (mut h, s) = mk(false, PendingAmend, true, Free, "0");
    qr(&mut h, s, 1, "0", "0", "60050.00");
    assert_eq!((h.state(s), h.slot(s).px), (Live, p("60050.00")), "OMS-065.4");
    assert!(h.out.updates.is_empty());
    let (mut h, s) = mk(false, Unknown, true, PendingNew, "0");
    qr(&mut h, s, 3, "0.01", "60000.00", "60000.00");
    assert_eq!(h.state(s), Filled, "OMS-065.5");
    assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 0);
    assert_eq!(h.out.fills[0].qty, q("0.01"));
    let (mut h, s) = mk(false, Filled, true, Free, "0.01");
    qr(&mut h, s, 3, "0.01", "60000.00", "60000.00");
    assert!(h.out.is_empty(), "OMS-065.6");
    let (mut h, s) = mk(false, Cancelled, true, Free, "0.004");
    qr(&mut h, s, 4, "0.006", "60000.00", "60000.00");
    assert_eq!(h.state(s), Cancelled, "OMS-065.7");
    assert_eq!(h.out.fills[0].qty, q("0.002"));
    assert_eq!(anomaly(&h), 0);
}

#[test]
fn oms_067_fee_only_row() {
    let fee = |h: &mut H, r| {
        ev(h, r, OrderEventKind::FeeOnly, |e| {
            e.fee = q("0.00001");
            e.fee_asset = BTC;
        })
    };
    for (row, st) in [
        ("OMS-067.1", PendingNew),
        ("OMS-067.2", Live),
        ("OMS-067.3", PendingCancel),
        ("OMS-067.4", PendingAmend),
        ("OMS-067.5", Unknown),
        ("OMS-067.6", Filled),
        ("OMS-067.7", Cancelled),
    ] {
        let (mut h, s) = mk(
            false,
            st,
            true,
            PendingNew,
            if st == Filled { "0.01" } else { "0" },
        );
        let u = h.o.table_view(VS, A1).unwrap().unknown_count;
        fee(&mut h, s);
        assert_eq!(h.state(s), st, "{row}");
        let f = h.out.fees.as_slice();
        assert_eq!(f.len(), 1, "{row}");
        let f = f[0];
        assert_eq!(
            (f.account, f.venue, f.inst, f.group, f.side, f.fee, f.fee_asset),
            (A1, VS, X, g1(), 1, 1000, BTC),
            "{row}"
        );
        assert!(
            transitions(&h).is_empty() && h.n_notes(notes::NEEDS_REST_QUERY) == 0,
            "{row}"
        );
        assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, u, "{row}");
    }
    // .8 the generation was recycled: attributed through the tombstone.
    let mut h = H::with(|c| c.slots.set(VS, A1, 1));
    let s = h.place_ok(&buy_x(), 0);
    h.what(s, OrderEventKind::CancelAck, 1);
    h.sweep(5002);
    let s2 = h.place_ok(
        &intent(
            VS,
            A1,
            X2,
            hft_types::Side::Sell,
            hft_types::Tif::Gtc,
            p("3000.00"),
            q("1"),
            hft_types::GROUP_NONE,
        ),
        5003,
    );
    assert_eq!(s2.slot, s.slot);
    let mut e = h.oe(s2, OrderEventKind::FeeOnly);
    e.r#gen = s.r#gen;
    e.fee = q("0.00001");
    e.fee_asset = BTC;
    h.ev(&e, 5004);
    let f = h.out.fees[0];
    assert_eq!((f.group, f.inst, f.side), (g1(), X, 1), "OMS-067.8");
    assert_eq!(h.state(s2), PendingNew, "OMS-067.8");
    let _ = AssetId(0);
}

#[test]
fn oms_068_outcome_unknown_row() {
    let ou = |h: &mut H, r| ev(h, r, OrderEventKind::OutcomeUnknown, |e| e.reject_code = 1007);
    let (mut h, s) = mk(false, PendingNew, false, Free, "0");
    ou(&mut h, s);
    assert_eq!(h.state(s), Unknown, "OMS-068.1");
    assert_eq!(transitions(&h)[0].2, 14);
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
    assert_eq!(h.o.inflight().open_buy(A1, X), q("0.01"));
    let (mut h, s) = mk(false, Live, true, Free, "0");
    ou(&mut h, s);
    assert_eq!(h.state(s), Live, "OMS-068.2");
    assert!(h.flags(s) & SlotFlags::VERIFY != 0);
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
    for (row, st) in [("OMS-068.3", PendingCancel), ("OMS-068.4", PendingAmend)] {
        let (mut h, s) = mk(false, st, true, Free, "0");
        ou(&mut h, s);
        assert_eq!(h.state(s), Unknown, "{row}");
        assert_eq!(h.decisions(OrderAction::Query).len(), 1, "{row}");
    }
    let (mut h, s) = mk(false, Unknown, true, PendingNew, "0");
    ou(&mut h, s);
    assert!(h.out.decisions.is_empty(), "OMS-068.5");
    let (mut h, s) = mk(false, Cancelled, true, Free, "0");
    let d = h.o.stats().dup;
    ou(&mut h, s);
    dup(&h, d);
}
