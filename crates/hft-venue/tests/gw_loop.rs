//! VEN-207, VEN-208, VEN-212, VEN-244, VEN-245: the gw round.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{GwOp, RepMsg, SendStatus};
use hft_net::ConnId;
use hft_rt::PollTask;
use hft_types::{AccountId, ClientOrderId};
use support::gwh::*;
use support::rd::*;
use support::stub::{self, Stub, StubProto, V_USER_STREAM};

const P: ConnId = ConnId(112);

fn ack_line(slot: u32) -> String {
    let id = ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot,
        r#gen: 1,
    };
    format!("O|{}|EX-{slot}|Ack|0|0|0|0|0|0", stub::cid_text(&id))
}

#[test]
fn ven_207_drain_after_each_frame() {
    let mut h = gw::<StubProto>();
    for s in 1..=64 {
        h.net.push(P, ack_line(s).as_bytes());
    }
    let mut out = h.out.take().unwrap();
    let mut place = h.place(A2, 900, 1, S2, true);
    place.decision_seq = 777;
    let mut n = 0;
    stub::set_report_hook(Some(Box::new(move |_p| {
        n += 1;
        if n == 11 {
            // Frame 10 is processed (and its drain_out ran) before frame 11 is decoded.
        }
        if n == 10 {
            out.try_push(place).unwrap();
        }
    })));
    h.poll(3_000_000);
    stub::set_report_hook(None);
    let v = h.reps();
    let pos_ev = |slot: u32| {
        v.iter()
            .position(|m| matches!(m, RepMsg::OrderEvent(e) if e.slot.0 == slot))
            .unwrap()
    };
    let pos_send = v
        .iter()
        .position(|m| matches!(m, RepMsg::SendReport(r) if r.decision_seq == 777))
        .unwrap();
    assert!(
        pos_ev(10) < pos_send && pos_send < pos_ev(11),
        "{} {} {}",
        pos_ev(10),
        pos_send,
        pos_ev(11)
    );
}

#[test]
fn ven_208_poll_cadence() {
    type U = Stub<2048, V_USER_STREAM>;
    let mut h = gw_with::<U, 2048>(g0(), &[A1], GhOpts::default()).unwrap();
    h.open(1_000, 3);
    let ids = [ConnId(112), ConnId(113), ConnId(114)];
    assert_eq!(h.rt.primary(A1), Some(ids[0]));
    assert_eq!(h.rt.standby(A1), Some(ids[1]));
    let base: Vec<u64> = ids.iter().map(|&i| h.rt.conn(i).unwrap().sys().recvmsg).collect();
    let n0 = h.rt.counters();
    for k in 0..80 {
        h.poll(10_000 + k);
    }
    let d: Vec<u64> = ids
        .iter()
        .zip(base.iter())
        .map(|(&i, b)| h.rt.conn(i).unwrap().sys().recvmsg - b)
        .collect();
    assert_eq!(d, vec![80, 10, 80], "primary, standby, user stream");
    for &i in &ids {
        assert_eq!(h.rt.conn(i).unwrap().sys().epoll_pwait, 0);
    }
    let n1 = h.rt.counters();
    assert_eq!(n1.standby_reads - n0.standby_reads, 10);
    assert_eq!(
        n1.standby_drains - n0.standby_drains,
        n1.standby_reads - n0.standby_reads
    );
}

/// Pushes `n` order commands and polls until `n` `SendReport`s sit unconsumed in the rep ring.
fn fill(h: &mut Gh<StubProto>, n: usize, t: &mut u64) {
    let mut sent = 0;
    while sent < n {
        let c = h.place(A2, 1000 + sent as u32, 1, S2, false);
        h.cmd(c);
        sent += 1;
    }
    for _ in 0..64 {
        *t += 1;
        h.poll(*t);
    }
}

fn push_acct_record(h: &mut Gh<StubProto>) {
    let b = h.acct.reserve(8).unwrap();
    b.copy_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    h.acct.commit();
}

