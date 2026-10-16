//! SLC-010–SLC-025: the simplified sequencer.

mod support;

use hft_event::{
    ConnState, EventKind, GwOp, LatencySample, OrderAction, OrderDecision, OrderEvent, OrderEventKind,
    RecordFlags, SendStatus,
};
use hft_types::{AccountId, InstId, Px, Qty, Side, SlotId, Tif};
use latency_slice::seq::Halt;
use support::{CFG0, ETH, H, MS, cfg_with, order_event};
use zerocopy::{FromBytes, TryFromBytes};

fn seq0() -> H {
    H::new(cfg_with(""), true)
}

fn decision(r: &support::Rec) -> OrderDecision {
    OrderDecision::try_read_from_prefix(&r.body).unwrap().0
}

fn sample(r: &support::Rec) -> LatencySample {
    LatencySample::read_from_prefix(&r.body).unwrap().0
}

fn of_kind(h: &mut H, k: EventKind) -> Vec<support::Rec> {
    h.input_records().into_iter().filter(|r| r.kind() == k).collect()
}

#[test]
fn slc_010_bbo_places_postonly() {
    let mut h = seq0();
    h.at(1000 * MS, 1_000);
    h.b(1000, 6_500_000, 6_500_010);
    let c = h.cmds();
    assert_eq!(c.len(), 1);
    let c = c[0];
    assert_eq!(
        (
            c.op, c.inst, c.side, c.tif, c.px, c.qty, c.slot, c.r#gen, c.account
        ),
        (
            GwOp::Place,
            InstId(1),
            Side::Buy,
            Tif::PostOnly,
            Px(6_305_009),
            Qty(9_000),
            SlotId(1),
            1,
            AccountId(1)
        )
    );
    assert_ne!(c.decided_tsc.0, 0);
    let recs = h.input_records();
    let bi = recs.iter().position(|r| r.kind() == EventKind::BBO).unwrap();
    let d = &recs[bi + 1];
    assert_eq!(d.kind(), EventKind::ORDER_DECISION);
    let od = decision(d);
    assert_eq!(
        (
            od.action,
            od.trigger_seq,
            od.slot,
            od.r#gen,
            od.px,
            od.qty,
            od.decided_tsc
        ),
        (
            OrderAction::New,
            recs[bi].hdr.seq,
            SlotId(1),
            1,
            Px(6_305_009),
            Qty(9_000),
            c.decided_tsc
        )
    );
    assert!(d.end());
    assert!(!recs[bi].end());
}

#[test]
fn slc_011_probe_period() {
    let rows: [(&str, u64, bool, usize); 4] = [
        ("SLC-011.1", 1_999_999_999, true, 0),
        ("SLC-011.2", 2_000_000_000, true, 1),
        ("SLC-011.3", 2_000_000_001, true, 1),
        ("SLC-011.4", 2_500_000_000, false, 0),
    ];
    for (row, t, done, want) in rows {
        let mut h = seq0();
        if done {
            h.cycle(1000);
        } else {
            h.place_ack(1000);
        }
        h.cmds();
        h.bbo_at(
            1,
            t,
            t,
            6_500_000,
            6_500_010,
            t as i64 - 5_000,
            t as i64 - 3_000,
            0,
        );
        let n = h.cmds().iter().filter(|c| c.op == GwOp::Place).count();
        assert_eq!(n, want, "{row}");
    }
}

#[test]
fn slc_012_burst_quota() {
    let mut h = H::new(cfg_with("[probe]\nburst_period_ms = 200\n"), true);
    h.cycle(1000);
    h.cmds();
    let px = |h: &mut H, t: u64, rx: u64| {
        h.bbo_at(
            1,
            t,
            rx,
            6_500_000,
            6_500_010,
            rx as i64 - 5_000,
            rx as i64 - 3_000,
            0,
        )
    };
    px(&mut h, 1300 * MS, 1300 * MS);
    px(&mut h, 1301 * MS, 1301 * MS);
    assert!(h.cmds().is_empty(), "step 1");
    px(&mut h, 1400 * MS, 1400 * MS);
    assert!(h.cmds().is_empty(), "step 2 first record");
    px(&mut h, 1400 * MS + 10, 1400 * MS);
    let c = h.cmds();
    assert_eq!(c.len(), 1, "step 2");
    let p = c[0];
    h.send(&p, SendStatus::Written, 1400 * MS + 20_000);
    h.ev(
        OrderEventKind::Ack,
        p.slot.0,
        p.r#gen,
        1401 * MS,
        1401 * MS as i64,
        0,
    );
    let cx = h.cmds().pop().unwrap();
    h.send(&cx, SendStatus::Written, 1401 * MS + 10_000);
    h.ev(
        OrderEventKind::CancelAck,
        p.slot.0,
        p.r#gen,
        1402 * MS,
        1402 * MS as i64,
        0,
    );
    px(&mut h, 1500 * MS, 1500 * MS);
    px(&mut h, 1500 * MS + 10, 1500 * MS);
    assert!(h.cmds().is_empty(), "step 3");
}

