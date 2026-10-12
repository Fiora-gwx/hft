//! hft-sequencer benchmarks (README "延迟与资源契约"; SEQ-317–SEQ-339). Each benchmark boots the
//! TESTS.md baseline B0 on the live driver over the in-memory `TestIo` (tests/common) and times
//! `Sequencer::step` over pre-queued Inputs; the harness's own ring and journal are part of the
//! measured path (budgets are for the production rings and RAM segment, see budgets.toml).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::too_many_arguments,
    dead_code
)] // allow-list: test code, not a hot path

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::{
    AdviceHdr, ConnRole, ConnStateKind, ControlCmd, EventKind, ExternalCause, ExternalFill, Timer,
};
use hft_sequencer::StepOutcome;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::EventRef;
use hft_types::{AccountId, AssetId, GroupId, InstId, Px, Qty, Side, StrategyId, Tsc};
use zerocopy::{FromZeros, IntoBytes};

#[path = "../tests/common/mod.rs"]
mod common;

use common::*;

fn steady() -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h
}

/// Runs rounds until the queued Inputs are consumed (an idle round ends it).
fn drain(h: &mut H) {
    for _ in 0..100_000 {
        if h.step() != StepOutcome::Busy {
            break;
        }
    }
}

/// Times `per` Inputs per fresh state: `setup` builds the state (not timed), `push` queues one
/// Input (not timed), then the rounds that consume them are timed; returns the time of `iters`
/// Inputs.
fn timed(iters: u64, per: u64, mut setup: impl FnMut() -> H, mut push: impl FnMut(&mut H, u64)) -> Duration {
    let mut total = Duration::ZERO;
    let mut done = 0u64;
    while done < iters {
        let mut h = setup();
        let n = per.min(iters - done);
        for k in 0..n {
            push(&mut h, done + k);
        }
        let t0 = Instant::now();
        drain(&mut h);
        total += t0.elapsed();
        done += n;
    }
    total
}

/// One timed Input per iteration on the same state (push not timed).
fn each(iters: u64, h: &mut H, mut push: impl FnMut(&mut H, u64)) -> Duration {
    let mut total = Duration::ZERO;
    for k in 0..iters {
        push(h, k);
        let t0 = Instant::now();
        drain(h);
        total += t0.elapsed();
    }
    total
}

fn bbo_elem(h: &mut H, i: InstId, bid: &str, ask: &str) {
    h.quote(i, bid, ask);
}

fn advice(s: StrategyId, a: AccountId, g: GroupId, len: usize) -> Vec<u8> {
    let mut hd = AdviceHdr::new_zeroed();
    hd.strategy = s;
    hd.account = a;
    hd.local_kind = 7;
    hd.group = g;
    hd.len = len as u16;
    hd.basis_seq = 1;
    hd.basis_tsc = Tsc(1);
    let mut v = hd.as_bytes().to_vec();
    v.extend(std::iter::repeat_n(0xA5u8, len));
    v
}

fn seq_317_overhead_per_event(c: &mut Criterion) {
    // A Timer{Sys, AdvisorTick} (id 6): append + engine_ns + dispatch + end_event, no component work.
    c.bench_function("seq/overhead_per_event", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, _| {
                let t = h.now();
                h.push(RingId::Hk, &sys_timer(6, t));
            })
        })
    });
}

fn seq_318_head_check(c: &mut Criterion) {
    // An idle round: every ring head read once plus the timer comparison.
    c.bench_function("seq/head_check", |b| {
        let mut h = steady();
        b.iter(|| black_box(h.step()));
    });
}

fn seq_320_conn_alive(c: &mut Criterion) {
    c.bench_function("seq/conn_alive", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, _| {
                let t = h.now();
                h.push(
                    RingId::Rep(0),
                    &conn(
                        A,
                        0,
                        ConnStateKind::Alive,
                        ConnRole::Md,
                        InstId(0),
                        AccountId(0),
                        t,
                    ),
                );
            })
        })
    });
}

