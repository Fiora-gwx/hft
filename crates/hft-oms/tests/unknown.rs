#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{ConnRole, ConnStateKind, GwOp, NoteLevel, OrderAction, OrderEventKind, SendStatus};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::{OrderRef, OrderUpdateKind};
use hft_types::{Px, Qty, RejectBits, Tif};

fn unknown_s(h: &mut H, it: &ApprovedIntent, t: u64) -> OrderRef {
    let s = h.place_ok(it, t);
    h.what(s, OrderEventKind::OutcomeUnknown, t);
    assert_eq!(h.state(s), OrderState::Unknown);
    s
}

#[test]
fn oms_032_unknown_never_renews() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    let mut all = Vec::new();
    h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, SendStatus::WriteError, 0, 0);
    all.extend(h.out.decisions.iter().copied());
    for t in [1000, 2000, 3000] {
        h.uq(s, t);
        all.extend(h.out.decisions.iter().copied());
    }
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Down, 3000);
    all.extend(h.out.decisions.iter().copied());
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 3000);
    all.extend(h.out.decisions.iter().copied());
    h.sweep(3500);
    all.extend(h.out.decisions.iter().copied());
    h.query(s, 7, Qty(0), Px(0), 6000);
    all.extend(h.out.decisions.iter().copied());
    h.recon(&recon(s, 4, 7, 1, Qty(0)), 6001);
    all.extend(h.out.decisions.iter().copied());
    assert!(all.iter().all(|d| d.action == OrderAction::Query));
    assert!(!all.is_empty());
    assert_eq!((h.state(s), h.slot(s).reject_code), (OrderState::Rejected, 0));
    let n = h.place_ok(&buy_x(), 6002);
    assert_ne!((n.slot, n.r#gen), (s.slot, s.r#gen));
}

#[test]
fn oms_066_query_status_mapping() {
    use OrderState::*;
    for (row, st, cum, want) in [
        ("OMS-066.1", 1u8, "0", Live),
        ("OMS-066.2", 2, "0.004", Live),
        ("OMS-066.3", 3, "0.01", Filled),
        ("OMS-066.4", 4, "0", Cancelled),
        ("OMS-066.5", 6, "0", Cancelled),
        ("OMS-066.6", 5, "0", Rejected),
        ("OMS-066.7", 7, "0", Unknown),
    ] {
        let mut h = H::new();
        let s = unknown_s(&mut h, &buy_x(), 0);
        let mut e = h.oe(s, OrderEventKind::QueryResult);
        e.detail = st;
        e.cum_filled = q(cum);
        e.avg_px = p("60000.00");
        e.reject_code = 2010;
        h.ev(&e, 1);
        assert_eq!(h.state(s), want, "{row}");
        if st == 2 {
            assert_eq!(h.out.fills[0].qty, q("0.004"), "{row}");
        }
        if st == 5 {
            assert_eq!(h.slot(s).reject_code, 2010, "{row}");
        }
    }
}

#[test]
fn oms_089_grace_from_cancel_sent() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, SendStatus::Written, 0, 0);
    h.ack(s, 1, 1);
    h.cancel(s, 4000).unwrap();
    h.send(
        VS,
        A1,
        GwOp::Cancel,
        s.slot,
        s.r#gen,
        SendStatus::Written,
        4000,
        4000,
    );
    assert_eq!(h.stamps(s).sent_tsc, ms(4000).0);
    h.fill(s, q("0.004"), p("60000.00"), 4050);
    let mut e = h.oe(s, OrderEventKind::CancelReject);
    e.detail = 1;
    e.cum_filled = q("0.004");
    h.ev(&e, 4100);
    assert_eq!(h.state(s), OrderState::Unknown);
    h.query(s, 7, q("0.004"), p("60000.00"), 9000);
    assert!(h.flags(s) & SlotFlags::HIST_WAIT == 0);
    assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 0);
    h.query(s, 7, q("0.004"), p("60000.00"), 9501);
    assert!(h.flags(s) & SlotFlags::HIST_WAIT != 0);
    assert_eq!(
        h.notes(notes::NEEDS_REST_QUERY)
            .iter()
            .filter(|n| n.v[0] >> 16 & 1 == 1)
            .count(),
        1
    );
}