#[test]
fn slc_013_no_probe_conditions() {
    let one = |extra: &str, up: bool, bid: i64, ask: i64| -> (usize, u64) {
        let mut h = H::new(cfg_with(extra), up);
        h.b(1000, bid, ask);
        let n = h.cmds().iter().filter(|c| c.op == GwOp::Place).count();
        (n, h.s().stats().skipped_spread)
    };
    assert_eq!(one("", true, 6_500_000, 6_566_001), (0, 1), "SLC-013.1");
    assert_eq!(one("", true, 6_500_000, 6_565_000).0, 1, "SLC-013.2");
    assert_eq!(one("", true, 0, 6_500_010).0, 0, "SLC-013.3");
    assert_eq!(one("", false, 6_500_000, 6_500_010).0, 0, "SLC-013.4");
    assert_eq!(
        one("[orders]\nenabled = false\n", true, 6_500_000, 6_500_010).0,
        0,
        "SLC-013.5"
    );
    // .6: max_open_orders = 1 and ETHUSDT (inst 2) has an open probe.
    let mut h = H::new(config_two("[orders]\nmax_open_orders = 1\n"), true);
    h.bbo_at(2, 900 * MS, 900 * MS, 300_000, 300_010, 0, 0, 0);
    assert_eq!(h.cmds().len(), 1);
    h.b(1000, 6_500_000, 6_500_010);
    assert!(h.cmds().is_empty(), "SLC-013.6");
    // .7: max_orders = 1, one order already placed and done.
    let mut h = H::new(cfg_with("[orders]\nmax_orders = 1\n"), true);
    h.cycle(1000);
    h.cmds();
    h.b(3000, 6_500_000, 6_500_010);
    assert!(h.cmds().is_empty(), "SLC-013.7");
    // .8: inst 0.
    let mut h = seq0();
    h.bbo_at(0, 1000 * MS, 1000 * MS, 6_500_000, 6_500_010, 0, 0, 0);
    assert!(h.cmds().is_empty(), "SLC-013.8");
}

fn config_two(extra: &str) -> latency_slice::config::Config {
    latency_slice::config::parse(&format!("{CFG0}\n{ETH}\n{extra}")).unwrap()
}

#[test]
fn slc_014_ack_cancels_same_event() {
    let mut h = seq0();
    h.b(1000, 6_500_000, 6_500_010);
    let p = h.cmds().pop().unwrap();
    h.send(&p, SendStatus::Written, 1_000_020_000);
    h.ev(OrderEventKind::Ack, 1, 1, 1_001_500_000, 1_001_496_000, 0);
    let c = h.cmds();
    assert_eq!(c.len(), 1);
    assert_eq!(
        (c[0].op, c[0].target_slot, c[0].inst),
        (GwOp::Cancel, SlotId(1), InstId(1))
    );
    let recs = h.input_records();
    let bbo_seq = recs.iter().find(|r| r.kind() == EventKind::BBO).unwrap().hdr.seq;
    let k = recs
        .iter()
        .rposition(|r| r.kind() == EventKind::ORDER_EVENT)
        .unwrap();
    let (e, s, d) = (&recs[k], &recs[k + 1], &recs[k + 2]);
    assert_eq!(
        (e.kind(), s.kind(), d.kind()),
        (
            EventKind::ORDER_EVENT,
            EventKind::LATENCY_SAMPLE,
            EventKind::ORDER_DECISION
        )
    );
    assert!(!e.end() && !s.end() && d.end());
    let ls = sample(s);
    assert_eq!((ls.src >> 4) & 3, 1);
    assert_eq!((ls.src >> 14) & 3, 1); // fixture flags, despite ts_expect_hw = false
    assert_ne!(e.hdr.flags & RecordFlags::HW_TS.0, 0);

    assert_eq!(
        (
            ls.action,
            ls.trigger_seq,
            ls.trigger_kind,
            ls.ack_seq,
            ls.link_role
        ),
        (0, bbo_seq, EventKind::BBO.0, e.hdr.seq, 0)
    );
    assert_eq!(
        (ls.hw_rx_ns, ls.rx_tsc.0, ls.sent_tsc.0, ls.ack_hw_rx_ns),
        (1_000_000_000 - 5_000, 1_000_000_000, 1_000_020_000, 1_001_496_000)
    );
    let od = decision(d);
    assert_eq!((od.action, od.target_slot), (OrderAction::Cancel, SlotId(1)));
}

