//! VEN-354–VEN-357, VEN-359, VEN-369: gw benchmarks (README "延迟与资源契约"). Scripted
//! connections except `venue_gw_place_loopback` (Linux loopback TCP + TLS 1.3).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: benchmark driver

#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;

use std::cell::Cell;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::GwOp;
use hft_net::ConnId;
use hft_net::sock::RecvStep;
use hft_types::{AccountId, ClientOrderId, KillBits};
use hft_venue::{GwCfg, kill_admit};
use support::gwh::*;
use support::rd::*;
use support::stub::{self, StubProto};
use support::ws_ref;

/// A1 and A2 open; no inflight timeouts during the run (requests are never answered here).
fn quiet_gw(accts: &[AccountId]) -> Gh<StubProto> {
    let cfg = GwCfg {
        inflight_timeout_ms: u32::MAX / 2,
        ..g0()
    };
    gw_full::<StubProto>(cfg, accts, GhOpts::default())
}

fn clear_sent(h: &mut Gh<StubProto>) {
    for id in 112..=127u8 {
        if let Some(c) = h.rt.conn(ConnId(id)) {
            c.script().unwrap().sent.clear();
        }
    }
}

fn ven_355_gw_place_mem(c: &mut Criterion) {
    stub::log_enabled(false);
    let mut h = quiet_gw(&[A1, A2]);
    let mut t = 10_000_000u64;
    let mut k = 0u32;
    c.bench_function("venue_gw_place_mem", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                // 10 orders/s per account: one order per 100 ms of TSC time.
                t += 100_000_001;
                k = k % 2000 + 1;
                let p = h.place(A1, k, 1, S1, false);
                h.cmd(p);
                let t0 = Instant::now();
                std::hint::black_box(h.rt.poll_once(t));
                total += t0.elapsed();
                h.reps();
                clear_sent(&mut h);
            }
            total
        })
    });
    stub::log_enabled(true);
}

fn ven_354_gw_place_loopback(c: &mut Criterion) {
    let name = "venue_gw_place_loopback";
    #[cfg(target_os = "linux")]
    {
        use hft_event::{OutMsg, RepMsg};
        use hft_net::{BuildReq, ConnRole, RetireMsg};
        use hft_rt::{Backing, byte_spsc, spsc};
        use hft_venue::{GwDeps, GwRuntime, Handover};
        use support::looppeer::*;
        stub::log_enabled(false);
        let peer = LoopPeer::start(LoopOpts::default());
        let mut k = connector(peer.port, &[(ConnRole::Order, 2)]);
        let (w, _j) = support::jrn::raw_journal("raw-gw-loop", 64);
        let (mut out_p, out_c) = spsc::<OutMsg>(1024, Backing::Heap).unwrap();
        let (rep_p, mut rep_c) = spsc::<RepMsg>(1024, Backing::Heap).unwrap();
        let (_acct_p, acct_c) = byte_spsc(1 << 16, Backing::Heap).unwrap();
        let (mut h_p, h_c) = spsc::<Handover>(8, Backing::Heap).unwrap();
        let (r_p, _r_c) = spsc::<RetireMsg>(8, Backing::Heap).unwrap();
        let (b_p, mut b_c) = spsc::<BuildReq>(8, Backing::Heap).unwrap();
        let deps = GwDeps {
            rd: rd_stub(),
            cmds: out_c,
            out: rep_p,
            acct: acct_c,
            raw: w,
            kill: kill_word(),
            kill_mask: 7,
            tx_tags: None,
            handoff: h_c,
            retire: r_p,
            build: b_p,
            clock: support::feedh::clock(),
        };
        let cfg = GwCfg {
            inflight_timeout_ms: u32::MAX / 2,
            ..g0()
        };
        let mut rt = GwRuntime::<StubProto>::new(cfg, deps, vec![spec::<StubProto>(A1, 1500)]).unwrap();
        rt.poll_once(1);
        let b = b_c.try_pop().unwrap();
        let (ho, mut pc) = open(&mut k, &peer, b.conn.0, ConnRole::Order);
        h_p.try_push(Handover::Ready(ho)).unwrap();
        rt.poll_once(2);
        if let Some(b2) = b_c.try_pop() {
            h_p.try_push(Handover::Failed(b2.conn)).unwrap();
        }
        out_p.try_push(hft_event::GwCmd::set_epoch(5, false, 0)).unwrap();
        rt.poll_once(3);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 1 << 16];
            while std::io::Read::read(&mut pc.s, &mut buf)
                .map(|n| n > 0)
                .unwrap_or(false)
            {}
        });
        let mut t = 10_000_000u64;
        let mut slot = 0u32;
        let mut seq = 0u64;
        c.bench_function(name, |bch| {
            bch.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    t += 100_000_001;
                    slot = slot % 2000 + 1;
                    seq += 1;
                    let mut p = base(GwOp::Place, A1);
                    p.decision_seq = seq;
                    p.slot = hft_types::SlotId(slot);
                    p.r#gen = 1;
                    p.inst = S1;
                    out_p.try_push(p).unwrap();
                    let t0 = Instant::now();
                    std::hint::black_box(rt.poll_once(t));
                    total += t0.elapsed();
                    while rep_c.try_pop().is_some() {}
                }
                total
            })
        });
        stub::log_enabled(true);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (c, name);
    }
}