/// a1@VX: p PendingNew, c PendingCancel (acked), m PendingAmend (acked), l Live (acked), f Filled.
fn vx_table(h: &mut H) -> [OrderRef; 5] {
    let p_ = h.place_ok(&sell_y(), 0);
    let c = h.place_ok(&sell_y(), 0);
    h.ack(c, 1, 0);
    h.cancel(c, 0).unwrap();
    let m = h.place_ok(&sell_y(), 0);
    h.ack(m, 2, 0);
    h.amend(m, p("60100.0"), q("0.01"), 0).unwrap();
    let l = h.place_ok(&sell_y(), 0);
    h.ack(l, 3, 0);
    let f = h.place_ok(&sell_y(), 0);
    h.fill(f, q("0.01"), p("60100.0"), 0);
    [p_, c, m, l, f]
}

#[test]
fn oms_130_order_conn_down() {
    let mut h = H::new();
    let [p_, c, m, l, f] = vx_table(&mut h);
    let leg = h.legs(A1, g1()).view.leg[1];
    assert_eq!(
        (leg.inflight_immediate, leg.inflight_resting),
        (Qty(-1_000_000), Qty(-3_000_000))
    );
    h.conn(VX, A1, ConnRole::Order, ConnStateKind::Down, 100);
    for (r, prev) in [
        (p_, OrderState::PendingNew),
        (c, OrderState::PendingCancel),
        (m, OrderState::PendingAmend),
    ] {
        assert_eq!((h.state(r), h.slot(r).prev_state), (OrderState::Unknown, prev));
    }
    assert!(transitions(&h).iter().all(|t| t.2 == 15));
    assert_eq!(
        h.notes(notes::UNKNOWN)
            .iter()
            .filter(|n| n.level == NoteLevel::P1)
            .count(),
        3
    );
    assert_eq!(
        h.out
            .updates
            .iter()
            .filter(|u| u.kind == OrderUpdateKind::Unknown)
            .count(),
        3
    );
    assert_eq!(h.state(l), OrderState::Live);
    assert!(h.flags(l) & SlotFlags::VERIFY != 0);
    assert_eq!(h.state(f), OrderState::Filled);
    let tv = h.o.table_view(VX, A1).unwrap();
    assert_eq!((tv.unknown_count, tv.conn_up), (3, false));
    assert!(!h.o.venue_ready(VX, A1));
    assert!(h.legs(A1, g1()).view.has_unknown);
    assert!(h.decisions(OrderAction::Query).is_empty());
    let t = h.timers(OmsTimerKind::UnknownQuery);
    assert_eq!(t.len(), 4);
    assert!(t.iter().all(|t| t.deadline_tsc == ms(1100)));
    let leg = h.legs(A1, g1()).view.leg[1];
    assert_eq!(
        (leg.inflight_immediate, leg.inflight_resting),
        (Qty(-3_000_000), Qty(-1_000_000))
    );
}

#[test]
fn oms_131_user_stream_down() {
    let mut h = H::new();
    let [p_, c, m, l, f] = vx_table(&mut h);
    h.conn(VX, A1, ConnRole::UserStream, ConnStateKind::Down, 100);
    for r in [p_, c, m, l] {
        assert!(h.flags(r) & SlotFlags::VERIFY != 0);
    }
    assert_eq!(h.state(p_), OrderState::PendingNew);
    assert_eq!(h.o.table_view(VX, A1).unwrap().unknown_count, 0);
    assert_eq!(h.decisions(OrderAction::Query).len(), 4);
    assert!(h.flags(f) & SlotFlags::VERIFY == 0);
}

