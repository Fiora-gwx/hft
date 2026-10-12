//! hft-oms benchmarks (README "延迟与资源契约"; OMS-440–OMS-455).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::too_many_arguments)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::{ExternalCause, OrderEventKind};
use hft_oms::*;
use hft_strategy_sdk::{IntentClass, OrderRef};
use hft_types::{GROUP_NONE, Qty, RejectBits, Side, Tif, Tsc};

#[path = "../tests/common/mod.rs"]
mod common;

use common::*;

/// Times `op` over `iters` calls, rebuilding the state with `setup` every `per` calls (not timed).
fn custom<S>(
    iters: u64,
    per: u64,
    mut setup: impl FnMut() -> S,
    mut op: impl FnMut(&mut S, u64),
) -> Duration {
    let mut total = Duration::ZERO;
    let mut done = 0;
    while done < iters {
        let mut s = setup();
        let n = per.min(iters - done);
        let t0 = Instant::now();
        for i in 0..n {
            op(&mut s, i);
        }
        total += t0.elapsed();
        done += n;
    }
    total
}

fn big() -> H {
    H::with(|c| c.slots.set(VS, A1, 65_535))
}

fn nogroup(it: ApprovedIntent) -> ApprovedIntent {
    let mut it = it;
    it.intent.group = GROUP_NONE;
    it
}

fn placed(n: u64, ack: bool) -> (H, Vec<OrderRef>) {
    let mut h = H::with(|c| c.slots.set(VS, A1, (n as u32).max(64)));
    let it = nogroup(buy_x());
    let rs = (0..n)
        .map(|i| {
            let r = h.place_ok(&it, 0);
            if ack {
                h.ack(r, i + 1, 0);
            }
            r
        })
        .collect();
    (h, rs)
}

fn oms_440_place(c: &mut Criterion) {
    let it = nogroup(buy_x());
    c.bench_function("oms/place", |b| {
        b.iter_custom(|n| {
            custom(n, 60_000, big, |h, _| {
                h.out.clear();
                black_box(h.o.place(&it, ms(1), 1, &mut h.out)).ok();
            })
        })
    });
}

fn oms_441_on_event_ack(c: &mut Criterion) {
    c.bench_function("oms/on_event_ack", |b| {
        b.iter_custom(|n| {
            custom(
                n,
                20_000,
                || placed(20_000, false),
                |(h, rs), i| {
                    let e = h.oe(rs[i as usize], OrderEventKind::Ack);
                    let hd = h.hdr(1);
                    h.out.clear();
                    h.o.on_order_event(black_box(&e), &hd, &mut h.out);
                },
            )
        })
    });
}

fn oms_442_on_event_fill(c: &mut Criterion) {
    c.bench_function("oms/on_event_fill", |b| {
        b.iter_custom(|n| {
            custom(
                n,
                20_000,
                || placed(20_000, true),
                |(h, rs), i| {
                    let mut e = h.oe(rs[i as usize], OrderEventKind::PartialFill);
                    e.cum_filled = q("0.004");
                    e.avg_px = p("60000.00");
                    let hd = h.hdr(1);
                    h.out.clear();
                    h.o.on_order_event(black_box(&e), &hd, &mut h.out);
                },
            )
        })
    });
}

fn oms_443_leglink_onfill(c: &mut Criterion) {
    let setup = || {
        let mut h = H::new();
        let (f, _) = place_first(&mut h, q("1000"), Tif::Gtc, plan_l1(), 0);
        h.ack(f, 1, 0);
        (h, f)
    };
    c.bench_function("oms/leglink_onfill", |b| {
        b.iter_custom(|n| {
            custom(n, 50_000, setup, |(h, f), i| {
                let mut e = h.oe(*f, OrderEventKind::PartialFill);
                e.cum_filled = Qty(100_000 * (i as i64 + 1));
                e.avg_px = p("60000.00");
                e.fee_asset = USDT;
                let hd = h.hdr(1);
                h.out.clear();
                h.o.on_order_event(&e, &hd, &mut h.out);
                black_box(h.out.hedges.len());
            })
        })
    });
}

