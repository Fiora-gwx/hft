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
use hft_strategy_sdk::{OrderRef, OrderUpdate, OrderUpdateKind};
use hft_types::{Px, Qty, Side, SlotId, Tif};

/// A foreign/unresolved report on a1@VS.
fn foreign(h: &H, eid: u64, cum: &str, side: Side) -> hft_event::OrderEvent {
    let mut e = h.oe(OrderRef::new(VS, A1, 1, 0), OrderEventKind::PartialFill);
    e.slot = SlotId(0);
    e.r#gen = 0;
    e.inst = X;
    e.exch_order_id = eid;
    e.cum_filled = q(cum);
    e.px = p("60000.00");
    e.detail = side as u8;
    e
}

#[test]
fn oms_035_foreign_order_no_fill() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 0);
    let mut e = foreign(&h, 555, "0", Side::Buy);
    e.what = OrderEventKind::Ack;
    let f0 = h.o.stats().foreign_orders;
    h.ev(&e, 1);
    let n = h.notes(notes::FOREIGN_ORDER);
    assert_eq!((n.len(), n[0].level), (1, NoteLevel::P1));
    assert!(h.out.fills.is_empty() && h.out.foreign_fills.is_empty() && h.out.decisions.is_empty());
    assert_eq!(h.state(s), OrderState::Live);
    assert_eq!(h.o.stats().foreign_orders, f0 + 1);
}

#[test]
fn oms_069_mismatch_skips_table() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 1));
    let s1 = h.place_ok(&buy_x(), 0);
    h.fill(s1, q("0.01"), p("60000.00"), 0);
    h.sweep(5001);
    let s = h.place_ok(&buy_x(), 5002);
    h.ack(s, 2, 5002);
    assert_eq!(s.r#gen, 2);
    let mut e = h.oe(s, OrderEventKind::Reject);
    e.r#gen = 3;
    let st0 = h.o.stats().stale_gen;
    h.ev(&e, 5003);
    assert_eq!((h.state(s), h.slot(s).filled), (OrderState::Live, Qty(0)));
    assert!(h.out.notes.is_empty());
    assert_eq!(h.o.stats().stale_gen, st0 + 1);
    let mut e = h.oe(s, OrderEventKind::CancelAck);
    e.r#gen = 1;
    e.cum_filled = q("0.01");
    h.ev(&e, 5004);
    assert_eq!(h.state(s), OrderState::Live);
    assert!(h.out.is_empty());
}

#[test]
fn oms_100_increment_notional() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 0);
    h.fill(s, q("0.004"), p("60000.00"), 1);
    assert_eq!(
        (h.out.fills[0].qty, h.out.fills[0].px),
        (q("0.004"), p("60000.00"))
    );
    assert_eq!(h.slot(s).filled_notional, 6_000_000 * 400_000);
    h.fill(s, q("0.01"), p("60001.00"), 2);
    assert_eq!(h.slot(s).filled_notional, 6_000_100 * 1_000_000);
    assert_eq!(
        (h.out.fills[0].qty, h.out.fills[0].px),
        (q("0.006"), p("60001.67"))
    );
}

#[test]
fn oms_101_non_increasing_cum() {
    for (row, cum, fill) in [
        ("OMS-101.1", "0.004", None),
        ("OMS-101.2", "0.003", None),
        ("OMS-101.3", "0.005", Some("0.001")),
    ] {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        h.ack(s, 1, 0);
        h.fill(s, q("0.004"), p("60000.00"), 1);
        let d0 = h.o.stats().dup;
        h.fill(s, q(cum), p("60000.00"), 2);
        match fill {
            None => {
                assert!(h.out.fills.is_empty(), "{row}");
                assert_eq!(h.slot(s).filled, q("0.004"), "{row}");
                if row == "OMS-101.1" {
                    assert_eq!(h.o.stats().dup, d0 + 1, "{row}");
                }
            }
            Some(f) => assert_eq!(h.out.fills[0].qty, q(f), "{row}"),
        }
    }
}

