//! CARRY-009: real SDK entry points; setup, reset and assertions are outside the timed region.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    dead_code,
    unused_imports
)]
#[path = "../tests/signals.rs"]
mod scenario;
use criterion::{Criterion, criterion_group, criterion_main};
use hft_strategy_sdk::ctx::Op;
use hft_strategy_sdk::prelude::*;
use hft_strategy_sdk::testing::{MockViews, ev};
use portfolio_carry::PortfolioCarry;
use scenario::common::*;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn held(v: &mut MockViews) {
    let mut pos = GroupPosView::EMPTY;
    pos.n_legs = 2;
    pos.legs[0].qty = Qty(-10_000_000);
    pos.legs[1].qty = Qty(10_000_000);
    MockViews::group_pos(v, A1, G4, pos);
}

fn measure(
    c: &mut Criterion,
    name: &str,
    mut s: PortfolioCarry,
    v: MockViews,
    e: ev::Ev,
    targets: usize,
    risk_exit: bool,
) {
    let mut snapshot = SnapshotWriter::new(4096);
    s.snapshot(&mut snapshot);
    let mut bufs = CtxBufs::new();
    c.bench_function(name, |b| {
        b.iter_custom(|n| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..n {
                // Every sample must execute the requested path, including timer/entry work.
                s.restore(&mut SnapshotReader::with_desc_version(
                    snapshot.as_bytes(),
                    PortfolioCarry::DESC.version,
                ))
                .unwrap();
                bufs.begin();
                let mut ctx = bufs.ctx();
                let event = e.as_ref();
                let start = Instant::now();
                s.on_event(black_box(&event), black_box(&v), &mut ctx);
                elapsed += start.elapsed();
                black_box(ctx);
                assert_eq!(bufs.full_count(), 0);
                assert_eq!(
                    bufs.ops().filter(|(_, op)| matches!(op, Op::Target(_))).count(),
                    targets
                );
                if targets != 0 {
                    assert!(bufs.ops().any(|(_, op)| matches!(op, Op::Target(t)
                        if t.legs[0].qty.0 < 0 && t.legs[0].qty.0 == -t.legs[1].qty.0)));
                }
                if risk_exit {
                    // MarginStress must latch a real exit, observed on the next group event.
                    bufs.begin();
                    s.on_event(&ev::timer(1, G4, 0).as_ref(), &v, &mut bufs.ctx());
                    assert!(bufs.ops().any(|(_, op)| matches!(op, Op::Target(t)
                        if t.reason == TargetReason::Exit && t.legs.iter().all(|l| l.qty.0 == 0))));
                }
            }
            elapsed
        })
    });
}

fn carry_009_entries(c: &mut Criterion) {
    let (s, mut v, _) = scenario::setup();
    v.conn(I1, Tsc(1_000_000_000), false, false);
    let e = ev::bbo(I2, 0, v.bbo_of(I2));
    measure(c, "strategies/portfolio-carry/tick_miss", s, v, e, 0, false);

    let (s, v, _) = scenario::setup();
    let e = ev::bbo(I2, 0, v.bbo_of(I2));
    measure(c, "strategies/portfolio-carry/tick_fire", s, v, e, 1, false);

    let (s, v, _) = scenario::setup();
    measure(
        c,
        "strategies/portfolio-carry/target_set",
        s,
        v,
        ev::timer(1, G4, 0),
        1,
        false,
    );

    let (s, mut v, _) = scenario::setup();
    held(&mut v);
    let e = ev::bbo(I2, 0, v.bbo_of(I2));
    measure(c, "strategies/portfolio-carry/hold", s, v, e, 0, false);

    let (s, mut v, _) = scenario::setup();
    held(&mut v);
    let e = ev::exec(
        0,
        ExecUpdate {
            group: G4,
            tag: 1,
            origin: TargetOrigin::Strategy,
            kind: ExecUpdateKind::Done {
                cleared_by_engine: false,
            },
            target: [Qty(-10_000_000), Qty(10_000_000), Qty(0), Qty(0)],
            filled: [Qty(-10_000_000), Qty(10_000_000), Qty(0), Qty(0)],
            inflight: [Qty(0); 4],
        },
    );
    measure(c, "strategies/portfolio-carry/exec_update", s, v, e, 0, false);

    let (s, mut v, _) = scenario::setup();
    held(&mut v);
    let e = ev::risk(
        A1,
        RiskEvent::MarginStress {
            venue: V2,
            level: MarginLevel::L1,
            liq_dist: Rate(10_000_000),
            ratio: Rate(900_000_000),
            deadline_tsc: Tsc(0),
        },
    );
    measure(c, "strategies/portfolio-carry/risk_event", s, v, e, 0, true);
}
criterion_group!(benches, carry_009_entries);
criterion_main!(benches);