#[test]
fn slc_015_cancel_sample() {
    let mut h = seq0();
    h.b(1000, 6_500_000, 6_500_010);
    let p = h.cmds().pop().unwrap();
    h.send(&p, SendStatus::Written, 1_000_020_000);
    h.ev(OrderEventKind::Ack, 1, 1, 1_001_500_000, 1_001_496_000, 0);
    let c = h.cmds().pop().unwrap();
    h.send(&c, SendStatus::Written, 1_001_510_000);
    h.ev(OrderEventKind::CancelAck, 1, 1, 1_003_000_000, 1_003_000_000, 0);
    let recs = h.input_records();
    let ack = recs
        .iter()
        .find(|r| {
            r.kind() == EventKind::ORDER_EVENT
                && OrderEvent::try_read_from_prefix(&r.body).unwrap().0.what == OrderEventKind::Ack
        })
        .unwrap()
        .clone();
    let samples: Vec<LatencySample> = recs
        .iter()
        .filter(|r| r.kind() == EventKind::LATENCY_SAMPLE)
        .map(sample)
        .collect();
    assert_eq!(samples.len(), 2);
    let s = samples[1];
    assert_eq!(
        (
            s.action,
            s.trigger_seq,
            s.trigger_kind,
            s.hw_rx_ns,
            s.rx_tsc.0,
            s.proc_tsc.0,
            s.sent_tsc.0,
            s.ack_hw_rx_ns
        ),
        (
            1,
            ack.hdr.seq,
            EventKind::ORDER_EVENT.0,
            1_001_496_000,
            1_001_500_000,
            ack.hdr.tsc,
            1_001_510_000,
            1_003_000_000
        )
    );
    assert_eq!(h.s().open_orders(), 0);
    h.ev(OrderEventKind::CancelAck, 1, 1, 1_004_000_000, 1_004_000_000, 0);
    assert_eq!(of_kind(&mut h, EventKind::LATENCY_SAMPLE).len(), 2, "step 2");
}

#[test]
fn slc_016_reject_and_consec_fail() {
    let mut h = H::new(cfg_with("[probe]\nmax_consec_fail = 2\n"), true);
    for (i, t) in [1000u64, 2000].into_iter().enumerate() {
        h.b(t, 6_500_000, 6_500_010);
        let p = h.cmds().pop().unwrap();
        h.send(&p, SendStatus::Written, t * MS + 20_000);
        let mut e = order_event(
            OrderEventKind::Reject,
            p.slot.0,
            p.r#gen,
            t * MS + 1_000_000,
            (t * MS + 1_000_000) as i64,
        );
        e.reject_code = 0xFFFF_EC62;
        h.rep
            .try_push(hft_event::RepMsg::OrderEvent(e, hft_event::RecordFlags(0)))
            .unwrap();
        h.at(t * MS + 1_000_000, 0);
        h.drain();
        assert_eq!(h.s().open_orders(), 0);
        if i == 0 {
            assert_eq!(of_kind(&mut h, EventKind::LATENCY_SAMPLE).len(), 1, "step 1");
            assert_eq!(h.s().halted(), None, "step 1");
        }
    }
    assert_eq!(h.s().halted(), Some(Halt::ConsecFail), "step 2");
    h.b(3000, 6_500_000, 6_500_010);
    assert!(h.cmds().is_empty(), "step 3");
}

#[test]
fn slc_017_outcome_unknown_cancels() {
    let mut h = seq0();
    h.b(1000, 6_500_000, 6_500_010);
    let p = h.cmds().pop().unwrap();
    h.send(&p, SendStatus::Written, 1_000_020_000);
    h.ev(OrderEventKind::OutcomeUnknown, 1, 1, 1_001_000_000, 0, 0);
    let c = h.cmds();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].op, c[0].target_slot), (GwOp::Cancel, SlotId(1)));
    assert!(of_kind(&mut h, EventKind::LATENCY_SAMPLE).is_empty());
}