fn seq_321_conn_down(c: &mut Criterion) {
    c.bench_function("seq/conn_down", |b| {
        b.iter_custom(|iters| {
            timed(iters, 1, steady, |h, _| {
                let t = h.now();
                h.push(
                    RingId::Rep(0),
                    &conn(
                        A,
                        0,
                        ConnStateKind::Down,
                        ConnRole::Md,
                        InstId(0),
                        AccountId(0),
                        t,
                    ),
                );
            })
        })
    });
}

fn seq_322_bbo_view(c: &mut Criterion) {
    c.bench_function("seq/bbo_view", |b| {
        let h = steady();
        let e = h.s.engine();
        b.iter(|| black_box(e.market.bbo_view(&e.books, black_box(A1))));
    });
}

fn seq_323_notify_continue(c: &mut Criterion) {
    // 64 queued notifications delivered by one NotifyContinue round (empty strategy calls).
    c.bench_function("seq/notify_continue", |b| {
        b.iter_custom(|iters| {
            timed(iters, 1, steady, |h, _| {
                let t = h.now();
                for k in 0..64u32 {
                    let _ = k;
                    h.push(RingId::Hk, &sys_timer(6, t));
                }
            })
        })
    });
}

fn seq_324_bbo_miss(c: &mut Criterion) {
    c.bench_function("seq/bbo_miss", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, k| {
                let ask = if k % 2 == 0 { "50001" } else { "50002" };
                bbo_elem(h, A1, "50000", ask);
            })
        })
    });
}

fn seq_325_bbo_target_idle(c: &mut Criterion) {
    c.bench_function("seq/bbo_target_idle", |b| {
        let mut h = steady();
        h.set_target_s2(target(
            AC2,
            G2,
            &[(0, q("0")), (1, q("0"))],
            hft_strategy_sdk::ExecSpec::TakerSlice(slice0()),
        ));
        h.quote(A1, "50000", "50001");
        h.run();
        clear_hook();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, k| {
                let ask = if k % 2 == 0 { "50001" } else { "50002" };
                bbo_elem(h, A1, "50000", ask);
            })
        })
    });
}

fn fire_setup() -> H {
    let h = steady();
    add_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(open_buy(hx, A1));
        }
    });
    h
}

fn seq_326_bbo_fire(c: &mut Criterion) {
    c.bench_function("seq/bbo_fire", |b| {
        b.iter_custom(|iters| timed(iters, 1, fire_setup, |h, _| bbo_elem(h, A1, "50000", "50001")))
    });
}

fn seq_327_bbo_exec_fire(c: &mut Criterion) {
    c.bench_function("seq/bbo_exec_fire", |b| {
        b.iter_custom(|iters| {
            timed(
                iters,
                1,
                || {
                    let mut h = steady();
                    h.set_target_s2(target(
                        AC2,
                        G2,
                        &[(0, q("0.001")), (1, q("-0.001"))],
                        hft_strategy_sdk::ExecSpec::TakerSlice(slice0()),
                    ));
                    h
                },
                |h, _| bbo_elem(h, A1, "50000", "50001"),
            )
        })
    });
}

fn seq_328_l2batch_20(c: &mut Criterion) {
    // 20-level batch on an inst without a live L2 book (the batch path up to the book).
    c.bench_function("seq/l2batch_20", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, k| {
                let mut hd = hft_event::L2BatchHdr::new_zeroed();
                hd.h.venue = A;
                hd.h.inst = A1;
                hd.h.rx_tsc = h.now();
                hd.prev_id = k;
                hd.last_id = k + 1;
                hd.n_bids = 10;
                hd.n_asks = 10;
                let mut body = hd.as_bytes().to_vec();
                for l in 0..20i64 {
                    let lv = hft_event::Level {
                        px: Px(5_000_000 + if l < 10 { -l } else { l }),
                        qty: Qty(1_000),
                    };
                    body.extend_from_slice(lv.as_bytes());
                }
                h.push_bytes(RingId::Bbo(0), EventKind::L2_BATCH, body);
            })
        })
    });
}