fn hedge_req(link: u32) -> HedgeRequest {
    HedgeRequest {
        link_id: link,
        role: hft_strategy_sdk::LinkRole::Second,
        venue: VX,
        account: A1,
        side: Side::Sell,
        reduce_only: false,
        inst: Y,
        group: g1(),
        strategy: hft_types::StrategyId(1),
        qty: q("0.012"),
        basis: q("0.012"),
        hedge_cap_qty: q("0.013"),
        ref_px: p("60010.0"),
        peer_venue: VS,
        peer_account: A1,
        trigger_seq: 1,
    }
}

fn oms_444_price_hedge(c: &mut Criterion) {
    let req = hedge_req(1);
    let plan = plan_l1();
    let book = bbo(p("59990.0"), p("60010.0"), ms(10));
    let l = lim();
    c.bench_function("oms/price_hedge", |b| {
        b.iter(|| black_box(price_hedge(black_box(&req), &plan, &book, ms(10), &l, 10, MS)))
    });
}

fn linked_owing() -> (H, u32) {
    let mut h = H::with(|c| c.hedge_deadline_ms = u32::MAX);
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    (h, l)
}

fn oms_445_hedge_gated(c: &mut Criterion) {
    c.bench_function("oms/hedge_gated", |b| {
        b.iter_custom(|n| {
            custom(n, 100_000, linked_owing, |(h, l), i| {
                h.out.clear();
                h.o.hedge_gated(
                    *l,
                    HedgeGate::Risk(RejectBits::KILLED),
                    Tsc(0),
                    ms(1 + i),
                    1,
                    &mut h.out,
                );
            })
        })
    });
}

fn oms_446_unknown_query(c: &mut Criterion) {
    let setup = || {
        let mut h = H::new();
        let s = h.place_ok(&nogroup(buy_x()), 0);
        h.what(s, OrderEventKind::OutcomeUnknown, 0);
        let id = OmsTimerReq::unknown_query_id(h.o.table_view(VS, A1).unwrap().table_ix, s.slot);
        (h, id)
    };
    c.bench_function("oms/unknown_query", |b| {
        b.iter_custom(|n| {
            custom(n, 100_000, setup, |(h, id), i| {
                h.out.clear();
                h.o.on_unknown_query(*id, ms(1000 + i), 1, &mut h.out);
            })
        })
    });
}

fn oms_447_tomb_late_fill(c: &mut Criterion) {
    let setup = || {
        let mut h = H::with(|c| c.slots.set(VS, A1, 1));
        let s = h.place_ok(&buy_x(), 0);
        h.what(s, OrderEventKind::CancelAck, 0);
        h.sweep(5001);
        (h, s)
    };
    c.bench_function("oms/tomb_late_fill", |b| {
        b.iter_custom(|n| {
            custom(n, 100_000, setup, |(h, s), i| {
                let mut e = h.oe(*s, OrderEventKind::PartialFill);
                e.cum_filled = Qty(1000 * (i as i64 + 1));
                e.avg_px = p("60000.00");
                let hd = h.hdr(6000);
                h.out.clear();
                h.o.on_order_event(&e, &hd, &mut h.out);
            })
        })
    });
}

fn oms_448_leg_adjust(c: &mut Criterion) {
    c.bench_function("oms/leg_adjust", |b| {
        b.iter_custom(|n| {
            custom(n, 1_000_000, H::new, |h, i| {
                if i % 2 == 0 {
                    h.out.clear();
                    h.o.external_adjust(A1, VX, g1(), Y, Qty(100), 0, ExternalCause::Adl, &mut h.out);
                } else {
                    h.o.net_adjust(A1, g1(), X, Qty(-100));
                }
            })
        })
    });
}

