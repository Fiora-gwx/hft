//! hft-ledger benchmarks (README "延迟与资源契约"; LED-250–LED-258).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: benchmark code, not a hot path

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use hft_event::{ExternalCause, OrderEventKind, RecordFlags, decode_record, encode_fixed};
use hft_ledger::{FixedVec, FundingReconItem, JournalLedger, Ledger, PerpCashSettlement};
use hft_refdata::FundingSettleKind;
use hft_strategy_sdk::SnapshotReader;
use hft_types::{INST_NONE, Notional, Px, Qty};
use zerocopy::FromZeros;

#[path = "../tests/common/mod.rs"]
mod common;

use common::journal::{CLOCK_HZ_MILLI, decision_new, epoch_start, order_event};
use common::*;

fn led_250_apply_fill(c: &mut Criterion) {
    let mut l = booted();
    let buy = f(Buy, BTC_S1, "0.001", "60000", "0.06", USDT, G1, 0);
    let sell = f(Sell, BTC_S1, "0.001", "60010", "0.06", USDT, G1, 0);
    let mut k = 0i64;
    c.bench_function("ledger/apply_fill", |b| {
        b.iter(|| {
            k += 1;
            let x = if k & 1 == 0 { &buy } else { &sell };
            black_box(l.apply_fill(black_box(x), s(1) + k));
        })
    });

    let mut l = booted();
    let buy = f(Buy, BTC_P2, "0.001", "60000", "0.06", USDT, G1, 1);
    let sell = f(Sell, BTC_P2, "0.001", "60010", "0.06", USDT, G1, 1);
    let opened = PerpCashSettlement {
        asset: USDT,
        amount: Qty(0),
    };
    let closed = PerpCashSettlement {
        asset: USDT,
        amount: usdt("0.01"),
    };
    let mut k = 0i64;
    c.bench_function("ledger/apply_settled_fill", |b| {
        b.iter(|| {
            k += 1;
            let (fill, cash) = if k & 1 == 1 {
                (&buy, opened)
            } else {
                (&sell, closed)
            };
            black_box(l.apply_settled_fill(black_box(fill), black_box(cash), s(1) + k)).expect("valid cash");
        })
    });
}

fn led_300_apply_spot_fill(c: &mut Criterion) {
    let mut l = booted();
    let buy = f(Buy, BTC_S1, "0.006", "60001.6667", "0", USDT, G1, 0);
    let sell = f(Sell, BTC_S1, "0.006", "60001.6667", "0", USDT, G1, 0);
    let mut k = 0i64;
    c.bench_function("ledger/apply_spot_fill", |b| {
        b.iter(|| {
            k += 1;
            let fill = if k & 1 == 1 { &buy } else { &sell };
            black_box(l.apply_spot_fill(black_box(fill), black_box(360_010_000_000_000), s(1) + k))
                .expect("valid spot notional");
        })
    });
}

fn led_251_apply_external(c: &mut Criterion) {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    let mut e = ext(ExternalCause::Liquidation, Buy, BTC_P2, "0.001", "60000");
    let mut k = 0u64;
    c.bench_function("ledger/apply_external", |b| {
        b.iter(|| {
            k += 1;
            e.trade_id = k;
            e.side = if k & 1 == 0 { Buy } else { Sell };
            black_box(l.apply_external(black_box(&e), false, s(2) + k as i64));
            l.clear_notes();
        })
    });
    e.cash_qty = e.qty;
    e.cash_amount = hft_types::Qty(0);
    c.bench_function("ledger/apply_settled_external", |b| {
        b.iter(|| {
            k += 1;
            e.trade_id = k;
            e.side = if k & 1 == 0 { Buy } else { Sell };
            black_box(l.apply_settled_external(black_box(&e), s(2) + k as i64)).expect("valid cash");
            l.clear_notes();
        })
    });
}

