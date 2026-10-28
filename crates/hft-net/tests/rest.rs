use crate::support::mempeer::{self, MemPeer, Opts};
use hft_net::sock::{RecvKind, RecvStep, SendKind, SendStep};
use hft_net::*;

const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
fn req(tag: u64) -> RestReq<'static> {
    RestReq {
        method: Method::Get,
        path_query: b"/q?a=1",
        headers: &[(b"X-K", b"v")],
        body: b"",
        tag,
    }
}
fn setup<const N: usize>(rx: u32) -> (RestPool<N>, Vec<MemPeer>) {
    setup_cfg(NetConfig {
        rx_buf: RoleBytes::all(rx),
        rest_timeout_ms: 2000,
        ..NetConfig::default()
    })
}
fn rest_conn(cfg: &NetConfig, id: u8) -> (Handoff, MemPeer) {
    let (c, s) = mempeer::pair_with(Opts {
        id: ConnId(id),
        role: ConnRole::Rest,
        cfg: ConnCfg::from_net(cfg, ConnRole::Rest),
        ..Opts::default()
    });
    (Handoff::test_from_conn(c), s)
}
fn setup_cfg<const N: usize>(cfg: NetConfig) -> (RestPool<N>, Vec<MemPeer>) {
    let mut p = RestPool::new(b"a.test", &cfg, mempeer::clock());
    let mut peers = Vec::new();
    for id in 0..N {
        let (c, s) = mempeer::pair_with(Opts {
            id: ConnId(id as u8 + 1),
            role: ConnRole::Rest,
            cfg: ConnCfg::from_net(&cfg, ConnRole::Rest),
            ..Opts::default()
        });
        p.install(Handoff::test_from_conn(c)).unwrap();
        peers.push(s);
    }
    (p, peers)
}
fn deliver<const N: usize>(p: &mut RestPool<N>, s: &mut MemPeer, id: u8, b: &[u8], ts: u64) {
    let wire = s.records(&[b]);
    mempeer::deliver(p.get(ConnId(id)).unwrap(), ts, ts, wire);
}
fn reads<const N: usize>(p: &mut RestPool<N>, id: u8) -> u64 {
    p.get(ConnId(id)).unwrap().sys().recvmsg
}
#[test]
fn net_260_content_length() {
    let (mut p, mut s) = setup::<2>(65536);
    p.submit(&req(11), 0).unwrap();
    s[0].pump(p.get(ConnId(1)).unwrap());
    let sent = String::from_utf8(s[0].client_plain.clone()).unwrap();
    for part in [
        "GET /q?a=1 HTTP/1.1\r\n",
        "Host: a.test\r\n",
        "Accept-Encoding: identity\r\n",
        "Connection: keep-alive\r\n",
        "X-K: v\r\n",
    ] {
        assert!(sent.contains(part));
    }
    deliver(&mut p, &mut s[0], 1, OK, 100);
    let r = p.poll(100).unwrap().unwrap();
    assert_eq!((r.tag, r.status, r.body), (11, 200, b"hello".as_slice()));
    assert!(
        r.headers[..r.header_count]
            .iter()
            .any(|h| h.name == "Content-Length")
    );
    assert!(p.take_retired().is_none());
}
#[test]
fn net_263_connection_close() {
    let (mut p, mut s) = setup::<1>(65536);
    p.submit(&req(1), 0).unwrap();
    deliver(
        &mut p,
        &mut s[0],
        1,
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
        1,
    );
    assert_eq!(p.poll(1).unwrap().unwrap().body, b"ok");
    assert_eq!(p.take_retired().unwrap().id(), ConnId(1));
    assert_eq!(p.next_build(1), Some(BuildWhy::Fill));
    let (mut p, mut s) = setup::<1>(65536);
    p.submit(&req(2), 0).unwrap();
    let mut wire = s[0].records(&[OK]);
    wire.extend(s[0].close_notify());
    mempeer::deliver(p.get(ConnId(1)).unwrap(), 2, 2, wire);
    assert_eq!(p.poll(2).unwrap().unwrap().body, b"hello");
    assert!(p.take_retired().is_some());
    assert_eq!(p.next_build(2), Some(BuildWhy::Fill));
}
#[test]
fn net_264_timeout_boundary() {
    let (mut p, mut s) = setup::<2>(65536);
    p.submit(&req(21), 0).unwrap();
    assert!(p.poll(1_999_999_999).is_none());
    assert_eq!(
        p.poll(2_000_000_000).unwrap().unwrap_err(),
        RestFail {
            tag: 21,
            err: RestErr::Timeout
        }
    );
    assert_eq!(p.take_retired().unwrap().id(), ConnId(1));
    assert!(p.get(ConnId(1)).is_none());
    p.submit(&req(22), 2_000_000_001).unwrap();
    deliver(&mut p, &mut s[1], 2, OK, 2_000_000_002);
    let r = p.poll(2_000_000_002).unwrap().unwrap();
    assert_eq!((r.tag, r.body), (22, b"hello".as_slice()));
}
#[test]
fn net_265_too_large() {
    for total in [4073, 4074, 4075] {
        let (mut p, mut s) = setup::<1>(4096);
        p.submit(&req(1), 0).unwrap();
        let mut wire = b"HTTP/1.1 200 OK\r\nContent-Length: 4034\r\n\r\n".to_vec();
        let len = total - wire.len();
        wire = format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\n\r\n").into_bytes();
        wire.resize(total, b'a');
        deliver(&mut p, &mut s[0], 1, &wire, 1);
        let r = p.poll(1).unwrap();
        if total <= 4074 {
            assert_eq!(r.unwrap().body.len(), len);
        } else {
            assert_eq!(r.unwrap_err().err, RestErr::TooLarge);
            assert!(p.take_retired().is_some());
        }
    }
    let (mut p, mut s) = setup::<1>(4096);
    p.submit(&req(1), 0).unwrap();
    deliver(
        &mut p,
        &mut s[0],
        1,
        b"HTTP/1.1 200 OK\r\nContent-Length: 5000\r\n\r\n",
        1,
    );
    assert_eq!(p.poll(1).unwrap().unwrap_err().err, RestErr::TooLarge);
}
#[test]
fn net_266_closed_mid_response() {
    let (mut p, mut s) = setup::<1>(65536);
    p.submit(&req(1), 0).unwrap();
    deliver(
        &mut p,
        &mut s[0],
        1,
        b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort",
        1,
    );
    assert!(p.poll(1).is_none());
    p.get(ConnId(1)).unwrap().script().unwrap().push_recv(RecvStep {
        pre_tsc: 2,
        post_tsc: 2,
        kind: RecvKind::Eof,
    });
    assert_eq!(p.poll(2).unwrap().unwrap_err().err, RestErr::Closed);
    assert!(p.take_retired().is_some());
}
#[test]
fn net_267_poll_only_inflight() {
    let (mut p, _) = setup::<2>(65536);
    assert!(p.poll(0).is_none());
    assert_eq!((reads(&mut p, 1), reads(&mut p, 2)), (0, 0));
    p.submit(&req(1), 0).unwrap();
    assert!(p.poll(1).is_none());
    assert_eq!((reads(&mut p, 1), reads(&mut p, 2)), (1, 0));
    p.submit(&req(2), 2).unwrap();
    assert!(p.poll(3).is_none());
    assert_eq!((reads(&mut p, 1), reads(&mut p, 2)), (2, 1));
}
#[test]
fn net_268_idle_check_detects_close() {
    for row in 0..5 {
        let (mut p, mut s) = setup::<2>(65536);
        match row {
            1 | 2 => p.get(ConnId(2)).unwrap().script().unwrap().push_recv(RecvStep {
                pre_tsc: 1,
                post_tsc: 1,
                kind: if row == 1 {
                    RecvKind::Eof
                } else {
                    RecvKind::Errno(104)
                },
            }),
            3 => {
                let b = s[1].close_notify();
                mempeer::deliver(p.get(ConnId(2)).unwrap(), 1, 1, b);
            }
            4 => deliver(&mut p, &mut s[1], 2, OK, 1),
            _ => {}
        }
        assert_eq!(p.idle_check(1), 2);
        let r = p.take_retired();
        if row == 0 {
            assert!(r.is_none());
        } else {
            let r = r.unwrap();
            assert_eq!(r.id(), ConnId(2));
            assert!(r.why.contains(
                [
                    ErrBits::empty(),
                    ErrBits::PEER_EOF,
                    ErrBits::RESET,
                    ErrBits::TLS_ALERT,
                    ErrBits::HTTP_PROTO
                ][row]
            ));
            assert_eq!(p.next_build(1), Some(BuildWhy::Fill));
        }
    }
}
#[test]
fn net_269_idle_check_skips_inflight() {
    let (mut p, _) = setup::<2>(65536);
    p.submit(&req(1), 0).unwrap();
    assert_eq!(p.idle_check(1), 1);
    assert_eq!((reads(&mut p, 1), reads(&mut p, 2)), (0, 1));
}
#[test]
fn net_271_submit_errors() {
    let (mut p, _) = setup::<2>(65536);
    let seq = p.get(ConnId(1)).unwrap().test_tx_seq();
    let big = vec![0; 20000];
    assert_eq!(
        p.submit(&RestReq { body: &big, ..req(1) }, 0),
        Err(RestErr::ReqTooLarge)
    );
    assert_eq!(p.get(ConnId(1)).unwrap().test_tx_seq(), seq);
    assert_eq!(p.get(ConnId(1)).unwrap().sys().sendmsg, 0);
    p.submit(&req(1), 0).unwrap();
    p.submit(&req(2), 0).unwrap();
    assert_eq!(p.submit(&req(3), 0), Err(RestErr::Busy));
    let mut empty = RestPool::<1>::new(b"a.test", &NetConfig::default(), mempeer::clock());
    assert_eq!(empty.submit(&req(1), 0), Err(RestErr::NoConn));
}
#[test]
fn net_272_submit_send_error() {
    let (mut p, _) = setup::<1>(65536);
    p.get(ConnId(1))
        .unwrap()
        .script()
        .unwrap()
        .send
        .push_back(SendStep {
            post_tsc: 1,
            kind: SendKind::Errno(104),
        });
    assert_eq!(p.submit(&req(1), 0), Err(RestErr::Send(SendErr::Io(104))));
    assert_eq!(p.take_retired().unwrap().id(), ConnId(1));
}
#[test]
fn net_273_one_inflight_per_conn() {
    let (mut p, mut s) = setup::<1>(65536);
    p.submit(&req(1), 0).unwrap();
    assert_eq!(p.submit(&req(2), 0), Err(RestErr::Busy));
    s[0].pump(p.get(ConnId(1)).unwrap());
    assert_eq!(s[0].recs.len(), 1);
    deliver(&mut p, &mut s[0], 1, OK, 1);
    p.poll(1).unwrap().unwrap();
    p.submit(&req(2), 2).unwrap();
    s[0].pump(p.get(ConnId(1)).unwrap());
    assert_eq!(s[0].recs.len(), 2);
    assert_eq!(p.get(ConnId(1)).unwrap().sys().sendmsg, 2);
}
#[test]
fn net_274_rest_resp_stamps() {
    let (mut p, mut s) = setup::<1>(65536);
    p.submit(&req(1), 0).unwrap();
    let a = s[0].records(&[&OK[..20]]);
    let b = s[0].records(&[&OK[20..]]);
    mempeer::deliver_stamped(p.get(ConnId(1)).unwrap(), 999, 1000, a, 40, 41);
    assert!(p.poll(1000).is_none());
    mempeer::deliver_stamped(p.get(ConnId(1)).unwrap(), 1999, 2000, b, 50, 51);
    let r = p.poll(2000).unwrap().unwrap();
    assert_eq!((r.conn, r.sent_tsc), (ConnId(1), 0));
    assert_eq!(r.ts.hw_rx_ns, 40);
    assert_eq!(r.ts.rx_tsc, 2000);
}
#[test]
fn net_275_rest_pool_build() {
    let mut p = RestPool::<2>::new(b"a.test", &NetConfig::default(), mempeer::clock());
    assert_eq!(p.next_build(0), Some(BuildWhy::Fill));
    assert_eq!(p.next_build(0), None);
    for id in 1..=3 {
        let (c, _) = mempeer::pair_with(Opts {
            id: ConnId(id),
            role: ConnRole::Rest,
            ..Opts::default()
        });
        let r = p.install(Handoff::test_from_conn(c));
        if id < 3 {
            assert!(r.is_ok());
            assert_eq!(p.next_build(0), if id == 1 { Some(BuildWhy::Fill) } else { None });
        } else {
            assert_eq!(r, Err(PoolErr::Full));
        }
    }
}
#[test]
fn net_276_only_submitted_requests_advance_tls() {
    let (mut p, mut s) = setup::<1>(65536);
    p.submit(&req(1), 0).unwrap();
    s[0].pump(p.get(ConnId(1)).unwrap());
    deliver(&mut p, &mut s[0], 1, OK, 1);
    p.poll(1).unwrap().unwrap();
    let big = vec![0; 20000];
    assert_eq!(
        p.submit(&RestReq { body: &big, ..req(2) }, 2),
        Err(RestErr::ReqTooLarge)
    );
    p.submit(&req(3), 3).unwrap();
    s[0].pump(p.get(ConnId(1)).unwrap());
    assert_eq!(s[0].recs.iter().map(|r| r.index).collect::<Vec<_>>(), [0, 1]);
}
#[test]
fn net_277_hot_rest_never_manages_connections() {
    let (mut p, mut s) = setup::<1>(65536);
    for turn in 0..10_000 {
        let now = turn * 10;
        let c = p.get(ConnId(1)).unwrap();
        c.script().unwrap().send.push_back(SendStep {
            post_tsc: now,
            kind: SendKind::Bytes(7),
        });
        p.submit(&req(turn), now).unwrap();
        assert!(p.poll(now + 1).is_none());
        s[0].pump(p.get(ConnId(1)).unwrap());
        deliver(&mut p, &mut s[0], 1, OK, now + 2);
        assert_eq!(p.poll(now + 2).unwrap().unwrap().tag, turn);
        assert_eq!(p.idle_check(now + 3), 1);
        assert!(p.take_retired().is_none());
    }
    let sys = p.get(ConnId(1)).unwrap().sys();
    assert_eq!(
        (
            sys.connect,
            sys.shutdown,
            sys.close,
            sys.getaddrinfo,
            sys.epoll_ctl
        ),
        (0, 0, 0, 0, 0)
    );
    p.get(ConnId(1)).unwrap().script().unwrap().push_recv(RecvStep {
        pre_tsc: 100001,
        post_tsc: 100001,
        kind: RecvKind::Eof,
    });
    assert_eq!(p.idle_check(100001), 1);
    assert!(p.take_retired().is_some());
}

