//! NET-285: steady-state receive and send make no allocation (`tls_mode = unbuffered`).
//! Only the connection's calls are counted; the in-process server (rustls) runs outside them.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod support;

use hft_net::sock::RecvStep;
use hft_net::{MaskedFrame, Opcode};
use hft_testkit::CountingAlloc;
use support::mempeer::pair;
use support::ws_ref::text;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

const ROUNDS: u32 = 1_000_000;
const WARMUP: u32 = 1_000;
/// Server records encrypted per batch (outside the counted calls).
const BATCH: usize = 1024;

#[test]
fn net_285_steady_state_zero_alloc() {
    assert!(CountingAlloc::is_installed());
    let (mut conn, mut peer) = pair();
    let frame = text(&[b'm'; 296]);
    let mut out = Box::new(MaskedFrame::<2048>::new());
    out.body_mut()[..292].fill(b'o');
    conn.script().unwrap().sent.reserve(1 << 20);
    let mut queued: std::collections::VecDeque<Vec<u8>> = std::collections::VecDeque::with_capacity(BATCH);
    let mut allocs = 0u64;
    let mut frames = 0u64;
    for round in 0..ROUNDS + WARMUP {
        if queued.is_empty() {
            let recs: Vec<&[u8]> = (0..BATCH).map(|_| frame.as_slice()).collect();
            let all = peer.records(&recs);
            for c in all.chunks(frame.len() + 22) {
                queued.push_back(c.to_vec());
            }
        }
        let rec = queued.pop_front().unwrap();
        conn.script()
            .unwrap()
            .push_recv(RecvStep::data(round as u64, round as u64 + 1, rec));
        out.unseal();
        out.seal(Opcode::Text, 292, round);
        let s = CountingAlloc::scope();
        conn.poll_rx(round as u64);
        while let Some(f) = conn.next_frame() {
            std::hint::black_box(f.payload);
            frames += 1;
        }
        let _ = conn.send_frame(&*out).unwrap();
        conn.finish_poll(round as u64);
        let n = s.allocs();
        if round >= WARMUP {
            allocs += n;
        }
        conn.script().unwrap().sent.clear();
    }
    let s = CountingAlloc::scope();
    for _ in 0..100_000 {
        conn.warm(&*out);
    }
    allocs += s.allocs();
    assert_eq!(frames, (ROUNDS + WARMUP) as u64);
    assert_eq!(allocs, 0);
}