#[test]
fn oms_133_force_unknown() {
    let mut h = H::new();
    let mut live = Vec::new();
    for i in 0..2 {
        let r = h.place_ok(&buy_x(), 0);
        h.ack(r, i + 1, 0);
        live.push(r);
    }
    let pn = h.place_ok(&buy_x(), 0);
    let f = h.place_ok(&buy_x(), 0);
    h.fill(f, q("0.01"), p("60000.00"), 0);
    let u = unknown_s(&mut h, &buy_x(), 0);
    h.out.clear();
    let n = h.o.force_unknown(VS, A1, ms(1), 1, &mut h.out);
    assert_eq!(n, 3);
    for r in [live[0], live[1], pn, u] {
        assert_eq!(h.state(r), OrderState::Unknown);
    }
    assert_eq!(transitions(&h).iter().filter(|t| t.2 == 21).count(), 3);
    assert_eq!(h.state(f), OrderState::Filled);
    assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 4);
    assert_eq!(h.decisions(OrderAction::Query).len(), 3);
}

#[test]
fn oms_135_query_on_enter_and_timer() {
    let mut h = H::new();
    let s = unknown_s(&mut h, &buy_x(), 0);
    let qd = h.decisions(OrderAction::Query);
    assert_eq!((qd.len(), qd[0].target_slot.0), (1, s.slot));
    let t = h.timers(OmsTimerKind::UnknownQuery);
    assert_eq!((t.len(), t[0].id, t[0].deadline_tsc), (1, s.slot << 8, ms(1000)));
    h.uq(s, 1000);
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
    assert_eq!(h.timers(OmsTimerKind::UnknownQuery)[0].deadline_tsc, ms(2000));
    h.query(s, 1, Qty(0), Px(0), 1500);
    h.uq(s, 2000);
    assert!(h.out.is_empty());
}

#[test]
fn oms_136_no_query_while_down() {
    let mut h = H::new();
    let rs = vx_table(&mut h);
    h.conn(VX, A1, ConnRole::Order, ConnStateKind::Down, 100);
    for r in &rs[..4] {
        h.uq(*r, 1100);
        assert!(h.out.decisions.is_empty());
        assert_eq!(h.timers(OmsTimerKind::UnknownQuery)[0].deadline_tsc, ms(2100));
    }
    h.conn(VX, A1, ConnRole::Order, ConnStateKind::Up, 1500);
    assert_eq!(h.decisions(OrderAction::Query).len(), 4);
    assert!(h.o.table_view(VX, A1).unwrap().conn_up);
    assert!(!h.o.venue_ready(VX, A1));
}

#[test]
fn oms_137_query_interval() {
    for (row, iv, d1) in [
        ("OMS-137.1", 1000u32, 1_000_000_000u64),
        ("OMS-137.2", 1, 1_000_000),
        ("OMS-137.3", 250, 250_000_000),
    ] {
        let mut h = H::with(|c| c.unknown_query_interval_ms = iv);
        let s = unknown_s(&mut h, &buy_x(), 0);
        assert_eq!(
            h.timers(OmsTimerKind::UnknownQuery)[0].deadline_tsc.0,
            d1,
            "{row}"
        );
        if iv == 250 {
            h.uq(s, 250);
            assert_eq!(
                h.timers(OmsTimerKind::UnknownQuery)[0].deadline_tsc.0,
                500_000_000,
                "{row}"
            );
        }
    }
}

