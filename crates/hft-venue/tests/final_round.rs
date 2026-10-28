//! Final M3-lite venue regressions. Synthetic protocols are never exchange-capture evidence.
use crate::support::{
    feedh::*,
    gwh::*,
    rd::*,
    stub::{self, StubProto},
};
use hft_event::{EventKind, NoteSource, OrderEventKind, RepMsg};
use hft_net::{ConnId, EndpointId, RestPool};
use hft_rt::{Backing, byte_spsc, spsc};
use hft_venue::*;
use hft_venue_proto::{AcctQueryKind, ArrayBuf, Credential, VenueProtocol};

#[test]
fn ven_221_unknown_event_notes_are_bounded_and_keep_event_name() {
    for (payload, n, notes) in [
        (b"E|listenKeyExtended".as_slice(), 10000, 0),
        (b"E|fooBar", 1, 1),
        (b"E|fooBar", 999, 1),
        (b"E|fooBar", 1000, 2),
        (b"E|fooBar", 1001, 2),
        (b"E|fooBar", 10000, 11),
    ] {
        stub::set_cfg(|c| *c = Default::default());
        let mut h = gw_full::<StubProto>(g0(), &[A1], Default::default());
        let mut found = Vec::new();
        for i in 0..n {
            h.net.push(ConnId(112), payload);
            h.poll(3000 + i);
            found.extend(h.reps());
        }
        assert_eq!(found.len(), notes);
        for m in found {
            let RepMsg::Note(note) = m else {
                panic!("unexpected report {m:?}")
            };
            assert_eq!(note.source, NoteSource::Venue);
            assert_eq!(note.code, codes::UNKNOWN_USER_EVENT);
            assert_eq!((note.v[0], note.v[1]), (V7.0 as u64, A1.0 as u64));
            assert_eq!(&note.v[2].to_le_bytes()[..6], b"fooBar");
        }
        let c = h.rt.counters();
        assert_eq!(c.user_evt_ignored + c.user_evt_unknown, n);
        assert_eq!(c.user_evt_unknown, if notes == 0 { 0 } else { n });
        assert_eq!(
            h.raw
                .records()
                .iter()
                .filter(|r| r.kind == EventKind::RAW_RX && r.payload == payload)
                .count(),
            n as usize
        );
        assert!(h.retired.is_empty());
        assert!(stub::calls("encode_rest").is_empty());
        assert!(stub::calls("acct_request").is_empty());
    }
}
#[test]
fn ven_226_fee_deviation_note_preserves_execution_fact() {
    stub::set_cfg(|c| {
        *c = Default::default();
        c.fee_rule_deviation = true;
    });
    let mut h = gw_full::<StubProto>(g0(), &[A1], Default::default());
    let id = hft_types::ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot: 42,
        r#gen: 1,
    };
    let text = format!("O|{}|99|Fill|2|2|100|-7|9|77", stub::cid_text(&id));
    h.net.push(ConnId(112), text.as_bytes());
    h.poll(3000);
    let reports = h.reps();
    let e = events(&reports);
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].what, e[0].fee.0, e[0].fee_asset.0),
        (OrderEventKind::Fill, -7, 9)
    );
    assert_eq!(h.rt.counters().fee_rule_deviation, 1);
    assert_eq!(reports.iter().filter(|r|matches!(r,RepMsg::Note(n) if n.source==NoteSource::Venue && n.code==codes::FEE_RULE_DEVIATION)).count(),1);
}
#[test]
fn ven_086_raw_full_drops_only_raw_and_recovers_after_preparation() {
    stub::set_cfg(|c| *c = Default::default());
    let (mut h, now) = bbo_up(|_| {}, &[S1]);
    let mut missed = None;
    for id in 1..5000u64 {
        let mut frame = b("S1", id);
        frame.push(b'|');
        frame.resize(15 * 1024, b'x');
        h.net.push(ConnId(0), &frame);
        assert!(h.poll(now + id).did_work);
        let v = h.drain_bbo();
        assert_eq!(bbos(&v).last().unwrap().update_id, id);
        assert!(states(&v).is_empty());
        if h.rt.raw_counters().append_err[0] > 0 {
            missed = Some((id, frame));
            break;
        }
    }
    let (id, frame) = missed.expect("exhaust prepared segments");
    assert_eq!(h.rt.raw_counters().append_err[0], 1);
    assert!(!h.raw.records().iter().any(|r| r.payload == frame));
    h.raw.prepare();
    let next = b("S1", id + 1);
    h.net.push(ConnId(0), &next);
    h.poll(now + id + 1);
    assert_eq!(bbos(&h.drain_bbo())[0].update_id, id + 1);
    assert!(h.raw.records().iter().any(|r| r.payload == next));
    assert!(h.retired.is_empty());
    assert_eq!(h.rt.raw_counters().append_err[0], 1);
}
#[test]
fn ven_035_warm_rest_connections_reuse_source_and_rebuild() {
    let mut snap = crate::snap_pool::Harness::new(1);
    snap.open(); // asserts source index 1 for every build
    let mut h = AccountHarness::new();
    h.open();
    assert_eq!(
        h.built.iter().map(|r| r.src_ip_idx).collect::<Vec<_>>(),
        vec![2, 2]
    );
    let req = hft_venue_proto::RestReqBuf {
        path_query: ArrayBuf::from_bytes(b"/probe").unwrap(),
        ..Default::default()
    };
    let pool = h.task.transport_mut(A1).unwrap();
    assert_eq!(pool.submit(&req, 99, 100), Ok(ConnId(80)));
    pool.abort();
    for n in 101..110 {
        h.poll(n);
    }
    assert!(h.built.len() > 2);
    assert!(
        h.built
            .iter()
            .all(|r| r.src_ip_idx == 2 && (80..=81).contains(&r.conn.0))
    );
    assert!(
        h.task
            .transport_mut(A1)
            .unwrap()
            .pool_mut(0)
            .unwrap()
            .get(ConnId(80))
            .is_some()
    );
}
struct AccountHarness {
    task: AcctFetchTask<StubProto, AcctRestPools<1>>,
    net: crate::support::net::Net,
    tx: hft_rt::Producer<AcctReq>,
    out: hft_rt::ByteConsumer,
    hand: hft_rt::Producer<Handover>,
    build: hft_rt::Consumer<hft_net::BuildReq>,
    retire: hft_rt::Consumer<hft_net::RetireMsg>,
    built: Vec<hft_net::BuildReq>,
}
impl AccountHarness {
    fn new() -> Self {
        use crate::support::{mempeer, net};
        let (tx, rx) = spsc(8, Backing::Heap).unwrap();
        let (out, tap) = byte_spsc(1 << 20, Backing::Heap).unwrap();
        let (hand, hc) = spsc(8, Backing::Heap).unwrap();
        let (bp, build) = spsc(8, Backing::Heap).unwrap();
        let (rp, retire) = spsc(8, Backing::Heap).unwrap();
        let mut transport = AcctRestPools::new(vec![
            RestPool::new(b"a.test", &net::net_cfg(), mempeer::clock()),
            RestPool::new(b"a.test", &net::net_cfg(), mempeer::clock()),
        ])
        .unwrap();
        transport
            .attach_connections(AcctConnections {
                venue: V7,
                account: A1,
                endpoints: vec![EndpointId(0), EndpointId(1)],
                src_ip_idx: 2,
                first_conn: 80,
                last_conn: 81,
                clock: clock(),
                timeout_ms: 3000,
                handoff: hc,
                retire: rp,
                build: bp,
            })
            .unwrap();
        let cfg = g0();
        let tpl =
            StubProto::build_templates(&cfg.venue_cfg(), rd_stub(), A1, &Credential::default()).unwrap();
        let mut task = AcctFetchTask::new(
            rd_stub(),
            cfg,
            clock(),
            rx,
            out,
            vec![AcctAccount {
                account: A1,
                templates: tpl,
                transport,
            }],
        )
        .unwrap();
        task.set_credential(
            A1,
            Credential {
                api_key: ArrayBuf::from_bytes(b"acct-key-sentinel").unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        Self {
            task,
            net: net::Net::new(0, 0),
            tx,
            out: tap,
            hand,
            build,
            retire,
            built: Vec::new(),
        }
    }
    fn poll(&mut self, now: u64) {
        self.task.poll_once(now);
        self.net.reap(&mut self.retire);
        while let Some(r) = self.build.try_pop() {
            self.built.push(r);
            let h = self.net.complete(r);
            self.hand.try_push(Handover::Ready(h)).unwrap();
        }
    }
    fn open(&mut self) {
        for now in 1..10 {
            self.poll(now);
        }
    }
    fn reply(&mut self, conn: u8, body: &[u8], now: u64) {
        let mut response = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        response.extend_from_slice(body);
        let chunks: Vec<_> = response.chunks(16000).collect();
        let wire = self.net.peer(ConnId(conn)).records(&chunks);
        let endpoint = if conn == 80 { 0 } else { 1 };
        let c = self
            .task
            .transport_mut(A1)
            .unwrap()
            .pool_mut(endpoint)
            .unwrap()
            .get(ConnId(conn))
            .unwrap();
        crate::support::mempeer::deliver(c, now, now, wire);
    }
    fn sequence(&mut self, request: AcctReq, now: u64, body: &[u8]) -> Vec<Vec<u8>> {
        let kind = request.kind;
        let seq = request.seq_no;
        self.tx.try_push(request).unwrap();
        let mut records = Vec::new();
        let mut step = 0;
        for tick in 0..100 {
            self.poll(now + tick);
            let ep = if kind == AcctQueryKind::Config {
                step % 2
            } else {
                0
            };
            let pending = self
                .task
                .transport_mut(A1)
                .unwrap()
                .pool_mut(ep)
                .unwrap()
                .pending_conn(u64::from(seq));
            if let Some(conn) = pending {
                self.reply(conn.0, body, now + tick);
                step += 1;
            }
            while let Some(b) = self.out.peek() {
                records.push(b.to_vec());
                self.out.advance();
            }
            if records
                .last()
                .is_some_and(|b| AcctRecHdr::read(b).unwrap().what != 1)
            {
                break;
            }
        }
        assert_eq!(AcctRecHdr::read(records.last().unwrap()).unwrap().what, 2);
        records
    }
}
impl Drop for AccountHarness {
    fn drop(&mut self) {
        self.net.reap(&mut self.retire);
        for ep in 0..2 {
            let p = self.task.transport_mut(A1).unwrap().pool_mut(ep).unwrap();
            p.retire_all();
            while let Some(r) = p.take_retired() {
                self.net.c().reclaim(r);
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[path = "support/no_wake_trace.rs"]
#[allow(dead_code)]
mod trace;
fn mark() {
    #[cfg(target_os = "linux")]
    trace::mark();
}
#[test]
fn ven_036_no_wake_to_warm() {
    #[cfg(target_os = "linux")]
    if trace::role().is_none() {
        let parts = trace::strace_segments("final_round::ven_036_no_wake_to_warm", Some("futex,write"));
        assert_eq!(parts.len(), 100);
        for p in &parts {
            assert_eq!(trace::n(p, "futex"), 0);
            assert_eq!(trace::n(p, "write"), 0);
        }
        return;
    }
    #[cfg(target_os = "linux")]
    trace::register_traced_thread();
    let mut counts = [0; 3];
    for _ in 0..100 {
        stub::set_cfg(|c| *c = Default::default());
        let (mut f, _) = crate::bbo_live::refresh_case(&[0, 1], &[0]);
        let mut g = gw_full::<StubProto>(GwCfg { aux_every: 1, ..g0() }, &[A1], Default::default());
        let (p, mut q) = spsc(8, Backing::Heap).unwrap();
        g.rt.set_account_queries(p);
        g.cmd(base(hft_event::GwOp::Rotate, A1));
        stub::log_enabled(false);
        mark();
        f.rt.poll_once(600_000_000);
        for k in 0..8 {
            g.rt.poll_once(3_000_000 + k);
        }
        g.rt.poll_once(1_004_000_000);
        mark();
        while f.snapreq.as_mut().unwrap().try_pop().is_some() {
            counts[0] += 1;
        }
        while q.try_pop().is_some() {
            counts[1] += 1;
        }
        let retired = g.net.reap(&mut g.retire);
        counts[2] += retired.len();
        while let Some(r) = g.build.try_pop() {
            assert_eq!(r.role, hft_event::ConnRole::Order);
            g.handoff.try_push(Handover::Failed(r.conn)).unwrap();
        }
        stub::log_enabled(true);
    }
    assert_eq!(counts, [200, 100, 100]);
}
#[test]
fn ven_158_feed_and_snapshot_contexts_remain_independent() {
    fn run(interleave: bool) -> (Vec<hft_event::Bbo>, Vec<Vec<u8>>) {
        use zerocopy::IntoBytes;
        let (mut feed, _) = bbo_up(|c| c.bbo_ping_ms = 60_000, &[S1]);
        let mut snap = crate::snap_pool::Harness::new(1);
        snap.open();
        stub::log_enabled(false);
        let mut a = Vec::new();
        let mut b = Vec::new();
        let feed_one = |feed: &mut Fh, n: u64, a: &mut Vec<_>| {
            let mut body = crate::support::feedh::b("S1", n);
            body.extend_from_slice(b"|ctx");
            feed.net.push(ConnId(0), &body);
            feed.poll(100_000 + n * 2_000_000);
            a.extend(bbos(&feed.drain_bbo()));
        };
        let snap_one = |snap: &mut crate::snap_pool::Harness, n: u64, b: &mut Vec<_>| {
            let now = 100_000 + n * 2_000_000;
            snap.req(n as u32, S1);
            snap.task.poll_once(now);
            let mut body = crate::support::feedh::b("S1", n);
            body.extend_from_slice(b"|ctx");
            snap.response(64, 200, &body, now + 1);
            snap.task.poll_once(now + 1);
            for (h, tail) in snap.records() {
                let mut bytes = h.as_bytes().to_vec();
                bytes.extend_from_slice(&tail);
                b.push(bytes);
            }
        };
        for n in 1..=10000 {
            feed_one(&mut feed, n, &mut a);
            if interleave {
                snap_one(&mut snap, n, &mut b);
            }
        }
        if !interleave {
            for n in 1..=10000 {
                snap_one(&mut snap, n, &mut b);
            }
        }
        stub::log_enabled(true);
        assert_eq!((a.len(), b.len()), (10000, 10000));
        assert!(a.windows(2).all(|p| p[1].h.ex_send_ns == p[0].h.ex_send_ns + 1));
        (a, b)
    }
    assert_eq!(run(true), run(false));
}
#[test]
fn ven_321_raw_coverage_includes_account_requests_and_connections() {
    use crate::support::stub::{Stub, V_USER_STREAM};
    stub::set_cfg(|c| *c = Default::default());
    let mut h = gw_full::<Stub<2048, V_USER_STREAM>>(g0(), &[A1], Default::default());
    let owner = h.rt.private_owner(A1).unwrap();
    for n in 0..3 {
        h.net.push(owner, b"E|listenKeyExtended");
        h.poll(3000 + n);
        h.reps();
    }
    let mut account = AccountHarness::new();
    account.open();
    let (queries, mut requests) = spsc(8, Backing::Heap).unwrap();
    h.rt.set_account_queries(queries);
    for i in 0..8 {
        h.poll(3_000_000 + i);
    }
    let request = requests.try_pop().unwrap();
    let records = account.sequence(request, 3_100_000, &1i64.to_le_bytes());
    for r in records {
        h.acct.reserve(r.len() as u32).unwrap().copy_from_slice(&r);
        h.acct.commit();
        h.poll(2_000_000);
        h.reps();
    }
    let raw = h.raw.records();
    let connects: Vec<_> = raw.iter().filter(|r| r.hdr.what == 6).collect();
    assert!(
        connects
            .iter()
            .any(|r| r.hdr.conn == ConnId(112) && r.payload == [A1.0, 2])
    );
    assert!(
        connects
            .iter()
            .any(|r| r.hdr.conn == owner && r.payload == [A1.0, 3])
    );
    assert_eq!(
        raw.iter().filter(|r| r.payload == b"E|listenKeyExtended").count(),
        3
    );
    let responses: Vec<_> = raw
        .iter()
        .filter(|r| r.kind == EventKind::RAW_RX && r.hdr.what == 3)
        .collect();
    assert_eq!(responses.len(), 3);
    assert_eq!(
        responses.iter().map(|r| r.hdr.conn.0).collect::<Vec<_>>(),
        vec![80, 81, 80]
    );
    assert!(responses.iter().all(|r| r.payload == 1i64.to_le_bytes()));
    assert_eq!(
        raw.iter()
            .filter(|r| r.kind == EventKind::RAW_TX && r.payload == b"\x00/account")
            .count(),
        3,
        "tx={:?}",
        raw.iter()
            .filter(|r| r.kind == EventKind::RAW_TX)
            .map(|r| &r.payload)
            .collect::<Vec<_>>()
    );
    // Even when no response arrives, raw metadata keeps the submitted pool connection.
    let mut timed = request;
    timed.seq_no += 1;
    account.tx.try_push(timed).unwrap();
    account.poll(4_000_000);
    for tick in 0..8 {
        account.poll(4_000_000_000 + tick);
    }
    let mut timeout = Vec::new();
    while let Some(b) = account.out.peek() {
        timeout.push(AcctRecHdr::read(b).unwrap());
        account.out.advance();
    }
    assert_eq!(timeout.len(), 2);
    assert_eq!((timeout[0].what, timeout[0].conn), (1, ConnId(80)));
    assert_eq!((timeout[1].what, timeout[1].fail), (3, AcctFail::Timeout));
}
#[test]
fn ven_320_redaction_scan_across_account_and_gateway_paths() {
    use crate::support::{
        mempeer, net,
        stub::{Stub, V_SIGN},
    };
    use hft_event::{GwOp, RawHdr};
    stub::set_cfg(|c| {
        *c = Default::default();
        c.rest_query = true;
        c.keepalive = true;
    });
    let mut h = gw_with::<Stub<2048, V_SIGN>, 2048>(
        GwCfg {
            inflight_timeout_ms: 60000,
            ..g0()
        },
        &[A1],
        Default::default(),
    )
    .unwrap();
    h.poll(1000);
    let build = h.builds.remove(0);
    let authenticated = crate::session::login_handoff(&mut h.net, build);
    h.handoff.try_push(Handover::Ready(authenticated)).unwrap();
    h.poll(2000);
    h.open(2001, 1);
    h.cmd(hft_event::GwCmd::set_epoch(5, false, 0));
    h.poll(2002);
    h.reps();
    h.rt.set_rest_pools(
        A1,
        vec![(
            EndpointId(0),
            RestPool::with_lanes(b"a.test", &net::net_cfg(), mempeer::clock(), 2, 0).unwrap(),
        )],
    )
    .unwrap();
    h.open(3000, 2);
    let mut account = AccountHarness::new();
    account.open();
    let mut account_raw = Vec::new();
    for n in 1..=10u32 {
        let now = u64::from(n) * 1_000_000_000;
        let c = h.place(A1, n, 1, S1, false);
        h.cmd(c);
        h.poll(now);
        h.reps();
        let c = h.targeted(GwOp::Cancel, A1, n);
        h.cmd(c);
        h.poll(now + 1);
        h.reps();
        let c = h.targeted(GwOp::Query, A1, n);
        h.cmd(c);
        h.poll(now + 2);
        h.reps();
        for op in [GwOp::Query, GwOp::SessionKeepalive] {
            if op == GwOp::SessionKeepalive {
                h.cmd(base(op, A1));
                h.poll(now + 20);
            }
            let wire = h
                .net
                .peer(ConnId(114))
                .records(&[b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"]);
            mempeer::deliver(h.rt.conn(ConnId(114)).unwrap(), now + 30, now + 30, wire);
            for k in 0..8 {
                h.poll(now + 30 + k);
                h.reps();
            }
        }
        h.cmd(hft_event::GwCmd {
            arg: 30000,
            ..base(GwOp::DeadManRefresh, A1)
        });
        h.poll(now + 40);
        h.reps();
        let mut ag = gw_full::<StubProto>(GwCfg { aux_every: 1, ..g0() }, &[A1], Default::default());
        let (queries, mut requests) = spsc(8, Backing::Heap).unwrap();
        ag.rt.set_account_queries(queries);
        let (bulk, mut tail) = byte_spsc(1 << 20, Backing::Heap).unwrap();
        ag.rt.set_rep_bulk(bulk);
        for kind in [AcctQueryKind::Config, AcctQueryKind::Margin] {
            ag.poll(now + 50);
            let request = requests.try_pop().unwrap();
            assert_eq!(request.kind, kind);
            for r in account.sequence(request, now + 60, &1i64.to_le_bytes()) {
                ag.acct.reserve(r.len() as u32).unwrap().copy_from_slice(&r);
                ag.acct.commit();
                ag.poll(now + 100);
                ag.reps();
                while tail.peek().is_some() {
                    tail.advance();
                }
            }
        }
        account_raw.extend(ag.raw.records());
    }
    let mut raw = h.raw.records();
    raw.extend(account_raw);
    for r in &raw {
        for secret in [
            b"test-signed-value".as_slice(),
            b"acct-key-sentinel",
            b"gateway-rest-test",
            b"SENTINEL-KEY-0123456789",
            b"LOGIN",
        ] {
            assert!(!r.payload.windows(secret.len()).any(|w| w == secret));
        }
    }
    let tx: Vec<_> = raw
        .iter()
        .filter(|r| r.kind == EventKind::RAW_TX && (r.payload.is_empty() || r.payload.starts_with(b"\x00/")))
        .collect();
    assert!(
        tx.len() >= 70,
        "tx={}, total rawtx={}",
        tx.len(),
        raw.iter().filter(|r| r.kind == EventKind::RAW_TX).count()
    );
    assert!(tx.iter().all(|r| r.hdr.rflags & RawHdr::REDACTED != 0));
    assert!(tx.iter().all(|r| !r.payload.contains(&b'?')));
}
