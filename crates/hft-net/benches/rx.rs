//! NET-320–NET-324: receive-path benchmarks (README "延迟与资源契约"). Memory-peer cases run on
//! every host; loopback cases only on Linux.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};

use support::mempeer::{MemPeer, cfg, deliver, pair_cfg};
use support::ws_ref::text;

/// Records each carrying one text frame of `payload` bytes, `n` of them, pre-encrypted.
fn records(peer: &mut MemPeer, payload: usize, n: usize) -> Vec<Vec<u8>> {
    let f = text(&vec![b'x'; payload]);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(peer.records(&[&f]));
    }
    out
}

/// Times `next_frame` on single-record single-frame input (no system call); the server encrypts
/// each batch outside the timed section.
fn next_frame_mem(c: &mut Criterion, name: &str, payload: usize) {
    let mut k = cfg();
    k.rx_buf = 262_144;
    let (mut conn, mut peer) = pair_cfg(k);
    c.bench_function(name, |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            let mut left = iters;
            while left > 0 {
                let batch = left.min(64);
                let mut bytes = Vec::new();
                for r in records(&mut peer, payload, batch as usize) {
                    bytes.extend(r);
                }
                deliver(&mut conn, 0, 0, bytes);
                conn.poll_rx(0);
                let t = Instant::now();
                for _ in 0..batch {
                    std::hint::black_box(conn.next_frame().map(|f| f.payload.len()));
                }
                total += t.elapsed();
                left -= batch;
            }
            total
        })
    });
}

fn net_320_rx_300b_mem(c: &mut Criterion) {
    // 300 B record: 274 B payload + 4 B header + 22 B TLS overhead.
    next_frame_mem(c, "net_rx_300b_mem", 274);
}

fn net_321_rx_2k_mem(c: &mut Criterion) {
    next_frame_mem(c, "net_rx_2k_mem", 2048);
}

fn net_322_rx_first_of_64k(c: &mut Criterion) {
    let mut k = cfg();
    k.rx_buf = 262_144;
    c.bench_function("net_rx_first_of_64k", |b| {
        b.iter_custom(|iters| {
            let (mut conn, mut peer) = pair_cfg(k);
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                // About 64 KiB of backlog: 64 records of one 1 000 B frame.
                let mut bytes = Vec::new();
                for r in records(&mut peer, 1000, 64) {
                    bytes.extend(r);
                }
                deliver(&mut conn, 0, 0, bytes);
                conn.poll_rx(0);
                let t = Instant::now();
                std::hint::black_box(conn.next_frame().map(|f| f.payload.len()));
                total += t.elapsed();
                while conn.next_frame().is_some() {}
            }
            total
        })
    });
}

#[cfg(target_os = "linux")]
fn loop_conn() -> (
    support::looppeer::LoopPeer,
    hft_net::Connector,
    Box<hft_net::ConnPool<128>>,
    support::looppeer::PeerConn,
) {
    use support::looppeer::*;
    let peer = LoopPeer::start(LoopOpts::default());
    let mut c = connector(peer.port, &[(hft_net::ConnRole::Md, 1)]);
    let (h, pc) = open(&mut c, &peer, 1, hft_net::ConnRole::Md);
    let mut p = pool();
    p.install(h, None).unwrap();
    (peer, c, p, pc)
}

fn net_323_poll_loopback(c: &mut Criterion) {
    let name = "net_poll_loopback";
    #[cfg(target_os = "linux")]
    {
        let (_peer, _c, mut p, mut pc) = loop_conn();
        let conn: &mut hft_net::Conn = p.get(hft_net::ConnId(1)).unwrap();
        c.bench_function(name, |b| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    pc.text(&[b'q'; 274]);
                    let t = Instant::now();
                    let mut got = false;
                    while !got {
                        conn.poll_rx(0);
                        got = conn.next_frame().is_some();
                    }
                    total += t.elapsed();
                }
                total
            })
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (c, name);
    }
}

fn net_324_poll_empty(c: &mut Criterion) {
    let name = "net_poll_empty";
    #[cfg(target_os = "linux")]
    {
        let (_peer, _c, mut p, _pc) = loop_conn();
        let conn: &mut hft_net::Conn = p.get(hft_net::ConnId(1)).unwrap();
        c.bench_function(name, |b| b.iter(|| std::hint::black_box(conn.poll_rx(0))));
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (c, name);
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(2)).warm_up_time(Duration::from_millis(300));
    targets = net_320_rx_300b_mem, net_321_rx_2k_mem, net_322_rx_first_of_64k, net_323_poll_loopback, net_324_poll_empty
);
criterion_main!(benches);