fn ack_frames(h: &mut Gh<StubProto>, n: usize, k0: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    for i in 0..n as u64 {
        let k = k0 + i;
        let id = ClientOrderId {
            epoch: 5,
            venue: V7,
            account: A1,
            slot: (k % 2000) as u32 + 1,
            r#gen: 1,
        };
        let line = format!("O|{}|{}|Ack|0|0|0|0|0|0", stub::cid_text(&id), 100_000 + k);
        bytes.extend(h.net.peer(ConnId(112)).frames(&[ws_ref::text(line.as_bytes())]));
    }
    bytes
}

fn ven_356_gw_report_path(c: &mut Criterion) {
    stub::log_enabled(false);
    let mut h = quiet_gw(&[A1, A2]);
    let mut t = 10_000_000u64;
    let mut k = 0u64;
    c.bench_function("venue_gw_report_path", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                t += 1;
                k += 1;
                let bytes = ack_frames(&mut h, 1, k);
                h.rt.conn(ConnId(112))
                    .unwrap()
                    .script()
                    .unwrap()
                    .push_recv(RecvStep::data(t, t, bytes));
                let t0 = Instant::now();
                std::hint::black_box(h.rt.poll_once(t));
                total += t0.elapsed();
                h.reps();
            }
            total
        })
    });
    stub::log_enabled(true);
}

thread_local! {
    static PUSHED: Cell<Option<Instant>> = const { Cell::new(None) };
    static TAKEN: Cell<Option<Instant>> = const { Cell::new(None) };
}

fn ven_357_gw_drain_during_burst(c: &mut Criterion) {
    stub::log_enabled(false);
    let mut h = quiet_gw(&[A1, A2]);
    let mut out = h.out.take().unwrap();
    // The order arrives while frame 32 of a 64-frame burst is being decoded; its encode marks
    // the moment `drain_out` took it.
    let mut n = 0u32;
    let mut seq = 0u64;
    stub::set_report_hook(Some(Box::new(move |_p| {
        n += 1;
        if n % 64 == 32 {
            seq += 1;
            let mut p = base(GwOp::Place, A2);
            p.decision_seq = seq;
            p.slot = hft_types::SlotId((seq % 2000) as u32 + 1);
            p.r#gen = 1;
            p.oflags = hft_event::OrderFlags::RISK_REDUCING.0;
            let _ = out.try_push(p);
            PUSHED.with(|c| c.set(Some(Instant::now())));
        }
    })));
    stub::set_encode_hook(Some(Box::new(|_c| {
        if TAKEN.with(|c| c.get()).is_none() {
            TAKEN.with(|c| c.set(Some(Instant::now())));
        }
    })));
    let mut t = 10_000_000u64;
    let mut k = 0u64;
    c.bench_function("venue_gw_drain_during_burst", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                t += 100_000_001;
                let bytes = ack_frames(&mut h, 64, k);
                k += 64;
                h.rt.conn(ConnId(112))
                    .unwrap()
                    .script()
                    .unwrap()
                    .push_recv(RecvStep::data(t, t, bytes));
                TAKEN.with(|c| c.set(None));
                h.rt.poll_once(t);
                let (p, q) = (PUSHED.with(|c| c.get()), TAKEN.with(|c| c.get()));
                if let (Some(p), Some(q)) = (p, q) {
                    total += q.saturating_duration_since(p);
                }
                h.reps();
                clear_sent(&mut h);
            }
            total
        })
    });
    stub::set_report_hook(None);
    stub::set_encode_hook(None);
    stub::log_enabled(true);
}

fn ven_359_kill_admit(c: &mut Criterion) {
    let w = kill_word();
    w.store(KillBits::account(3).0, std::sync::atomic::Ordering::Release);
    let bits = KillBits::account(0).0 | KillBits::account_hard(0).0;
    c.bench_function("venue_kill_admit", |b| {
        b.iter(|| {
            let word = std::hint::black_box(w).load(std::sync::atomic::Ordering::Acquire);
            std::hint::black_box(kill_admit(word, 7, bits, 0, GwOp::Place, 0))
        })
    });
}

fn ven_358_idtable(c: &mut Criterion) {
    let mut table = hft_venue::IdTable::new(16384, 4096, 120_000_000_000);
    let keys: Vec<_> = (0..8192).map(|n| n.to_string().into_bytes()).collect();
    for (n, k) in keys.iter().take(8192).enumerate() {
        let _ = table.insert(k, 5, n as u32, 1, 0);
    }
    for k in keys.iter().take(4096) {
        table.terminal(k, 0);
    }
    let mut n = 0usize;
    c.bench_function("venue_idtable", |b| {
        b.iter(|| {
            n = (n + 1) % 8192;
            std::hint::black_box(table.get(std::hint::black_box(&keys[n])));
        })
    });
}