fn oms_449_sweep_256(c: &mut Criterion) {
    c.bench_function("oms/sweep_256", |b| {
        b.iter_custom(|n| {
            custom(
                n,
                10_000,
                || placed(256, true).0,
                |h, i| {
                    h.out.clear();
                    h.o.on_sweep(ms(10 + i), &mut h.out);
                },
            )
        })
    });
}

fn resume64() -> H {
    let mut h = H::with(|c| c.links_per_account = 128);
    for _ in 0..64 {
        let (f, _) = place_first(&mut h, q("0.001"), Tif::Ioc, plan_l1(), 498_800);
        h.fill(f, q("0.001"), p("60000.00"), 498_800);
    }
    h.out.clear();
    h.o.recovery_end(ms(10_000), ms(500_000), HZ, HZ, 1, &mut h.out);
    h
}

fn oms_450_hedge_gate_open_64(c: &mut Criterion) {
    c.bench_function("oms/hedge_gate_open_64", |b| {
        b.iter_custom(|n| {
            custom(n, 1, resume64, |h, _| {
                h.out.clear();
                h.o.on_hedge_gate_open(A1, VX, ms(30_000), 1, &mut h.out);
            })
        })
    });
}

fn oms_451_continue_16(c: &mut Criterion) {
    let setup = || {
        let mut g = H::with(|c| c.slots.set(VG, A1, 4096));
        let it = intent(
            VG,
            A1,
            Z,
            Side::Sell,
            Tif::Gtc,
            p("60100.0"),
            q("0.001"),
            GROUP_NONE,
        );
        for i in 0..4096u64 {
            let r = g.place_ok(&it, 0);
            g.ack(r, i + 1, 0);
        }
        g.out.clear();
        g.o.cancel_all(CancelScope::VenueAccount(VG, A1), false, ms(1), 1, &mut g.out);
        g
    };
    c.bench_function("oms/continue_16", |b| {
        b.iter_custom(|n| {
            custom(n, 250, setup, |h, _| {
                h.out.clear();
                h.o.on_continue(ms(2), 2, &mut h.out);
            })
        })
    });
}

fn oms_452_cancel_resting_8(c: &mut Criterion) {
    let setup = || {
        let (h, _) = placed(8, true);
        h
    };
    c.bench_function("oms/cancel_resting_8", |b| {
        b.iter_custom(|n| {
            custom(n, 1, setup, |h, _| {
                h.out.clear();
                h.o.cancel_resting_on(A1, X, ms(1), 1, &mut h.out);
            })
        })
    });
}

fn oms_453_smoke(c: &mut Criterion) {
    let hot = *rd().inst(X);
    let book = bbo(p("60000.00"), p("60000.01"), ms(1));
    let req = SmokeReq {
        account: A1,
        venue: VS,
        inst: X,
        side: Side::Buy,
        ttl_ms: 60_000,
    };
    c.bench_function("oms/smoke", |b| {
        b.iter_custom(|n| {
            custom(n, 1, H::new, |h, _| {
                let it = h.o.smoke_intent(&req, &book, &hot, 2000, ms(1)).unwrap();
                let mut a =
                    ApprovedIntent::new(it, Origin::Command, hft_types::StrategyId(1), IntentClass::Open);
                a.smoke_ttl_ms = 60_000;
                h.out.clear();
                let _ = h.o.place(&a, ms(1), 1, &mut h.out);
                h.out.clear();
                h.o.on_smoke_ttl(A1, VS, false, ms(61_000), 2, &mut h.out);
            })
        })
    });
}

fn oms_454_write_off(c: &mut Criterion) {
    c.bench_function("oms/write_off", |b| {
        b.iter_custom(|n| {
            custom(n, 1_000_000, H::new, |h, _| {
                h.o.test_set_net(A1, g1(), X, q("0.0004"));
                black_box(h.o.write_off_residual(A1, g1())).ok();
            })
        })
    });
}

