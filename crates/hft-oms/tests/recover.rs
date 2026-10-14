#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ConnRole, ConnStateKind, GwOp, NoteLevel, OrderAction, OrderEventKind, SendStatus};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::{HedgePlan, LinkRole, OnFail, OrderRef, SnapshotReader, SnapshotWriter};
use hft_types::{GROUP_NONE, Px, Qty, Side, Tif, Tsc};

fn snapshot(h: &H) -> Vec<u8> {
    let mut w = SnapshotWriter::new(1 << 22);
    h.o.snapshot(&mut w);
    w.as_bytes().to_vec()
}

fn restored(bytes: &[u8]) -> H {
    let o = Oms::restore(&mut SnapshotReader::new(bytes), &p0(), rd()).unwrap();
    H {
        o,
        out: OmsOut::new(),
        seq: 10_000,
    }
}

fn rec_end(h: &mut H, now: u64, stop: u64) {
    h.out.clear();
    h.o.recovery_end(ms(now), ms(stop), HZ, HZ, 1, &mut h.out);
}

fn check_simultaneous_recovery_waits_for_both_terminal() {
    for policy in [OnFail::Reverse, OnFail::Hold] {
        let mut h = H::new();
        let mut hp = plan_l1();
        hp.kind = hft_strategy_sdk::HedgeKind::Simultaneous;
        hp.on_fail = policy;
        let (first, link) = place_first(&mut h, q("0.012"), Tif::Ioc, hp, 0);
        let req = one_hedge(&h);
        let second = place_hedge(&mut h, &req, 0).unwrap();
        h.fill(first, q("0.005"), p("60000"), 1);
        let bytes = snapshot(&h);
        h = restored(&bytes);
        rec_end(&mut h, 10_000, 2);
        gate(&mut h, VS, 10_000);
        gate(&mut h, VX, 10_000);
        h.query(first, 1, q("0.005"), p("60000"), 10_001);
        assert!(h.out.hedges.is_empty());
        h.query(second, 1, q("0.008"), p("59990"), 10_002);
        assert!(
            h.out.hedges.is_empty(),
            "live original orders must not be reversed"
        );
        h.what(first, OrderEventKind::Expired, 10_003);
        assert!(h.out.hedges.is_empty());
        h.what(second, OrderEventKind::Expired, 10_004);
        if policy == OnFail::Reverse {
            let r = one_hedge(&h);
            assert_eq!((r.inst, r.side, r.qty), (Y, Side::Buy, q("0.003")));
            let rv = place_hedge(&mut h, &r, 10_004).unwrap();
            // A second crash with the reverse still live must not duplicate the reverse.
            h = restored(&snapshot(&h));
            rec_end(&mut h, 20_000, 10_005);
            gate(&mut h, VX, 20_000);
            h.query(rv, 1, Qty(0), p("60000"), 20_001);
            assert!(h.out.hedges.is_empty());
            h.fill(rv, q("0.003"), p("60000"), 20_002);
        }
        assert_eq!(h.link(link).state, LinkState::Done);
    }
}

#[test]
fn oms_464_late_first_fill_does_not_double_resume() {
    let mut h = H::new();
    let (first, link) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    let bytes = snapshot(&h);
    h = restored(&bytes);
    rec_end(&mut h, 10_000, 1);
    h.o.on_hedge_gate_open(A1, VX, ms(10_000), 1, &mut h.out);
    assert_ne!(h.link(link).flags & link_flags::RESUME_HOLD, 0);
    h.fill(first, q("0.012"), p("60000"), 10_001);
    let req = one_hedge(&h);
    assert_eq!(req.role, LinkRole::Second);
    assert_eq!(req.qty, q("0.012"));
    assert_eq!(
        h.link(link).flags & (link_flags::RESUME_HOLD | link_flags::RESUME_RETRY),
        0
    );
    let second = place_hedge(&mut h, &req, 10_001).unwrap();
    h.fill(second, q("0.012"), p("60000"), 10_002);
    assert_eq!(h.link(link).state, LinkState::Done);
}