#[test]
fn net_264_cancel_retires_lane_and_discards_late_response() {
    let (mut p, mut s) = setup::<2>(65536);
    p.submit(&req(21), 0).unwrap();
    deliver(&mut p, &mut s[0], 1, OK, 1);
    p.cancel(21);
    assert!(p.poll(2).is_none());
    assert_eq!(p.take_retired().unwrap().id(), ConnId(1));
    p.submit(&req(22), 3).unwrap();
    deliver(&mut p, &mut s[1], 2, OK, 4);
    assert_eq!(p.poll(4).unwrap().unwrap().tag, 22);
    p.retire_all();
    assert_eq!(p.take_retired().unwrap().id(), ConnId(2));
    assert!(p.get(ConnId(2)).is_none());
}

#[test]
fn net_278_rest_timeout_from_config() {
    let (mut p, mut s) = setup_cfg::<2>(NetConfig {
        rest_timeout_ms: 5000,
        ..NetConfig::default()
    });
    // .1 a slow answer inside the configured timeout is read.
    p.submit(&req(31), 0).unwrap();
    assert!(p.poll(2_000_000_000).is_none(), "NET-278.1");
    deliver(&mut p, &mut s[0], 1, OK, 4_999_999_999);
    let r = p.poll(4_999_999_999).unwrap().unwrap();
    assert_eq!((r.tag, r.body), (31, b"hello".as_slice()), "NET-278.1");
    assert!(p.take_retired().is_none(), "NET-278.1");
    // .2 no answer: Timeout exactly at the configured value.
    p.submit(&req(32), 10_000_000_000).unwrap();
    assert!(p.poll(14_999_999_999).is_none(), "NET-278.2");
    assert_eq!(
        p.poll(15_000_000_000).unwrap().unwrap_err(),
        RestFail {
            tag: 32,
            err: RestErr::Timeout
        },
        "NET-278.2"
    );
    assert_eq!(p.take_retired().unwrap().id(), ConnId(1), "NET-278.2");
}