#[test]
fn oms_138_unknown_stuck() {
    for (row, max, writes, last_rl, stuck) in [
        ("OMS-138.1", 5u8, 5u32, false, 0usize),
        ("OMS-138.2", 5, 6, false, 1),
        ("OMS-138.3", 5, 7, false, 1),
        ("OMS-138.4", 0, 1, false, 1),
        ("OMS-138.5", 5, 6, true, 0),
    ] {
        let mut h = H::with(|c| c.unknown_max_queries = max);
        let s = unknown_s(&mut h, &buy_x(), 0);
        let mut n = 0;
        for i in 0..writes {
            let st = if last_rl && i == writes - 1 {
                SendStatus::RateLimited
            } else {
                SendStatus::Written
            };
            h.send(VS, A1, GwOp::Query, s.slot, s.r#gen, st, 1, 1);
            n += h
                .notes(notes::UNKNOWN_STUCK)
                .iter()
                .filter(|x| x.level == NoteLevel::P0)
                .count();
        }
        assert_eq!(n, stuck, "{row}");
        if row == "OMS-138.1" || row == "OMS-138.5" {
            assert_eq!(h.slot(s).query_count, 5, "{row}");
        }
    }
}

#[test]
fn oms_138_stuck_query_stops_on_timer_and_reconnect_until_reconciled() {
    for unsupported in [false, true] {
        let mut h = H::with(|c| c.unknown_max_queries = 1);
        let s = unknown_s(&mut h, &buy_x(), 0);
        for _ in 0..2 {
            h.send(
                VS,
                A1,
                GwOp::Query,
                s.slot,
                s.r#gen,
                if unsupported {
                    SendStatus::QueryUnsupported
                } else {
                    SendStatus::Written
                },
                1,
                1,
            );
        }
        assert_ne!(h.flags(s) & SlotFlags::REST_QUERY, 0);
        for t in 2..12 {
            h.uq(s, t * 1000);
            assert!(h.decisions(OrderAction::Query).is_empty());
            assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 1);
            assert_eq!(h.state(s), OrderState::Unknown);
        }
        h.conn(VS, A1, ConnRole::Order, ConnStateKind::Down, 13000);
        h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 14000);
        assert!(h.decisions(OrderAction::Query).is_empty());
        assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 1);
        h.recon(&recon(s, 4, 4, 1, Qty(0)), 15000);
        assert_eq!(h.state(s), OrderState::Cancelled);
        assert_eq!(h.flags(s) & SlotFlags::REST_QUERY, 0);
        h.uq(s, 16000);
        assert!(h.decisions(OrderAction::Query).is_empty());
        assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 0);
    }
}

#[test]
fn oms_139_notfound_within_grace() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.what(s, OrderEventKind::OutcomeUnknown, 100);
    h.query(s, 7, Qty(0), Px(0), 1000);
    assert_eq!(h.state(s), OrderState::Unknown);
    assert!(h.flags(s) & SlotFlags::HIST_WAIT == 0);
    assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 0);
    h.uq(s, 1100);
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
}

#[test]
fn oms_140_grace_boundary() {
    for (row, on_vg, now, hist) in [
        ("OMS-140.1", false, 5499u64, false),
        ("OMS-140.2", false, 5500, false),
        ("OMS-140.3", false, 5501, true),
        ("OMS-140.4", true, 2001, true),
        ("OMS-140.5", true, 20_000, true),
    ] {
        let mut h = H::new();
        let it = if on_vg {
            intent(
                VG,
                A1,
                Z,
                hft_types::Side::Buy,
                Tif::Gtc,
                p("60000.0"),
                q("0.001"),
                g2(),
            )
        } else {
            buy_x()
        };
        let s = unknown_s(&mut h, &it, 0);
        h.query(s, 7, Qty(0), Px(0), now);
        assert_eq!(h.state(s), OrderState::Unknown, "{row}");
        assert_eq!(h.flags(s) & SlotFlags::HIST_WAIT != 0, hist, "{row}");
        let nr = h.notes(notes::NEEDS_REST_QUERY);
        if hist {
            let v = if on_vg { 10u64 } else { 2 };
            assert_eq!((nr.len(), nr[0].v[0]), (1, v | 1 << 8 | 1 << 16), "{row}");
        } else {
            assert!(nr.is_empty(), "{row}");
        }
    }
}

fn hist_wait(h: &mut H) -> OrderRef {
    let s = unknown_s(h, &buy_x(), 0);
    h.query(s, 7, Qty(0), Px(0), 6000);
    assert!(h.flags(s) & SlotFlags::HIST_WAIT != 0);
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.query_count = 2);
    s
}

#[test]
fn oms_141_hist_wait_rerequest() {
    let mut h = H::new();
    let s = hist_wait(&mut h);
    for i in 0..20u64 {
        h.uq(s, 7000 + i * 1000);
        let nr = h.notes(notes::NEEDS_REST_QUERY);
        assert_eq!((nr.len(), nr[0].v[0] >> 16 & 1), (1, 1));
        assert!(h.out.decisions.is_empty());
        assert_eq!(h.n_notes(notes::UNKNOWN_STUCK), 0);
    }
    assert_eq!(h.slot(s).query_count, 2);
}