#[test]
fn oms_102_order_event_zero_avg_px() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 0);
    h.fill(s, q("0.004"), p("60000.00"), 1);
    let mut e = h.oe(s, OrderEventKind::PartialFill);
    e.cum_filled = q("0.006");
    e.px = p("60010.00");
    h.ev(&e, 2);
    let f = h.out.fills[0];
    assert_eq!((f.qty, f.px), (q("0.002"), p("60010.00")));
    assert_eq!(f.flags & fill_flags::PX_EST, 0);
    assert_eq!(h.n_notes(notes::FILL_PX_ESTIMATED), 0);
    assert_eq!(
        h.slot(s).filled_notional,
        6_000_000 * 400_000 + 6_001_000 * 200_000
    );
}

#[test]
fn oms_103_query_zero_avg_px() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.what(s, OrderEventKind::OutcomeUnknown, 0);
    h.query(s, 3, q("0.01"), Px(0), 1);
    let f = h.out.fills[0];
    assert_eq!((f.qty, f.px), (q("0.01"), p("60000.00")));
    assert!(f.flags & fill_flags::PX_EST != 0);
    assert_eq!(h.notes(notes::FILL_PX_ESTIMATED)[0].level, NoteLevel::P1);
    let nr = h.notes(notes::NEEDS_REST_QUERY);
    assert!(nr.iter().any(|n| n.v[0] >> 16 & 1 == 1));
    assert_eq!(h.state(s), OrderState::Filled);
}

#[test]
fn oms_104_overfill() {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.01"), Tif::Ioc, plan_l1(), 0);
    h.ack(f, 1, 0);
    h.fill(f, q("0.012"), p("60000.00"), 1);
    let fo = h.out.fills[0];
    assert_eq!(fo.qty, q("0.012"));
    assert!(fo.flags & fill_flags::OVERFILL != 0);
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.012"));
    assert_eq!(h.link(l).first_net, q("0.01"));
    assert!(h.out.hedges.len() <= 1);
    if let Some(r) = h.out.hedges.first() {
        assert!(r.basis <= q("0.01"));
    }
    assert_eq!(h.link(l).state, LinkState::Broken);
    assert_eq!(h.n_notes(notes::OVERFILL), 1);
    assert_eq!(h.notes(notes::HEDGE_BROKEN)[0].level, NoteLevel::P0);
    let lu: Vec<_> = h
        .out
        .links
        .iter()
        .filter(|u| u.state == LinkState::Broken)
        .collect();
    assert_eq!((lu.len(), lu[0].broken_reason as u8), (1, 4));
}

/// Slot 3 of a1@VS with current gen 5 and tombstones of gens 3, 4.
fn gen5(h: &mut H) -> OrderRef {
    let mut t = 0;
    let mut r = OrderRef::new(VS, A1, 3, 0);
    for _ in 0..5 {
        r = h.place_ok(&buy_x(), t);
        if r.r#gen == 5 {
            break;
        }
        h.what(r, OrderEventKind::CancelAck, t);
        h.sweep(t + 5001);
        t += 5002;
    }
    r
}

