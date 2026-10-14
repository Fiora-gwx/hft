//! Synthetic 50 KiB account sequence through the bounded production worker (not capture evidence).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]
#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;
use criterion::{Criterion, criterion_group, criterion_main};
use hft_rt::{Backing, byte_spsc, spsc};
use hft_venue::{AcctAccount, AcctFetchTask, AcctRecHdr, AcctReq, AcctResponse, AcctTransport, GwCfg};
use hft_venue_proto::{AcctQueryKind, VenueProtocol};
use std::time::{Duration, Instant};
use support::{
    rd::*,
    stub::{self, StubProto},
};
struct Replay {
    body: Vec<u8>,
    waiting: bool,
}
impl AcctTransport for Replay {
    fn submit(
        &mut self,
        _: &hft_venue_proto::RestReqBuf,
        _: u64,
        _: u64,
    ) -> Result<hft_net::ConnId, hft_net::RestErr> {
        self.waiting = true;
        Ok(hft_net::ConnId(80))
    }
    fn poll(&mut self, now: u64) -> Option<Result<AcctResponse<'_>, hft_net::RestErr>> {
        if !self.waiting {
            return None;
        }
        self.waiting = false;
        Some(Ok(AcctResponse {
            status: 200,
            body: &self.body,
            conn: hft_net::ConnId(80),
            ts: hft_net::Timestamps {
                rx_tsc: now,
                ..Default::default()
            },
        }))
    }
    fn idle_check(&mut self, _: u64) {}
    fn abort(&mut self) {
        self.waiting = false;
    }
}
fn ven_365_acct_fetch_50k(c: &mut Criterion) {
    stub::log_enabled(false);
    let (mut tx, rx) = spsc(8, Backing::Heap).unwrap();
    let (out, mut tap) = byte_spsc(1 << 20, Backing::Heap).unwrap();
    let cfg = GwCfg::default();
    let tpl = StubProto::build_templates(&cfg.venue_cfg(), rd_stub(), A1, &Default::default()).unwrap();
    let mut body = vec![0; 50 * 1024];
    body[..8].copy_from_slice(&100i64.to_le_bytes());
    let mut task = AcctFetchTask::<StubProto, Replay>::new(
        rd_stub(),
        cfg,
        support::feedh::clock(),
        rx,
        out,
        vec![AcctAccount {
            account: A1,
            templates: tpl,
            transport: Replay { body, waiting: false },
        }],
    )
    .unwrap();
    let mut seq = 0;
    let mut now = 0;
    c.bench_function("venue_acct_fetch_50k", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                seq += 1;
                tx.try_push(AcctReq {
                    kind: AcctQueryKind::Margin,
                    account: A1,
                    why: 1,
                    session: 0,
                    seq_no: seq,
                    poll_no: 0,
                    _pad: 0,
                    session_uid: [0; 24],
                })
                .unwrap();
                let mut done = false;
                let mut raw_bytes = 0;
                for _ in 0..32 {
                    now += 1;
                    let start = Instant::now();
                    task.poll_once(now);
                    total += start.elapsed();
                    while let Some(bytes) = tap.peek() {
                        let h = AcctRecHdr::read(bytes).unwrap();
                        if h.what == 1 {
                            raw_bytes += h.body_len;
                        } else {
                            assert_eq!(h.what, 2);
                            done = true;
                        }
                        tap.advance();
                    }
                    if done {
                        break;
                    }
                }
                assert!(done);
                assert_eq!(raw_bytes, 2 * 50 * 1024);
            }
            total
        })
    });
    stub::log_enabled(true);
}
criterion_group!(name=benches;config=Criterion::default().sample_size(20).measurement_time(Duration::from_secs(2)).warm_up_time(Duration::from_millis(300));targets=ven_365_acct_fetch_50k);
criterion_main!(benches);