#[test]
fn oms_142_history_unchecked_recorded() {
    let mut h = H::new();
    let s = hist_wait(&mut h);
    h.recon(&recon(s, 4, 7, 0, Qty(0)), 7000);
    assert_eq!(h.state(s), OrderState::Unknown);
    assert!(h.flags(s) & SlotFlags::HIST_WAIT != 0);
    assert!(h.out.notes.is_empty() && h.out.updates.is_empty());
    assert_eq!(h.slot(s).query_count, 2);
}

#[test]
fn oms_143_history_confirmed_not_found() {
    let mut h = H::new();
    let s = hist_wait(&mut h);
    h.recon(&recon(s, 4, 7, 1, Qty(0)), 7000);
    assert_eq!((h.state(s), h.slot(s).reject_code), (OrderState::Rejected, 0));
    assert_eq!(transitions(&h), vec![(8, 7, 13)]);
    assert_eq!(
        h.out.updates[0].kind,
        OrderUpdateKind::Rejected {
            bits: RejectBits(0),
            venue_code: 0
        }
    );
    assert_eq!(h.o.table_view(VS, A1).unwrap().unknown_count, 0);
    assert!(h.flags(s) & SlotFlags::HIST_WAIT == 0);
    let ib = h.o.inflight().pos(A1, X);
    assert_eq!((ib.open_buy, ib.imm_buy), (Qty(0), Qty(0)));
    assert!(h.o.venue_ready(VS, A1));
    assert_eq!(h.n_notes(notes::VENUE_READY), 1);
}

#[test]
fn oms_144_history_confirmed_with_fills() {
    for (row, st, qty, want) in [
        ("OMS-144.1", 3u8, "0.01", OrderState::Filled),
        ("OMS-144.2", 4, "0.004", OrderState::Cancelled),
    ] {
        let mut h = H::new();
        let s = hist_wait(&mut h);
        h.recon(&recon(s, 4, st, 1, q(qty)), 7000);
        let f = h.out.fills[0];
        assert_eq!((f.qty, f.px), (q(qty), p("60000.00")), "{row}");
        assert!(f.flags & fill_flags::PX_EST != 0, "{row}");
        assert_eq!(h.state(s), want, "{row}");
        if st == 3 {
            assert_eq!(h.n_notes(notes::FILL_PX_ESTIMATED), 1, "{row}");
            assert_eq!(
                h.notes(notes::NEEDS_REST_QUERY)
                    .iter()
                    .filter(|n| n.v[0] >> 16 & 1 == 1)
                    .count(),
                1,
                "{row}"
            );
        }
    }
}

#[test]
fn oms_145_recon_entry_ignored() {
    let gen2 = || {
        let mut h = H::with(|c| c.slots.set(VS, A1, 1));
        let s = h.place_ok(&buy_x(), 0);
        h.what(s, OrderEventKind::CancelAck, 0);
        h.sweep(5001);
        let s = h.place_ok(&buy_x(), 5002);
        assert_eq!(s.r#gen, 2);
        (h, s)
    };
    let anomaly4 = |h: &H| {
        h.notes(notes::ANOMALY)
            .iter()
            .filter(|n| n.v[0] == 4 && n.level == NoteLevel::P1)
            .count()
    };
    let (mut h, s) = gen2();
    h.what(s, OrderEventKind::OutcomeUnknown, 5003);
    let mut e = recon(s, 4, 3, 1, q("0.01"));
    e.r#gen = 1;
    h.recon(&e, 5004);
    assert_eq!((h.state(s), anomaly4(&h)), (OrderState::Unknown, 1), "OMS-145.1");
    let mut e = recon(s, 4, 3, 1, q("0.01"));
    e.epoch = 6;
    h.recon(&e, 5005);
    assert_eq!((h.state(s), anomaly4(&h)), (OrderState::Unknown, 1), "OMS-145.2");
    let (mut h, s) = gen2();
    h.ack(s, 1, 5003);
    h.recon(&recon(s, 3, 0, 0, Qty(0)), 5004);
    assert_eq!(anomaly4(&h), 1, "OMS-145.3");
    h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.sflags.0 |= SlotFlags::VERIFY);
    h.recon(&recon(s, 3, 0, 0, Qty(0)), 5005);
    assert_eq!(anomaly4(&h), 0, "OMS-145.4");
    assert!(h.flags(s) & SlotFlags::VERIFY == 0, "OMS-145.4");
    let (mut h, s) = gen2();
    h.what(s, OrderEventKind::OutcomeUnknown, 5003);
    h.recon(&recon(s, 3, 0, 0, Qty(0)), 5004);
    assert_eq!(h.state(s), OrderState::Live, "OMS-145.5");
    assert_eq!(h.out.updates[0].kind, OrderUpdateKind::Acked, "OMS-145.5");
}

