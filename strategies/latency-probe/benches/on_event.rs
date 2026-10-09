//! latency-probe `on_event` benchmarks (README "延迟与资源契约"; LPR-072–LPR-077). Each case measures
//! only the `on_event` call; the state reset between iterations is outside the timed region.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: bench code

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use common::*;
use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::{
    CapScopeView, CtxBufs, ExecUpdate, ExecUpdateKind, OrderUpdate, RiskEvent, Strategy, TargetOrigin,
};
use hft_types::{CapSource, InstId, Qty, StrategyId, VenueId};

/// One timed `on_event`.
fn timed(h: &mut H, bufs: &mut CtxBufs, e: &ev::Ev) -> Duration {
    let r = e.as_ref();
    bufs.begin();
    let mut ctx = bufs.ctx();
    let t = Instant::now();
    h.s.on_event(&r, &h.v, &mut ctx);
    let d = t.elapsed();
    black_box(&ctx);
    d
}

fn bbo_ev(h: &H, i: InstId) -> ev::Ev {
    let mut b = h.bbo_view(i);
    b.rx_tsc = h.now().tsc;
    ev::bbo(i, h.gix_containing(i), b)
}

fn lpr_072_tick_miss(c: &mut Criterion) {
    let mut h = H::new(&[], vec![gq()]);
    h.bbo(I2);
    let mut bufs = CtxBufs::new();
    let mut k = 0i64;
    c.bench_function("strategies/latency-probe/tick_miss", |b| {
        b.iter_custom(|n| {
            let mut d = Duration::ZERO;
            for _ in 0..n {
                k += 1;
                h.at_ens(E0 + (k % 900) * 1_000_000);
                let e = bbo_ev(&h, I2);
                d += timed(&mut h, &mut bufs, &e);
            }
            d
        })
    });
}

fn lpr_073_tick_fire(c: &mut Criterion) {
    let mut h = H::new(&[], vec![gq().with(f::PROBE_PERIOD_MS, 100)]);
    let mut bufs = CtxBufs::new();
    let mut t = 0i64;
    c.bench_function("strategies/latency-probe/tick_fire", |b| {
        b.iter_custom(|n| {
            let mut d = Duration::ZERO;
            for _ in 0..n {
                t += 100;
                h.at(t);
                let e = bbo_ev(&h, I2);
                d += timed(&mut h, &mut bufs, &e);
                // Finish the probe (untimed) so the next tick fires again.
                h.order(GQ, r_of(1, V2), probe_tag(0), cancelled());
            }
            d
        })
    });
}

fn lpr_074_ack_cancel(c: &mut Criterion) {
    let mut h = H::new(&[], vec![gq().with(f::PROBE_PERIOD_MS, 100)]);
    let mut bufs = CtxBufs::new();
    let mut t = 0i64;
    c.bench_function("strategies/latency-probe/ack_cancel", |b| {
        b.iter_custom(|n| {
            let mut d = Duration::ZERO;
            for _ in 0..n {
                t += 100;
                h.at(t);
                let r = h.bbo(I2).refs.first().copied().unwrap_or(r_of(1, V2));
                let e = ev::order(
                    0,
                    OrderUpdate {
                        r,
                        group: GQ,
                        tag: probe_tag(0),
                        kind: acked(),
                    },
                );
                d += timed(&mut h, &mut bufs, &e);
                h.order(GQ, r, probe_tag(0), cancelled());
            }
            d
        })
    });
}

fn lpr_075_target_set(c: &mut Criterion) {
    let mut h = H::new(&[], vec![gts()]);
    h.bbo(I2);
    let mut bufs = CtxBufs::new();
    let mut t = 0i64;
    c.bench_function("strategies/latency-probe/target_set", |b| {
        b.iter_custom(|n| {
            let mut d = Duration::ZERO;
            for _ in 0..n {
                t += 1;
                h.at(t);
                h.timer(GTS, 1);
                let e = bbo_ev(&h, I1);
                d += timed(&mut h, &mut bufs, &e);
                // Back to Idle (untimed): target reached, flat at FLAT.
                h.exec(
                    GTS,
                    ExecUpdateKind::Done {
                        cleared_by_engine: false,
                    },
                );
                h.timer(GTS, 2);
            }
            d
        })
    });
}

