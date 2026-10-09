#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{ConnRole, ConnStateKind, NoteLevel, OrderAction, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_refdata::InstHot;
use hft_strategy_sdk::{BboView, IntentClass, OrderRef, SnapshotReader, SnapshotWriter};
use hft_types::{GROUP_NONE, Notional, Qty, Side, StrategyId, Tsc};

const NOW: u64 = 1000;

fn req(side: Side) -> SmokeReq {
    SmokeReq {
        account: A1,
        venue: VS,
        inst: X,
        side,
        ttl_ms: 60_000,
    }
}

fn xbbo() -> BboView {
    bbo(p("60000.00"), p("60000.01"), ms(NOW))
}

fn hot() -> InstHot {
    *rd().inst(X)
}

fn snap(h: &H) -> Vec<u8> {
    let mut w = SnapshotWriter::new(1 << 20);
    h.o.snapshot(&mut w);
    w.as_bytes().to_vec()
}

fn smoke_place(h: &mut H, t: u64) -> OrderRef {
    let it =
        h.o.smoke_intent(&req(Side::Buy), &xbbo(), &hot(), 2000, ms(NOW))
            .unwrap();
    let mut a = ApprovedIntent::new(it, Origin::Command, StrategyId(1), IntentClass::Open);
    a.smoke_ttl_ms = 60_000;
    h.place_ok(&a, t)
}

#[test]
fn oms_260_smoke_buy_intent() {
    let h = H::new();
    let before = snap(&h);
    let it =
        h.o.smoke_intent(&req(Side::Buy), &xbbo(), &hot(), 2000, ms(NOW))
            .unwrap();
    assert_eq!(
        (it.venue, it.account, it.side, it.tif, it.inst, it.group),
        (VS, A1, Side::Buy, hft_types::Tif::PostOnly, X, GROUP_NONE)
    );
    assert_eq!(
        (it.px, it.qty, it.link.some, it.max_rest_ms),
        (p("58200.00"), q("0.0001"), 0, 0)
    );
    assert_eq!(snap(&h), before);
}

#[test]
fn oms_261_smoke_sell_intent() {
    let h = H::new();
    let it =
        h.o.smoke_intent(&req(Side::Sell), &xbbo(), &hot(), 2000, ms(NOW))
            .unwrap();
    assert_eq!((it.px, it.qty), (p("61800.00"), q("0.00009")));
}

#[test]
fn oms_262_smoke_qty() {
    for (row, mq, mn, want) in [
        ("OMS-262.1", "0.00001", "5", "0.0001"),
        ("OMS-262.2", "0.001", "5", "0.001"),
        ("OMS-262.3", "0.00001", "100", "0.0019"),
        ("OMS-262.4", "0.00001", "5.82", "0.00011"),
    ] {
        let h = H::new();
        let mut hx = hot();
        hx.min_qty = q(mq);
        hx.min_notional = Notional(usdt(mn));
        let it =
            h.o.smoke_intent(&req(Side::Buy), &xbbo(), &hx, 2000, ms(NOW))
                .unwrap();
        assert_eq!(it.qty, q(want), "{row}");
    }
}

#[test]
fn oms_263_smoke_distance() {
    for (row, far, ask, want) in [
        ("OMS-263.1", "0.03", "60000.00", "58200.00"),
        ("OMS-263.2", "0.03", "60000.01", "58200.00"),
        ("OMS-263.3", "0.0001", "60000.00", "59994.00"),
    ] {
        let h = H::with(|c| c.smoke_far_bp = r(far));
        let b = bbo(p("59999.00"), p(ask), ms(NOW));
        let it =
            h.o.smoke_intent(&req(Side::Buy), &b, &hot(), 2000, ms(NOW))
                .unwrap();
        assert_eq!(it.px, p(want), "{row}");
        let dist = (i128::from(p(ask).0 - it.px.0)) * 1_000_000_000 / i128::from(p(ask).0);
        assert!(dist >= i128::from(r(far).0), "{row}");
    }
}