/// L1 first leg filled 0.012 at t=0 and its second leg placed on VX; returns (h, second, link).
fn second_leg(h: &mut H) -> (OrderRef, u32) {
    let (f, l) = place_first(h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    let req = one_hedge(h);
    let hs = place_hedge(h, &req, 0).unwrap();
    (hs, l)
}

#[test]
fn oms_146_expire_at_linked_needs_history() {
    let mut h = H::new();
    let (hs, _) = second_leg(&mut h);
    h.what(hs, OrderEventKind::OutcomeUnknown, 0);
    h.query(hs, 7, Qty(0), Px(0), 3501);
    let mut nr = h.notes(notes::NEEDS_REST_QUERY).len();
    h.query(hs, 7, Qty(0), Px(0), 10_000);
    nr += h.notes(notes::NEEDS_REST_QUERY).len();
    assert_eq!(h.state(hs), OrderState::Unknown);
    assert!(h.flags(hs) & SlotFlags::HIST_WAIT != 0);
    assert!(nr >= 1);
    assert!(!transitions(&h).iter().any(|t| t.2 == 22));
}

fn vx_unknown(h: &mut H) -> OrderRef {
    unknown_s(h, &sell_y(), 0)
}

#[test]
fn oms_147_expire_at_terminal() {
    for (row, now, rej) in [
        ("OMS-147.1", 3499u64, false),
        ("OMS-147.2", 3500, false),
        ("OMS-147.3", 3501, true),
    ] {
        let mut h = H::new();
        let t = vx_unknown(&mut h);
        h.query(t, 7, Qty(0), Px(0), now);
        if rej {
            assert_eq!(
                (h.state(t), h.slot(t).reject_code),
                (OrderState::Rejected, 0),
                "{row}"
            );
            assert_eq!(transitions(&h)[0].2, 22, "{row}");
            assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 0, "{row}");
        } else {
            assert_eq!(h.state(t), OrderState::Unknown, "{row}");
        }
    }
}

#[test]
fn oms_148_fill_after_not_found() {
    let mut h = H::new();
    let t = vx_unknown(&mut h);
    h.query(t, 7, Qty(0), Px(0), 3501);
    h.fill(t, q("0.01"), p("60100.0"), 3600);
    assert_eq!(h.state(t), OrderState::Filled, "OMS-148.1");
    assert_eq!(h.out.fills[0].qty, q("0.01"));
    assert_eq!(h.notes(notes::ANOMALY).iter().filter(|n| n.v[0] == 1).count(), 1);
    assert_eq!(h.n_notes(notes::NEEDS_REST_QUERY), 1);
    let mut h = H::new();
    let t = vx_unknown(&mut h);
    h.query(t, 7, Qty(0), Px(0), 3501);
    let mut e = h.oe(t, OrderEventKind::PartialFill);
    e.cum_filled = q("0.004");
    e.avg_px = p("60100.0");
    h.sweep(8502);
    assert_eq!(h.state(t), OrderState::Free);
    h.ev(&e, 8600);
    assert!(h.out.fills[0].flags & fill_flags::LATE != 0, "OMS-148.2");
    assert_eq!(h.notes(notes::LATE_FILL)[0].level, NoteLevel::P0);
}

