//! hft-exec benchmarks (README "延迟与资源契约"; EXE-480–EXE-494). Each iteration restores the
//! measured cell (a `Copy` of `TargetCell` / `AcctState`) so that it repeats the same work.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_exec::*;
use hft_strategy_sdk::{BrokenReason, ExternalCause, MarginLevel, TargetState, Views};
use hft_types::{HedgePxLimits, InstId, Px, Rate, Side};

#[path = "../tests/common/mod.rs"]
mod common;

use common::*;

fn exe_480_exec_step_idle(c: &mut Criterion) {
    let mut fx = Fx::new();
    c.bench_function("exec_step/idle", |x| x.iter(|| fx.ex.touch_inst(black_box(Q))));
}

fn exe_481_exec_step_hold(c: &mut Criterion) {
    let mut fx = Fx::new();
    let o = fx.go(&t1("-10")).orders[0];
    fx.placed(&o);
    let now = Views::now(&fx.v.m);
    c.bench_function("exec_step/hold", |x| {
        x.iter(|| {
            fx.ex.touch_cell(A1, G8);
            fx.ex.advance(&fx.v, now, &mut fx.out);
            fx.out.clear();
        })
    });
}

/// Advance of (a, g) from the saved cell state, `n` times per iteration batch.
fn step_from(c: &mut Criterion, name: &str, fx: &mut Fx, g: hft_types::GroupId, kind: ExecOrderKind) {
    let saved = fx.cell(A1, g);
    let now = Views::now(&fx.v.m);
    *fx.cm(A1, g) = saved;
    fx.ex.touch_cell(A1, g);
    fx.ex.advance(&fx.v, now, &mut fx.out);
    assert!(
        fx.out.orders.iter().any(|o| o.kind == kind),
        "{name}: no {kind:?}"
    );
    fx.out.clear();
    c.bench_function(name, |x| {
        x.iter(|| {
            *fx.ex.cell_mut(A1, g).unwrap() = saved;
            fx.ex.touch_cell(A1, g);
            fx.ex.advance(&fx.v, now, &mut fx.out);
            fx.out.clear();
        })
    });
}

fn exe_482_exec_step_slice_pair(c: &mut Criterion) {
    let mut fx = Fx::new();
    fx.book(S, "99.90", "100.10", "40", 10);
    fx.book(P, "99.80", "100.20", "50", 10);
    fx.spot(A1, A, G7, "0", "100000");
    inject(&mut fx, &t0("10", "-10"));
    step_from(c, "exec_step/slice_pair", &mut fx, G7, ExecOrderKind::PairFirst);
}

fn exe_483_exec_step_auto_fix(c: &mut Criterion) {
    let mut fx = Fx::new();
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    {
        let cm = fx.cm(A1, G7);
        cm.view.state = TargetState::Broken(BrokenReason::LinkBroken);
        cm.auto_fix.phase = AutoFixPhase::Primed as u8;
        cm.auto_fix.due_tsc = t(0);
    }
    step_from(c, "exec_step/auto_fix", &mut fx, G7, ExecOrderKind::AutoFix);
}

fn exe_484_set_target(c: &mut Criterion) {
    let mut fx = Fx::new();
    let ts = [t0("10", "-10"), t0("11", "-11")];
    let now = Views::now(&fx.v.m);
    let mut k = 0usize;
    c.bench_function("exec/set_target", |x| {
        x.iter(|| {
            k ^= 1;
            let r = fx
                .ex
                .set_target(I1, black_box(&ts[k]), false, &fx.v, now, &mut fx.out);
            fx.out.clear();
            r
        })
    });
}

fn exe_485_on_link(c: &mut Criterion) {
    let mut fx = Fx::new();
    let o = fx.go(&t0("10", "-10")).orders[0];
    let rf = fx.placed(&o);
    let mut l = link(1, A1, G7, LinkPhase::Hedging, LinkBrokenCause::None);
    l.first = rf;
    let now = Views::now(&fx.v.m);
    c.bench_function("exec/on_link", |x| {
        x.iter(|| {
            fx.ex.on_link(black_box(&l), now, &mut fx.out);
            fx.out.clear();
        })
    });
}

fn exe_486_on_external(c: &mut Criterion) {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    let f = xfill(G7, 1, P, B, Side::Sell, "0.5", ExternalCause::Manual);
    let saved = fx.cell(A1, G7);
    let now = Views::now(&fx.v.m);
    c.bench_function("exec/on_external", |x| {
        x.iter(|| {
            *fx.ex.cell_mut(A1, G7).unwrap() = saved;
            let r = fx.ex.on_external(A1, black_box(&f), &fx.v, now, &mut fx.out);
            fx.out.clear();
            r
        })
    });
}

fn exe_487_on_risk_64(c: &mut Criterion) {
    let mut fx = fx_many(64);
    let evs = [ms_margin(MarginLevel::L1), ms_margin(MarginLevel::None)];
    let now = Views::now(&fx.v.m);
    let mut k = 0usize;
    c.bench_function("exec/on_risk_64", |x| {
        x.iter(|| {
            k ^= 1;
            fx.ex.on_risk(A1, black_box(&evs[k]), now);
        })
    });
}

