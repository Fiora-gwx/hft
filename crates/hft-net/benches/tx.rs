//! NET-326–NET-328: send-path benchmarks (README "延迟与资源契约").
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;

use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_net::{MaskedFrame, Opcode};
use support::mempeer::pair;

fn net_326_seal_300b(c: &mut Criterion) {
    let mut f = Box::new(MaskedFrame::<2048>::new());
    f.body_mut()[..292].fill(b's');
    let mut key = 1u32;
    c.bench_function("net_seal_300b", |b| {
        b.iter(|| {
            f.unseal();
            key = key.wrapping_mul(747_796_405).wrapping_add(1);
            f.seal(Opcode::Text, 292, key);
            std::hint::black_box(f.as_bytes().len())
        })
    });
}

fn net_327_encrypt_300b_mem(c: &mut Criterion) {
    // Scripted socket: `send_frame` encrypts once and hands the record to an in-memory sink.
    let (mut conn, _peer) = pair();
    conn.script().unwrap().sent.reserve(1 << 22);
    let mut f = Box::new(MaskedFrame::<2048>::new());
    f.body_mut()[..292].fill(b'e');
    f.seal(Opcode::Text, 292, 7);
    c.bench_function("net_encrypt_300b_mem", |b| {
        b.iter(|| {
            let r = conn.send_frame(&*f);
            let s = conn.script().unwrap();
            if s.sent.len() > 1 << 21 {
                s.sent.clear();
            }
            std::hint::black_box(r.is_ok())
        })
    });
}

fn net_328_send_loopback(c: &mut Criterion) {
    let name = "net_send_loopback";
    #[cfg(target_os = "linux")]
    {
        use support::looppeer::*;
        let peer = LoopPeer::start(LoopOpts::default());
        let mut k = connector(peer.port, &[(hft_net::ConnRole::Order, 1)]);
        let (h, mut pc) = open(&mut k, &peer, 1, hft_net::ConnRole::Order);
        let mut p = pool();
        p.install(h, None).unwrap();
        // The peer drains in the background.
        std::thread::spawn(move || {
            let mut b = vec![0u8; 1 << 16];
            loop {
                if std::io::Read::read(&mut pc.s, &mut b)
                    .map(|n| n == 0)
                    .unwrap_or(false)
                {
                    break;
                }
            }
        });
        let conn = p.get(hft_net::ConnId(1)).unwrap();
        let mut f = Box::new(MaskedFrame::<2048>::new());
        f.body_mut()[..292].fill(b'l');
        f.seal(Opcode::Text, 292, 9);
        c.bench_function(name, |b| {
            b.iter_custom(|iters| {
                let mut total = std::time::Duration::ZERO;
                for _ in 0..iters {
                    // Untimed: wait until the previous frame left tx (the peer drains).
                    while conn.tx_pending() > 0 {
                        conn.finish_poll(0);
                    }
                    let t = std::time::Instant::now();
                    let r = conn.send_frame(&*f);
                    total += t.elapsed();
                    std::hint::black_box(r.is_ok());
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

fn net_330_pool_swap(c: &mut Criterion) {
    use hft_net::{ConnId, ConnPool, Handoff, PoolMode, RotatePolicy};
    use support::mempeer::{Opts, pair_with};
    let mut pool = ConnPool::<2>::new(PoolMode::PrimaryStandby, 2, RotatePolicy::default());
    for id in [1, 2] {
        let (conn, _) = pair_with(Opts {
            id: ConnId(id),
            ..Opts::default()
        });
        pool.install(Handoff::test_from_conn(conn), None).unwrap();
    }
    c.bench_function("net_pool_swap", |b| {
        b.iter(|| std::hint::black_box(pool.swap_primary()))
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(2)).warm_up_time(Duration::from_millis(300));
    targets = net_326_seal_300b, net_327_encrypt_300b_mem, net_328_send_loopback, net_330_pool_swap
);
criterion_main!(benches);