#[test]
fn oms_092_mirror_restore_batches() {
    let mut h = H::with(|c| c.slots.set(VS, A1, 400));
    let mut open = Vec::new();
    for i in 0..350u64 {
        let it = intent(
            VS,
            A1,
            X,
            Side::Buy,
            Tif::Gtc,
            p("60000.00"),
            q("0.01"),
            GROUP_NONE,
        );
        let r = h.place_ok(&it, 0);
        h.ack(r, 1000 + i, 0);
        match i {
            0..120 => h.force(r, OrderState::Unknown, true, OrderState::Live),
            120..150 => {
                h.o.test_slot_mut(VS, A1, r.slot, |sl, _| sl.sflags.0 |= SlotFlags::VERIFY)
            }
            300.. => h.force(r, OrderState::Filled, true, OrderState::Live),
            _ => {}
        }
        if i < 300 {
            open.push(r);
        }
    }
    let mut buf = FixedVec::<MirrorRestore, 256>::new();
    let mut cur = 0;
    assert!(h.o.mirror_restore(VS, A1, &mut buf, &mut cur));
    assert_eq!(buf.len(), 256);
    let mut all: Vec<_> = buf.to_vec();
    buf.clear();
    assert!(!h.o.mirror_restore(VS, A1, &mut buf, &mut cur));
    assert_eq!(buf.len(), 44);
    all.extend(buf.iter());
    assert_eq!(all.len(), 300);
    for (m, r) in all.iter().zip(&open) {
        let sl = h.slot(*r);
        assert_eq!(
            (m.slot, m.r#gen, m.epoch, m.inst, m.exch_order_id),
            (r.slot, sl.r#gen, sl.epoch, sl.inst, sl.exch_order_id)
        );
    }
}

#[test]
fn oms_093_query_unsupported_after_mirror() {
    let mut h = H::new();
    let r = h.place_ok(&buy_x(), 0);
    let mut h = restored(&snapshot(&h));
    rec_end(&mut h, 10, 5);
    assert_eq!(h.state(r), OrderState::Unknown);
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 11);
    assert_eq!(h.decisions(OrderAction::Query).len(), 1);
    h.send(
        VS,
        A1,
        GwOp::Query,
        r.slot,
        r.r#gen,
        SendStatus::QueryUnsupported,
        12,
        12,
    );
    assert_eq!((h.state(r), h.slot(r).query_count), (OrderState::Unknown, 0));
    let n = h.notes(notes::NEEDS_REST_QUERY);
    assert_eq!(
        (n.len(), n[0].v[0] >> 16 & 1, n[0].v[1], n[0].v[2]),
        (1, 0, u64::from(r.slot), u64::from(r.r#gen))
    );
}

#[test]
fn oms_310_restore_loads_only() {
    let mut h = H::new();
    let pn = h.place_ok(&buy_x(), 100);
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 50);
    h.o.test_tomb_mut(VS, A1, 60, 1, |tb| {
        tb.r#gen = 1;
        tb.freed_tsc = ms(80);
    });
    let bytes = snapshot(&h);
    let mut r = restored(&bytes);
    assert_eq!(r.slot(pn).t_state, ms(100));
    assert_eq!(r.link(l).first_fill_tsc, ms(50));
    assert_eq!(r.o.tomb(VS, A1, 60, 1).unwrap().freed_tsc, ms(80));
    assert_eq!(r.legs(A1, g1()).last_fill_tsc, ms(50));
    assert_eq!(r.o.resume_hold_until(), Tsc(0));
    h.sweep(2101);
    r.sweep(2101);
    assert_eq!(r.state(pn), OrderState::Unknown);
    assert_eq!(transitions(&r)[0].2, 16);
    assert_eq!(format!("{:?}", h.out), format!("{:?}", r.out));
}

fn y_none() -> ApprovedIntent {
    let mut it = sell_y();
    it.intent.group = GROUP_NONE;
    it
}

/// VX: Live ×2, PendingNew, Filled, smoke Live; L1 Hedging (first leg filled at 498 800 ms).
fn rec_setup() -> (H, Vec<OrderRef>, OrderRef, OrderRef, u32) {
    let mut h = H::new();
    let mut open = Vec::new();
    for i in 0..2 {
        let t = h.place_ok(&y_none(), 0);
        h.ack(t, 10 + i, 0);
        open.push(t);
    }
    open.push(h.place_ok(&y_none(), 0));
    let fl = h.place_ok(&y_none(), 0);
    h.fill(fl, q("0.01"), p("60100.0"), 0);
    let mut sm = intent(
        VX,
        A1,
        Y,
        Side::Buy,
        Tif::PostOnly,
        p("58200.0"),
        q("0.001"),
        GROUP_NONE,
    );
    sm.origin = Origin::Command;
    sm.smoke_ttl_ms = 60_000;
    let k = h.place_ok(&sm, 0);
    h.ack(k, 20, 0);
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 498_800);
    h.fill(f, q("0.012"), p("60000.00"), 498_800);
    (h, open, fl, k, l)
}

#[test]
fn oms_311_recovery_end_state() {
    let (mut h, open, fl, k, l) = rec_setup();
    rec_end(&mut h, 10_000, 500_000);
    for r in open.iter().chain([&k]) {
        assert_eq!(h.state(*r), OrderState::Unknown);
    }
    assert!(transitions(&h).iter().all(|t| t.2 == 20));
    assert_eq!(transitions(&h).len(), 4);
    assert!(h.flags(k) & SlotFlags::SMOKE_DUE != 0);
    assert_eq!(h.state(fl), OrderState::Filled);
    let t = h.timers(OmsTimerKind::UnknownQuery);
    assert_eq!(t.len(), 4);
    assert!(t.iter().all(|t| t.deadline_tsc == ms(11_000)));
    assert!(h.out.decisions.is_empty());
    for v in [VS, VX, VG] {
        let tv = h.o.table_view(v, A1).unwrap();
        assert_eq!((tv.conn_up, tv.hedge_gate, tv.decode_fails), (false, false, 0));
    }
    assert_eq!(h.o.resume_hold_until(), Tsc(40_000_000_000));
    assert!(h.link(l).flags & link_flags::RESUME_HOLD != 0);
    assert!(h.out.links.is_empty());
}

#[test]
fn oms_312_tsc_shift() {
    // (row, value tick, stop tick, now tick, hz_prev, hz_cur, want tick)
    let g = 1_000_000_000_000u64;
    let rows: [(&str, u64, u64, u64, u64, u64, u64); 9] = [
        (
            "OMS-312.1",
            ms(499_000).0,
            ms(500_000).0,
            ms(10_000).0,
            g,
            g,
            ms(9000).0,
        ),
        (
            "OMS-312.2",
            ms(499_000).0,
            ms(500_000).0,
            ms(10_000).0,
            g,
            g,
            ms(9000).0,
        ),
        (
            "OMS-312.3",
            ms(499_000).0,
            ms(500_000).0,
            ms(10_000).0,
            g,
            g,
            ms(9000).0,
        ),
        (
            "OMS-312.4",
            ms(499_000).0,
            ms(500_000).0,
            ms(10_000).0,
            g,
            g,
            ms(9000).0,
        ),
        (
            "OMS-312.5",
            499_000_000_000,
            500_000_000_000,
            20_000_000_000,
            g,
            2 * g,
            18_000_000_000,
        ),
        ("OMS-312.6", ms(499_000).0, ms(500_000).0, ms(500).0, g, g, 1),
        ("OMS-312.7", 0, ms(500_000).0, ms(10_000).0, g, g, 0),
        (
            "OMS-312.8",
            ms(499_000).0,
            ms(500_000).0,
            ms(600_000).0,
            g,
            g,
            ms(599_000).0,
        ),
        (
            "OMS-312.9",
            ms(500_100).0,
            ms(500_000).0,
            ms(10_000).0,
            g,
            g,
            ms(10_100).0,
        ),
    ];
    for (row, v, stop, now, hp, hc, want) in rows {
        let mut h = H::new();
        let s = h.place_ok(&buy_x(), 0);
        h.what(s, OrderEventKind::CancelAck, 0);
        h.o.test_slot_mut(VS, A1, s.slot, |sl, _| sl.t_state = Tsc(v));
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
        h.fill(f, q("0.012"), p("60000.00"), 0);
        h.o.test_link_mut(l, |e| e.first_fill_tsc = Tsc(v));
        h.o.test_tomb_mut(VS, A1, 60, 1, |tb| {
            tb.r#gen = 1;
            tb.freed_tsc = Tsc(v);
        });
        h.o.test_leg_mut(A1, g1(), |st| st.last_fill_tsc = Tsc(v));
        h.out.clear();
        h.o.recovery_end(Tsc(now), Tsc(stop), hp, hc, 1, &mut h.out);
        assert_eq!(h.slot(s).t_state, Tsc(want), "{row} t_state");
        assert_eq!(h.link(l).first_fill_tsc, Tsc(want), "{row} first_fill_tsc");
        if v != 0 {
            assert_eq!(
                h.o.tomb(VS, A1, 60, 1).unwrap().freed_tsc,
                Tsc(want),
                "{row} freed_tsc"
            );
        }
        assert_eq!(h.legs(A1, g1()).last_fill_tsc, Tsc(want), "{row} last_fill_tsc");
    }
}

#[test]
fn oms_313_held_ms() {
    let g = 1_000_000_000_000u64;
    for (row, fft, hp, pre, want) in [
        ("OMS-313.1", ms(498_800).0, g, None, 1200u32),
        ("OMS-313.2", ms(495_000).0, g, None, 3000),
        ("OMS-313.3", 0, g, None, 0),
        ("OMS-313.4", 1_000_000_000_000 - 2_400_000_000, 2 * g, None, 1200),
        ("OMS-313.5", ms(400_000).0, g, Some(700u32), 700),
    ] {
        let mut h = H::new();
        let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
        h.fill(f, q("0.012"), p("60000.00"), 0);
        h.o.test_link_mut(l, |e| {
            e.first_fill_tsc = Tsc(fft);
            if let Some(x) = pre {
                e.flags |= link_flags::RESUME_HOLD;
                e.held_ms = x;
            }
        });
        let stop = if hp == 2 * g {
            Tsc(1_000_000_000_000)
        } else {
            ms(500_000)
        };
        h.out.clear();
        h.o.recovery_end(ms(10_000), stop, hp, g, 1, &mut h.out);
        assert_eq!(h.link(l).held_ms, want, "{row}");
    }
}

/// L1 (first leg `tif`, `qty`, filled `cum` at 498 800 ms, stop 500 000) recovered at now = 10 000.
fn held(plan: HedgePlan, tif: Tif, qty: &str, cum: &str) -> (H, OrderRef, u32) {
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q(qty), tif, plan, 498_800);
    if tif != Tif::Ioc {
        h.ack(f, 1, 498_800);
    }
    h.fill(f, q(cum), p("60000.00"), 498_800);
    rec_end(&mut h, 10_000, 500_000);
    assert_eq!(h.link(l).held_ms, 1200);
    (h, f, l)
}

