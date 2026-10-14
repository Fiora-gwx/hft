//! Diagnostic subdivisions of seq-decide; B0 and the same quiet test strategies as `seq`.
//! Instrumented and uninstrumented samples are separate. Run an optimized bench binary.
#![allow(
    dead_code,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)]

#[path = "../tests/common/mod.rs"]
mod common;
use common::*;
use hft_sequencer::{
    StepOutcome,
    bench_stages::{N, NAMES},
};
use hft_strategy_sdk::EventRef;
use std::hint::black_box;

#[path = "support/bbo_io.rs"]
mod bbo_io;

fn ram(hz: u64, single: bool) {
    let inputs = bbo_io::setup_inputs(single);
    let subscribers = if single { 1 } else { 3 };
    for direct in [false, true] {
        for profile in [false, true] {
            reset_script();
            SCRIPT.with(|s| s.borrow_mut().quiet = true);
            let (mut s, mut tx, root) =
                bbo_io::fixture(&inputs, usize::from(direct) * 2 + usize::from(profile), single);
            let mut input = hft_sequencer::InputSlot::with_capacity(256);
            let mut totals = Vec::with_capacity(20_000);
            let mut samples: [Vec<u64>; N] = core::array::from_fn(|_| Vec::with_capacity(20_000));
            for k in 0..22_000 {
                let b = bbo(
                    A1,
                    A,
                    0,
                    "50000",
                    if k % 2 == 0 { "50001" } else { "50002" },
                    10_000 + k,
                    s.engine().now.tsc.0,
                );
                if direct {
                    input.set(hft_event::RecordFlags(0), s.engine().now.tsc, &b);
                } else {
                    tx.try_push(b).unwrap();
                }
                let start = hft_rt::tsc_ordered().0;
                if profile {
                    s.engine_mut().bench_stages.start();
                }
                if direct {
                    s.bench_input(&input).unwrap();
                } else {
                    bbo_io::drain(&mut s);
                }
                if profile {
                    s.engine_mut().bench_stages.stop();
                }
                let end = hft_rt::tsc_ordered().0;
                black_box(s.engine().cur_seq);
                if k >= 2_000 {
                    totals.push(end - start);
                    if profile {
                        for (v, t) in samples.iter_mut().zip(s.engine().bench_stages.ticks) {
                            v.push(t);
                        }
                    }
                }
            }
            println!(
                "{{\"case\":\"seq/bbo_miss_ram\",\"subscribers\":{subscribers},\"direct\":{direct},\"profile\":{profile},\"n\":20000,\"warmup\":2000,\"hz\":{hz},{}}}",
                distribution(&mut totals, hz)
            );
            if profile {
                for (name, mut v) in NAMES.into_iter().zip(samples) {
                    println!("{{\"stage\":\"{name}\",{}}}", distribution(&mut v, hz));
                }
            }
            drop(s);
            drop(tx);
            drop(root);
        }
    }
}

fn steady(hit: bool) -> H {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    SCRIPT.with(|s| s.borrow_mut().quiet = true);
    if hit {
        add_hook(|hx, ev, ctx| {
            if let EventRef::Bbo { inst, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
            {
                let _ = ctx.place(open_buy(hx, A1));
            }
        });
    }
    h
}

fn drain(h: &mut H) {
    loop {
        match h.step() {
            StepOutcome::Busy => {}
            StepOutcome::Idle => break,
            x => panic!("benchmark stopped: {x:?}"),
        }
    }
}

fn percentile(v: &mut [u64], p: usize, hz: u64) -> u64 {
    v.sort_unstable();
    let ticks = v[(v.len() - 1) * p / 100];
    (u128::from(ticks) * 1_000_000_000 / u128::from(hz)) as u64
}

fn run(hit: bool, profile: bool, hz: u64) {
    let n = if hit { 1_000 } else { 20_000 };
    let warmup = if hit { 100 } else { 2_000 };
    let mut total = Vec::with_capacity(n);
    let mut samples: [Vec<u64>; N] = core::array::from_fn(|_| Vec::with_capacity(n));
    let mut h = steady(hit);
    for k in 0..n + warmup {
        // Fresh hit state keeps every sample a successfully accepted order, with no limit/tomb
        // buildup. Setup and destruction are excluded, matching seq/bbo_fire.
        if hit && k != 0 {
            h = steady(true);
        }
        h.quote(A1, "50000", if k % 2 == 0 { "50001" } else { "50002" });
        let begin = hft_rt::tsc_ordered().0;
        if profile {
            h.s.engine_mut().bench_stages.start();
        }
        drain(&mut h);
        if profile {
            h.s.engine_mut().bench_stages.stop();
        }
        let end = hft_rt::tsc_ordered().0;
        black_box(h.s.engine().cur_seq);
        if k >= warmup {
            total.push(end - begin);
            if profile {
                for (v, t) in samples.iter_mut().zip(h.s.engine().bench_stages.ticks) {
                    v.push(t);
                }
            }
        }
    }
    let case = if hit { "seq/bbo_hit" } else { "seq/bbo_miss" };
    println!(
        "{{\"case\":\"{case}\",\"profile\":{profile},\"n\":{n},\"warmup\":{warmup},\"hz\":{hz},{}}}",
        distribution(&mut total, hz)
    );
    if profile {
        for (name, mut v) in NAMES.into_iter().zip(samples) {
            println!("{{\"stage\":\"{name}\",{}}}", distribution(&mut v, hz));
        }
    }
}

fn main() {
    // Nominal counter frequency when the CPU reports one; otherwise one calibration round
    // (hosted CI runners do not expose it), as hft-testkit's bench harness does.
    let hz = hft_rt::tsc_hz_nominal()
        .or_else(|| {
            let cfg = hft_rt::CalibConfig {
                rounds: 1,
                ..hft_rt::CalibConfig::default()
            };
            hft_rt::TscClock::calibrate(&cfg).ok().map(|c| c.hz_milli / 1000)
        })
        .expect("counter frequency: neither nominal nor calibrated");
    if std::env::args().any(|a| a == "--single") {
        ram(hz, true);
        return;
    }
    let mut empty = Vec::with_capacity(20_000);
    for _ in 0..20_000 {
        let a = hft_rt::tsc_ordered().0;
        empty.push(hft_rt::tsc_ordered().0 - a);
    }
    println!(
        "{{\"counter_pair_p50_ns\":{},\"counter_pair_p99_ns\":{}}}",
        percentile(&mut empty, 50, hz),
        percentile(&mut empty, 99, hz)
    );
    for hit in [false, true] {
        for profile in [false, true] {
            run(hit, profile, hz);
        }
    }
    ram(hz, false);
}

fn distribution(v: &mut [u64], hz: u64) -> String {
    v.sort_unstable();
    let ns = |x| (u128::from(x) * 1_000_000_000 / u128::from(hz)) as u64;
    format!(
        "\"p50_ns\":{},\"p90_ns\":{},\"p99_ns\":{},\"p999_ns\":{},\"max_ns\":{}",
        ns(v[(v.len() - 1) * 50 / 100]),
        ns(v[(v.len() - 1) * 90 / 100]),
        ns(v[(v.len() - 1) * 99 / 100]),
        ns(v[(v.len() - 1) * 999 / 1000]),
        ns(v[v.len() - 1])
    )
}
