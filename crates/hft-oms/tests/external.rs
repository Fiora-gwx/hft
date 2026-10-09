#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ExternalCause, NoteLevel, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_types::{GROUP_NONE, Qty, Tif};

#[test]
fn oms_111_ext_held_evict() {
    let run = |eids: &[u64]| {
        let mut h = H::with(|c| c.ext_held_track.set(VS, A1, 2));
        let mut ev = 0;
        for e in eids {
            h.out.clear();
            h.o.external_adjust(
                A1,
                VS,
                g1(),
                X,
                q("0.001"),
                *e,
                ExternalCause::Unknown,
                &mut h.out,
            );
            ev += h.notes(notes::ANOMALY).iter().filter(|n| n.v[0] == 2).count();
        }
        (h, ev)
    };
    let (h, ev) = run(&[601, 602]);
    assert_eq!(
        (h.o.ext_held_of(VS, A1, 601), h.o.ext_held_of(VS, A1, 602), ev),
        (Some(q("0.001")), Some(q("0.001")), 0),
        "OMS-111.1"
    );
    let (h, ev) = run(&[601, 602, 603]);
    assert_eq!((h.o.ext_held_of(VS, A1, 601), ev), (None, 1), "OMS-111.2");
}

/// L1 first leg f (IOC buy X) in Unknown with exch_order_id 4444 and `ext_held[4444] = held`.
fn unknown_first(h: &mut H, qty: &str, held: &str) -> (hft_strategy_sdk::OrderRef, u32) {
    let (f, l) = place_first(h, q(qty), Tif::Ioc, plan_l1(), 0);
    let mut e = h.oe(f, OrderEventKind::Ack);
    e.exch_order_id = 4444;
    h.ev(&e, 0);
    h.what(f, OrderEventKind::OutcomeUnknown, 0);
    h.o.test_slot_mut(VS, A1, f.slot, |sl, _| sl.state = OrderState::Unknown);
    h.out.clear();
    h.o.external_adjust(A1, VS, g1(), X, q(held), 4444, ExternalCause::Unknown, &mut h.out);
    (f, l)
}

#[test]
fn oms_113_ext_held_dedup_on_slot() {
    let mut h = H::new();
    let (f, l) = unknown_first(&mut h, "0.01", "0.004");
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.004"));
    h.query(f, 3, q("0.01"), p("60000.00"), 1);
    assert_eq!((h.slot(f).filled, h.state(f)), (q("0.01"), OrderState::Filled));
    assert_eq!(h.link(l).first_net, q("0.01"));
    let fo = h.out.fills[0];
    assert_eq!((fo.qty, fo.net_qty), (q("0.006"), q("0.006")));
    assert!(fo.flags & fill_flags::EXT_DEDUP != 0);
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.01"));
    assert_eq!(h.o.ext_held_of(VS, A1, 4444).unwrap_or(Qty(0)), Qty(0));
    assert_eq!(h.notes(notes::EXT_DEDUP)[0].level, NoteLevel::P1);
    assert_eq!(one_hedge(&h).qty, q("0.01"));
}

#[test]
fn oms_250_external_adjust() {
    for (row, net, eid, cause, held) in [
        ("OMS-250.1", "0.004", 0u64, ExternalCause::Liquidation, None),
        ("OMS-250.2", "0.004", 0, ExternalCause::Adl, None),
        ("OMS-250.3", "0.004", 0, ExternalCause::Manual, None),
        ("OMS-250.4", "0.004", 9001, ExternalCause::Unknown, Some("0.004")),
        ("OMS-250.5", "-0.004", 9002, ExternalCause::Unknown, Some("0.004")),
        ("OMS-250.6", "0.004", 0, ExternalCause::Unknown, None),
    ] {
        let mut h = H::new();
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
        h.fill(f, q("0.012"), p("60000.00"), 0);
        let rq = one_hedge(&h);
        let hs = place_hedge(&mut h, &rq, 0).unwrap();
        h.fill(hs, q("0.012"), p("59990.0"), 1);
        let e0 = h.link(l);
        h.out.clear();
        h.o.external_adjust(A1, VX, g1(), Y, q(net), eid, cause, &mut h.out);
        let want = q("-0.012").0 + q(net).0;
        assert_eq!(h.legs(A1, g1()).view.leg[1].net, Qty(want), "{row}");
        assert_eq!(h.legs(A1, g1()).view.naked_qty, Qty(q(net).0.abs()), "{row}");
        let e1 = h.link(l);
        assert_eq!(
            (e1.first_net, e1.hedge_filled, e1.hedge_req),
            (e0.first_net, e0.hedge_filled, e0.hedge_req),
            "{row}"
        );
        assert!(h.out.decisions.is_empty() && h.out.hedges.is_empty(), "{row}");
        match held {
            Some(x) => assert_eq!(h.o.ext_held_of(VX, A1, eid), Some(q(x)), "{row}"),
            None => assert_eq!(h.o.table_view(VX, A1).unwrap().n_ext_held, 0, "{row}"),
        }
    }
}

#[test]
fn oms_251_group_none_external() {
    let mut h = H::new();
    let before = (h.legs(A1, g1()).view, h.legs(A1, g2()).view);
    h.out.clear();
    h.o.external_adjust(
        A1,
        VX,
        GROUP_NONE,
        Y,
        q("0.004"),
        9003,
        ExternalCause::Unknown,
        &mut h.out,
    );
    assert_eq!((h.legs(A1, g1()).view, h.legs(A1, g2()).view), before);
    assert!(h.legs(A1, GROUP_NONE).is_zero());
    assert_eq!(h.o.ext_held_of(VX, A1, 9003), Some(q("0.004")));
}

#[test]
fn oms_253_ext_flattened_no_double_hedge() {
    let mut h = H::new();
    let (f, l) = unknown_first(&mut h, "0.012", "0.012");
    h.o.test_set_net(A1, g1(), Y, q("-0.012"));
    h.query(f, 3, q("0.012"), p("60000.00"), 1);
    assert_eq!(h.state(f), OrderState::Filled);
    assert_eq!(h.link(l).first_net, q("0.012"));
    assert!(h.out.fills.is_empty());
    assert_eq!(h.legs(A1, g1()).view.leg[0].net, q("0.012"));
    assert_eq!(h.n_notes(notes::EXT_DEDUP), 1);
    assert!(h.out.hedges.is_empty());
    assert_eq!(h.n_notes(notes::HEDGE_NAKED_CAPPED), 1);
    assert_eq!(h.link(l).hedge_req, Qty(0));
}