#[test]
fn oms_314_resume_hold_behaviour() {
    let (mut h, f, l) = held(plan_l1(), Tif::Gtc, "0.02", "0.012");
    h.retry(l, 15_000);
    assert!(h.out.is_empty(), "OMS-314.1");
    h.sweep(20_000);
    assert_ne!(h.link(l).state, LinkState::Broken, "OMS-314.2");
    let mut e = h.oe(f, OrderEventKind::PartialFill);
    e.cum_filled = q("0.015");
    e.avg_px = p("60000.00");
    h.ev(&e, 20_000);
    let r = one_hedge(&h);
    assert_eq!((r.role, r.qty), (LinkRole::Second, q("0.003")), "OMS-314.3");
    let hs = place_hedge(&mut h, &r, 20_000).unwrap();
    let r0 = h.link(l).retries;
    let e = h.oe(hs, OrderEventKind::Reject);
    h.ev(&e, 20_100);
    assert_eq!(h.timers(OmsTimerKind::HedgeRetry).len(), 1, "OMS-314.4");
    assert_ne!(h.link(l).state, LinkState::Broken, "OMS-314.4");
    assert_eq!(h.link(l).retries, r0 + 1, "OMS-314.4");
}

fn gate(h: &mut H, v: hft_types::VenueId, t: u64) {
    h.out.clear();
    h.o.on_hedge_gate_open(A1, v, ms(t), 1, &mut h.out);
}

