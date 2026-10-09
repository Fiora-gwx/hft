#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{OrderAction, OrderEventKind};
use hft_oms::*;
use hft_strategy_sdk::OrderRef;
use hft_types::{Px, Qty, Side, Tif};

#[test]
fn oms_040_place_allocates() {
    let mut h = H::new();
    let mut it = buy_x();
    it.intent.tag = 42;
    it.trig.kind = 0x0101;
    h.out.clear();
    let r = h.o.place(&it, ms(1000), 500, &mut h.out).unwrap();
    assert_eq!(r, OrderRef::new(VS, A1, 1, 1));
    let d = h.decisions(OrderAction::New);
    assert_eq!(d.len(), 1);
    let d = d[0];
    assert_eq!(
        (d.slot.0, d.r#gen, d.px, d.qty, d.group, d.inst),
        (1, 1, Px(6_000_000), Qty(1_000_000), g1(), X)
    );
    assert_eq!(
        (
            d.target_slot.0,
            d.reject,
            d.reason,
            d.link_role,
            d.oflags,
            d.trigger_seq
        ),
        (0, 0, 0, 0, 0, 500)
    );
    assert_eq!(transitions(&h), vec![(0, 1, 1)]);
    let sl = h.slot(r);
    assert_eq!((sl.state, sl.t_state), (OrderState::PendingNew, ms(1000)));
    let ex = h.ext(r);
    assert_eq!((ex.tag, ex.origin, ex.class, ex.leg_ix), (42, 1, 1, 0));
    assert_eq!(h.stamps(r).op, 0);
    let ib = h.o.inflight();
    assert_eq!((ib.open_buy(A1, X), ib.imm_buy(A1, X)), (q("0.01"), q("0.01")));
    assert_eq!(ib.buy_notional(A1, VS).0, usdt("600.00"));
    assert_eq!(h.legs(A1, g1()).view.leg[0].inflight_immediate, q("0.01"));
}

#[test]
fn oms_041_place_no_free_slot() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 2));
    for i in 0..2 {
        let r = h.place_ok(&buy_x(), 0);
        h.ack(r, i + 1, 1);
    }
    let ib = h.o.inflight().clone();
    let legs = h.legs(A1, g1());
    let free = h.o.table_view(VS, A1).unwrap().free_len;
    assert_eq!(h.place(&buy_x(), 2), Err(PlaceError::NoFreeSlot));
    assert!(h.out.is_empty());
    assert_eq!(h.o.inflight().pos(A1, X), ib.pos(A1, X));
    assert_eq!(h.o.inflight().buy_notional(A1, VS), ib.buy_notional(A1, VS));
    assert_eq!(
        hft_strategy_sdk_bytes(&h.legs(A1, g1())),
        hft_strategy_sdk_bytes(&legs)
    );
    assert_eq!(h.o.table_view(VS, A1).unwrap().free_len, free);
}

fn hft_strategy_sdk_bytes(l: &LegState) -> Vec<u8> {
    use zerocopy::IntoBytes;
    l.as_bytes().to_vec()
}

#[test]
fn oms_042_place_no_table() {
    let mut h = H::new();
    let it = intent(
        VG,
        A2,
        Z,
        Side::Buy,
        Tif::Gtc,
        p("60000.0"),
        q("0.001"),
        hft_types::GROUP_NONE,
    );
    assert_eq!(h.place(&it, 0), Err(PlaceError::NoTable));
    assert!(h.out.is_empty());
}

fn live(h: &mut H, it: &hft_oms::ApprovedIntent, eid: u64) -> OrderRef {
    let r = h.place_ok(it, 0);
    h.ack(r, eid, 1);
    r
}