#[test]
fn net_279_idle_connection_recycled() {
    const IDLE: u64 = 25_000_000_000;
    // .1 rest_max_idle_ms = 0: an idle connection is never recycled.
    let (mut p, _s) = setup::<2>(65536);
    for now in [0, IDLE, 1_000 * IDLE] {
        assert_eq!(p.idle_check(now), 2, "NET-279.1");
    }
    assert!(p.take_retired().is_none(), "NET-279.1");
    let cfg = NetConfig {
        rest_max_idle_ms: 25_000,
        ..NetConfig::default()
    };
    let (mut p, mut s) = setup_cfg::<2>(cfg.clone());
    // .2 both idle since the first check; one is retired when it reaches the age, not before.
    p.idle_check(0);
    p.idle_check(IDLE - 1);
    assert!(p.take_retired().is_none(), "NET-279.2");
    p.idle_check(IDLE);
    let r = p.take_retired().unwrap();
    assert_eq!(r.id(), ConnId(1), "NET-279.2");
    assert!(r.why.contains(ErrBits::IDLE_TIMEOUT), "NET-279.2");
    assert!(p.take_retired().is_none(), "NET-279.2");
    assert_eq!(p.next_build(IDLE), Some(BuildWhy::Fill), "NET-279.2");
    // .3 while the replacement is built the other aged connection stays and serves requests.
    p.idle_check(IDLE + 1);
    assert!(p.take_retired().is_none(), "NET-279.3");
    p.submit(&req(41), IDLE + 2).unwrap();
    assert_eq!(p.pending_conn(41), Some(ConnId(2)), "NET-279.3");
    deliver(&mut p, &mut s[1], 2, OK, IDLE + 3);
    assert_eq!(p.poll(IDLE + 3).unwrap().unwrap().tag, 41, "NET-279.3");
    // .4 the fresh connection installed; the other one's age restarted at its response.
    let (h, _fresh) = rest_conn(&cfg, 3);
    assert_eq!(p.install(h), Ok(ConnId(3)), "NET-279.4");
    p.idle_check(IDLE + 4);
    p.idle_check(2 * IDLE + 2);
    assert!(p.take_retired().is_none(), "NET-279.4");
    p.idle_check(2 * IDLE + 3);
    assert_eq!(p.take_retired().unwrap().id(), ConnId(2), "NET-279.4");
    // .5 an in-flight request is never recycled; the fresh connection carries it.
    p.submit(&req(42), 2 * IDLE + 4).unwrap();
    assert_eq!(p.pending_conn(42), Some(ConnId(3)), "NET-279.5");
    let (h, _other) = rest_conn(&cfg, 4);
    p.next_build(2 * IDLE + 4);
    p.install(h).unwrap();
    p.idle_check(10 * IDLE);
    assert!(p.take_retired().is_none(), "NET-279.5");
    p.idle_check(11 * IDLE);
    let r = p.take_retired().unwrap();
    assert_eq!(r.id(), ConnId(4), "NET-279.5");
    assert_eq!(p.pending_conn(42), Some(ConnId(3)), "NET-279.5");
}