#[test]
fn oms_105_stale_gen_fill_booked() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 3));
    for _ in 0..2 {
        let r = h.place_ok(&buy_x(), 0);
        h.ack(r, 9, 0);
    }
    let s = gen5(&mut h);
    assert_eq!((s.slot, s.r#gen), (3, 5));
    let mut e = h.oe(s, OrderEventKind::PartialFill);
    e.r#gen = 1;
    e.exch_order_id = 4444;
    e.cum_filled = q("0.004");
    e.avg_px = p("60000.00");
    e.fee = Qty(240_000);
    e.fee_asset = USDT;
    e.detail = 1;
    let before = h.slot(s);
    h.ev(&e, 30_000);
    assert_eq!(h.slot(s).state, before.state);
    let ff = h.out.foreign_fills[0];
    assert_eq!(
        (
            ff.venue,
            ff.account,
            ff.inst,
            ff.side,
            ff.qty,
            ff.px,
            ff.exch_order_id,
            ff.trade_id,
            ff.cum_filled
        ),
        (
            VS,
            A1,
            X,
            Side::Buy,
            q("0.004"),
            p("60000.00"),
            4444,
            0,
            q("0.004")
        )
    );
    let sg = h.notes(notes::STALE_GEN_FILL);
    assert_eq!(sg[0].level, NoteLevel::P0);
    assert_eq!(&sg[0].v[..7], &[258, 3, 1, 1, 7, 4444, 400_000]);
    let ffn = h.notes(notes::FOREIGN_FILL);
    assert_eq!((ffn.len(), ffn[0].v[0] >> 24 & 0xFF), (1, 2));
    assert_eq!(
        h.notes(notes::NEEDS_REST_QUERY)
            .iter()
            .filter(|n| n.v[0] >> 16 & 1 == 1)
            .count(),
        1
    );
}

#[test]
fn oms_106_stale_gen_no_eid() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    let mut e = h.oe(s, OrderEventKind::PartialFill);
    e.r#gen = 9;
    e.exch_order_id = 0;
    e.cum_filled = q("0.004");
    e.detail = 1;
    h.ev(&e, 1);
    assert!(h.out.foreign_fills.is_empty());
    assert_eq!(h.n_notes(notes::FOREIGN_FILL), 0);
    let sg = h.notes(notes::STALE_GEN_FILL);
    assert_eq!((sg.len(), sg[0].v[2]), (1, 0));
    assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 1);
}

#[test]
fn oms_107_stale_gen_ext_held() {
    for (row, held, ff, rest, dedup, booked) in [
        ("OMS-107.1", Some("0.004"), Some("0.006"), None, true, 1),
        ("OMS-107.2", Some("0.01"), None, None, true, 0),
        ("OMS-107.3", Some("0.012"), None, Some("0.002"), true, 0),
        ("OMS-107.4", None, Some("0.01"), None, false, 1),
    ] {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        if let Some(b) = held {
            h.o.test_set_ext_held(VS, A1, 4444, q(b));
        }
        let mut e = h.oe(s, OrderEventKind::PartialFill);
        e.r#gen = 9;
        e.exch_order_id = 4444;
        e.cum_filled = q("0.01");
        e.detail = 1;
        h.ev(&e, 1);
        match ff {
            Some(x) => assert_eq!(
                (h.out.foreign_fills[0].qty, h.out.foreign_fills[0].cum_filled),
                (q(x), q("0.01")),
                "{row}"
            ),
            None => assert!(h.out.foreign_fills.is_empty(), "{row}"),
        }
        assert_eq!(
            h.o.ext_held_of(VS, A1, 4444).unwrap_or(Qty(0)),
            rest.map_or(Qty(0), q),
            "{row}"
        );
        assert_eq!(h.n_notes(notes::EXT_DEDUP), usize::from(dedup), "{row}");
        assert_eq!(h.notes(notes::STALE_GEN_FILL)[0].v[2], booked, "{row}");
    }
}

#[test]
fn oms_108_defensive_path_dedup() {
    let mut h = H::new();
    let e = foreign(&h, 555, "0.004", Side::Sell);
    h.ev(&e, 1);
    let ff = h.out.foreign_fills[0];
    assert_eq!(
        (ff.qty, ff.cum_filled, ff.side),
        (q("0.004"), q("0.004"), Side::Sell)
    );
    assert_eq!(
        h.notes(notes::FOREIGN_FILL_VIA_ORDER_EVENT)[0].level,
        NoteLevel::P1
    );
    assert_eq!(h.notes(notes::FOREIGN_FILL)[0].v[0] >> 24 & 0xFF, 1);
    assert_eq!(h.o.foreign_of(VS, A1, 555), Some(q("0.004")));
    let e = foreign(&h, 555, "0.01", Side::Sell);
    h.ev(&e, 2);
    assert_eq!(
        (h.out.foreign_fills[0].qty, h.out.foreign_fills[0].cum_filled),
        (q("0.006"), q("0.01"))
    );
    assert_eq!(h.o.foreign_of(VS, A1, 555), Some(q("0.01")));
    h.ev(&e, 3);
    assert!(h.out.foreign_fills.is_empty() && h.out.notes.is_empty());
}