fn exec_ev(kind: ExecUpdateKind) -> ev::Ev {
    ev::exec(
        0,
        ExecUpdate {
            group: GTS,
            tag: 0,
            origin: TargetOrigin::Strategy,
            kind,
            target: [Qty(0); 4],
            filled: [Qty(0); 4],
            inflight: [Qty(0); 4],
        },
    )
}

fn lpr_076_exec_update(c: &mut Criterion) {
    let mut h = H::new(&[], vec![gts()]);
    h.bbo(I2);
    h.timer(GTS, 1);
    h.bbo(I1);
    let mut bufs = CtxBufs::new();
    let e = exec_ev(ExecUpdateKind::Progress {
        leg_ix: 0,
        d: Qty(1_000),
    });
    c.bench_function("strategies/latency-probe/exec_update", |b| {
        b.iter_custom(|n| (0..n).map(|_| timed(&mut h, &mut bufs, &e)).sum())
    });
}

fn lpr_077_risk_event(c: &mut Criterion) {
    let mut h = H::new(&[], vec![gq()]);
    h.bbo(I2);
    let mut bufs = CtxBufs::new();
    let scope = CapScopeView {
        strategy: StrategyId(1),
        venue: VenueId(0),
        account: T1,
        group: GQ,
        inst: InstId(0),
    };
    let set = ev::risk(
        T1,
        RiskEvent::CapSet {
            source: CapSource::HALTED,
            scope,
        },
    );
    let clr = ev::risk(
        T1,
        RiskEvent::CapCleared {
            source: CapSource::HALTED,
            scope,
        },
    );
    let mut k = 0u64;
    c.bench_function("strategies/latency-probe/risk_event", |b| {
        b.iter_custom(|n| {
            let mut d = Duration::ZERO;
            for _ in 0..n {
                k += 1;
                d += timed(&mut h, &mut bufs, if k.is_multiple_of(2) { &set } else { &clr });
            }
            d
        })
    });
    let _ = <latency_probe::LatencyProbe as Strategy>::DESC;
}

fn lpr_082_slow_update(c: &mut Criterion) {
    // No slow side: the B leg's Bbo of a pair group (Idle, PAIR_TICK already registered).
    let mut h = H::new(&[], vec![gps()]);
    h.bbo(I2);
    let mut bufs = CtxBufs::new();
    let mut k = 0i64;
    c.bench_function("strategies/latency-probe/slow_update", |b| {
        b.iter_custom(|n| {
            let mut d = Duration::ZERO;
            for _ in 0..n {
                k += 1;
                h.at_ens(E0 + (k % 900) * 1_000_000);
                let e = bbo_ev(&h, I2);
                d += timed(&mut h, &mut bufs, &e);
            }
            d
        })
    });
}

fn lpr_083_advice(c: &mut Criterion) {
    // No Model: an Advice is never delivered; the strategy ignores it.
    let mut h = H::new(&[], vec![gq()]);
    h.bbo(I2);
    let mut bufs = CtxBufs::new();
    let e = ev::advice_raw(0, 1, &[1, 0, 0, 0, 0, 0, 0, 0], 1, hft_types::Tsc(1));
    c.bench_function("strategies/latency-probe/advice", |b| {
        b.iter_custom(|n| (0..n).map(|_| timed(&mut h, &mut bufs, &e)).sum())
    });
}

criterion_group!(
    benches,
    lpr_072_tick_miss,
    lpr_073_tick_fire,
    lpr_074_ack_cancel,
    lpr_075_target_set,
    lpr_076_exec_update,
    lpr_077_risk_event,
    lpr_082_slow_update,
    lpr_083_advice
);
criterion_main!(benches);
