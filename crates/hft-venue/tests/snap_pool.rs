use crate::support::{
    feedh::clock,
    mempeer,
    net::{self, Net},
    rd::*,
    stub::{self, StubProto},
};
use hft_net::{BuildReq, ConnId, RestPool};
use hft_rt::{Backing, ByteConsumer, Consumer, PollTask, Producer, byte_spsc, spsc};
use hft_venue::{Handover, PooledSnapFetchTask, SnapCfg, SnapConnections, SnapDeps, SnapRecHdr, SnapReq};
use zerocopy::FromBytes;

pub(crate) struct Harness {
    pub(crate) task: PooledSnapFetchTask<StubProto>,
    pub(crate) net: Net,
    req: Producer<SnapReq>,
    out: ByteConsumer,
    hand: Producer<Handover>,
    build: Consumer<BuildReq>,
    retire: Consumer<hft_net::RetireMsg>,
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.task.pool_mut().retire_all();
        while let Some(m) = self.task.pool_mut().take_retired() {
            self.net.c().reclaim(m);
        }
    }
}
impl Harness {
    pub(crate) fn new(parallel: u8) -> Self {
        Self::with_rd(parallel, rd_stub())
    }
    fn with_rd(parallel: u8, rd: &'static hft_refdata::RefData) -> Self {
        stub::set_cfg(|c| *c = Default::default());
        let cfg = SnapCfg {
            parallel,
            max_record_bytes: 1024,
            request_gap_ms: 1,
            ..Default::default()
        };
        let (req, rx) = spsc(256, Backing::Heap).unwrap();
        let (out, tap) = byte_spsc(4096, Backing::Heap).unwrap();
        let (hand, hc) = spsc(16, Backing::Heap).unwrap();
        let (bp, build) = spsc(16, Backing::Heap).unwrap();
        let (rp, retire) = spsc(16, Backing::Heap).unwrap();
        let pool = RestPool::new(b"a.test", &net::net_cfg(), mempeer::clock());
        let mut task = PooledSnapFetchTask::new(
            cfg,
            SnapDeps {
                rd,
                reqs: [Some(rx), None],
                out: [Some(out), None],
                clock: clock(),
            },
            (),
            pool,
        )
        .unwrap();
        task.set_connections(SnapConnections {
            handoff: hc,
            retire: rp,
            build: bp,
            endpoint: hft_net::EndpointId(0),
            src_ip_idx: 1,
        })
        .unwrap();
        Self {
            task,
            net: Net::new(0, 0),
            req,
            out: tap,
            hand,
            build,
            retire,
        }
    }
    fn poll(&mut self, now: u64) {
        self.task.poll_once(now);
        self.net.reap(&mut self.retire);
        while let Some(r) = self.build.try_pop() {
            assert_eq!(r.src_ip_idx, 1);
            let h = self.net.complete(r);
            self.hand.try_push(Handover::Ready(h)).unwrap();
        }
    }
    pub(crate) fn open(&mut self) {
        for t in 1..5 {
            self.poll(t);
        }
    }
    pub(crate) fn req(&mut self, id: u32, inst: hft_types::InstId) {
        self.req
            .try_push(SnapReq {
                kind: SnapReq::BBO,
                _pad: 0,
                attempt: 0,
                inst,
                req_id: id,
                _pad2: 0,
            })
            .unwrap();
    }
    pub(crate) fn response(&mut self, id: u8, status: u16, body: &[u8], now: u64) {
        let mut msg = format!(
            "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        msg.extend_from_slice(body);
        let wire = self.net.peer(ConnId(id)).records(&[&msg]);
        mempeer::deliver(self.task.pool_mut().get(ConnId(id)).unwrap(), now, now, wire);
    }
    pub(crate) fn records(&mut self) -> Vec<(SnapRecHdr, Vec<u8>)> {
        let mut out = Vec::new();
        while let Some(b) = self.out.peek() {
            let (h, tail) = SnapRecHdr::read_from_prefix(b).unwrap();
            out.push((h, tail.to_vec()));
            self.out.advance();
        }
        out
    }
}

pub(crate) fn measure_pooled_snapshot() {
    let mut h = Harness::new(1);
    h.open();
    stub::log_enabled(false);
    h.task
        .pool_mut()
        .get(ConnId(64))
        .unwrap()
        .script()
        .unwrap()
        .sent
        .reserve(1 << 20);
    for n in 0..101_000u32 {
        let now = u64::from(n + 1) * 2_000_000;
        h.req(n, S1);
        let scope = hft_testkit::CountingAlloc::scope();
        h.task.poll_once(now);
        let submit = scope.allocs();
        h.response(64, 200, &crate::support::feedh::b("S1", u64::from(n)), now + 1);
        let scope = hft_testkit::CountingAlloc::scope();
        h.task.poll_once(now + 1);
        let complete = scope.allocs();
        assert_eq!(h.records().len(), 1);
        if n >= 1000 {
            assert_eq!((submit, complete), (0, 0));
        }
        h.task
            .pool_mut()
            .get(ConnId(64))
            .unwrap()
            .script()
            .unwrap()
            .sent
            .clear();
    }
    stub::log_enabled(true);
}
#[test]
fn ven_160_response_raw_is_written_once_by_feed() {
    use zerocopy::IntoBytes;
    let (mut feed, now) = crate::support::feedh::bbo_up(|_| {}, &[S1]);
    for id in [ConnId(0), ConnId(1)] {
        feed.net.at(id, 0, net::Ev::Rst);
    }
    feed.poll(now + 10);
    feed.complete_builds();
    feed.poll(now + 20);
    feed.ack(ConnId(0));
    feed.poll(now + 30);
    feed.poll(now + 600_000_000);
    let req = feed.snapreq.as_mut().unwrap().try_pop().unwrap();
    let mut pool = Harness::new(1);
    pool.open();
    pool.req(req.req_id, req.inst);
    pool.poll(now + 600_000_001);
    let body = crate::support::feedh::b("S1", 9);
    pool.response(64, 200, &body, now + 600_000_002);
    pool.poll(now + 600_000_002);
    for (head, tail) in pool.records() {
        let out = feed.snap.as_mut().unwrap();
        let dst = out
            .reserve((size_of::<SnapRecHdr>() + tail.len()) as u32)
            .unwrap();
        dst[..size_of::<SnapRecHdr>()].copy_from_slice(head.as_bytes());
        dst[size_of::<SnapRecHdr>()..].copy_from_slice(&tail);
        out.commit();
    }
    feed.poll(now + 600_000_003);
    let raw = feed.raw.records();
    let records: Vec<_> = raw
        .iter()
        .filter(|r| r.hdr.what == hft_venue::raw_what::REST_RESPONSE)
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].hdr.conn, ConnId(64));
    assert_eq!(records[0].payload, body);
}
#[test]
fn ven_151_pooled_requests_each_complete_once() {
    timeout_retires_socket_and_parallel_responses_keep_tags();
    for parallel in [1, 2] {
        let mut h = Harness::new(parallel);
        h.open();
        for i in 0..120u32 {
            let t = u64::from(i + 1) * 3_000_000_000;
            h.req(i, S1);
            h.poll(t);
            let body = crate::support::feedh::b("S1", u64::from(i));
            match i % 6 {
                0 => h.response(64, 200, &body, t + 1),
                1 => h.response(64, 503, b"unavailable", t + 1),
                2 => h.response(64, 429, b"slow", t + 1),
                3 => h.response(64, 200, b"bad", t + 1),
                4 => h.response(64, 200, &[b'x'; 1100], t + 1),
                _ => h.response(64, 200, &body, t + 1),
            }
            h.poll(t + 1);
            let r = h.records();
            assert_eq!(r.len(), 1);
            assert_eq!(r[0].0.req_id, i);
            assert_eq!(
                r[0].0.status,
                [
                    SnapRecHdr::OK,
                    SnapRecHdr::HTTP,
                    SnapRecHdr::RATE,
                    SnapRecHdr::DECODE,
                    SnapRecHdr::TOO_LARGE,
                    SnapRecHdr::OK
                ][i as usize % 6]
            );
            h.poll(t + 2);
            assert!(h.records().is_empty());
        }
    }
}
#[test]
fn ven_153_invalid_snapshot_request_completes_without_http() {
    let mut h = Harness::new(1);
    h.open();
    stub::set_cfg(|c| c.bbo_snapshot = false);
    h.req(9, S1);
    h.poll(1000);
    let r = h.records();
    assert_eq!(
        (r.len(), r[0].0.req_id, r[0].0.status, r[0].0.body_len),
        (1, 9, SnapRecHdr::REQUEST, 0)
    );
}
#[test]
fn ven_157_batch_backpressure_preserves_raw_and_request_identity() {
    let mut h = Harness::new(1);
    h.open();
    // Two independently requested instruments share one HTTP response. The raw body occurs once.
    h.req(11, S1);
    h.req(12, S2);
    h.poll(1000);
    h.response(
        64,
        200,
        b"B|S1|1|100.00|1.000|101.00|2.000\nB|S2|2|100.00|1.000|101.00|2.000",
        2000,
    );
    h.poll(2000);
    h.poll(2001);
    let r = h.records();
    assert_eq!(r.len(), 2);
    assert_eq!((r[0].0.req_id, r[1].0.req_id), (11, 12));
    assert_eq!((r[0].0.status, r[1].0.status), (SnapRecHdr::OK, SnapRecHdr::OK));
    assert!(r[0].0.body_len > 0);
    assert_eq!(r[1].0.body_len, 0);
    // Fill the output ring with bounded HTTP error records; completion waits without losing data.
    for i in 0..4 {
        let t = 2_000_000 + i * 2_000_000;
        h.req(i as u32 + 20, S1);
        h.poll(t);
        h.response(64, 503, &[b'x'; 900], t + 1);
        h.poll(t + 1);
    }
    h.req(99, S1);
    h.poll(12_000_000);
    assert!(h.task.has_pending());
    let r = h.records();
    assert_eq!(r.len(), 3);
    h.poll(12_000_002);
    let r = h.records();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0.req_id, 23);
    h.response(64, 503, &[b'x'; 900], 12_000_003);
    h.poll(12_000_003);
    let r = h.records();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0.req_id, 99);
}
#[test]
fn ven_159_idle_pool_rebuilds_closed_connection_on_existing_source_ip() {
    let mut h = Harness::new(2);
    h.open();
    h.task
        .pool_mut()
        .get(ConnId(64))
        .unwrap()
        .script()
        .unwrap()
        .push_recv(hft_net::sock::RecvStep {
            pre_tsc: 1_000_000_000,
            post_tsc: 1_000_000_000,
            kind: hft_net::sock::RecvKind::Eof,
        });
    h.poll(999_999_999);
    assert!(h.task.pool_mut().get(ConnId(64)).is_some());
    h.poll(1_000_000_000);
    h.poll(1_000_000_001);
    assert!(h.task.pool_mut().get(ConnId(64)).is_none());
    assert!(h.task.pool_mut().get(ConnId(66)).is_some());
}
#[test]
fn ven_161_pooled_snapshot_pending_and_deadline() {
    let mut h = Harness::new(1);
    h.open();
    assert!(!h.task.has_pending());
    h.req(1, S1);
    assert!(h.task.has_pending());
    h.poll(1000);
    assert!(h.task.next_deadline().is_some());
    h.response(64, 200, &crate::support::feedh::b("S1", 1), 2000);
    h.poll(2000);
    assert_eq!(h.records().len(), 1);
    assert!(!h.task.has_pending());
}