#[test]
fn oms_110_foreign_table_evict() {
    let run = |seq: &[(u64, &str)]| {
        let mut h = H::with(|c| c.foreign_fill_track.set(VS, A1, 2));
        let mut ffs = Vec::new();
        let mut ev = 0;
        for (eid, cum) in seq {
            let e = foreign(&h, *eid, cum, Side::Buy);
            h.ev(&e, 1);
            ffs.extend(h.out.foreign_fills.iter().copied());
            ev += h.notes(notes::ANOMALY).iter().filter(|n| n.v[0] == 2).count();
        }
        (h, ffs, ev)
    };
    let (h, ffs, ev) = run(&[(501, "0.001"), (502, "0.001")]);
    assert_eq!(
        (ffs.len(), ev, h.o.table_view(VS, A1).unwrap().n_foreign),
        (2, 0, 2),
        "OMS-110.1"
    );
    let (h, _, ev) = run(&[(501, "0.001"), (502, "0.001"), (503, "0.001")]);
    assert_eq!(ev, 1, "OMS-110.2");
    assert!(
        h.o.foreign_of(VS, A1, 501).is_none()
            && h.o.foreign_of(VS, A1, 502).is_some()
            && h.o.foreign_of(VS, A1, 503).is_some()
    );
    let (_, ffs, _) = run(&[(501, "0.001"), (502, "0.001"), (503, "0.001"), (501, "0.002")]);
    assert_eq!(ffs.last().unwrap().qty, q("0.002"), "OMS-110.3");
}

#[test]
fn oms_114_foreign_fill_note_fields() {
    let mut h = H::new();
    let mut e = foreign(&h, 555, "0.004", Side::Buy);
    e.fee = Qty(400);
    e.fee_asset = BTC;
    h.ev(&e, 1);
    let n = h.notes(notes::FOREIGN_FILL);
    assert_eq!(n.len(), 1);
    let n = n[0];
    assert_eq!((n.level, n.inst, n.group.0), (NoteLevel::Info, X, 0));
    assert_eq!(n.v[0], 4_311_810_306);
    assert_eq!(&n.v[1..8], &[400_000, 6_000_000, 400, 555, 0, 400_000, 400_000]);
}

/// OMS-460: a fill report whose quantity a `CancelReject{GONE}` already brought books its fee
/// once (`FeeOut`), repeats book nothing.
#[test]
fn oms_460_fee_of_fill_after_non_fill_report() {
    let fill = |h: &mut H, r: OrderRef, cum: &str, fee: i64, t: u64| {
        let mut e = h.oe(r, OrderEventKind::PartialFill);
        if q(cum) == q("0.01") {
            e.what = OrderEventKind::Fill;
        }
        e.cum_filled = q(cum);
        e.avg_px = p("60000.00");
        e.fee = Qty(fee);
        e.fee_asset = USDT;
        h.ev(&e, t);
    };
    let gone = |h: &mut H, r: OrderRef, t: u64| {
        h.cancel(r, t).unwrap();
        let mut e = h.oe(r, OrderEventKind::CancelReject);
        e.detail = 1;
        e.cum_filled = q("0.01");
        e.avg_px = p("60000.00");
        h.ev(&e, t + 1);
        assert_eq!(h.state(r), OrderState::Filled);
        assert_eq!(h.out.fills.len(), 1);
        assert_eq!(h.out.fills[0].fee, 0);
    };
    // Steps 1–3: one execution.
    let mut h = H::new();
    let r = h.place_ok(&buy_x(), 0);
    h.ack(r, 7, 1);
    gone(&mut h, r, 2);
    fill(&mut h, r, "0.01", 1000, 4);
    assert!(h.out.fills.is_empty(), "OMS-460 step 2: no second fill");
    let f = h.out.fees.as_slice();
    assert_eq!(f.len(), 1, "OMS-460 step 2");
    assert_eq!(
        (f[0].inst, f[0].fee, f[0].fee_asset),
        (X, 1000, USDT),
        "OMS-460 step 2"
    );
    assert_eq!(h.ext(r).fee_cum, q("0.01"), "OMS-460 step 2");
    assert_eq!(h.slot(r).fee_accum, 1000, "OMS-460 step 2");
    fill(&mut h, r, "0.01", 1000, 5);
    assert!(
        h.out.fees.is_empty() && h.out.fills.is_empty(),
        "OMS-460 step 3: repeat"
    );
    // Steps 4–6: two executions behind one CancelReject.
    let mut h = H::new();
    let r = h.place_ok(&buy_x(), 0);
    h.ack(r, 8, 1);
    gone(&mut h, r, 2);
    fill(&mut h, r, "0.004", 400, 4);
    assert_eq!(h.out.fees.as_slice().len(), 1, "OMS-460 step 5");
    assert_eq!(h.out.fees[0].fee, 400, "OMS-460 step 5");
    fill(&mut h, r, "0.004", 400, 5);
    assert!(h.out.fees.is_empty(), "OMS-460 step 5: repeat");
    fill(&mut h, r, "0.01", 600, 6);
    assert_eq!(h.out.fees.as_slice().len(), 1, "OMS-460 step 6");
    assert_eq!(h.out.fees[0].fee, 600, "OMS-460 step 6");
    assert_eq!(h.slot(r).fee_accum, 1000, "OMS-460 step 6");
}