#[test]
fn oms_264_smoke_errors() {
    const N2: u64 = 10_000;
    let base = bbo(p("60000.00"), p("60000.01"), ms(N2));
    let mut h = H::new();
    h.o.test_set_smoke_slot(VS, A1, 4);
    assert_eq!(
        h.o.smoke_intent(&req(Side::Buy), &base, &hot(), 2000, ms(N2)),
        Err(SmokeErr::Busy),
        "OMS-264.1"
    );
    let h = H::new();
    let mut ry = req(Side::Buy);
    ry.inst = Y;
    assert_eq!(
        h.o.smoke_intent(&ry, &base, rd().inst(Y), 2000, ms(N2)),
        Err(SmokeErr::BadInst),
        "OMS-264.2"
    );
    let rows: [(&str, BboView, bool); 6] = [
        (
            "OMS-264.3",
            BboView {
                conn_up: false,
                ..base
            },
            false,
        ),
        ("OMS-264.4", BboView { resync: true, ..base }, false),
        (
            "OMS-264.5",
            BboView {
                ask_px: hft_types::Px(0),
                ..base
            },
            false,
        ),
        (
            "OMS-264.6",
            BboView {
                bid_px: hft_types::Px(0),
                ..base
            },
            false,
        ),
        (
            "OMS-264.7",
            BboView {
                conn_live_tsc: ms(N2 - 2000),
                ..base
            },
            true,
        ),
        (
            "OMS-264.8",
            BboView {
                conn_live_tsc: Tsc(ms(N2).0 - ms(2001).0),
                ..base
            },
            false,
        ),
    ];
    for (row, b, ok) in rows {
        let r = h.o.smoke_intent(&req(Side::Buy), &b, &hot(), 2000, ms(N2));
        assert_eq!(r.is_ok(), ok, "{row}");
        if !ok {
            assert_eq!(r, Err(SmokeErr::NoQuote), "{row}");
        }
    }
}

#[test]
fn oms_265_smoke_place() {
    let mut h = H::new();
    let legs = (h.legs(A1, g1()).view, h.legs(A1, g2()).view);
    let k = smoke_place(&mut h, NOW);
    let sl = h.slot(k);
    assert_eq!(
        (sl.oflags, h.ext(k).origin, sl.group, sl.strategy.0, sl.link_id),
        (0x22, 4, GROUP_NONE, 1, 0)
    );
    assert_eq!(h.o.table_view(VS, A1).unwrap().smoke_slot, k.slot);
    let t = h.timers(OmsTimerKind::SmokeTtl);
    assert_eq!(
        (t.len(), t[0].id, t[0].deadline_tsc, t[0].cancel),
        (1, 513, ms(61_000), false)
    );
    assert_eq!((h.legs(A1, g1()).view, h.legs(A1, g2()).view), legs);
    assert_eq!(h.o.inflight().open_buy(A1, X), q("0.0001"));
    assert_eq!(h.o.inflight().buy_notional(A1, VS).0, usdt("5.82"));
}

#[test]
fn oms_266_smoke_no_owner_update() {
    let mut h = H::new();
    let k = smoke_place(&mut h, NOW);
    h.ack(k, 1, NOW);
    assert_eq!((transitions(&h).len(), h.out.updates.len()), (1, 0));
    h.what(k, OrderEventKind::CancelAck, NOW);
    assert_eq!((transitions(&h).len(), h.out.updates.len()), (1, 0));
}

#[test]
fn oms_267_smoke_filled() {
    let mut h = H::new();
    let k = smoke_place(&mut h, NOW);
    h.ack(k, 1, NOW);
    let legs = h.legs(A1, g1()).view;
    h.fill(k, q("0.00005"), p("58200.00"), NOW);
    let f = h.out.fills[0];
    assert_eq!((h.out.fills.len(), f.group, f.qty), (1, GROUP_NONE, q("0.00005")));
    let a = h.notes(notes::ANOMALY);
    assert!(a.iter().any(|n| n.v[0] == 3 && n.level == NoteLevel::P0));
    assert_eq!(h.legs(A1, g1()).view, legs);
    assert!(h.out.updates.is_empty());
}