#[test]
fn ven_212_rep_ring_reserve() {
    let opts = GhOpts {
        rep_cap: 64,
        ..GhOpts::default()
    };
    // .1: 17 free.
    let mut h = gw_opts::<StubProto>(opts);
    let mut t = 3_000_000;
    fill(&mut h, 47, &mut t);
    for s in 1..=5 {
        h.net.push(P, ack_line(s).as_bytes());
    }
    push_acct_record(&mut h);
    let rf0 = h.rt.counters().ring_full;
    let ls = h.poll(t + 1);
    let v = h.reps();
    assert_eq!(v.len(), 48, "VEN-212.1: one report frame processed");
    assert_eq!(ls.ring_full, 0, "VEN-212.1");
    assert_eq!(h.rt.counters().ring_full, rf0);
    // .2: 16 free, 3 Places.
    let mut h = gw_opts::<StubProto>(opts);
    let mut t = 3_000_000;
    fill(&mut h, 48, &mut t);
    for s in 1..=5 {
        h.net.push(P, ack_line(s).as_bytes());
    }
    for s in 0..3 {
        let c = h.place(A1, 500 + s, 1, S1, false);
        h.cmd(c);
    }
    let ls = h.poll(t + 1);
    let v = h.reps();
    assert_eq!(events(&v).len(), 0, "VEN-212.2: no report frame read");
    let r = reports(&v);
    assert_eq!(
        r.iter().filter(|x| x.account == A1).count(),
        3,
        "VEN-212.2: the Places went out"
    );
    assert_eq!(ls.ring_full, 1, "VEN-212.2");
    // .3: 0 free.
    let mut h = gw_opts::<StubProto>(opts);
    let mut t = 3_000_000;
    fill(&mut h, 64, &mut t);
    for s in 0..3 {
        let c = h.place(A1, 500 + s, 1, S1, false);
        h.cmd(c);
    }
    let ls = h.poll(t + 1);
    assert_eq!(ls.ring_full, 1, "VEN-212.3");
    assert!(!ls.did_work, "VEN-212.3");
    assert_eq!(h.reps().len(), 64, "VEN-212.3: nothing new");
    assert!(
        PollTask::has_pending(&h.rt),
        "the 3 commands are still in the out ring"
    );
}

#[test]
fn ven_244_drain_when_idle() {
    let mut h = gw::<StubProto>();
    // Reach a round right after an aux round (the next 7 are not aux).
    let mut t = 3_000_000;
    loop {
        let s0 = h.rt.counters().standby_reads;
        t += 1;
        h.poll(t);
        if h.rt.counters().standby_reads > s0 {
            break;
        }
    }
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    let d0 = h.rt.counters().drain_calls;
    h.rt.trace(true);
    h.poll(t + 1);
    assert_eq!(h.rt.counters().drain_calls - d0, 2);
    let tr = h.rt.take_trace();
    assert_eq!(
        tr.first(),
        Some(&hft_venue::Effect::Send(P)),
        "sent in the first drain_out: {tr:?}"
    );
    assert_eq!(statuses(&h.reps()), vec![SendStatus::Written]);
}

#[test]
fn ven_245_has_pending() {
    // .1
    let mut h = gw_opts::<StubProto>(GhOpts {
        rep_cap: 64,
        ..GhOpts::default()
    });
    h.poll(3_000_000);
    h.reps();
    assert!(!PollTask::has_pending(&h.rt), "VEN-245.1");
    // .2
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    assert!(PollTask::has_pending(&h.rt), "VEN-245.2");
    h.poll(3_000_001);
    h.reps();
    // .3
    push_acct_record(&mut h);
    assert!(PollTask::has_pending(&h.rt), "VEN-245.3");
    // .4: a report frame left in the primary's TLS buffer (reading stopped at the reserve).
    let mut h = gw_opts::<StubProto>(GhOpts {
        rep_cap: 64,
        ..GhOpts::default()
    });
    let mut t = 3_000_000;
    fill(&mut h, 47, &mut t);
    for s in 1..=3 {
        h.net.push(P, ack_line(s).as_bytes());
    }
    h.poll(t + 1);
    // 48 records in the ring; free the reserve.
    let mut taken = 0;
    while taken < 10 {
        h.rep.try_pop().unwrap();
        taken += 1;
    }
    assert!(PollTask::has_pending(&h.rt), "VEN-245.4");
    // .5: same buffer, ring back at the reserve.
    fill(&mut h, 10, &mut t);
    assert!(!PollTask::has_pending(&h.rt), "VEN-245.5");
    let _ = (GwOp::Place, AccountId(0));
}
