//! NET-335: any bytes as server plaintext, split at fuzz-chosen points into reads through the
//! in-memory peer, then `poll_rx`/`next_frame`: no panic, no out-of-bounds, bounded work, and
//! every payload lies in the receive buffer or the fragment area.
#![no_main]

#[path = "../../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;

use libfuzzer_sys::fuzz_target;
use support::mempeer::{cfg, deliver, pair_cfg};

fn net_335_fuzz_ws_deframe(data: &[u8]) {
    // First byte: number of split points (0–15); then 2 bytes per split point; the rest is plaintext.
    let (n, rest) = match data.split_first() {
        Some((n, r)) => ((*n & 15) as usize, r),
        None => return,
    };
    if rest.len() < 2 * n {
        return;
    }
    let (cuts, plain) = rest.split_at(2 * n);
    if plain.is_empty() {
        return;
    }
    let mut k = cfg();
    k.rx_buf = 65_536;
    k.max_msg = 16_384;
    let (mut conn, mut peer) = pair_cfg(k);
    let cipher = peer.split(plain, &[16_384; 8]);
    let mut points: Vec<usize> = cuts.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]]) as usize % cipher.len().max(1)).collect();
    points.sort();
    points.dedup();
    let mut prev = 0;
    for p in points.into_iter().chain(std::iter::once(cipher.len())) {
        if p > prev {
            deliver(&mut conn, 0, 0, cipher[prev..p].to_vec());
            prev = p;
        }
    }
    let (lo, hi) = conn.test_rx_range();
    for _ in 0..64 {
        conn.poll_rx(0);
        for _ in 0..100_000 {
            let Some(f) = conn.next_frame() else { break };
            let a = f.payload.as_ptr() as usize;
            if !f.reassembled {
                assert!(a >= lo && a + f.payload.len() <= hi);
            }
            assert!(f.payload.len() <= 16_384);
        }
        conn.finish_poll(0);
    }
}

fuzz_target!(|data: &[u8]| net_335_fuzz_ws_deframe(data));
