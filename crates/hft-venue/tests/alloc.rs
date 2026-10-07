//! VEN-340: steady-state `poll_once` makes no allocation. Only `poll_once` is counted; the scripted
//! peers (encryption, scheduling) and the taps run outside the counted scope.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::GwOp;
use hft_net::ConnId;
use hft_net::sock::RecvStep;
use hft_testkit::CountingAlloc;
use hft_types::ClientOrderId;
use hft_venue::{FeedCfg, FeedRoutes, streams};
use support::feedh::*;
use support::gwh::*;
use support::rd::*;
use support::stub::{self, StubProto};
use support::ws_ref;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const ROUNDS: u64 = 100_000;
const WARMUP: u64 = 1_000;

fn queue(h: &mut Fh, id: ConnId, payload: &[u8], t: u64) {
    let b = h.net.peer(id).frames(&[ws_ref::text(payload)]);
    h.rt.conn(id)
        .unwrap()
        .script()
        .unwrap()
        .push_recv(RecvStep::data(t, t, b));
}

#[test]
fn ven_340_zero_alloc() {
    assert!(CountingAlloc::is_installed());
    stub::log_enabled(false);
    // .1: FeedRuntime, routes = All: BBO, trades, TopN snapshots, pongs.
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::All,
        ..f0()
    });
    let mut now = 100;
    h.up(&[S1], streams::BBO | streams::BOOK | streams::TRADE, &mut now);
    let ids: Vec<ConnId> = h.rt.conn_ids().collect();
    assert_eq!(ids.len(), 4);
    for &id in &ids {
        h.rt.conn(id).unwrap().script().unwrap().sent.reserve(1 << 22);
    }
    let mut allocs = 0u64;
    for k in 0..ROUNDS + WARMUP {
        let t = now + (k + 1) * 1_000_000;
        let u = k + 1;
        queue(&mut h, ids[0], &b("S1", u), t);
        queue(&mut h, ids[1], &b("S1", u), t);
        queue(
            &mut h,
            ids[2],
            format!("D|S1|{u}|2|100.00 1.000|99.99 2.000|100.01 1.000|100.02 3.000").as_bytes(),
            t,
        );
        queue(&mut h, ids[3], format!("T|S1|{u}|100.00|1.000|B").as_bytes(), t);
        if k % 1000 == 0 {
            for &id in &ids {
                let b = h.net.peer(id).frames(&[ws_ref::server_frame(true, 0xA, &[0; 8])]);
                h.rt.conn(id)
                    .unwrap()
                    .script()
                    .unwrap()
                    .push_recv(RecvStep::data(t, t, b));
            }
        }
        for &id in &ids {
            let s = h.rt.conn(id).unwrap().script().unwrap();
            s.idle_tsc = t;
            s.sent.clear();
        }
        let s = CountingAlloc::scope();
        h.rt.poll_once(t);
        let n = s.allocs();
        if k >= WARMUP {
            allocs += n;
        }
        h.clear();
    }
    assert_eq!(allocs, 0, "VEN-340.1");
    drop(h);

    // .2: GwRuntime: Place, report (Ack + response), Cancel, CancelAck, response.
    let mut g = gw::<StubProto>();
    for id in [ConnId(112), ConnId(113), ConnId(114), ConnId(115)] {
        g.rt.conn(id).unwrap().script().unwrap().sent.reserve(1 << 22);
    }
    let p = ConnId(112);
    let mut allocs = 0u64;
    let mut t = 3_000_000u64;
    for k in 0..ROUNDS + WARMUP {
        t += 250_000_000;
        let slot = (k % 1500) as u32 + 1;
        let r#gen = (k / 1500) as u32 + 1;
        // Round A: a Place goes out.
        let mut c = g.place(A1, slot, r#gen, S1, false);
        c.r#gen = r#gen;
        g.cmd(c);
        let s = CountingAlloc::scope();
        g.rt.poll_once(t);
        let n = s.allocs();
        // Round B: its Ack and response arrive, a Cancel goes out.
        let id = ClientOrderId {
            epoch: 5,
            venue: V7,
            account: A1,
            slot,
            r#gen,
        };
        let cid = stub::cid_text(&id);
        let mut bytes = Vec::new();
        for line in [
            format!("O|{cid}|{}|Ack|0|0|0|0|0|0", 10_000_000 + k),
            format!("R|{slot}-{gen}-1-5|0", gen = r#gen),
        ] {
            bytes.extend(g.net.peer(p).frames(&[ws_ref::text(line.as_bytes())]));
        }
        g.rt.conn(p)
            .unwrap()
            .script()
            .unwrap()
            .push_recv(RecvStep::data(t, t, bytes));
        let cancel = g.targeted(GwOp::Cancel, A1, slot);
        g.cmd(cancel);
        t += 1;
        let s2 = CountingAlloc::scope();
        g.rt.poll_once(t);
        let n2 = s2.allocs();
        // Round C: CancelAck and response.
        let mut bytes = Vec::new();
        for line in [
            format!("O|{cid}|{}|CancelAck|0|0|0|0|0|0", 10_000_000 + k),
            format!("R|{slot}-{gen}-2-5|0", gen = r#gen),
        ] {
            bytes.extend(g.net.peer(p).frames(&[ws_ref::text(line.as_bytes())]));
        }
        g.rt.conn(p)
            .unwrap()
            .script()
            .unwrap()
            .push_recv(RecvStep::data(t, t, bytes));
        t += 1;
        let s3 = CountingAlloc::scope();
        g.rt.poll_once(t);
        let n3 = s3.allocs();
        if k >= WARMUP {
            allocs += n + n2 + n3;
        }
        g.reps();
        for id in [ConnId(112), ConnId(113), ConnId(114), ConnId(115)] {
            if let Some(c) = g.rt.conn(id) {
                let s = c.script().unwrap();
                s.sent.clear();
                s.idle_tsc = t;
            }
        }
    }
    assert!(g.rt.inflight(p) <= 2);
    assert_eq!(allocs, 0, "VEN-340.2");
    stub::log_enabled(true);
}