#[test]
fn oms_045_cancel_live() {
    let mut h = H::new();
    let s = live(&mut h, &buy_x(), 777);
    let ib = h.o.inflight().pos(A1, X);
    let leg = h.legs(A1, g1()).view.leg[0];
    h.out.clear();
    h.o.cancel(s, &TrigStamps::default(), ms(2000), 600, &mut h.out)
        .unwrap();
    assert_eq!(h.state(s), OrderState::PendingCancel);
    assert!(h.flags(s) & SlotFlags::CANCEL_SENT != 0);
    let d = h.decisions(OrderAction::Cancel);
    assert_eq!((d.len(), d[0].target_slot.0, d[0].r#gen), (1, s.slot, s.r#gen));
    assert_eq!(transitions(&h), vec![(2, 3, 2)]);
    assert_eq!(h.o.inflight().pos(A1, X), ib);
    assert_eq!(h.legs(A1, g1()).view.leg[0], leg);
}

#[test]
fn oms_046_cancel_pending_new() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.cancel(s, 1).unwrap();
    assert_eq!(h.state(s), OrderState::PendingCancel);
    let l = h.legs(A1, g1()).view.leg[0];
    assert_eq!((l.inflight_immediate, l.inflight_resting), (q("0.01"), Qty(0)));
    h.ack(s, 777, 2);
    assert_eq!(h.state(s), OrderState::PendingCancel);
    assert_eq!(h.slot(s).exch_order_id, 777);
    assert!(h.flags(s) & SlotFlags::ACKED != 0);
    let l = h.legs(A1, g1()).view.leg[0];
    assert_eq!((l.inflight_immediate, l.inflight_resting), (Qty(0), q("0.01")));
    h.what(s, OrderEventKind::CancelAck, 3);
    assert_eq!(h.state(s), OrderState::Cancelled);
    let l = h.legs(A1, g1()).view.leg[0];
    assert_eq!((l.inflight_immediate, l.inflight_resting), (Qty(0), Qty(0)));
}

#[test]
fn oms_047_cancel_pending_amend() {
    let mut h = H::new();
    let t = live(&mut h, &sell_y(), 1);
    h.amend(t, p("60100.0"), q("0.01"), 2).unwrap();
    assert_eq!(h.state(t), OrderState::PendingAmend);
    h.cancel(t, 3).unwrap();
    assert_eq!(h.state(t), OrderState::PendingCancel);
    let f = h.flags(t);
    assert!(f & SlotFlags::CANCEL_SENT != 0 && f & SlotFlags::AMEND_SENT != 0);
    assert_eq!(h.decisions(OrderAction::Cancel).len(), 1);
}

#[test]
fn oms_048_cancel_rejected_states() {
    use OrderState::*;
    for (row, st) in [
        ("OMS-048.1", PendingCancel),
        ("OMS-048.2", Unknown),
        ("OMS-048.3", Filled),
        ("OMS-048.4", Cancelled),
        ("OMS-048.5", Rejected),
    ] {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        h.set_state(s, st);
        assert_eq!(h.cancel(s, 1), Err(OpError::NotAllowed), "{row}");
        assert!(h.out.is_empty(), "{row}");
    }
    let mut h = H::new();
    let s = live(&mut h, &buy_x(), 1);
    let old = OrderRef::new(s.venue, s.account, s.slot, s.r#gen - 1);
    assert_eq!(h.cancel(old, 2), Err(OpError::StaleRef), "OMS-048.6");
    let free = OrderRef::new(VS, A1, 9, 0);
    assert_eq!(h.cancel(free, 2), Err(OpError::StaleRef), "OMS-048.7");
    assert!(h.out.is_empty());
}

#[test]
fn oms_049_amend_live() {
    let mut h = H::new();
    let t = live(&mut h, &sell_y(), 1);
    h.amend(t, p("60200.0"), q("0.008"), 2).unwrap();
    let sl = h.slot(t);
    assert_eq!(sl.state, OrderState::PendingAmend);
    assert_eq!((sl.pending_px, sl.pending_qty), (p("60200.0"), q("0.008")));
    assert!(sl.sflags.0 & SlotFlags::AMEND_SENT != 0);
    assert_eq!((sl.px, sl.qty), (p("60100.0"), q("0.01")));
    let d = h.decisions(OrderAction::Amend);
    assert_eq!(
        (d.len(), d[0].px, d[0].qty, d[0].target_slot.0),
        (1, Px(6_020_000), Qty(800_000), t.slot)
    );
    assert_eq!(transitions(&h), vec![(2, 4, 3)]);
}

#[test]
fn oms_050_amend_rejected_states() {
    use OrderState::*;
    let mut h = H::new();
    let s = live(&mut h, &buy_x(), 1);
    assert_eq!(
        h.amend(s, p("60200.00"), q("0.008"), 2),
        Err(OpError::NotAllowed),
        "OMS-050.1"
    );
    assert!(h.out.is_empty());
    for (row, st) in [
        ("OMS-050.2", PendingNew),
        ("OMS-050.3", PendingCancel),
        ("OMS-050.4", PendingAmend),
        ("OMS-050.5", Unknown),
        ("OMS-050.6", Filled),
    ] {
        let mut h = H::new();
        let t = h.place_ok(&sell_y(), 0);
        h.set_state(t, st);
        assert_eq!(
            h.amend(t, p("60200.00"), q("0.008"), 1),
            Err(OpError::NotAllowed),
            "{row}"
        );
        assert!(h.out.is_empty(), "{row}");
    }
    let mut h = H::new();
    let t = live(&mut h, &sell_y(), 1);
    let old = OrderRef::new(t.venue, t.account, t.slot, t.r#gen + 1);
    assert_eq!(
        h.amend(old, p("60200.00"), q("0.008"), 2),
        Err(OpError::StaleRef),
        "OMS-050.7"
    );
}

#[test]
fn oms_051_amend_below_filled() {
    let mut h = H::new();
    let t = live(&mut h, &sell_y(), 1);
    h.fill(t, q("0.009"), p("60100.0"), 2);
    h.amend(t, p("60100.0"), q("0.008"), 3).unwrap();
    let mut e = h.oe(t, OrderEventKind::AmendAck);
    e.px = p("60100.0");
    e.qty = q("0.008");
    e.cum_filled = q("0.009");
    h.ev(&e, 4);
    let sl = h.slot(t);
    assert_eq!((sl.qty, sl.state), (q("0.009"), OrderState::Filled));
    assert_eq!(transitions(&h), vec![(4, 5, 10)]);
    assert_eq!(h.o.inflight().open_sell(A1, Y), Qty(0));
}

#[test]
fn oms_052_partial_is_live() {
    let mut h = H::new();
    let s = live(&mut h, &buy_x(), 1);
    h.fill(s, q("0.004"), p("60000.00"), 2);
    let sl = h.slot(s);
    assert_eq!((sl.state as u8, sl.filled), (2, q("0.004")));
}

#[test]
fn oms_456_amend_qty_down_only() {
    let rd_down = build_rd(&RdOpts {
        vx_amend_down_only: true,
        ..RdOpts::default()
    });
    let rows: [(&str, bool, &str, &str, Result<(), OpError>); 6] = [
        ("OMS-456.1", true, "60100.0", "0.008", Ok(())),
        ("OMS-456.2", true, "60100.0", "0.01", Ok(())),
        (
            "OMS-456.3",
            true,
            "60100.0",
            "0.011",
            Err(OpError::AmendUnsupported),
        ),
        (
            "OMS-456.4",
            true,
            "60200.0",
            "0.008",
            Err(OpError::AmendUnsupported),
        ),
        (
            "OMS-456.5",
            true,
            "60099.9",
            "0.01",
            Err(OpError::AmendUnsupported),
        ),
        ("OMS-456.6", false, "60200.0", "0.011", Ok(())),
    ];
    for (row, down, px, qty, want) in rows {
        let mut h = if down {
            H::from_cfg(&p0(), rd_down)
        } else {
            H::new()
        };
        let t = live(&mut h, &sell_y(), 1);
        let r = h.amend(t, p(px), q(qty), 2);
        assert_eq!(r, want, "{row}");
        if r.is_ok() {
            assert_eq!(h.state(t), OrderState::PendingAmend, "{row}");
            assert_eq!(h.decisions(OrderAction::Amend).len(), 1, "{row}");
        } else {
            assert_eq!(h.state(t), OrderState::Live, "{row}");
            assert!(h.flags(t) & SlotFlags::AMEND_SENT == 0, "{row}");
            assert!(h.out.is_empty(), "{row}");
        }
    }
    let mut h = H::from_cfg(&p0(), rd_down);
    let t = live(&mut h, &sell_y(), 1);
    h.cancel(t, 2).unwrap();
    assert_eq!(
        h.amend(t, p("60100.0"), q("0.008"), 3),
        Err(OpError::NotAllowed),
        "OMS-456.7"
    );
}