#[test]
fn slc_018_notional_cap_halts() {
    let cfg =
        latency_slice::config::parse(&CFG0.replace("min_notional = \"5\"", "min_notional = \"60\"")).unwrap();
    let mut h = H::new(cfg, true);
    h.b(1000, 6_500_000, 6_500_010);
    assert!(h.cmds().is_empty());
    assert_eq!(h.s().halted(), Some(Halt::NotionalCap));
}

#[test]
fn slc_019_fill_halts() {
    let mut h = H::new(config_two(""), true);
    // slot 1 (inst 1) acked, cancel in flight; slot 2 (inst 2) sent, no answer.
    h.place_ack(1000);
    h.bbo_at(2, 1100 * MS, 1100 * MS, 300_000, 300_010, 0, 0, 0);
    let p2 = h.cmds().pop().unwrap();
    assert_eq!(p2.slot, SlotId(2));
    h.send(&p2, SendStatus::Written, 1100 * MS + 20_000);
    h.ev(OrderEventKind::Fill, 1, 1, 1200 * MS, 1200 * MS as i64, 0);
    assert_eq!(h.s().halted(), Some(Halt::Fill));
    let c = h.cmds();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].op, c[0].target_slot), (GwOp::Cancel, SlotId(2)));
    h.b(5000, 6_500_000, 6_500_010);
    assert!(h.cmds().iter().all(|c| c.op != GwOp::Place));
}

#[test]
fn slc_020_send_fail() {
    let mut h = seq0();
    h.b(1000, 6_500_000, 6_500_010);
    let p = h.cmds().pop().unwrap();
    h.send(&p, SendStatus::NoConn, 1_000_020_000);
    assert_eq!(h.s().open_orders(), 0);
    assert_eq!(h.s().stats().send_fail, 1);
    assert!(of_kind(&mut h, EventKind::LATENCY_SAMPLE).is_empty());
}

#[test]
fn slc_021_rest_timeout() {
    let mut h = H::new(cfg_with("[probe]\nquote_rest_ms = 2000\n"), true);
    h.b(1000, 6_500_000, 6_500_010);
    h.cmds();
    let sweep = |h: &mut H, t_ms: u64| {
        h.at(t_ms * MS, 0);
        for _ in 0..4 {
            h.s().poll();
        }
        h.cmds()
    };
    assert!(sweep(&mut h, 2999).is_empty(), "2999");
    for t in [3000u64, 5000, 7000] {
        let c = sweep(&mut h, t);
        assert_eq!(c.len(), 1, "{t}");
        assert_eq!((c[0].op, c[0].target_slot), (GwOp::Cancel, SlotId(1)), "{t}");
    }
    assert_eq!(h.s().halted(), None);
    sweep(&mut h, 9000);
    assert_eq!(h.s().halted(), Some(Halt::Stuck));
}

#[test]
fn slc_022_drain_cancels_all() {
    let mut h = H::new(config_two(""), true);
    h.place_ack(1000);
    h.bbo_at(2, 1100 * MS, 1100 * MS, 300_000, 300_010, 0, 0, 0);
    let p2 = h.cmds().pop().unwrap();
    h.send(&p2, SendStatus::Written, 1100 * MS + 20_000);
    h.at(1200 * MS, 0);
    h.s().begin_drain(1200 * MS);
    let c = h.cmds();
    assert_eq!(c.len(), 1, "step 1");
    assert_eq!((c[0].op, c[0].target_slot), (GwOp::Cancel, SlotId(2)), "step 1");
    assert!(!h.s().drained(), "step 1");
    h.ev(OrderEventKind::CancelAck, 1, 1, 1300 * MS, 0, 0);
    h.ev(OrderEventKind::CancelAck, 2, 1, 1301 * MS, 0, 0);
    assert!(h.s().drained(), "step 2");
    h.b(5000, 6_500_000, 6_500_010);
    assert!(h.cmds().is_empty(), "step 2");
}

#[test]
fn slc_023_bbo_ts_flags() {
    let rows: [(&str, i64, i64, u16); 4] = [
        ("SLC-023.1", 1000, 2000, RecordFlags::HW_TS.0),
        ("SLC-023.1 equal values", 2000, 2000, RecordFlags::HW_TS.0),
        ("SLC-023.2", 2000, 2000, RecordFlags::SW_TS.0),
        ("SLC-023.3", 0, 2000, 0),
    ];
    for (row, hw, sw, want) in rows {
        let mut h = H::new(cfg_with(""), false);
        let mut event: hft_event::Bbo = zerocopy::FromZeros::new_zeroed();
        event.h.hw_rx_ns = hw;
        event.h.sw_rx_ns = sw;
        h.bbo
            .try_push(hft_event::FeedMsg::Bbo(event, RecordFlags(want)))
            .unwrap();
        h.at(1000 * MS, 0);
        h.drain();
        let r = of_kind(&mut h, EventKind::BBO).pop().unwrap();
        let f = r.hdr.flags & (RecordFlags::HW_TS.0 | RecordFlags::SW_TS.0);
        assert_eq!(f, want, "{row}");
    }
}