#[test]
fn oms_315_resume_second_retry() {
    let (mut h, _f, l) = held(plan_l1(), Tif::Ioc, "0.012", "0.012");
    gate(&mut h, VX, 30_000);
    let e = h.link(l);
    assert!(e.flags & link_flags::RESUME_HOLD == 0 && e.flags & link_flags::RESUME_RETRY != 0);
    assert_eq!(e.first_fill_tsc, ms(28_800));
    assert!(h.o.table_view(VX, A1).unwrap().hedge_gate);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.qty, r.basis),
        (LinkRole::SecondRetry, q("0.012"), q("0.012"))
    );
    let n = h.notes(notes::RESUME_RETRY);
    assert_eq!((n.len(), n[0].level), (1, NoteLevel::P1));
    assert_eq!(&n[0].v[..4], &[u64::from(l), 1200, 1_200_000, 1_200_000]);
    assert_eq!(h.out.links.iter().filter(|u| u.flags == 0).count(), 1);
}

#[test]
fn oms_316_resume_venue() {
    for (row, on_fail, v, req) in [
        ("OMS-316.1", OnFail::Retry, VX, Some(LinkRole::SecondRetry)),
        ("OMS-316.2", OnFail::Retry, VS, None),
        ("OMS-316.3", OnFail::Reverse, VS, Some(LinkRole::Reverse)),
        ("OMS-316.4", OnFail::Reverse, VX, None),
        ("OMS-316.5", OnFail::Hold, VX, None),
    ] {
        let mut plan = plan_l1();
        plan.on_fail = on_fail;
        let (mut h, _f, l) = held(plan, Tif::Ioc, "0.012", "0.012");
        gate(&mut h, v, 30_000);
        match req {
            Some(role) => {
                let r = one_hedge(&h);
                assert_eq!(r.role, role, "{row}");
                if role == LinkRole::Reverse {
                    assert_eq!((r.venue, r.qty), (VS, q("0.012")), "{row}");
                    assert!(h.link(l).flags & link_flags::RESUME_RETRY != 0, "{row}");
                }
            }
            None => {
                assert!(h.out.hedges.is_empty(), "{row}");
                if on_fail == OnFail::Hold {
                    assert!(
                        h.link(l).flags & (link_flags::RESUME_HOLD | link_flags::RESUME_RETRY) == 0,
                        "{row}"
                    );
                    let n = h.notes(notes::RESUME_RETRY);
                    assert_eq!((n.len(), n[0].v[3]), (1, 0), "{row}");
                    assert_eq!(h.out.links.len(), 1, "{row}");
                } else {
                    assert!(h.link(l).flags & link_flags::RESUME_HOLD != 0, "{row}");
                }
            }
        }
    }
    let (mut h, _f, _l) = held(plan_l1(), Tif::Ioc, "0.012", "0.012");
    gate(&mut h, VX, 30_000);
    gate(&mut h, VX, 30_001);
    assert!(h.out.is_empty(), "OMS-316.6");
}

