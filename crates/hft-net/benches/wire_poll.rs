//! NET-320/NET-322: paired buffered-receive profile and the SPSC handoff operations used by gw.
//! Scripted I/O counts API calls, not measured Linux syscalls. TLS encryption/setup is untimed.
#![allow(clippy::disallowed_macros)] // allow-list: benchmark reporting outside hot paths

#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;
use hft_testkit::bench::HistBench;
use std::hint::black_box;
use support::{mempeer, ws_ref};

const WARMUP: u64 = 5_000;
const SAMPLES: u64 = 50_000;

fn receive(buffered: bool, payload: usize) {
    let (mut conn, mut peer) = mempeer::pair();
    let frame = ws_ref::text(&vec![b'x'; payload]);
    let mut bytes = Vec::with_capacity((WARMUP + SAMPLES) as usize * (payload + 32));
    for _ in 0..WARMUP + SAMPLES {
        bytes.extend(peer.records(&[&frame]));
    }
    mempeer::deliver(&mut conn, 100, 120, bytes);
    let mode = if buffered { "buffered" } else { "eager" };
    let name = format!("net_wire_poll/{payload}/{mode}");
    let h = HistBench::smoke(&name)
        .warmup(WARMUP)
        .iters(SAMPLES)
        .run(|| {
            if buffered {
                conn.poll_rx_if_needed(0);
            } else {
                conn.poll_rx(0);
            }
            black_box(conn.next_frame().unwrap().payload.len());
        })
        .unwrap();
    println!(
        "{}",
        serde_json::json!({"name":name,"n":h.n,"warmup":h.warmup,
        "p50_ns":h.p50_ns,"p99_ns":h.p99_ns,"p999_ns":h.p999_ns,"max_ns":h.max_ns,
        "scripted_recv_calls_per_message":conn.sys().recvmsg as f64 / (WARMUP + SAMPLES) as f64})
    );
}

fn handoff(batch: bool) {
    let (mut tx, mut rx) = hft_rt::spsc::<[u64; 8]>(128, hft_rt::Backing::Heap).unwrap();
    let mode = if batch { "batch" } else { "item" };
    let name = format!("net_wire_handoff/{mode}");
    let h = HistBench::smoke(&name)
        .warmup(WARMUP)
        .iters(SAMPLES)
        .run(|| {
            let mut w = tx.write_batch(16);
            for _ in 0..16 {
                assert!(w.push(black_box([42; 8])));
            }
            w.commit();
            if batch {
                let r = rx.read_batch(16);
                let (a, b) = r.slices();
                for x in a.iter().chain(b) {
                    black_box(*x);
                }
                r.release();
            } else {
                for _ in 0..16 {
                    black_box(rx.try_pop().unwrap());
                }
            }
        })
        .unwrap();
    println!(
        "{}",
        serde_json::json!({"name":name,"n":h.n,"warmup":h.warmup,
        "messages_per_sample":16,"p50_ns":h.p50_ns,"p99_ns":h.p99_ns,
        "p999_ns":h.p999_ns,"max_ns":h.max_ns})
    );
}

fn main() {
    for payload in [274, 2048] {
        receive(false, payload);
        receive(true, payload);
    }
    handoff(false);
    handoff(true);
}
