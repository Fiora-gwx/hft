//! Real strategy entry points; fixture setup, the per-sample state reset and output inspection are
//! outside the timed region. `tick_fire` is an armed instance's book update that sends the spend
//! buy, `tick_miss` an idle instance's book update, `exec_update` a partial fill of the open buy,
//! `risk_event` an ignored risk event.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: bench code

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use common::{A1, D, G1, U, filled, r_of};
use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::ev;
use pm_taker_probe::fast::Phase;

fn measure(c: &mut Criterion, name: &str, phase: Phase, event: ev::Ev) {
    let mut d = D::new(&[]);
    let mut bufs = CtxBufs::new();
    c.bench_function(name, |b| {
        b.iter_custom(|n| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..n {
                d.s.fast.phase = phase;
                d.s.fast.r = Some(r_of(1));
                bufs.begin();
                let mut ctx = bufs.ctx();
                let event = event.as_ref();
                let start = Instant::now();
                d.s.on_event(black_box(&event), black_box(&d.v), &mut ctx);
                elapsed += start.elapsed();
                black_box(ctx);
            }
            elapsed
        });
    });
}

fn on_event(c: &mut Criterion) {
    measure(
        c,
        "strategies/pm-taker-probe/tick_miss",
        Phase::Idle,
        ev::book(U, 0),
    );
    measure(
        c,
        "strategies/pm-taker-probe/tick_fire",
        Phase::Armed,
        ev::book(U, 0),
    );
    measure(
        c,
        "strategies/pm-taker-probe/exec_update",
        Phase::BuyLive,
        ev::order(
            0,
            OrderUpdate {
                r: r_of(1),
                group: G1,
                tag: 1,
                kind: filled(5_000_000, false),
            },
        ),
    );
    measure(
        c,
        "strategies/pm-taker-probe/risk_event",
        Phase::BuyLive,
        ev::risk(A1, RiskEvent::HedgeBroken { group: G1 }),
    );
}

criterion_group!(benches, on_event);
criterion_main!(benches);