#[test]
fn oms_317_resume_waits_unknown() {
    check_simultaneous_recovery_waits_for_both_terminal();
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 498_800);
    h.fill(f, q("0.012"), p("60000.00"), 498_800);
    let rq = one_hedge(&h);
    let hs = place_hedge(&mut h, &rq, 498_800).unwrap();
    rec_end(&mut h, 10_000, 500_000);
    assert_eq!(h.state(hs), OrderState::Unknown);
    gate(&mut h, VX, 30_000);
    assert!(h.out.hedges.is_empty());
    assert!(h.link(l).flags & link_flags::RESUME_HOLD != 0);
    assert!(h.o.table_view(VX, A1).unwrap().hedge_gate);
    h.conn(VX, A1, ConnRole::Order, ConnStateKind::Up, 30_500);
    h.query(hs, 4, Qty(0), Px(0), 31_000);
    assert_eq!(h.state(hs), OrderState::Cancelled);
    let r = one_hedge(&h);
    assert_eq!((r.role, r.qty), (LinkRole::SecondRetry, q("0.012")));
    assert_eq!(h.n_notes(notes::RESUME_RETRY), 1);
    assert_eq!(h.link(l).first_fill_tsc, ms(31_000 - 1200));
}

#[test]
fn oms_318_resume_capped_zero() {
    let (mut h, _f, l) = held(plan_l1(), Tif::Ioc, "0.012", "0.012");
    h.o.test_set_net(A1, g1(), Y, q("-0.012"));
    gate(&mut h, VX, 30_000);
    assert!(h.out.hedges.is_empty(), "OMS-318.1");
    assert_eq!(
        (h.link(l).hedge_req, h.link(l).state),
        (Qty(0), LinkState::Done),
        "OMS-318.1"
    );
    assert!(h.link(l).flags & link_flags::RESUME_RETRY == 0, "OMS-318.1");
    let (mut h, f, l) = held(plan_l1(), Tif::Gtc, "0.02", "0.012");
    h.conn(VS, A1, ConnRole::Order, ConnStateKind::Up, 20_000);
    h.query(f, 2, q("0.012"), p("60000.00"), 20_000);
    assert_eq!(h.state(f), OrderState::Live);
    h.o.test_set_net(A1, g1(), Y, q("-0.012"));
    gate(&mut h, VX, 30_000);
    assert!(h.out.hedges.is_empty(), "OMS-318.2");
    let e = h.link(l);
    assert_eq!((e.hedge_req, e.state), (Qty(0), LinkState::Hedging), "OMS-318.2");
    assert!(
        e.flags & (link_flags::RESUME_HOLD | link_flags::RESUME_RETRY) == 0,
        "OMS-318.2"
    );
}

