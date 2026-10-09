//! Connection liveness (TESTS.md "连接活性": SEQ-113–SEQ-120).

use hft_event::{ConnRole, ConnState, ConnStateKind, EventKind};
use hft_sequencer::driver::live::RingId;
use hft_types::{AccountId, InstId, Tsc};

use crate::common::*;

fn md(v: hft_types::VenueId, k: u8, st: ConnStateKind, i: InstId, rx: u64) -> ConnState {
    conn(v, k, st, ConnRole::Md, i, AccountId(0), rx)
}

/// B0 steady with venue A slots 0 and 1 up (live 1 000), a1 member of 0 and 1, a2 of 0.
fn base() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Up, InstId(0), 1_000));
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Up, A1, 1_000));
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Alive, InstId(0), 1_000));
    h.run();
    for k in 0..2u8 {
        if let Some(c) = h.s.engine_mut().market.conn_mut(A, k) {
            c.live_tsc = 1_000;
        }
    }
    h
}

fn slot(h: &H, v: hft_types::VenueId, k: u8) -> hft_sequencer::market::ConnLive {
    h.s.engine().market.conns[usize::from(v.0)][usize::from(k)]
}
fn mask(h: &H, i: InstId) -> u8 {
    h.s.engine().market.row(i).unwrap().bbo_member_mask
}
fn view(h: &H, i: InstId) -> hft_strategy_sdk::BboView {
    let e = h.s.engine();
    e.market.bbo_view(&e.books, i)
}

#[test]
fn seq_113_conn_live_transitions() {
    let mut h = base();
    assert_eq!(mask(&h, A1), 0b11);
    // .1
    let mut b = bbo(A1, A, 0, "50000", "50002", 500_000, 5_000);
    b.h.hw_rx_ns = 42;
    h.push(RingId::Bbo(0), &b);
    h.run();
    assert_eq!(slot(&h, A, 0).live_tsc, 5_000);
    assert_eq!(h.s.engine().market.row(A1).unwrap().bbo_hw_rx_ns, 42);
    // .2 older rx does not move live_tsc back.
    h.push(RingId::Bbo(0), &bbo(A1, A, 0, "50000", "50002", 500_001, 800));
    h.run();
    assert_eq!(slot(&h, A, 0).live_tsc, 5_000);
    // .3
    h.push(RingId::Bbo(0), &bbo(A3, A, 0, "10", "11", 500_002, 5_100));
    h.run();
    assert_eq!(mask(&h, A3) & 1, 1);
    // .4
    h.push(RingId::Rep(0), &md(A, 2, ConnStateKind::Up, InstId(0), 6_000));
    h.run();
    let s2 = slot(&h, A, 2);
    assert!(s2.up && !s2.resync && s2.live_tsc == 6_000);
    // .5
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Up, A3, 7_000));
    h.run();
    assert_eq!(mask(&h, A3) & 2, 2);
    assert_eq!(slot(&h, A, 1).live_tsc, 7_000);
    // .6
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Down, A1, 0));
    h.run();
    assert_eq!(mask(&h, A1), 0b01);
    assert!(slot(&h, A, 1).up);
    // .7
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Down, InstId(0), 0));
    h.run();
    assert!(!slot(&h, A, 0).up);
    assert_eq!(mask(&h, A1), 0b00);
    assert_eq!(mask(&h, A2), 0);
}

#[test]
fn seq_113_conn_live_transitions_alive_resync() {
    // .8 Alive: live_tsc moves, no Conn delivery.
    let mut h = base();
    clear_log();
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Alive, InstId(0), 9_000));
    h.run();
    assert_eq!(slot(&h, A, 0).live_tsc, 9_000);
    assert!(!log().iter().any(|g| g.kind == "Conn"));
    // .9 Resync on the connection; a Bbo does not clear it, Up does.
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Resync, InstId(0), 9_100));
    h.run();
    assert!(slot(&h, A, 0).resync);
    h.push(RingId::Bbo(0), &bbo(A1, A, 0, "50000", "50002", 600_000, 9_200));
    h.run();
    assert!(slot(&h, A, 0).resync);
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Up, InstId(0), 9_300));
    h.run();
    assert!(!slot(&h, A, 0).resync);
    // .10 per-inst resync, cleared by the next applied Bbo.
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Resync, A1, 9_400));
    h.run();
    assert_eq!(h.s.engine().market.row(A1).unwrap().bbo_flags & 1, 1);
    assert_eq!(h.s.engine().market.row(A2).unwrap().bbo_flags & 1, 0);
    h.push(RingId::Bbo(0), &bbo(A1, A, 0, "50000", "50002", 600_001, 9_500));
    h.run();
    assert_eq!(h.s.engine().market.row(A1).unwrap().bbo_flags & 1, 0);
    // .11/.12 Rotated and Degraded change nothing.
    let before = (h.s.engine().market.conns[usize::from(A.0)], mask(&h, A1), mask(&h, A2));
    for st in [ConnStateKind::Rotated, ConnStateKind::Degraded] {
        h.push(RingId::Rep(0), &md(A, 0, st, InstId(0), 9_600));
        h.run();
        assert_eq!((h.s.engine().market.conns[usize::from(A.0)], mask(&h, A1), mask(&h, A2)), before);
    }
}