fn oms_455_recovery_end_4096(c: &mut Criterion) {
    let setup = || {
        let mut h = H::with(|c| {
            c.slots.set(VS, A1, 4096);
            c.links_per_account = 4096;
        });
        for _ in 0..4096 {
            let _ = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 1);
        }
        h
    };
    c.bench_function("oms/recovery_end_4096", |b| {
        b.iter_custom(|n| {
            custom(n, 1, setup, |h, _| {
                h.out.clear();
                h.o.recovery_end(ms(10_000), ms(500_000), HZ, HZ, 1, &mut h.out);
            })
        })
    });
}

fn oms_474_foreign_cash_provenance_64(c: &mut Criterion) {
    let mut h = H::new();
    let mut e = hft_event::ExternalFill {
        venue: VX,
        account: A1,
        side: Side::Sell,
        cause: ExternalCause::Unknown,
        inst: Y,
        qty: Qty(1),
        px: p("60000"),
        fee: Qty(0),
        fee_asset: USDT,
        _pad: [0; 6],
        exch_order_id: 1,
        trade_id: 1,
        ex_ts_ns: 0,
        rx_tsc: Tsc(0),
        fill_seq: 0,
        cash_qty: Qty(1),
        cash_amount: Qty(0),
    };
    for id in 1..=64 {
        e.exch_order_id = id;
        h.o.remember_external_cash(&e).unwrap();
    }
    c.bench_function("oms/foreign_cash_seen", |b| {
        b.iter(|| black_box(h.o.has_foreign_cash(VX, A1, Y, black_box(64))))
    });
    c.bench_function("oms/external_cash_guard", |b| {
        b.iter(|| black_box(h.o.external_cash_guard(black_box(&e))))
    });
    c.bench_function("oms/remember_external_cash", |b| {
        b.iter(|| black_box(h.o.remember_external_cash(black_box(&e))))
    });
    {
        use hft_event::{ReconcileBatchHdr, ReconcileBatchView, ReconcileReport};
        use zerocopy::{FromZeros, IntoBytes};
        let mut summary = ReconcileReport::new_zeroed();
        summary.report_id = 7;
        summary.account = A1;
        summary.venue = VX;
        summary.count = 2;
        summary.item = 5;
        let mut p = summary;
        p.idx = 1;
        p.item = 1;
        p.inst = Y;
        p.engine_qty = Qty(1);
        p.sflags = ReconcileReport::AUTO_EXTERNAL;
        let hdr = ReconcileBatchHdr {
            report_id: 7,
            n_reports: 2,
            n_fills: 1,
            account: A1,
            venue: VX,
            _pad: 0,
        };
        let bytes = [hdr.as_bytes(), summary.as_bytes(), p.as_bytes(), e.as_bytes()].concat();
        let batch = ReconcileBatchView::read(&bytes).unwrap();
        c.bench_function("oms/reconcile_cash_guard", |b| {
            b.iter(|| black_box(h.o.reconcile_cash_guard(black_box(&batch))).unwrap())
        });
    }
}

criterion_group!(
    benches,
    oms_440_place,
    oms_441_on_event_ack,
    oms_442_on_event_fill,
    oms_443_leglink_onfill,
    oms_444_price_hedge,
    oms_445_hedge_gated,
    oms_446_unknown_query,
    oms_447_tomb_late_fill,
    oms_448_leg_adjust,
    oms_449_sweep_256,
    oms_450_hedge_gate_open_64,
    oms_451_continue_16,
    oms_452_cancel_resting_8,
    oms_453_smoke,
    oms_454_write_off,
    oms_474_foreign_cash_provenance_64
);
criterion_group! {
    name = slow;
    config = Criterion::default().sample_size(10);
    targets = oms_455_recovery_end_4096
}
criterion_main!(benches, slow);