#[test]
fn slc_024_input_priority_and_events() {
    let mut h = H::new(cfg_with(""), false);
    let c = hft_event::ClockStatus {
        tsc: hft_types::Tsc(500 * MS),
        realtime_ns: 0,
        phc_ns: 0,
        tsc_hz_milli: 1_000_000_000_000,
        chrony_offset_ns: 0,
        max_error_ns: 0,
        rms_ns: 0,
        calib_err_ns: 0,
        source: 2,
        state: 1,
        _pad: 0,
    };
    h.hk.try_push(hft_event::HkMsg::ClockStatus(c)).unwrap();
    let up = ConnState {
        venue: hft_types::VenueId::BINANCE_SPOT,
        conn: 112,
        state: hft_event::ConnStateKind::Up,
        role: hft_event::ConnRole::Order,
        inst: InstId(0),
        reason: 0,
        account: AccountId(1),
        _pad2: [0; 3],
        rx_tsc: 0,
    };
    // Pushed in reverse priority order.
    let t = 1000 * MS;
    let b = {
        hft_event::Bbo {
            h: hft_event::MarketHdr {
                venue: hft_types::VenueId::BINANCE_SPOT,
                conn: hft_types::ConnId(0),
                _pad: 0,
                inst: InstId(1),
                ex_ts_ns: 0,
                ex_send_ns: 0,
                hw_rx_ns: 0,
                rx_tsc: t,
                sw_rx_ns: 0,
                rx_empty_gap_ns: 0,
                _rsv: 0,
            },
            bid_px: Px(6_500_000),
            bid_qty: Qty(1),
            ask_px: Px(6_500_010),
            ask_qty: Qty(1),
            update_id: 1,
        }
    };
    h.bbo
        .try_push(hft_event::FeedMsg::Bbo(b, hft_event::RecordFlags(0)))
        .unwrap();
    h.rep.try_push(hft_event::RepMsg::ConnState(up)).unwrap();
    h.at(t, 0);
    for _ in 0..3 {
        assert!(h.s().poll());
    }
    let recs = h.input_records();
    let kinds: Vec<EventKind> = recs.iter().map(|r| r.kind()).collect();
    assert_eq!(
        kinds,
        [
            EventKind::CLOCK_STATUS,
            EventKind::CONN_STATE,
            EventKind::BBO,
            EventKind::ORDER_DECISION
        ]
    );
    assert!(recs[1].end() && !recs[2].end() && recs[3].end());
    for w in recs.windows(2) {
        assert_eq!(w[1].hdr.seq, w[0].hdr.seq + 1);
    }
}

#[test]
fn slc_025_clock_and_host_profile() {
    let mut h = H::new(cfg_with("[journal]\nsegment_mib = 1\n"), false);
    let mut clocks = 0;
    for i in 0..20_000u64 {
        let t = 1000 * MS + i * 1_000;
        if i % 4_000 == 0 {
            h.clock_status(t);
            clocks += 1;
        }
        h.bbo_at(1, t, t, 6_500_000, 6_500_010, 0, 0, 0);
        h.side.prep_poll();
    }
    assert!(h.s().stats().rolls >= 1, "the journal rolled");
    let recs = h.records();
    let profiles: Vec<&support::Rec> = recs
        .iter()
        .filter(|r| r.kind() == EventKind::HOST_PROFILE)
        .collect();
    assert_eq!(profiles.len() as u64, h.s().stats().rolls + 1);
    for p in profiles {
        let hp = hft_event::HostProfile::read_from_prefix(&p.body).unwrap().0;
        assert_eq!(hp.tsc_hz_milli, 1_000_000_000_000);
    }
    let cs: Vec<u64> = recs
        .iter()
        .filter(|r| r.kind() == EventKind::CLOCK_STATUS)
        .map(|r| hft_event::ClockStatus::read_from_prefix(&r.body).unwrap().0.tsc.0)
        .collect();
    assert_eq!(cs.len(), clocks);
    assert!(cs.windows(2).all(|w| w[0] < w[1]));
}