#[test]
fn seq_114_bbo_view_projection() {
    let mut h = base();
    let set = |h: &mut H, k: u8, up: bool, resync: bool, live: u64| {
        if let Some(c) = h.s.engine_mut().market.conn_mut(A, k) {
            c.up = up;
            c.resync = resync;
            c.live_tsc = live;
        }
    };
    let chk = |h: &H, up: bool, live: u64, rs: bool| {
        let v = view(h, A1);
        assert_eq!((v.conn_up, v.conn_live_tsc, v.resync), (up, Tsc(live), rs));
    };
    set(&mut h, 0, true, false, 5_000);
    set(&mut h, 1, true, false, 7_000);
    chk(&h, true, 7_000, false);
    set(&mut h, 0, true, true, 9_000);
    chk(&h, true, 7_000, false);
    set(&mut h, 1, true, true, 7_000);
    chk(&h, true, 9_000, true);
    set(&mut h, 0, false, false, 9_000);
    set(&mut h, 1, true, false, 7_000);
    chk(&h, true, 7_000, false);
    set(&mut h, 1, false, false, 7_000);
    chk(&h, false, 0, true);
    set(&mut h, 0, true, false, 5_000);
    set(&mut h, 1, true, false, 7_000);
    if let Some(r) = h.s.engine_mut().market.row_mut(A1) {
        r.bbo_flags |= 1;
    }
    chk(&h, true, 7_000, true);
    if let Some(r) = h.s.engine_mut().market.row_mut(A1) {
        r.bbo_flags = 0;
        r.bbo_member_mask = 0;
    }
    chk(&h, false, 0, true);
}

#[test]
fn seq_115_quiet_inst_on_busy_conn() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let t = h.now();
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Up, A2, t));
    h.run();
    for k in 1..=20u64 {
        let rx = 1_000_000 + k * 100_000_000;
        h.push(RingId::Bbo(0), &bbo(A2, A, 0, "3000", "3001", 700_000 + k, rx));
        h.run();
        let v = view(&h, A1);
        assert_eq!(v.conn_live_tsc, Tsc(rx));
        assert!(!v.resync);
    }
}

#[test]
fn seq_116_alive_keeps_quiet_market_live() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let top0 = *h.s.engine().books.top(A1);
    clear_log();
    let mut last = 0;
    for k in 1..=10u64 {
        last = 2_000_000 + k * 1_000_000_000;
        h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Alive, InstId(0), last));
        h.run();
    }
    assert_eq!(view(&h, A1).conn_live_tsc, Tsc(last));
    assert!(!log().iter().any(|g| g.kind == "Conn"));
    assert_eq!(*h.s.engine().books.top(A1), top0);
}

#[test]
fn seq_117_slot_reuse_membership() {
    let mut h = base();
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Down, InstId(0), 0));
    h.run();
    assert!(view(&h, A1).conn_up);
    assert_eq!(mask(&h, A1), 0b10);
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Up, InstId(0), 2_000));
    h.run();
    assert_eq!(mask(&h, A1), 0b10);
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Up, A1, 2_000));
    h.run();
    assert_eq!(mask(&h, A1), 0b11);
}

#[test]
fn seq_118_all_members_down() {
    let mut h = base();
    h.push(RingId::Rep(0), &md(A, 0, ConnStateKind::Down, InstId(0), 0));
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Down, InstId(0), 0));
    h.run();
    let v = view(&h, A1);
    assert_eq!((v.conn_up, v.conn_live_tsc, v.resync), (false, Tsc(0), true));
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Up, InstId(0), 3_000));
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Up, A1, 3_000));
    h.run();
    let v = view(&h, A1);
    assert!(v.conn_up && !v.resync);
}

#[test]
fn seq_119_depth_conn_ignored() {
    let mut h = base();
    let s0 = h.s.engine().market.conns[usize::from(A.0)];
    let m0 = (mask(&h, A1), mask(&h, A2));
    h.push(RingId::Rep(0), &md(A, 8, ConnStateKind::Alive, InstId(0), 99_000));
    h.run();
    assert_eq!(h.s.engine().market.conns[usize::from(A.0)], s0);
    h.push(RingId::Rep(0), &md(A, 8, ConnStateKind::Down, InstId(0), 0));
    h.run();
    assert_eq!(h.s.engine().market.conns[usize::from(A.0)], s0);
    assert_eq!((mask(&h, A1), mask(&h, A2)), m0);
    h.push(RingId::Rep(0), &md(A, 7, ConnStateKind::Alive, InstId(0), 99_500));
    h.run();
    assert_eq!(slot(&h, A, 7).live_tsc, 99_500);
    let s7 = h.s.engine().market.conns[usize::from(A.0)];
    let mut t = trade(A1, 5);
    t.h.conn = hft_types::ConnId(8);
    h.push(RingId::Md(0), &t);
    h.run();
    assert_eq!(h.s.engine().market.conns[usize::from(A.0)], s7);
}

#[test]
fn seq_120_liveness_from_keyframe() {
    let mut h = base();
    h.push(RingId::Rep(0), &md(A, 1, ConnStateKind::Resync, InstId(0), 1_500));
    h.run();
    let kf = h.keyframe();
    for k in 0..200u64 {
        if k % 10 == 0 {
            h.push(RingId::Rep(0), &md(A, (k % 3) as u8, ConnStateKind::Alive, InstId(0), 2_000 + k));
        }
        h.push(RingId::Bbo(0), &bbo(A1, A, (k % 2) as u8, "50000", "50002", 800_000 + k, 2_000 + k));
        h.run();
    }
    let m = h.verify_from(Some(kf.seq));
    assert!(m.is_empty(), "{m:?}");
    let _ = EventKind::BBO;
}
