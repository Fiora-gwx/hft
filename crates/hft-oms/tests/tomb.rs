#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{NoteLevel, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::OrderRef;
use hft_types::{Qty, Tif};

/// L1 first leg on slot 3 with gen 2, Filled 0.01 with BTC fee 0.00001, t_state 0; returns (slot, link).
fn filled_gen2(h: &mut H) -> (OrderRef, u32) {
    for _ in 0..2 {
        let r = h.place_ok(&buy_x(), 0);
        h.ack(r, 1, 0);
    }
    let x = h.place_ok(&buy_x(), 0);
    h.what(x, OrderEventKind::CancelAck, 0);
    h.sweep(5001);
    // Slot 3 is free again (gen 1 recycled); the next place reuses it with gen 2.
    let (f, l) = place_first(h, q("0.01"), Tif::Ioc, plan_l1(), 0);
    assert_eq!((f.slot, f.r#gen), (3, 2));
    let mut e = h.oe(f, OrderEventKind::Ack);
    e.exch_order_id = 777;
    h.ev(&e, 0);
    let mut e = h.oe(f, OrderEventKind::Fill);
    e.cum_filled = q("0.01");
    e.avg_px = p("60000.00");
    e.fee = Qty(1000);
    e.fee_asset = BTC;
    h.ev(&e, 0);
    h.o.test_slot_mut(VS, A1, 3, |sl, _| sl.t_state = ms(0));
    (f, l)
}

#[test]
fn oms_160_recycle_writes_tomb() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 3));
    let (f, l) = filled_gen2(&mut h);
    let rec0 = h.o.stats().recycled;
    h.sweep(5001);
    assert_eq!(h.slot(f).state, OrderState::Free);
    let tb = *h.o.tomb(VS, A1, 3, 2).unwrap();
    assert_eq!(
        (tb.r#gen, tb.epoch, tb.oflags, tb.inst, tb.group, tb.side),
        (2, 7, 0x04, X, g1(), hft_types::Side::Buy)
    );
    assert_eq!((tb.leg_ix, tb.strategy.0, tb.role, tb.link_id), (0, 1, 1, l));
    assert_eq!(
        (tb.filled, tb.net_filled, tb.exch_order_id, tb.freed_tsc),
        (q("0.01"), q("0.00999"), 777, ms(5001))
    );
    assert_eq!(h.o.table_view(VS, A1).unwrap().free_tail, 3);
    assert!(h.out.notes.is_empty() && h.out.updates.is_empty());
    assert_eq!(h.o.stats().recycled, rec0 + 1);
    assert_eq!(h.stamps(f).op, SlotStamps::OP_NONE);
}

#[test]
fn oms_161_reuse_quarantine() {
    for (row, rq, now, recycled) in [
        ("OMS-161.1", 5000, 4999, false),
        ("OMS-161.2", 5000, 5000, false),
        ("OMS-161.3", 5000, 5001, true),
        ("OMS-161.4", 0, 1, true),
    ] {
        let mut h = H::with(|c| c.reuse_quarantine_ms = rq);
        let s = h.place_ok(&buy_x(), 0);
        h.what(s, OrderEventKind::CancelAck, 0);
        h.sweep(now);
        assert_eq!(h.slot(s).state == OrderState::Free, recycled, "{row}");
        assert_eq!(h.o.tomb(VS, A1, s.slot, s.r#gen).is_some(), recycled, "{row}");
    }
}

#[test]
fn oms_162_late_fill_hits_tomb() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 3));
    let (f, l) = filled_gen2(&mut h);
    // The second leg was never placed; settle the link.
    h.out.clear();
    h.sweep(5001);
    let r3 = h.place_ok(&buy_x(), 5002);
    assert_eq!((r3.slot, r3.r#gen), (3, 3));
    h.ack(r3, 5, 5002);
    let link_state = h.link(l).state;
    let net0 = h.legs(A1, g1()).view.leg[0].net;
    let mut e = h.oe(r3, OrderEventKind::PartialFill);
    e.r#gen = 2;
    e.cum_filled = q("0.012");
    e.avg_px = p("60000.00");
    e.fee = Qty(720_000);
    e.fee_asset = USDT;
    h.ev(&e, 5003);
    let fo = h.out.fills[0];
    assert_eq!(
        (h.out.fills.len(), fo.qty, fo.net_qty, fo.group, fo.leg_ix),
        (1, q("0.002"), q("0.002"), g1(), 0)
    );
    assert!(fo.flags & fill_flags::LATE != 0);
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, Qty(net0.0 + 200_000));
    let tb = *h.o.tomb(VS, A1, 3, 2).unwrap();
    assert_eq!((tb.filled, tb.net_filled), (q("0.012"), q("0.01199")));
    assert_eq!(h.notes(notes::LATE_FILL)[0].level, NoteLevel::P0);
    assert_eq!(h.state(r3), OrderState::Live);
    assert_eq!(h.link(l).state, link_state);
    assert!(h.out.hedges.is_empty() && h.out.decisions.is_empty() && h.out.links.is_empty());
    let _ = f;
}