fn ven_369_gw_idle_8sock(c: &mut Criterion) {
    stub::log_enabled(false);
    // Four accounts × (primary, standby) = 8 idle sockets.
    let mut h = quiet_gw(&[A1, A2, AccountId(2), AccountId(3)]);
    let mut t = 10_000_000u64;
    let mut k = 0u32;
    c.bench_function("venue_gw_idle_8sock", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                t += 100_000_001;
                k = k % 2000 + 1;
                let p = h.place(AccountId(3), k, 1, S1, false);
                h.cmd(p);
                let t0 = Instant::now();
                std::hint::black_box(h.rt.poll_once(t));
                total += t0.elapsed();
                h.reps();
                clear_sent(&mut h);
            }
            total
        })
    });
    stub::log_enabled(true);
}

// Synthetic bounded path measurements; production capture coverage is tracked separately.
fn report_bench(c: &mut Criterion, name: &str, payload: &[u8], margin: bool) {
    stub::set_cfg(|c| *c = Default::default());
    stub::log_enabled(false);
    let mut h = quiet_gw(&[A1]);
    let (bulk, mut tail) = hft_rt::byte_spsc(1 << 20, hft_rt::Backing::Heap).unwrap();
    h.rt.set_rep_bulk(bulk);
    let mut t = 10_000_000;
    c.bench_function(name, |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                t += 1;
                let bytes = h.net.peer(ConnId(112)).frames(&[ws_ref::text(payload)]);
                h.rt.conn(ConnId(112))
                    .unwrap()
                    .script()
                    .unwrap()
                    .push_recv(RecvStep::data(t, t, bytes));
                let start = Instant::now();
                std::hint::black_box(h.rt.poll_once(t));
                total += start.elapsed();
                let rows = h.reps();
                if margin {
                    assert!(
                        rows.iter()
                            .any(|r| matches!(r,hft_event::RepMsg::MarginReport(h) if h.n==1024))
                    );
                    assert_eq!(tail.peek().unwrap().len(), 16384);
                    tail.advance();
                }
                h.raw.prepare();
            }
            total
        })
    });
    stub::log_enabled(true);
}
fn ven_361_gw_external_fill(c: &mut Criterion) {
    report_bench(c, "venue_gw_external_fill", b"X|external", false);
}
fn ven_362_gw_skip_event(c: &mut Criterion) {
    report_bench(c, "venue_gw_skip_event", b"E|listenKeyExtended", false);
    report_bench(c, "venue_gw_skip_event_unknown", b"E|fooBar", false);
}
fn ven_363_gw_margin_push(c: &mut Criterion) {
    report_bench(c, "venue_gw_margin_push", b"MP|1024", true);
}
fn ven_364_gw_acct_apply_16k(c: &mut Criterion) {
    use hft_venue::{AcctFail, AcctRecHdr};
    stub::log_enabled(false);
    let mut h = quiet_gw(&[A1]);
    let mut record = vec![0; 80 + 16384];
    let mut t = 10_000_000u64;
    let (queries, mut requests) = hft_rt::spsc(8, hft_rt::Backing::Heap).unwrap();
    h.rt.set_account_queries(queries);
    for _ in 0..8 {
        h.poll(t);
    }
    let request = requests.try_pop().unwrap();
    AcctRecHdr {
        seq_no: request.seq_no,
        account: A1,
        what: 1,
        kind: request.kind,
        step: 0,
        part: 0,
        parts: 1,
        fail: AcctFail::None,
        conn: ConnId(80),
        http_status: 200,
        sent_tsc: t,
        ts: hft_net::Timestamps {
            rx_tsc: t,
            ..Default::default()
        },
        req_len: 0,
        body_len: 16384,
    }
    .write(&mut record)
    .unwrap();
    c.bench_function("venue_gw_acct_apply_16k", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                t += 1;
                h.acct
                    .reserve(record.len() as u32)
                    .unwrap()
                    .copy_from_slice(&record);
                h.acct.commit();
                let start = Instant::now();
                std::hint::black_box(h.rt.poll_once(t));
                total += start.elapsed();
                h.reps();
                assert_eq!(h.rt.counters().report_decode_err, 0);
                h.raw.prepare();
            }
            total
        })
    });
    stub::log_enabled(true);
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(2)).warm_up_time(Duration::from_millis(300));
    targets = ven_354_gw_place_loopback, ven_355_gw_place_mem, ven_356_gw_report_path, ven_357_gw_drain_during_burst,
        ven_358_idtable, ven_359_kill_admit, ven_361_gw_external_fill, ven_362_gw_skip_event, ven_363_gw_margin_push, ven_364_gw_acct_apply_16k, ven_369_gw_idle_8sock
);
criterion_main!(benches);
