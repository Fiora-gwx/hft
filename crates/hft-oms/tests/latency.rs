#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ConnRole, ConnStateKind, GwOp, OrderEventKind, RecordFlags, SendStatus};
use hft_oms::*;
use hft_strategy_sdk::{OrderRef, SnapshotReader, SnapshotWriter};
use hft_types::{Tif, Tsc};

fn trig(seq: u64, kind: u16) -> TrigStamps {
    TrigStamps {
        ex_ts_ns: 100,
        ex_send_ns: 110,
        hw_rx_ns: 120,
        sw_rx_ns: 125,
        rx_tsc: Tsc(1000),
        proc_tsc: Tsc(1010),
        trigger_seq: seq,
        kind,
        rx_flags: RecordFlags::HW_TS.0 as u8,
    }
}

/// place with trig, decided 1020, SendReport{Written, sent 1030 tick}.
fn placed(h: &mut H, it: &ApprovedIntent, t: TrigStamps) -> OrderRef {
    let mut it = *it;
    it.trig = t;
    let s = h.place_ok(&it, 0);
    h.o.stamp_decided(s, Tsc(1020));
    h.out.clear();
    let hd = h.hdr(0);
    let sr = hft_event::SendReport {
        decision_seq: 0,
        sent_tsc: Tsc(1030),
        slot: hft_types::SlotId(s.slot),
        r#gen: s.r#gen,
        target_slot: hft_types::SlotId(0),
        target_gen: 0,
        venue: s.venue,
        account: s.account,
        conn: hft_types::ConnId(0),
        status: SendStatus::Written,
        op: GwOp::Place,
        _pad: [0; 3],
        os_errno: 0,
        _pad2: 0,
    };
    h.o.on_send_report(&sr, &hd, &mut h.out);
    s
}