#[test]
fn oms_268_smoke_ttl_by_state() {
    use OrderState::*;
    let ttl = |h: &mut H| {
        h.out.clear();
        h.o.on_smoke_ttl(A1, VS, false, ms(NOW + 60_000), 1, &mut h.out);
    };
    for (row, st) in [("OMS-268.1", Live), ("OMS-268.2", PendingNew)] {
        let mut h = H::new();
        let k = smoke_place(&mut h, NOW);
        if st == Live {
            h.ack(k, 1, NOW);
        }
        ttl(&mut h);
        assert_eq!(h.decisions(OrderAction::Cancel).len(), 1, "{row}");
        assert_eq!(transitions(&h)[0].2, 25, "{row}");
        assert_eq!(h.state(k), PendingCancel, "{row}");
    }
    let mut h = H::new();
    let k = smoke_place(&mut h, NOW);
    h.what(k, OrderEventKind::OutcomeUnknown, NOW);
    ttl(&mut h);
    assert!(h.flags(k) & SlotFlags::SMOKE_DUE != 0, "OMS-268.3");
    assert!(h.out.decisions.is_empty(), "OMS-268.3");
    h.query(k, 1, Qty(0), hft_types::Px(0), NOW + 60_001);
    assert_eq!(h.decisions(OrderAction::Cancel).len(), 1, "OMS-268.3");
    let mut h = H::new();
    let k = smoke_place(&mut h, NOW);
    h.fill(k, q("0.0001"), p("58200.00"), NOW);
    assert_eq!(h.state(k), Filled);
    h.o.test_set_smoke_slot(VS, A1, k.slot);
    ttl(&mut h);
    assert!(h.out.is_empty(), "OMS-268.4");
    let mut h = H::new();
    ttl(&mut h);
    assert!(h.out.is_empty(), "OMS-268.5");
}

#[test]
fn oms_269_smoke_terminal_clears() {
    let mut h = H::new();
    let k = smoke_place(&mut h, NOW);
    h.ack(k, 1, NOW);
    h.what(k, OrderEventKind::CancelAck, NOW + 1);
    assert_eq!(h.o.table_view(VS, A1).unwrap().smoke_slot, 0);
    let t = h.timers(OmsTimerKind::SmokeTtl);
    assert_eq!((t.len(), t[0].id, t[0].cancel), (1, 513, true));
    assert!(
        h.o.smoke_intent(&req(Side::Buy), &xbbo(), &hot(), 2000, ms(NOW))
            .is_ok()
    );
}

#[test]
fn oms_270_smoke_keep() {
    for (row, v, keep, cancel) in [
        ("OMS-270.1", VX, true, false),
        ("OMS-270.2", VS, true, true),
        ("OMS-270.3", VX, false, true),
    ] {
        let mut h = H::new();
        let k = if v == VX {
            let mut it = intent(
                VX,
                A1,
                Y,
                Side::Buy,
                hft_types::Tif::PostOnly,
                p("58200.0"),
                q("0.001"),
                GROUP_NONE,
            );
            it.origin = Origin::Command;
            it.smoke_ttl_ms = 60_000;
            h.place_ok(&it, NOW)
        } else {
            smoke_place(&mut h, NOW)
        };
        h.ack(k, 1, NOW);
        h.out.clear();
        h.o.on_smoke_ttl(A1, v, keep, ms(NOW + 60_000), 1, &mut h.out);
        assert_eq!(
            h.decisions(OrderAction::Cancel).len(),
            usize::from(cancel),
            "{row}"
        );
        if !cancel {
            assert!(h.flags(k) & SlotFlags::SMOKE_DUE == 0, "{row}");
            assert_eq!(h.o.table_view(v, A1).unwrap().smoke_slot, k.slot, "{row}");
        }
    }
}

#[test]
fn oms_271_sweep_recancels_smoke() {
    for (row, st) in [
        ("OMS-271.1", OrderState::Live),
        ("OMS-271.2", OrderState::PendingCancel),
        ("OMS-271.3", OrderState::Unknown),
    ] {
        let mut h = H::new();
        let k = smoke_place(&mut h, NOW);
        h.ack(k, 1, NOW);
        match st {
            OrderState::PendingCancel => {
                h.out.clear();
                h.o.on_smoke_ttl(A1, VS, false, ms(NOW), 1, &mut h.out);
            }
            OrderState::Unknown => h.force(k, OrderState::Unknown, true, OrderState::Live),
            _ => {}
        }
        h.o.test_slot_mut(VS, A1, k.slot, |sl, _| sl.sflags.0 |= SlotFlags::SMOKE_DUE);
        h.sweep(NOW + 1);
        let n = h.decisions(OrderAction::Cancel).len();
        assert_eq!(n, usize::from(st == OrderState::Live), "{row}");
    }
}