fn seq_329_fill_to_hedge(c: &mut Criterion) {
    c.bench_function("seq/fill_to_hedge", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                reset_script();
                let mut h = b0().boot();
                h.steady();
                let d = {
                    let m = h.mark();
                    add_hook(|hx, ev, ctx| {
                        if let EventRef::Bbo { inst, .. } = ev
                            && *inst == A1
                            && hx.s.strategy == S1
                        {
                            let _ = ctx.place(pair_first(hx, q("0.001")));
                        }
                    });
                    h.quote(A1, "50000", "50001");
                    h.run();
                    clear_hook();
                    decisions(&h.since(m))
                        .into_iter()
                        .find(|x| x.1.inst == A1)
                        .map(|x| x.1)
                };
                let Some(d) = d else { continue };
                h.ack(&d);
                let mut ev = order_event(&d, hft_event::OrderEventKind::Fill, h.epoch());
                ev.qty = d.qty;
                ev.cum_filled = d.qty;
                ev.px = d.px;
                ev.avg_px = d.px;
                let r = h.rep_ring(d.account);
                h.push(r, &ev);
                let t0 = Instant::now();
                drain(&mut h);
                total += t0.elapsed();
            }
            total
        })
    });
}

fn seq_330_advice_route(c: &mut Criterion) {
    c.bench_function("seq/advice_route", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, _| {
                h.push_bytes(RingId::Advice, EventKind::ADVICE, advice(S2, AC2, G2, 512))
            })
        })
    });
}

fn seq_331_margin_report(c: &mut Criterion) {
    c.bench_function("seq/margin_report", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, _| {
                h.margin_report(AC1, "100000", "0", "100000")
            })
        })
    });
}

fn seq_332_external_fill(c: &mut Criterion) {
    c.bench_function("seq/external_fill", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, k| {
                let x = ExternalFill {
                    venue: B,
                    account: AC1,
                    side: if k % 2 == 0 { Side::Buy } else { Side::Sell },
                    cause: ExternalCause::Manual,
                    inst: B1,
                    qty: q("0.001"),
                    px: px("50000"),
                    fee: Qty(0),
                    fee_asset: AssetId(0),
                    _pad: [0; 6],
                    exch_order_id: 0,
                    trade_id: 1_000_000 + k,
                    ex_ts_ns: 0,
                    rx_tsc: Tsc(h.now()),
                    fill_seq: 0,
                    cash_qty: hft_types::Qty(0),
                    cash_amount: hft_types::Qty(0),
                };
                h.push(RingId::Rep(0), &x);
            })
        })
    });
}

fn recon_item(item: u8, idx: u16, count: u16, rid: u64) -> hft_event::ReconcileReport {
    let mut x = hft_event::ReconcileReport::new_zeroed();
    x.report_id = rid;
    x.venue = B;
    x.account = AC1;
    x.item = item;
    x.idx = idx;
    x.count = count;
    x
}

fn seq_333_recon_funding_item(c: &mut Criterion) {
    c.bench_function("seq/recon_funding_item", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, k| {
                let mut x = recon_item(4, 0, 2, 50_000 + k);
                x.inst = B1;
                h.push(RingId::Rep(0), &x);
            })
        })
    });
}

fn seq_334_recon_group(c: &mut Criterion) {
    c.bench_function("seq/recon_group", |b| {
        b.iter_custom(|iters| {
            timed(iters, 1, steady, |h, k| {
                let rid = 60_000 + k;
                for i in 0..256u16 {
                    let mut x = recon_item(1, i, 257, rid);
                    x.inst = B1;
                    h.push(RingId::Rep(0), &x);
                }
                h.push(RingId::Rep(0), &recon_item(5, 256, 257, rid));
            })
        })
    });
}

fn seq_335_inject_warmup(c: &mut Criterion) {
    c.bench_function("seq/inject_warmup", |b| {
        let mut h = steady();
        let payload = vec![7u8; 4_096];
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, _| {
                h.cmd(
                    AccountId(0),
                    &ControlCmd::InjectWarmup {
                        instance: S1,
                        last: 1,
                        local_kind: 3,
                        chunk_no: 0,
                        len: 4_096,
                        _pad: [0; 6],
                        payload: &payload,
                    },
                );
            })
        })
    });
}