#[test]
fn oms_319_resume_retry_terminal() {
    for (row, t, fill) in [
        ("OMS-319.1", 30_200u64, true),
        ("OMS-319.2", 30_500, false),
        ("OMS-319.3", 31_801, false),
    ] {
        let (mut h, _f, l) = held(plan_l1(), Tif::Ioc, "0.012", "0.012");
        h.conn(VX, A1, ConnRole::Order, ConnStateKind::Up, 29_000);
        gate(&mut h, VX, 30_000);
        let rq = one_hedge(&h);
        let rs = place_hedge(&mut h, &rq, 30_000).unwrap();
        let r0 = h.link(l).retries;
        if fill {
            h.fill(rs, q("0.012"), p("59990.0"), t);
            assert!(h.link(l).flags & link_flags::RESUME_RETRY == 0, "{row}");
            assert_eq!(h.link(l).state, LinkState::Done, "{row}");
        } else {
            let e = h.oe(rs, OrderEventKind::Reject);
            h.ev(&e, t);
            assert!(h.link(l).flags & link_flags::RESUME_RETRY == 0, "{row}");
            if t == 30_500 {
                assert_eq!(h.timers(OmsTimerKind::HedgeRetry).len(), 1, "{row}");
                assert_eq!(h.link(l).retries, r0 + 1, "{row}");
            } else {
                assert_eq!(h.link(l).state, LinkState::Broken, "{row}");
                assert_eq!(
                    h.out
                        .links
                        .iter()
                        .find(|u| u.state == LinkState::Broken)
                        .unwrap()
                        .broken_reason,
                    LinkBroken::Deadline,
                    "{row}"
                );
            }
        }
    }
}

/// L1 (exposed, gate closed) and L4 (first leg Unknown, zero exposure) after recovery.
fn timeout_setup(l1_second_unknown: bool) -> (H, u32, u32, OrderRef) {
    let mut h = H::new();
    let (f, l1) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 498_800);
    h.fill(f, q("0.012"), p("60000.00"), 498_800);
    if l1_second_unknown {
        let rq = one_hedge(&h);
        place_hedge(&mut h, &rq, 498_800).unwrap();
    }
    let (f4, l4) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 499_000);
    rec_end(&mut h, 10_000, 500_000);
    assert_eq!(h.state(f4), OrderState::Unknown);
    (h, l1, l4, f4)
}