#[test]
fn oms_272_smoke_restart_cancel_once() {
    let mut h = H::new();
    let k = smoke_place(&mut h, NOW);
    h.ack(k, 1, NOW);
    let mut w = SnapshotWriter::new(1 << 20);
    h.o.snapshot(&mut w);
    let o = Oms::restore(&mut SnapshotReader::new(w.as_bytes()), &p0(), rd()).unwrap();
    let mut h = H {
        o,
        out: OmsOut::new(),
        seq: 900,
    };
    let mut cancels = 0;
    h.out.clear();
    h.o.recovery_end(ms(10_000), ms(2000), HZ, HZ, 1, &mut h.out);
    assert!(h.flags(k) & SlotFlags::SMOKE_DUE != 0);
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 10_001);
    h.query(k, 1, Qty(0), hft_types::Px(0), 10_002);
    cancels += h.decisions(OrderAction::Cancel).len();
    assert_eq!(cancels, 1);
    h.out.clear();
    h.o.on_smoke_ttl(A1, VS, false, ms(10_003), 1, &mut h.out);
    cancels += h.decisions(OrderAction::Cancel).len();
    h.what(k, OrderEventKind::CancelAck, 10_004);
    h.out.clear();
    h.o.on_smoke_ttl(A1, VS, false, ms(70_000), 1, &mut h.out);
    cancels += h.decisions(OrderAction::Cancel).len();
    assert!(h.out.is_empty());
    assert_eq!(cancels, 1);
}

#[test]
fn oms_273_bulk_cancels_include_smoke() {
    for row in 1..=3 {
        let mut h = H::new();
        let k = smoke_place(&mut h, NOW);
        h.ack(k, 1, NOW);
        h.out.clear();
        match row {
            1 => {
                h.o.cancel_all(CancelScope::Account(A1), true, ms(NOW), 1, &mut h.out)
            }
            2 => {
                h.o.cancel_all(CancelScope::Account(A1), false, ms(NOW), 1, &mut h.out)
            }
            _ => h.o.cancel_resting_on(A1, X, ms(NOW), 1, &mut h.out),
        }
        assert_eq!(h.state(k), OrderState::PendingCancel, "OMS-273.{row}");
    }
}

#[test]
fn oms_274_smoke_snapshot() {
    let mut h = H::new();
    for _ in 0..6 {
        let r = h.place_ok(&buy_x(), 0);
        h.ack(r, 9, 0);
    }
    let k = smoke_place(&mut h, NOW);
    assert_eq!(k.slot, 7);
    h.what(k, OrderEventKind::OutcomeUnknown, NOW);
    h.out.clear();
    h.o.on_smoke_ttl(A1, VS, false, ms(NOW), 1, &mut h.out);
    assert!(h.flags(k) & SlotFlags::SMOKE_DUE != 0);
    let bytes = snap(&h);
    let o = Oms::restore(&mut SnapshotReader::new(&bytes), &p0(), rd()).unwrap();
    assert_eq!(o.table_view(VS, A1).unwrap().smoke_slot, 7);
    assert!(o.raw_slot(VS, A1, 7).unwrap().0.sflags.0 & SlotFlags::SMOKE_DUE != 0);
}

#[test]
fn oms_275_smoke_rejected() {
    let mut h = H::new();
    let k = smoke_place(&mut h, NOW);
    let mut e = h.oe(k, OrderEventKind::Reject);
    e.reject_code = 170_193;
    h.ev(&e, NOW);
    assert_eq!(h.state(k), OrderState::Rejected);
    assert_eq!(h.o.table_view(VS, A1).unwrap().smoke_slot, 0);
    assert!(h.decisions(OrderAction::New).is_empty() && h.out.updates.is_empty());
}
