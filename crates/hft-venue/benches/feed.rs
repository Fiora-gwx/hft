//! VEN-350, VEN-351, VEN-360, VEN-366, VEN-367: feed-task benchmarks (README "延迟与资源契约").
//! Scripted connections (hft-net `test-io`): the frames are encrypted outside the timed region;
//! only `poll_once` is timed.
//! (CI check: bench smoke after freeing runner disk.)
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: benchmark driver

#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;

use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use hft_net::ConnId;
use hft_net::sock::RecvStep;
use hft_venue::FeedRoutes;
use support::feedh::*;
use support::rd::*;
use support::stub;
use support::ws_ref;

/// Times `poll_once` with one frame (`payload(k)`) queued on connection 0 before each call.
fn frame_bench(c: &mut Criterion, name: &str, dup: bool) {
    stub::log_enabled(false);
    let (mut h, now) = bbo_up(|cfg| cfg.bbo_conns = 1, &[S1]);
    let mut t = now;
    let mut u = 1_000_000u64;
    c.bench_function(name, |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                t += 1;
                if !dup {
                    u += 1;
                }
                let bytes = h
                    .net
                    .peer(ConnId(0))
                    .frames(&[ws_ref::text(&support::feedh::b("S1", u))]);
                let s = h.rt.conn(ConnId(0)).unwrap().script().unwrap();
                s.push_recv(RecvStep::data(t, t, bytes));
                s.idle_tsc = t;
                let t0 = Instant::now();
                std::hint::black_box(h.rt.poll_once(t));
                total += t0.elapsed();
                h.drain_bbo();
            }
            total
        })
    });
    stub::log_enabled(true);
}

fn ven_350_feed_bbo_path(c: &mut Criterion) {
    frame_bench(c, "venue_feed_bbo_path", false);
}

fn ven_351_feed_dup_drop(c: &mut Criterion) {
    frame_bench(c, "venue_feed_dup_drop", true);
}

/// Times idle `poll_once` rounds of a `routes = Bbo` task with `n` connections.
fn idle_bench(c: &mut Criterion, name: &str, n: u8, ready: bool) {
    stub::log_enabled(false);
    let (mut h, now) = if ready {
        let cfg = hft_venue::FeedCfg {
            routes: FeedRoutes::Bbo,
            ready: true,
            bbo_conns: n,
            ..f0()
        };
        let mut h = feed(cfg);
        for i in 0..n {
            h.net.set_ready(ConnId(i), true);
        }
        let mut now = 100;
        h.up(&[S1], hft_venue::streams::BBO, &mut now);
        for i in 0..n {
            h.net.set_ready(ConnId(i), false);
        }
        (h, now)
    } else {
        bbo_up(|cfg| cfg.bbo_conns = n, &[S1])
    };
    let mut t = now;
    c.bench_function(name, |b| {
        b.iter(|| {
            t += 1;
            std::hint::black_box(h.rt.poll_once(t))
        })
    });
    stub::log_enabled(true);
}

fn ven_360_bbo_live_tick(c: &mut Criterion) {
    // One BBO connection, no data: the round is the connection's health maintenance plus its
    // empty read.
    idle_bench(c, "venue_bbo_live_tick", 1, false);
}

fn ven_366_idle_loop_bbo(c: &mut Criterion) {
    idle_bench(c, "venue_idle_loop_bbo", 2, false);
}

fn ven_367_idle_loop_bbo_ready(c: &mut Criterion) {
    idle_bench(c, "venue_idle_loop_bbo_ready", 6, true);
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(2)).warm_up_time(Duration::from_millis(300));
    targets = ven_350_feed_bbo_path, ven_351_feed_dup_drop, ven_360_bbo_live_tick, ven_366_idle_loop_bbo, ven_367_idle_loop_bbo_ready
);
criterion_main!(benches);