/// `RestPool::<16>::with_lanes(lanes, reserve)` with `open` of its lanes installed (ids 1..).
fn lanes(lanes: usize, reserve: usize, open: u8) -> (RestPool<16>, Vec<MemPeer>) {
    let cfg = NetConfig {
        rx_buf: RoleBytes::all(65536),
        ..NetConfig::default()
    };
    let mut p = RestPool::<16>::with_lanes(b"a.test", &cfg, mempeer::clock(), lanes, reserve).unwrap();
    let mut peers = Vec::new();
    for id in 1..=open {
        let (h, s) = rest_conn(&cfg, id);
        p.install(h).unwrap();
        peers.push(s);
    }
    (p, peers)
}

#[test]
fn net_377_lane_count_from_constructor() {
    let cfg = NetConfig::default();
    let make = |n, r| RestPool::<16>::with_lanes(b"a.test", &cfg, mempeer::clock(), n, r);
    // .1 sizes outside 1..=N, or a reserve not below the lane count, are refused.
    for (n, r) in [(0, 0), (17, 0), (4, 4), (4, 5)] {
        assert!(make(n, r).is_none(), "NET-377.1 {n}/{r}");
    }
    // .2 four lanes: four builds and installs, then the pool is full.
    let mut p = make(4, 0).unwrap();
    assert_eq!(p.lanes(), 4, "NET-377.2");
    for id in 1..=5u8 {
        let want = p.next_build(0);
        let (h, _s) = rest_conn(&cfg, id);
        if id <= 4 {
            assert_eq!(want, Some(BuildWhy::Fill), "NET-377.2 {id}");
            assert_eq!(p.install(h), Ok(ConnId(id)), "NET-377.2 {id}");
        } else {
            assert_eq!(want, None, "NET-377.2");
            assert_eq!(p.install(h), Err(PoolErr::Full), "NET-377.2");
        }
    }
    // .3 four concurrent requests on four connections; the fifth is Busy.
    let (mut p, _s) = lanes(4, 0, 4);
    let mut conns = Vec::new();
    for tag in 1..=4 {
        p.submit(&req(tag), 0).unwrap();
        conns.push(p.pending_conn(tag).unwrap());
    }
    conns.sort();
    conns.dedup();
    assert_eq!(conns.len(), 4, "NET-377.3");
    assert_eq!(p.submit(&req(5), 0), Err(RestErr::Busy), "NET-377.3");
    assert_eq!(p.free_lanes(), 0, "NET-377.3");
    // .4 `new` uses every lane, none reserved.
    let p = RestPool::<3>::new(b"a.test", &cfg, mempeer::clock());
    assert_eq!(p.lanes(), 3, "NET-377.4");
}