pub(crate) fn timeout_retires_socket_and_parallel_responses_keep_tags() {
    let mut h = Harness::new(2);
    h.open();
    h.req(1, S1);
    h.poll(1_000_000);
    h.req(2, S2);
    h.poll(2_000_000);
    h.response(65, 200, &crate::support::feedh::b("S2", 9), 3_000_000);
    h.poll(3_000_000);
    let r = h.records();
    assert_eq!((r[0].0.req_id, r[0].0.conn), (2, ConnId(65)));
    h.poll(2_001_000_000);
    let r = h.records();
    assert_eq!(
        (r.len(), r[0].0.req_id, r[0].0.status),
        (1, 1, SnapRecHdr::TIMEOUT)
    );
    assert!(h.task.pool_mut().get(ConnId(64)).is_none());
    h.poll(2_001_000_001);
    h.poll(2_001_000_002);
    assert!(h.records().is_empty());
}

#[test]
fn ven_154_weight_deferred_without_consuming_requests_or_empty_pool_quota() {
    let rd = rd_with(&RdOpts {
        rest_limit: Some(hft_refdata::RateLimit {
            scope: hft_refdata::LimitScope::Ip,
            bucket: hft_refdata::LimitBucket::Weight,
            window_ms: 60_000,
            limit: 10,
            default_cost: 2,
        }),
        ..Default::default()
    });
    let mut h = Harness::with_rd(1, rd);
    h.req(1, S1);
    // Repeated NoConn polls may start builds, but must never charge a request that wasn't sent.
    for t in 1..100 {
        h.task.poll_once(t);
    }
    h.poll(100);
    h.open();
    for i in 1..=5u32 {
        let t = u64::from(i) * 2_000_000;
        if i > 1 {
            h.req(i, S1);
        }
        h.poll(t);
        h.response(64, 200, &crate::support::feedh::b("S1", u64::from(i)), t + 1);
        h.poll(t + 1);
        let r = h.records();
        assert_eq!((r.len(), r[0].0.req_id), (1, i));
    }
    h.req(6, S1);
    h.poll(12_000_000);
    assert!(h.records().is_empty());
    assert!(!h.task.pool_mut().has_pending());
    h.poll(60_000_000_000);
    h.response(64, 200, &crate::support::feedh::b("S1", 6), 60_000_000_001);
    h.poll(60_000_000_001);
    assert_eq!(h.records()[0].0.req_id, 6);
}