fn exe_488_resolve_broken(c: &mut Criterion) {
    let mut fx = Fx::new();
    inject(&mut fx, &t0("10", "-10"));
    fx.legs(A1, G7, &["1.00", "-1.5"]);
    c.bench_function("exec/resolve_broken", |x| {
        x.iter(|| fx.ex.resolve_broken(A1, black_box(G7), &fx.v))
    });
}

fn exe_489_drain_ready_64(c: &mut Criterion) {
    let fx = fx_many(64);
    c.bench_function("exec/drain_ready_64", |x| {
        x.iter(|| black_box(fx.ex.drain_ready()) ^ black_box(fx.ex.engine_reduce_pending()))
    });
}

fn exe_490_force_step_8(c: &mut Criterion) {
    let mut fx = fx_many(8);
    fx.risk(A1, |r| r.margin_level = MarginLevel::L3);
    for k in 0..8 {
        fx.legs(A1, many(k), &["5.00", "-5.0"]);
        fx.spot(A1, A, many(k), "5", "5");
    }
    fx.cm(A1, many(7)).view.state = TargetState::Broken(BrokenReason::LinkBroken);
    let cells: Vec<TargetCell> = (0..8).map(|k| fx.cell(A1, many(k))).collect();
    let acct = *fx.acct(A1);
    let now = Views::now(&fx.v.m);
    let frac = rate("0.2");
    c.bench_function("exec/force_step_8", |x| {
        x.iter(|| {
            for (k, cell) in cells.iter().enumerate() {
                *fx.ex.cell_mut(A1, many(k as u32)).unwrap() = *cell;
            }
            *fx.ex.acct_mut(A1).unwrap() = acct;
            fx.ex.force_deleverage(A1, B, frac, now, &mut fx.out);
            fx.ex.advance(&fx.v, now, &mut fx.out);
            fx.out.clear();
        })
    });
}

fn exe_491_force_ungrouped_512(c: &mut Criterion) {
    let mut insts = default_insts();
    let extra: Vec<InstId> = (0..512u32).map(|k| InstId(41 + k)).collect();
    for i in &extra {
        insts.push(inst_p(*i, B, true, "0.1"));
    }
    let mut fx = Fx::with_cfg(cfg(c0()), vec![g7(), g8()], insts);
    for (k, i) in extra.iter().enumerate() {
        fx.mark(*i, "100");
        if k % 64 == 0 {
            fx.perp(A1, B, *i, "-3");
        }
    }
    let cell = fx.cell(A1, GNONE);
    let acct = *fx.acct(A1);
    let now = Views::now(&fx.v.m);
    let frac = rate("0.2");
    c.bench_function("exec/force_ungrouped_512", |x| {
        x.iter(|| {
            *fx.ex.cell_mut(A1, GNONE).unwrap() = cell;
            *fx.ex.acct_mut(A1).unwrap() = acct;
            fx.ex.force_deleverage(A1, B, frac, now, &mut fx.out);
            fx.ex.advance(&fx.v, now, &mut fx.out);
            fx.out.clear();
        })
    });
}

fn exe_492_price_cap(c: &mut Criterion) {
    let fx = Fx::new();
    let h = *fx.rd.inst(P);
    let b = Views::bbo(&fx.v.m, P);
    let lim = HedgePxLimits {
        conn_liveness_ms: 2000,
        rr_max_cross: rate("0.01"),
    };
    let (mc, cap) = (rate("0.005"), usd("1000"));
    c.bench_function("exec/price_cap", |x| {
        x.iter(|| {
            let px = hft_types::cap_cross_px(
                Side::Buy,
                black_box(b.ask_px),
                Px::mid_floor(b.bid_px, b.ask_px),
                mc,
                &lim,
                h.tick,
            );
            calc::qty_under_cap(black_box(cap), px, Rate(1_000_000_000), &h)
        })
    });
}

fn exe_493_recovery_end_1024(c: &mut Criterion) {
    // 1 021 groups + one ungrouped cell per account = 1 024 cells.
    let mut fx = fx_many(1021);
    assert_eq!(fx.ex.cell_count(), 1024);
    for k in 0..1021 {
        let cm = fx.cm(A1, many(k));
        cm.wake_at = t(u64::from(k));
        cm.ext_deadline = t(5000);
    }
    let hz = 1_000_000_000_000;
    c.bench_function("exec/recovery_end_1024", |x| {
        x.iter(|| fx.ex.recovery_end(black_box(t(10)), t(0), hz, hz))
    });
}

fn exe_494_pair_lot(c: &mut Criterion) {
    c.bench_function("exec/pair_lot", |x| {
        x.iter(|| hft_types::pair_lot(black_box(300_000), black_box(200_000), black_box(3), black_box(2)))
    });
}

criterion_group!(
    benches,
    exe_480_exec_step_idle,
    exe_481_exec_step_hold,
    exe_482_exec_step_slice_pair,
    exe_483_exec_step_auto_fix,
    exe_484_set_target,
    exe_485_on_link,
    exe_486_on_external,
    exe_487_on_risk_64,
    exe_488_resolve_broken,
    exe_489_drain_ready_64,
    exe_490_force_step_8,
    exe_491_force_ungrouped_512,
    exe_492_price_cap,
    exe_493_recovery_end_1024,
    exe_494_pair_lot
);
criterion_main!(benches);