fn seq_336_strategy_cmd(c: &mut Criterion) {
    c.bench_function("seq/strategy_cmd", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, _| {
                h.cmd_confirmed(
                    AC1,
                    &ControlCmd::StrategyCmd {
                        instance: S1,
                        reduce_only: 0,
                        len: 32,
                        group: G1,
                        payload: [0x11; 256],
                    },
                );
            })
        })
    });
}

fn seq_337_snapshot_keyframe(c: &mut Criterion) {
    c.bench_function("seq/snapshot_keyframe", |b| {
        let mut h = steady();
        b.iter_custom(|iters| {
            each(iters, &mut h, |h, _| {
                let t = h.now();
                h.push(RingId::Hk, &sys_timer(1, t));
            })
        })
    });
}

/// A journal of `n` Bbo Inputs after a keyframe.
fn bbo_journal(n: u64) -> (H, u64) {
    let mut h = steady();
    let kf = h.keyframe().seq;
    for k in 0..n {
        let ask = if k % 2 == 0 { "50001" } else { "50002" };
        h.quote(A1, "50000", ask);
        if k % 32 == 31 {
            h.run();
        }
    }
    h.run();
    (h, kf)
}

fn seq_338_replay_throughput(c: &mut Criterion) {
    let mut g = c.benchmark_group("seq");
    g.sample_size(10);
    let n = 20_000u64;
    let (h, kf) = bbo_journal(n);
    g.throughput(criterion::Throughput::Elements(n));
    g.bench_function("replay_throughput", |b| {
        b.iter(|| black_box(verify_journal::<TestSet>(h.j(), &h.setup, Some(kf))))
    });
    g.finish();
}

fn seq_339_mirror_throughput(c: &mut Criterion) {
    // The advisor mirror is a Mirror-mode replay over the same journal (no Model).
    let mut g = c.benchmark_group("seq");
    g.sample_size(10);
    let n = 20_000u64;
    let (h, kf) = bbo_journal(n);
    g.throughput(criterion::Throughput::Elements(n));
    g.bench_function("mirror_throughput", |b| {
        b.iter(|| {
            let log = hft_sequencer::driver::replay::Log::from_segments(&h.j().segments);
            let drv = hft_sequencer::driver::replay::ReplayDriver::new(
                hft_sequencer::driver::replay::ReplayMode::Mirror,
                log,
                0,
                1_024,
            );
            let p = plan(hft_sequencer::SnapshotSel::AtSeq(kf), true, 0, 0);
            let mut s: hft_sequencer::Sequencer<hft_sequencer::driver::replay::ReplayDriver, TestSet> =
                match hft_sequencer::Sequencer::boot(
                    hft_sequencer::BootSource::Journal(p),
                    &h.setup,
                    drv,
                    hb(),
                ) {
                    Ok(s) => s,
                    Err(e) => panic!("mirror boot: {e:?}"),
                };
            for _ in 0..1_000_000 {
                if s.step() != StepOutcome::Busy {
                    break;
                }
            }
            black_box(s.engine().cur_seq)
        })
    });
    g.finish();
}

fn timer_noop(_t: Timer) {}

criterion_group!(
    benches,
    seq_317_overhead_per_event,
    seq_318_head_check,
    seq_320_conn_alive,
    seq_321_conn_down,
    seq_322_bbo_view,
    seq_323_notify_continue,
    seq_324_bbo_miss,
    seq_325_bbo_target_idle,
    seq_326_bbo_fire,
    seq_327_bbo_exec_fire,
    seq_328_l2batch_20,
    seq_329_fill_to_hedge,
    seq_330_advice_route,
    seq_331_margin_report,
    seq_332_external_fill,
    seq_333_recon_funding_item,
    seq_334_recon_group,
    seq_335_inject_warmup,
    seq_336_strategy_cmd,
    seq_337_snapshot_keyframe,
    seq_338_replay_throughput,
    seq_339_mirror_throughput
);
criterion_main!(benches);