fn led_294_reconciled_cash(c: &mut Criterion) {
    use hft_event::{ReconcileBatchHdr, ReconcileBatchView, ReconcileReport};
    use zerocopy::IntoBytes;
    let mut e = ext_tid(ExternalCause::Adl, Buy, BTC_P2, "0.001", "60000", 71);
    e.cash_qty = e.qty;
    c.bench_function("ledger/apply_reconciled_external", |b| {
        b.iter_batched_ref(
            booted,
            |l| {
                black_box(l.apply_reconciled_external(black_box(&e), s(2))).unwrap();
            },
            BatchSize::PerIteration,
        )
    });
    let rd = rd_with(&RdOpts {
        p2_cash: true,
        ..RdOpts::default()
    });
    let l = booted_with(rd);
    let mut summary = ReconcileReport::new_zeroed();
    summary.report_id = 7;
    summary.account = A1;
    summary.venue = P2;
    summary.count = 2;
    summary.item = 5;
    let mut p = summary;
    p.idx = 1;
    p.item = 1;
    p.inst = BTC_P2;
    p.engine_qty = Qty(-e.qty.0);
    p.sflags = ReconcileReport::AUTO_EXTERNAL;
    let hdr = ReconcileBatchHdr {
        report_id: 7,
        n_reports: 2,
        n_fills: 1,
        account: A1,
        venue: P2,
        _pad: 0,
    };
    let bytes = [hdr.as_bytes(), summary.as_bytes(), p.as_bytes(), e.as_bytes()].concat();
    let batch = ReconcileBatchView::read(&bytes).unwrap();
    assert!(l.validate_reconcile_batch(&batch).is_ok());
    c.bench_function("ledger/validate_reconcile_batch", |b| {
        b.iter(|| black_box(l.validate_reconcile_batch(black_box(&batch))).unwrap())
    });
}

fn bop() -> Ledger {
    let mut l = ledger_with(rd_funding(FundingSettleKind::BalanceOnlyPush), &cfg_lf());
    boot_a1(&mut l, 0);
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l
}

fn led_252_apply_funding_push(c: &mut Criterion) {
    let mut l = bop();
    // Fill the push-seen ring first (64 entries).
    for k in 0..64 {
        let mut p = push(INST_NONE, "0", T1);
        p.amount = Notional(1_000_000 + k);
        l.apply_funding_push(&p, s(2));
    }
    let mut p = push(INST_NONE, "0", T1);
    let mut k = 0i64;
    c.bench_function("ledger/apply_funding_push", |b| {
        b.iter(|| {
            k += 1;
            p.amount = Notional(2_000_000 + k);
            p.settle_ts_ns = D + (k % 16 + 1) * 8 * H;
            black_box(l.apply_funding_push(black_box(&p), s(3) + k));
            l.clear_notes();
        })
    });
}

fn led_253_reconcile_funding(c: &mut Criterion) {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.on_funding_interval(BTC_P2, 3_600);
    // 48 hourly records inside the window.
    for h in 1..=48 {
        l.apply_funding(&push(BTC_P2, "1.0", D + h * H), s(2));
    }
    let now = D + 49 * H;
    let mut k = 0i64;
    c.bench_function("ledger/reconcile_funding", |b| {
        b.iter(|| {
            k += 1;
            let it = FundingReconItem {
                account: A1,
                venue: P2,
                inst: BTC_P2,
                settle_ts_ns: D + (k % 48 + 1) * H,
                exch_amount: Notional(1_000_000 + (k & 1) * 20_000),
                engine_amount: Notional(1_000_000),
                report_id: k as u64,
            };
            black_box(l.reconcile_funding(black_box(&it), now, s(3) + k));
            l.clear_notes();
        })
    });
}

fn led_254_funding_window_sweep(c: &mut Criterion) {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.on_funding_interval(BTC_P2, 1_800);
    // A full no-history table (64 items before the watermark), none expiring.
    for k in 1..=64 {
        let it = FundingReconItem {
            account: A1,
            venue: P2,
            inst: BTC_P2,
            settle_ts_ns: D - k * 1_800_000_000_000,
            exch_amount: Notional(1_000_000),
            engine_amount: Notional(0),
            report_id: 1,
        };
        l.reconcile_funding(&it, D + H, s(2));
    }
    l.clear_notes();
    let mut out = FixedVec::new();
    let mut k = 0i64;
    c.bench_function("ledger/funding_window_sweep", |b| {
        b.iter(|| {
            k += 1;
            l.funding_window_sweep(A1, P2, black_box(D + H), s(3) + k, &mut out);
            black_box(out.len());
        })
    });
}