/// OMS-490 slot: spend buy X 0.01 @ 60000.00 IOC on a1@VS, acked.
fn spend_slot(h: &mut H, spend: bool) -> OrderRef {
    let mut it = intent(VS, A1, X, Side::Buy, Tif::Ioc, p("60000.00"), q("0.01"), g1());
    if spend {
        it.intent.flags = hft_strategy_sdk::OrderIntent::SPEND;
    }
    let r = h.place_ok(&it, 0);
    h.ack(r, 1, 0);
    r
}

/// `(d, cum)` of the owner's `Filled{done: true}` updates of the last call.
fn done_updates(h: &H) -> Vec<(Qty, Qty)> {
    h.out
        .updates
        .iter()
        .filter_map(|u| match u.kind {
            OrderUpdateKind::Filled {
                d, cum, done: true, ..
            } => Some((d, cum)),
            _ => None,
        })
        .collect()
}

fn spend_fill(h: &mut H, r: OrderRef, what: OrderEventKind, cum: &str) {
    let mut e = h.oe(r, what);
    e.cum_filled = q(cum);
    e.avg_px = p("50000.00");
    e.fee_asset = USDT;
    h.ev(&e, 1);
}

#[test]
fn oms_490_spend_buy() {
    use hft_event::OrderFlags;
    use hft_strategy_sdk::{LegLinkOpt, OrderIntent};
    let spend_bit = |h: &H, r: OrderRef| h.slot(r).oflags & OrderFlags::SPEND.0 != 0;
    // .1 spends 600 for 0.012: a normal fill, the slot stays open.
    let mut h = H::new();
    let r = spend_slot(&mut h, true);
    assert!(spend_bit(&h, r), "OMS-490.1");
    spend_fill(&mut h, r, OrderEventKind::PartialFill, "0.012");
    assert_eq!(h.out.fills.len(), 1, "OMS-490.1");
    assert_eq!(h.out.fills[0].qty, q("0.012"), "OMS-490.1");
    assert_eq!(h.out.fills[0].flags & fill_flags::OVERFILL, 0, "OMS-490.1");
    assert_eq!(h.n_notes(notes::OVERFILL), 0, "OMS-490.1");
    assert_eq!(h.slot(r).state, OrderState::Live, "OMS-490.1");
    assert_eq!(h.slot(r).filled, q("0.012"), "OMS-490.1");
    assert!(
        matches!(
            h.out.updates.as_slice(),
            [OrderUpdate {
                kind: OrderUpdateKind::Filled { done: false, .. },
                ..
            }]
        ),
        "OMS-490.1 {:?}",
        h.out.updates
    );
    // .2 the venue's Fill ends it; the owner hears the end once.
    spend_fill(&mut h, r, OrderEventKind::Fill, "0.012");
    assert_eq!(h.slot(r).state, OrderState::Filled, "OMS-490.2");
    assert!(h.out.fills.is_empty(), "OMS-490.2");
    assert_eq!(done_updates(&h), vec![(Qty(0), q("0.012"))], "OMS-490.2");
    // .3 605 spent: overfill.
    let mut h = H::new();
    let r = spend_slot(&mut h, true);
    spend_fill(&mut h, r, OrderEventKind::PartialFill, "0.0121");
    assert!(h.out.fills[0].flags & fill_flags::OVERFILL != 0, "OMS-490.3");
    assert_eq!(h.n_notes(notes::OVERFILL), 1, "OMS-490.3");
    // .4 in flight = the unspent amount at the order price.
    let mut h = H::new();
    let r = spend_slot(&mut h, true);
    spend_fill(&mut h, r, OrderEventKind::PartialFill, "0.006");
    assert_eq!(h.o.inflight().pos(A1, X).open_buy, q("0.005"), "OMS-490.4");
    h.what(r, OrderEventKind::Expired, 2);
    assert_eq!(h.slot(r).state, OrderState::Cancelled, "OMS-490.4");
    assert_eq!(h.o.inflight().pos(A1, X).open_buy, Qty(0), "OMS-490.4");
    // .5 Expired after more than qty: Filled.
    let mut h = H::new();
    let r = spend_slot(&mut h, true);
    spend_fill(&mut h, r, OrderEventKind::PartialFill, "0.012");
    h.what(r, OrderEventKind::Expired, 2);
    assert_eq!(h.slot(r).state, OrderState::Filled, "OMS-490.5");
    assert_eq!(done_updates(&h), vec![(Qty(0), q("0.012"))], "OMS-490.5");
    // .6 QueryResult{Filled} ends it.
    let mut h = H::new();
    let r = spend_slot(&mut h, true);
    spend_fill(&mut h, r, OrderEventKind::PartialFill, "0.012");
    h.query(r, 3, q("0.012"), p("50000.00"), 2);
    assert_eq!(h.slot(r).state, OrderState::Filled, "OMS-490.6");
    assert_eq!(done_updates(&h), vec![(Qty(0), q("0.012"))], "OMS-490.6");
    // .6 a Fill with an increment ends it with that one update.
    let mut h = H::new();
    let r = spend_slot(&mut h, true);
    spend_fill(&mut h, r, OrderEventKind::Fill, "0.012");
    assert_eq!(h.slot(r).state, OrderState::Filled, "OMS-490.6");
    assert_eq!(done_updates(&h), vec![(q("0.012"), q("0.012"))], "OMS-490.6");
    // .7 shape.
    let base = intent(VS, A1, X, Side::Sell, Tif::Ioc, p("60000.00"), q("0.01"), g1()).intent;
    let with = |side, tif, link: bool| {
        let mut it = base;
        it.side = side;
        it.tif = tif;
        it.flags = OrderIntent::SPEND;
        if link {
            it.link = LegLinkOpt::some(link_l1(plan_l1()));
        }
        it
    };
    assert!(spend_valid(&base), "OMS-490.7");
    assert!(spend_valid(&with(Side::Buy, Tif::Fok, false)), "OMS-490.7");
    assert!(!spend_valid(&with(Side::Sell, Tif::Ioc, false)), "OMS-490.7");
    assert!(!spend_valid(&with(Side::Buy, Tif::Gtc, false)), "OMS-490.7");
    assert!(!spend_valid(&with(Side::Buy, Tif::Ioc, true)), "OMS-490.7");
    let mut h = H::new();
    let mut it = intent(VS, A1, X, Side::Sell, Tif::Ioc, p("60000.00"), q("0.01"), g1());
    it.intent.flags = OrderIntent::SPEND;
    let r = h.place_ok(&it, 0);
    assert!(!spend_bit(&h, r), "OMS-490.7");
    // .8 other slots: unchanged overfill rule.
    let mut h = H::new();
    let r = spend_slot(&mut h, false);
    spend_fill(&mut h, r, OrderEventKind::Fill, "0.012");
    assert!(h.out.fills[0].flags & fill_flags::OVERFILL != 0, "OMS-490.8");
    assert_eq!(h.n_notes(notes::OVERFILL), 1, "OMS-490.8");
}
