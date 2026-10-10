//! hft-risk benchmarks (README "延迟与资源契约"; RSK-250–RSK-264).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_risk::mode::ModeInput;
use hft_risk::{EquityView, LegSummary, Origin, Risk};
use hft_strategy_sdk::IntentClass;
use hft_types::{CapSource, GroupId, Notional, Rate, Side, Tif};

#[path = "../tests/common/mod.rs"]
mod common;

use common::*;

/// Token refill step: one token per 100 ms on the 10 s / 1 min windows of R0.
const STEP: i64 = 100 * MS;

fn rsk_250_classify(c: &mut Criterion) {
    let w = r0();
    let it = open(1, G1, B1, Side::Buy, "0.01", "50000.0", Tif::Gtc);
    let d = draft(&it, IntentClass::Reduce);
    let pos = w.mv.pos_facts(1, B1);
    let caps = w.rd.caps(B);
    c.bench_function("risk/classify", |x| {
        x.iter(|| black_box(Risk::classify(black_box(&d), Origin::Strategy, &pos, caps)))
    });
}

/// `evaluate` + `commit` of `it` with engine_ns advancing so every iteration passes.
fn pass_loop(c: &mut Criterion, name: &str, w: &mut W, it: &hft_risk::IntentView) {
    let mut t = w.now;
    c.bench_function(name, |x| {
        x.iter(|| {
            t += STEP;
            w.mv.tsc = t as u64;
            let now = now_at(t);
            if let Ok(a) = w.risk.evaluate(black_box(it), &w.mv, &now) {
                w.risk.commit(&a, &now);
            }
        })
    });
}

fn rsk_251_check_pass_open(c: &mut Criterion) {
    let mut w = r0();
    let it = open(1, G1, A1, Side::Buy, "0.0001", "50000.00", Tif::Gtc);
    pass_loop(c, "risk/check_pass_open", &mut w, &it);
}

fn rsk_252_check_pass_linked_open(c: &mut Criterion) {
    let mut w = r0();
    let it = link_open(
        1,
        G1,
        A1,
        Side::Buy,
        "0.001",
        "50000.00",
        (1, B, B1, Side::Sell, "0.001", "50"),
    );
    pass_loop(c, "risk/check_pass_linked_open", &mut w, &it);
}

fn rsk_253_check_reject_kill(c: &mut Criterion) {
    let mut w = r0();
    w.ok(kill_global(true));
    let it = open(1, G1, A1, Side::Buy, "0.0001", "50000.00", Tif::Gtc);
    w.mv.tsc = w.now as u64;
    let now = w.nowv();
    c.bench_function("risk/check_reject_kill", |x| {
        x.iter(|| black_box(w.risk.evaluate(black_box(&it), &w.mv, &now)))
    });
}

fn rsk_254_check_pass_hedge(c: &mut Criterion) {
    let mut w = r0();
    let it = hedge(1, G1, B1, Side::Sell, "0.001", "49995.0", "0.001");
    pass_loop(c, "risk/check_pass_hedge", &mut w, &it);
}

fn rsk_255_mode_apply(c: &mut Criterion) {
    let mut w = r0();
    // Fill the cap table to 256 records with Halted caps of distinct cells.
    let mut k = 1u32;
    while w.risk.mode.caps().len() < 254 && k < 4_096 {
        let _ = w.apply(halted(1, 1, GroupId(0x0100_0000 | (k + 16))));
        k += 1;
    }
    let add = kill_acct(2, false);
    let del = resume(CapSource::KILL, sc_acct(2));
    w.mm.sync(&w.risk);
    let mut seq = w.seq;
    c.bench_function("risk/mode_apply", |x| {
        x.iter(|| {
            seq += 1;
            let r = w.risk.mode.apply(
                black_box(if seq.is_multiple_of(2) { add } else { del }),
                seq,
                &w.mm,
                &mut w.out,
            );
            black_box(r)
        })
    });
}

fn rsk_256_effective(c: &mut Criterion) {
    let w = r0();
    let k = cell(1, 1, G1);
    c.bench_function("risk/effective", |x| {
        x.iter(|| black_box(w.risk.mode.effective(black_box(k))))
    });
}

fn rsk_264_check_mode(c: &mut Criterion) {
    let mut w = r0();
    w.ok(kill_acct(2, false));
    w.mm.sync(&w.risk);
    let input = resume(CapSource::KILL, sc_acct(2));
    c.bench_function("risk/check_mode", |x| {
        x.iter(|| black_box(w.risk.check_mode(black_box(&input), &w.mm)))
    });
}

fn rsk_257_on_pnl(c: &mut Criterion) {
    let mut w = r0();
    let eq = EquityView {
        total: Notional(200_000_000_000),
        valid: true,
    };
    let p = Notional(-1_000_000);
    c.bench_function("risk/on_pnl", |x| {
        x.iter(|| black_box(w.risk.on_pnl(acc(1), black_box(p), eq)))
    });
}

fn rsk_258_on_legs(c: &mut Criterion) {
    let mut w = r0();
    let s = LegSummary {
        naked_ratio: Rate(100_000_000),
        naked_notional: Notional(50_000_000),
        naked_filled_notional: Notional(0),
        quiescent: true,
    };
    c.bench_function("risk/on_legs", |x| {
        x.iter(|| black_box(w.risk.on_legs(acc(1), G1, black_box(&s))))
    });
}

fn rsk_259_on_margin(c: &mut Criterion) {
    let mut w = r0();
    let s = msample("0.50", "0.10", T0);
    let now = now_at(T0);
    c.bench_function("risk/on_margin", |x| {
        x.iter(|| {
            w.risk.on_margin(acc(1), B, black_box(&s), &now, &mut w.mo);
            black_box(&w.mo);
        })
    });
}

fn rsk_260_hedge_px_limits(c: &mut Criterion) {
    let w = r0();
    c.bench_function("risk/hedge_px_limits", |x| {
        x.iter(|| black_box(w.risk.hedge_px_limits(acc(1), B, black_box(B1))))
    });
}

fn rsk_261_leverage(c: &mut Criterion) {
    let w = r0();
    c.bench_function("risk/leverage", |x| {
        x.iter(|| black_box(w.risk.leverage(acc(1), B, black_box(B1))))
    });
}

fn rsk_262_view_for(c: &mut Criterion) {
    let w = r0();
    let now = w.nowv();
    c.bench_function("risk/view_for", |x| {
        x.iter(|| black_box(w.risk.view_for(acc(1), black_box(A1), &now)))
    });
}

fn rsk_263_on_account_cfg(c: &mut Criterion) {
    let mut w = r0();
    let h = hdr(1, B, 0, KEYS);
    let levs: Vec<_> = (0..1_024)
        .map(|k| lev(if k % 2 == 0 { B1 } else { B2 }, "3.00"))
        .collect();
    c.bench_function("risk/on_account_cfg", |x| {
        x.iter(|| w.risk.on_account_cfg(black_box(&h), &levs))
    });
}

criterion_group!(
    benches,
    rsk_250_classify,
    rsk_251_check_pass_open,
    rsk_252_check_pass_linked_open,
    rsk_253_check_reject_kill,
    rsk_254_check_pass_hedge,
    rsk_255_mode_apply,
    rsk_256_effective,
    rsk_257_on_pnl,
    rsk_258_on_legs,
    rsk_259_on_margin,
    rsk_260_hedge_px_limits,
    rsk_261_leverage,
    rsk_262_view_for,
    rsk_263_on_account_cfg,
    rsk_264_check_mode
);
criterion_main!(benches);

#[allow(dead_code)]
fn unused(_: ModeInput<'static>) {}