fn led_255_on_ref_px(c: &mut Criterion) {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0", USDT, G1, 0), s(1));
    let mut k = 0i64;
    c.bench_function("ledger/on_ref_px", |b| {
        b.iter(|| {
            k += 1;
            let p = 600_000_000 + (k & 15) * 100;
            l.on_ref_px(BTC_S1, black_box(Px(p)), Px(p + 100), s(2) + k);
        })
    });
}

fn led_256_on_mark(c: &mut Criterion) {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.apply_fill(&f(Sell, ETH_P2, "10.0", "3000", "0", USDT, G3, 0), s(1));
    mark(&mut l, ETH_P2, "3000", s(1));
    let items = margin_items(&[(BTC_P2, px("75000"), 2), (ETH_P2, px("3750"), 2)]);
    on_margin(
        &mut l,
        margin_hdr(A1, P2, "50000", "300", "40000", "0.006", 2),
        &items,
        s(2),
    );
    let mut touched = FixedVec::new();
    let mut k = 0i64;
    c.bench_function("ledger/on_mark", |b| {
        b.iter(|| {
            k += 1;
            touched.clear();
            let p = 600_000_000 + (k & 15) * 1_000;
            l.on_mark(BTC_P2, black_box(Px(p)), s(3) + k, &mut touched);
        })
    });
}

fn led_257_equity(c: &mut Criterion) {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0", USDT, G1, 0), s(1));
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    c.bench_function("ledger/equity", |b| b.iter(|| black_box(l.equity(black_box(A1)))));
}

fn enc<B: hft_event::Body>(out: &mut Vec<Vec<u8>>, seq: u64, tsc: u64, b: &B) {
    let mut buf = vec![0u8; 256];
    let len = encode_fixed(&mut buf, seq, tsc, RecordFlags::EVENT_END, b) as usize;
    buf.truncate(len);
    out.push(buf);
}

/// A synthetic journal of `n` fill-heavy records (decisions, partial and full fills).
fn day_journal(n: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(n + 2);
    let mut clock = hft_event::EngineClock::new_zeroed();
    clock.base_engine_ns = s(1);
    clock.tsc_hz_milli = CLOCK_HZ_MILLI;
    enc(&mut out, 0, 0, &clock);
    enc(&mut out, 1, 0, &epoch_start(7));
    let mut seq = 2u64;
    let mut slot = 0u32;
    while out.len() < n {
        slot += 1;
        let side = if slot & 1 == 0 { Buy } else { Sell };
        let tsc = u64::from(slot) * 1_000;
        enc(
            &mut out,
            seq,
            tsc,
            &decision_new(slot, 1, A1, S1, BTC_S1, side, G1, Qty(2_000)),
        );
        seq += 1;
        for (what, cum) in [
            (OrderEventKind::PartialFill, 1_000),
            (OrderEventKind::Fill, 2_000),
        ] {
            enc(
                &mut out,
                seq,
                tsc,
                &order_event(what, 7, slot, 1, A1, S1, BTC_S1, Qty(cum), px("60000")),
            );
            seq += 1;
        }
    }
    out
}

fn led_258_replay_day(c: &mut Criterion) {
    let records = day_journal(10_000);
    let start = snap(&booted());
    c.bench_function("ledger/replay_day", |b| {
        b.iter_batched(
            || {
                let mut r = SnapshotReader::new(&start);
                JournalLedger::from_snapshot(&mut r, rd_lf(), &cfg_lf(), &caps_lf()).expect("from_snapshot")
            },
            |mut j| {
                for rec in &records {
                    if let Ok(rv) = decode_record(rec) {
                        j.apply(&rv).expect("valid journal");
                    }
                }
                black_box(j.ledger().pos(A1, BTC_S1).qty)
            },
            BatchSize::LargeInput,
        )
    });
}

criterion_group!(
    benches,
    led_250_apply_fill,
    led_300_apply_spot_fill,
    led_251_apply_external,
    led_294_reconciled_cash,
    led_252_apply_funding_push,
    led_253_reconcile_funding,
    led_254_funding_window_sweep,
    led_255_on_ref_px,
    led_256_on_mark,
    led_257_equity,
    led_258_replay_day
);
criterion_main!(benches);