#[test]
fn oms_152_venue_ready_note() {
    let mut h = H::new();
    let s = unknown_s(&mut h, &buy_x(), 0);
    h.query(s, 1, Qty(0), Px(0), 1);
    let n = h.notes(notes::VENUE_READY);
    assert_eq!((n.len(), n[0].v[0]), (1, 258), "OMS-152.1");
    let mut h = H::new();
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Down, 1);
    assert!(!h.o.venue_ready(VS, A1));
    assert_eq!(h.n_notes(notes::VENUE_READY), 0, "OMS-152.2");
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 2);
    assert_eq!(h.n_notes(notes::VENUE_READY), 1, "OMS-152.2");
    let mut h = H::new();
    let s = unknown_s(&mut h, &buy_x(), 0);
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Down, 1);
    h.query(s, 1, Qty(0), Px(0), 2);
    assert_eq!(h.n_notes(notes::VENUE_READY), 0, "OMS-152.3");
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 3);
    assert_eq!(h.n_notes(notes::VENUE_READY), 1, "OMS-152.3");
    let mut h = H::new();
    let s = unknown_s(&mut h, &buy_x(), 0);
    let _s2 = unknown_s(&mut h, &buy_x(), 0);
    h.query(s, 1, Qty(0), Px(0), 2);
    assert!(!h.o.venue_ready(VS, A1));
    assert_eq!(h.n_notes(notes::VENUE_READY), 0, "OMS-152.4");
}

#[test]
fn oms_153_linked_notfound_deadline() {
    let mut h = H::with(|c| c.not_found_grace_ms.set(VX, Some(1000)));
    let (hs, l) = second_leg(&mut h);
    h.send(
        VX,
        A1,
        GwOp::Place,
        hs.slot,
        hs.r#gen,
        SendStatus::Written,
        10,
        10,
    );
    h.what(hs, OrderEventKind::OutcomeUnknown, 50);
    h.query(hs, 7, Qty(0), Px(0), 2000);
    assert!(h.flags(hs) & SlotFlags::HIST_WAIT != 0);
    h.uq(hs, 3000);
    assert_eq!(h.link(l).state, LinkState::Hedging);
    h.uq(hs, 3001);
    assert_eq!(h.link(l).state, LinkState::Broken);
    let lu = h.out.links.iter().find(|u| u.state == LinkState::Broken).unwrap();
    assert_eq!(lu.broken_reason, LinkBroken::Deadline);
    assert_eq!(h.notes(notes::HEDGE_BROKEN)[0].level, NoteLevel::P0);
    h.recon(&recon(hs, 4, 3, 1, q("0.012")), 8000);
    assert_eq!(h.state(hs), OrderState::Filled);
    assert_eq!(h.legs(A1, g1()).view.leg[1].net, q("-0.012"));
    assert_eq!(
        (h.link(l).hedge_filled, h.link(l).state),
        (q("0.012"), LinkState::Broken)
    );
    assert!(h.out.hedges.is_empty() && h.decisions(OrderAction::New).is_empty());
}

#[test]
fn oms_154_query_vs_late_fill() {
    let mut h = H::new();
    let s = unknown_s(&mut h, &buy_x(), 0);
    h.query(s, 2, q("0.004"), p("60000.00"), 1);
    let mut fills: Vec<Qty> = h.out.fills.iter().map(|f| f.qty).collect();
    h.fill(s, q("0.01"), p("60000.00"), 2);
    fills.extend(h.out.fills.iter().map(|f| f.qty));
    assert_eq!(fills, vec![q("0.004"), q("0.006")], "OMS-154.1");
    assert_eq!((h.state(s), h.slot(s).filled), (OrderState::Filled, q("0.01")));
    let mut h = H::new();
    let s = unknown_s(&mut h, &buy_x(), 0);
    h.fill(s, q("0.01"), p("60000.00"), 1);
    assert_eq!(h.out.fills.len(), 1, "OMS-154.2");
    h.query(s, 2, q("0.004"), p("60000.00"), 2);
    assert!(h.out.fills.is_empty(), "OMS-154.2");
    assert_eq!((h.state(s), h.slot(s).filled), (OrderState::Filled, q("0.01")));
}