#[test]
fn net_378_reserved_lanes_only_for_urgent_requests() {
    // .1 four lanes, one reserved: three requests, the fourth is Busy with one lane free.
    let (mut p, mut s) = lanes(4, 1, 4);
    for tag in 1..=3 {
        p.submit(&req(tag), 0).unwrap();
    }
    assert_eq!(p.submit(&req(4), 0), Err(RestErr::Busy), "NET-378.1");
    assert_eq!(p.free_lanes(), 1, "NET-378.1");
    assert_eq!(p.get(ConnId(4)).unwrap().sys().sendmsg, 0, "NET-378.1");
    // .2 an urgent request takes the reserved lane; then every lane is busy.
    p.submit_urgent(&req(10), 1).unwrap();
    assert_eq!(p.pending_conn(10), Some(ConnId(4)), "NET-378.2");
    assert_eq!(p.submit_urgent(&req(11), 1), Err(RestErr::Busy), "NET-378.2");
    // .3 one response frees one lane: still reserved for urgent requests.
    deliver(&mut p, &mut s[0], 1, OK, 2);
    assert_eq!(p.poll(2).unwrap().unwrap().tag, 1, "NET-378.3");
    assert_eq!(p.submit(&req(12), 3), Err(RestErr::Busy), "NET-378.3");
    p.submit_urgent(&req(13), 3).unwrap();
    assert_eq!(p.pending_conn(13), Some(ConnId(1)), "NET-378.3");
    // .4 no open connection: both kinds get NoConn.
    let (mut p, _s) = lanes(4, 1, 0);
    assert_eq!(p.submit(&req(1), 0), Err(RestErr::NoConn), "NET-378.4");
    assert_eq!(p.submit_urgent(&req(1), 0), Err(RestErr::NoConn), "NET-378.4");
    // .5 reserve 0: every lane serves ordinary requests (the default).
    let (mut p, _s) = lanes(2, 0, 2);
    p.submit(&req(1), 0).unwrap();
    p.submit(&req(2), 0).unwrap();
    assert_eq!(p.submit(&req(3), 0), Err(RestErr::Busy), "NET-378.5");
}