#[test]
fn oms_080_new_sample_on_first_response() {
    // Explicit receive domains survive independently for trigger and acknowledgment.
    for (trigger, ack, expected_trigger, expected_ack) in [
        (RecordFlags::SW_TS, RecordFlags::HW_TS, 2, 1),
        (RecordFlags::HW_TS, RecordFlags::SW_TS, 1, 2),
        (RecordFlags(0), RecordFlags(0), 0, 0),
    ] {
        let mut h = H::new();
        let mut t = trig(77, 0x0101);
        t.rx_flags = trigger.0 as u8;
        let s = placed(&mut h, &buy_x(), t);
        let mut e = h.oe(s, OrderEventKind::Ack);
        e.hw_rx_ns = 1990;
        e.rx_tsc = 2000;
        let mut hdr = h.hdr(0);
        hdr.flags = ack.0;
        h.o.on_order_event(&e, &hdr, &mut h.out);
        let src = h.out.latency[0].src;
        assert_eq!((src >> 4) & 3, expected_trigger);
        assert_eq!((src >> 14) & 3, expected_ack);
        assert_eq!((src >> 12) & 3, 3); // sent_tsc
    }
    let mut h = H::new();
    let s = placed(&mut h, &buy_x(), trig(77, 0x0101));
    let mut e = h.oe(s, OrderEventKind::Ack);
    e.rx_tsc = 2000;
    e.hw_rx_ns = 1990;
    e.ex_ts_ns = 1980;
    h.seq = 79;
    h.ev(&e, 0);
    assert_eq!(h.out.latency.len(), 1);
    let l = h.out.latency[0];
    assert_eq!(
        (
            l.trigger_seq,
            l.ack_seq,
            l.slot.0,
            l.r#gen,
            l.venue,
            l.account,
            l.strategy.0
        ),
        (77, 80, s.slot, s.r#gen, VS, A1, 1)
    );
    assert_eq!(
        (l.ex_ts_ns, l.ex_send_ns, l.hw_rx_ns, l.rx_tsc, l.proc_tsc),
        (100, 110, 120, Tsc(1000), Tsc(1010))
    );
    assert_eq!(
        (
            l.decided_tsc,
            l.sent_tsc,
            l.ack_hw_rx_ns,
            l.ack_rx_tsc,
            l.ack_ex_ts_ns
        ),
        (Tsc(1020), Tsc(1030), 1990, Tsc(2000), 1980)
    );
    assert_eq!(
        (l.sw_rx_ns, l.link_role, l.action, l.trigger_kind),
        (125, 0, 0, 0x0101)
    );
    assert_eq!(h.stamps(s).op, SlotStamps::OP_NONE);
    h.fill(s, q("0.004"), p("60000.00"), 1);
    assert!(h.out.latency.is_empty());
}

#[test]
fn oms_081_quote_style_cancel_sample() {
    let mut h = H::new();
    let s = placed(&mut h, &buy_x(), trig(77, 0x0101));
    h.seq = 79;
    h.ack(s, 1, 0);
    assert_eq!(
        (
            h.out.latency.len(),
            h.out.latency[0].action,
            h.out.latency[0].trigger_seq
        ),
        (1, 0, 77)
    );
    let t = TrigStamps {
        trigger_seq: 80,
        kind: 0x0203,
        rx_tsc: Tsc(2000),
        ..TrigStamps::default()
    };
    h.out.clear();
    h.o.cancel(s, &t, Tsc(2000), 80, &mut h.out).unwrap();
    h.o.stamp_decided(s, Tsc(2010));
    h.send(VS, A1, GwOp::Cancel, s.slot, s.r#gen, SendStatus::Written, 0, 0);
    h.o.test_stamps_mut(VS, A1, s.slot, |st| st.sent_tsc = 2020);
    let mut e = h.oe(s, OrderEventKind::CancelAck);
    e.rx_tsc = 3000;
    h.seq = 89;
    h.ev(&e, 3);
    let l = h.out.latency[0];
    assert_eq!(
        (
            l.action,
            l.trigger_seq,
            l.trigger_kind,
            l.decided_tsc,
            l.sent_tsc,
            l.ack_seq,
            l.ack_rx_tsc
        ),
        (1, 80, 0x0203, Tsc(2010), Tsc(2020), 90, Tsc(3000))
    );
}

#[test]
fn oms_082_amend_sample() {
    for row in ["OMS-082.1", "OMS-082.2", "OMS-082.3"] {
        let mut h = H::new();
        let t = h.place_ok(&sell_y(), 0);
        h.ack(t, 1, 0);
        assert_eq!(h.stamps(t).op, SlotStamps::OP_NONE);
        let tg = TrigStamps {
            trigger_seq: 300,
            kind: 0x0101,
            ..TrigStamps::default()
        };
        h.out.clear();
        h.o.amend(t, p("60200.0"), q("0.008"), &tg, ms(1), 1, &mut h.out)
            .unwrap();
        h.send(VX, A1, GwOp::Amend, t.slot, t.r#gen, SendStatus::Written, 0, 0);
        h.o.test_stamps_mut(VX, A1, t.slot, |st| st.sent_tsc = 4000);
        if row == "OMS-082.3" {
            h.fill(t, q("0.002"), p("60100.0"), 2);
            assert!(h.out.latency.is_empty(), "{row}");
            assert_eq!(h.stamps(t).op, SlotStamps::OP_AMEND, "{row}");
        }
        let what = if row == "OMS-082.2" {
            OrderEventKind::AmendReject
        } else {
            OrderEventKind::AmendAck
        };
        h.what(t, what, 3);
        let l = h.out.latency[0];
        assert_eq!(
            (h.out.latency.len(), l.action, l.trigger_seq, l.sent_tsc),
            (1, 2, 300, Tsc(4000)),
            "{row}"
        );
        assert_eq!(h.stamps(t).op, SlotStamps::OP_NONE, "{row}");
    }
}

#[test]
fn oms_083_pending_new_cancel_not_stamped() {
    let mut h = H::new();
    let s = placed(&mut h, &buy_x(), trig(77, 0x0101));
    let t = TrigStamps {
        trigger_seq: 99,
        ..TrigStamps::default()
    };
    h.out.clear();
    h.o.cancel(s, &t, ms(1), 1, &mut h.out).unwrap();
    h.send(VS, A1, GwOp::Cancel, s.slot, s.r#gen, SendStatus::Written, 1, 1);
    assert_eq!(
        (h.stamps(s).trigger_seq, h.stamps(s).op),
        (77, SlotStamps::OP_NEW)
    );
    h.ack(s, 1, 2);
    assert_eq!(
        (
            h.out.latency.len(),
            h.out.latency[0].action,
            h.out.latency[0].trigger_seq
        ),
        (1, 0, 77)
    );
    h.what(s, OrderEventKind::CancelAck, 3);
    assert!(h.out.latency.is_empty());
}

#[test]
fn oms_084_unsent_cancel_no_sample() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 0);
    let t = TrigStamps {
        trigger_seq: 99,
        ..TrigStamps::default()
    };
    h.out.clear();
    h.o.cancel(s, &t, ms(1), 1, &mut h.out).unwrap();
    h.send(
        VS,
        A1,
        GwOp::Cancel,
        s.slot,
        s.r#gen,
        SendStatus::RateLimited,
        1,
        1,
    );
    assert_eq!(h.stamps(s).op, SlotStamps::OP_NONE);
    h.what(s, OrderEventKind::CancelAck, 2);
    assert!(h.out.latency.is_empty());
}

#[test]
fn oms_085_bulk_cancels_unstamped() {
    for row in 1..=5 {
        let mut h = H::new();
        let s = if row == 5 {
            let mut sm = buy_x();
            sm.intent.tif = Tif::PostOnly;
            sm.intent.group = hft_types::GROUP_NONE;
            sm.origin = Origin::Command;
            sm.smoke_ttl_ms = 60_000;
            h.place_ok(&sm, 0)
        } else if row == 1 || row == 4 {
            h.place_ok(
                &intent(
                    VG,
                    A1,
                    Z,
                    hft_types::Side::Buy,
                    Tif::Gtc,
                    p("60000.0"),
                    q("0.001"),
                    g2(),
                ),
                0,
            )
        } else {
            h.place_ok(&buy_x(), 0)
        };
        h.ack(s, 1, 0);
        h.out.clear();
        match row {
            1 => {
                h.o.cancel_all(CancelScope::Account(A1), false, ms(1), 1, &mut h.out)
            }
            2 => h.o.cancel_open(
                CellKey {
                    strategy: hft_types::StrategyId(1),
                    account: A1,
                    group: g1(),
                },
                ms(1),
                1,
                &mut h.out,
            ),
            3 => h.o.cancel_resting_on(A1, X, ms(1), 1, &mut h.out),
            4 => {
                h.o.reissue_cancels(CancelScope::Account(A1), false, ms(1), 1, &mut h.out)
            }
            _ => h.o.on_smoke_ttl(A1, VS, false, ms(1), 1, &mut h.out),
        }
        assert_eq!(h.state(s), OrderState::PendingCancel, "OMS-085.{row}");
        assert_eq!(h.stamps(s).op, SlotStamps::OP_NONE, "OMS-085.{row}");
        h.what(s, OrderEventKind::CancelAck, 2);
        assert!(h.out.latency.is_empty(), "OMS-085.{row}");
    }
}

#[test]
fn oms_086_restored_slot_no_sample() {
    let mut h = H::new();
    let s = placed(&mut h, &buy_x(), trig(77, 0x0101));
    assert_eq!(h.stamps(s).op, SlotStamps::OP_NEW);
    let mut w = SnapshotWriter::new(1 << 20);
    h.o.snapshot(&mut w);
    let o = Oms::restore(&mut SnapshotReader::new(w.as_bytes()), &p0(), rd()).unwrap();
    let mut h = H {
        o,
        out: OmsOut::new(),
        seq: 500,
    };
    let st = h.stamps(s);
    assert_eq!(st.op, SlotStamps::OP_NONE);
    assert_eq!(
        (st.trigger_seq, st.trig_rx_tsc, st.decided_tsc, st.sent_tsc),
        (0, 0, 0, 0)
    );
    h.out.clear();
    h.o.recovery_end(ms(10), ms(5), HZ, HZ, 1, &mut h.out);
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 11);
    h.query(s, 1, q("0"), p("0"), 12);
    assert!(h.out.latency.is_empty());
    h.fill(s, q("0.004"), p("60000.00"), 13);
    assert!(h.out.latency.is_empty());
}

#[test]
fn oms_087_recycle_clears_stamp() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 1));
    let s = h.place_ok(&buy_x(), 0);
    h.send(VS, A1, GwOp::Place, s.slot, s.r#gen, SendStatus::WriteError, 0, 0);
    assert_eq!(h.stamps(s).op, SlotStamps::OP_NEW);
    h.query(s, 3, q("0.01"), p("60000.00"), 1);
    assert!(h.out.latency.is_empty());
    h.sweep(5002);
    assert_eq!(h.stamps(s).op, SlotStamps::OP_NONE);
    let mut it = buy_x();
    it.trig = trig(500, 0x0101);
    let s2 = h.place_ok(&it, 5003);
    assert_eq!(s2.slot, s.slot);
    let st = h.stamps(s2);
    assert_eq!(
        (st.trigger_seq, st.trig_kind, st.op, st.sent_tsc),
        (500, 0x0101, SlotStamps::OP_NEW, 0)
    );
}

#[test]
fn oms_088_trig_passthrough() {
    for (row, sw, kind) in [("OMS-088.1", 1_234_567i64, 0x0101u16), ("OMS-088.2", 0, 0x0203)] {
        let mut h = H::new();
        let mut t = trig(1, kind);
        t.sw_rx_ns = sw;
        let s = placed(&mut h, &buy_x(), t);
        h.ack(s, 1, 1);
        let l = h.out.latency[0];
        assert_eq!((l.sw_rx_ns, l.trigger_kind), (sw, kind), "{row}");
    }
    let mut h = H::new();
    let (f, _) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 1);
    let req = one_hedge(&h);
    let hs = place_hedge(&mut h, &req, 1).unwrap();
    h.ack(hs, 5, 2);
    assert_eq!(h.out.latency[0].link_role, 2, "OMS-088.3");
}