#[test]
fn oms_163_tomb_overwritten() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 3));
    for _ in 0..2 {
        let r = h.place_ok(&buy_x(), 0);
        h.ack(r, 1, 0);
    }
    let mut t = 0;
    for _ in 0..3 {
        let r = h.place_ok(&buy_x(), t);
        h.what(r, OrderEventKind::CancelAck, t);
        h.sweep(t + 5001);
        t += 5002;
    }
    assert!(h.o.tomb(VS, A1, 3, 1).is_none());
    assert!(h.o.tomb(VS, A1, 3, 2).is_some() && h.o.tomb(VS, A1, 3, 3).is_some());
    let mut e = h.oe(OrderRef::new(VS, A1, 3, 3), OrderEventKind::PartialFill);
    e.r#gen = 1;
    e.exch_order_id = 8888;
    e.cum_filled = q("0.001");
    e.detail = 1;
    h.ev(&e, t);
    assert_eq!(
        (
            h.out.foreign_fills.len(),
            h.out.foreign_fills[0].qty,
            h.out.foreign_fills[0].exch_order_id
        ),
        (1, q("0.001"), 8888)
    );
    assert_eq!(h.n_notes(notes::STALE_GEN_FILL), 1);
    assert_eq!(h.n_notes(notes::LATE_FILL), 0);
}

#[test]
fn oms_462_tomb_duplicate_count_and_fee_only() {
    use OrderEventKind::{Ack, CancelAck, FeeOnly, Fill, PartialFill, QueryResult};

    for (row, what, cum) in [
        ("OMS-462.1", Ack, "0"),
        ("OMS-462.2", CancelAck, "0"),
        ("OMS-462.3", QueryResult, "0.01"),
        ("OMS-462.4", Fill, "0.01"),
        ("OMS-462.5", PartialFill, "0.004"),
        ("OMS-462.6", FeeOnly, "0.01"),
    ] {
        let mut h = H::with(|c| c.slots.set(VS, A1, 1));
        let s = h.place_ok(&buy_x(), 0);
        h.ack(s, 777, 0);
        let mut fill = h.oe(s, Fill);
        fill.cum_filled = q("0.01");
        fill.avg_px = p("60000");
        h.ev(&fill, 1);
        h.sweep(5002);
        assert_eq!(h.slot(s).state, OrderState::Free, "{row}");
        let dup = h.o.stats().dup;
        let mut e = h.oe(s, what);
        e.inst = X;
        e.cum_filled = q(cum);
        e.avg_px = p("60000");
        e.fee = Qty(1000);
        e.fee_asset = USDT;
        h.ev(&e, 5003);
        assert_eq!(h.o.stats().dup, dup + u64::from(what != FeeOnly), "{row}");
        assert_eq!(h.slot(s).state, OrderState::Free, "{row}");
        assert_eq!(
            h.o.tomb(VS, A1, s.slot, s.r#gen).unwrap().filled,
            q("0.01"),
            "{row}"
        );
        assert!(h.out.fills.is_empty() && h.out.foreign_fills.is_empty(), "{row}");
        assert!(h.out.decisions.is_empty() && h.out.hedges.is_empty(), "{row}");
        assert!(h.out.notes.is_empty(), "{row}");
        if what == FeeOnly {
            assert_eq!(h.out.fees.len(), 1, "{row}");
            let fee = h.out.fees[0];
            assert_eq!(
                (fee.account, fee.venue, fee.inst, fee.fee),
                (A1, VS, X, 1000),
                "{row}"
            );
        } else {
            assert!(h.out.fees.is_empty(), "{row}");
        }
    }
}

#[test]
fn oms_164_tomb_keep() {
    for (row, now, kept) in [
        ("OMS-164.1", 3_599_999u64, true),
        ("OMS-164.2", 3_600_000, true),
        ("OMS-164.3", 3_600_001, false),
    ] {
        let mut h = H::with(|c| c.slots.set(VS, A1, 1));
        let s = h.place_ok(&buy_x(), 0);
        let mut e = h.oe(s, OrderEventKind::Ack);
        e.exch_order_id = 777;
        h.ev(&e, 0);
        h.what(s, OrderEventKind::CancelAck, 0);
        h.sweep(5001);
        h.o.test_tomb_mut(VS, A1, 1, 1, |tb| tb.freed_tsc = ms(0));
        h.sweep(now);
        assert_eq!(h.o.tomb(VS, A1, 1, 1).is_some(), kept, "{row}");
        let mut e = h.oe(s, OrderEventKind::PartialFill);
        e.exch_order_id = 777;
        e.cum_filled = q("0.004");
        e.detail = 1;
        h.ev(&e, now);
        if kept {
            assert_eq!(h.n_notes(notes::LATE_FILL), 1, "{row}");
        } else {
            assert_eq!(h.n_notes(notes::STALE_GEN_FILL), 1, "{row}");
        }
    }
}