#[test]
fn oms_320_resume_timeout() {
    let (mut h, _l1, _l4, _) = timeout_setup(false);
    h.out.clear();
    h.o.on_sweep(Tsc(ms(40_000).0 - 1), &mut h.out);
    assert!(
        h.out.links.is_empty() && h.n_notes(notes::RESUME_TIMEOUT) == 0,
        "OMS-320.1"
    );
    let (mut h, l1, l4, _) = timeout_setup(false);
    h.sweep(40_000);
    assert_eq!(h.link(l1).state, LinkState::Broken, "OMS-320.2");
    let n = h.notes(notes::RESUME_TIMEOUT);
    let n1 = n.iter().find(|x| x.v[0] == u64::from(l1)).unwrap();
    assert_eq!(n1.level, NoteLevel::P0);
    assert_eq!(&n1.v[..6], &[u64::from(l1), 1281, 1200, 1_200_000, 1, 1_200_000]);
    assert!(h.out.links.iter().any(|u| u.link_id == l1
        && u.state == LinkState::Broken
        && u.broken_reason == LinkBroken::ResumeTimeout));
    assert_ne!(h.link(l4).state, LinkState::Broken);
    assert!(h.link(l4).flags & link_flags::RESUME_HOLD == 0);
    let n4 = n.iter().find(|x| x.v[0] == u64::from(l4)).unwrap();
    assert_eq!((n4.level, n4.v[4]), (NoteLevel::P1, 1));
    assert!(
        h.out
            .links
            .iter()
            .any(|u| u.link_id == l4 && u.flags == 0 && u.state != LinkState::Broken)
    );
    let (mut h, l1, _, _) = timeout_setup(true);
    gate(&mut h, VX, 20_000);
    h.sweep(40_000);
    assert_eq!(h.link(l1).state, LinkState::Broken, "OMS-320.3");
    let n = h.notes(notes::RESUME_TIMEOUT);
    assert_eq!(
        n.iter().find(|x| x.v[0] == u64::from(l1)).unwrap().v[4],
        2,
        "OMS-320.3"
    );
}

#[test]
fn oms_321_crash_during_recovery() {
    let (h, _o, _fl, _k, l) = rec_setup();
    let mut h = h;
    rec_end(&mut h, 10_000, 500_000);
    let bytes = snapshot(&h);
    let mut h = restored(&bytes);
    rec_end(&mut h, 5000, 15_000);
    assert_eq!(h.link(l).held_ms, 1200);
    assert_eq!(h.o.resume_hold_until(), ms(35_000));
}

#[test]
fn oms_322_tomb_expiry_after_reboot() {
    for (row, now, kept) in [("OMS-322.1", 3_609_000u64, true), ("OMS-322.2", 3_609_001, false)] {
        let mut h = H::new();
        h.o.test_tomb_mut(VS, A1, 60, 1, |tb| {
            tb.r#gen = 1;
            tb.freed_tsc = ms(499_000);
        });
        rec_end(&mut h, 10_000, 500_000);
        assert_eq!(h.o.tomb(VS, A1, 60, 1).unwrap().freed_tsc, ms(9000));
        h.sweep(now);
        assert_eq!(h.o.tomb(VS, A1, 60, 1).is_some(), kept, "{row}");
    }
}

#[test]
fn oms_323_zero_exposure_followup() {
    let after = || {
        let (mut h, _l1, l4, f4) = timeout_setup(false);
        h.sweep(40_000);
        h.o.test_slot_mut(VS, A1, f4.slot, |sl, _| sl.sflags.0 |= SlotFlags::HIST_WAIT);
        assert_eq!(h.link(l4).first_fill_tsc, Tsc(0));
        (h, l4, f4)
    };
    let (mut h, l4, f4) = after();
    h.recon(&recon(f4, 4, 7, 1, Qty(0)), 45_000);
    assert_eq!(h.state(f4), OrderState::Rejected, "OMS-323.1");
    assert_eq!(h.link(l4).state, LinkState::Done, "OMS-323.1");
    let (mut h, l4, f4) = after();
    h.recon(&recon(f4, 4, 3, 1, q("0.012")), 45_000);
    let r = one_hedge(&h);
    assert_eq!(
        (r.role, r.qty, r.link_id),
        (LinkRole::Second, q("0.012"), l4),
        "OMS-323.2"
    );
    assert_eq!(h.link(l4).first_fill_tsc, ms(45_000), "OMS-323.2");
    let hs = place_hedge(&mut h, &r, 45_000).unwrap();
    let e = h.oe(hs, OrderEventKind::Reject);
    h.ev(&e, 48_001);
    assert_eq!(h.link(l4).state, LinkState::Broken, "OMS-323.3");
    assert_eq!(
        h.out
            .links
            .iter()
            .find(|u| u.state == LinkState::Broken)
            .unwrap()
            .broken_reason,
        LinkBroken::Deadline
    );
}
