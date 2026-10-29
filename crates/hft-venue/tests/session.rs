//! VEN-310, VEN-312, VEN-315: session set-up on the warm thread (README §7).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{ConnRole, GwCmd, SendStatus};
use hft_net::sock::{RecvStep, ScriptIo};
use hft_net::{
    BuildReq, ConnId, ConnPool, Connector, EndpointId, JobId, PoolMode, RotatePolicy, TICKET_WAIT_NS,
};
use hft_rt::{Backing, spsc};
use hft_types::AccountId;
use hft_venue::{SessEvent, SessionCfg, SessionDriver, SessionOp, streams};
use hft_venue_proto::{ArrayBuf, Credential, VenueCfg, VenueEnv};
use support::feedh::*;
use support::gwh::*;
use support::mempeer::{self, MemPeer};
use support::net::Net;
use support::rd::*;
use support::stub::{self, LoginMode, StubProto};
use support::ws_ref;

const SENTINEL: &[u8] = b"SENTINEL-KEY-0123456789";

fn cred() -> Credential {
    Credential {
        api_key: ArrayBuf::from_bytes(SENTINEL).unwrap(),
        ..Credential::default()
    }
}

fn scfg() -> SessionCfg {
    SessionCfg {
        venue: VenueCfg {
            env: VenueEnv::Prod,
            stale_guard_ms: 1500,
        },
        ready_bbo: None,
        ready_md: None,
    }
}

fn req(conn: u8, role: ConnRole) -> BuildReq {
    BuildReq {
        venue: V7,
        conn: ConnId(conn),
        role,
        endpoint: EndpointId(0),
        src_ip_idx: 0,
        rx_queue: 0,
        deadline_ns: i64::MAX,
        account: hft_types::AccountId(0),
    }
}

/// Runs a session to its event: handshake + Upgrade against `peer`, then login answers from
/// `answer` (login frame text → reply text).
fn run<T: hft_venue::AcctTransport>(
    drv: &mut SessionDriver<StubProto, T>,
    c: &mut Connector,
    job: JobId,
    peer: &mut MemPeer,
    t0: i64,
    mut answer: impl FnMut(&str) -> Option<String>,
) -> SessEvent {
    assert!(drv.drive_at(c, t0, t0 as u64).is_none());
    let out = c.script(job).unwrap().take_sent();
    peer.feed_handshake(&out);
    let back = peer.drain();
    c.script(job).unwrap().push_recv(RecvStep::data(0, 0, back));
    assert!(drv.drive_at(c, t0 + 1, (t0 + 1) as u64).is_none());
    let out = c.script(job).unwrap().take_sent();
    peer.feed_handshake(&out);
    let back = peer.answer_upgrade();
    c.script(job).unwrap().push_recv(RecvStep::data(0, 0, back));
    let mut t = t0 + 2;
    if let Some(e) = drv.drive_at(c, t, t as u64) {
        return e;
    }
    t += TICKET_WAIT_NS;
    for _ in 0..20 {
        if let Some(e) = drv.drive_at(c, t, t as u64) {
            return e;
        }
        t += if drv.rest_mut().is_some() {
            1_000_000_000
        } else {
            1
        };
        let Some(s) = c.script(job) else { continue };
        let out = s.take_sent();
        peer.feed_client(&out);
        for f in peer.client_frames() {
            let text = String::from_utf8_lossy(&f.payload).into_owned();
            if let Some(r) = answer(&text) {
                let b = peer.frames(&[ws_ref::text(r.as_bytes())]);
                c.script(job).unwrap().push_recv(RecvStep::data(0, 0, b));
            }
        }
    }
    panic!("session did not finish");
}

fn start(
    drv: &mut SessionDriver<StubProto>,
    net: &mut Net,
    r: BuildReq,
    acct: Option<AccountId>,
) -> (JobId, MemPeer) {
    net.c().push_script(ScriptIo::new());
    let job = drv.start(net.c(), r, acct, 1).unwrap();
    (job, mempeer::server_only(0))
}

#[test]
fn ven_310_session_flow() {
    check_prepared_bootstrap_reconnect_authenticates_again();
    // .1: Md, no bootstrap, no login.
    let mut net = Net::new(64, 16);
    let mut drv = SessionDriver::<StubProto>::new(scfg(), vec![]);
    stub::take_log();
    let (job, mut peer) = start(&mut drv, &mut net, req(3, ConnRole::Md), None);
    let ev = run(&mut drv, net.c(), job, &mut peer, 10, |_| None);
    let SessEvent::Ready(h) = ev else { panic!("{ev:?}") };
    assert_eq!(h.id(), ConnId(3), "VEN-310.1");
    let names: Vec<&str> = stub::take_log()
        .iter()
        .map(|c| c.name)
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|n| *n != "classify")
        .collect();
    assert_eq!(
        names.iter().filter(|n| **n == "endpoint").count(),
        0,
        "VEN-310.1 endpoint"
    );
    assert_eq!(
        drv.log().take(1).collect::<Vec<_>>(),
        vec![SessionOp::Submit(ConnId(3))]
    );
    let mut p = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
    p.install(h, None).unwrap();
    net.c()
        .reclaim(p.retire(ConnId(3), hft_net::ErrBits::empty()).unwrap());
    // .3: Order, Continue then Ok → two login frames.
    stub::set_cfg(|c| c.login = LoginMode::Two);
    let mut drv = SessionDriver::<StubProto>::new(scfg(), vec![(A1, cred())]);
    let (job, mut peer) = start(&mut drv, &mut net, req(112, ConnRole::Order), Some(A1));
    let mut seen = Vec::new();
    let ev = run(&mut drv, net.c(), job, &mut peer, 100, |t| {
        seen.push(t.to_string());
        if t.starts_with("LOGIN0|") {
            Some("LOGIN_CONT".into())
        } else if t.starts_with("LOGIN1|") {
            Some("LOGIN_OK".into())
        } else {
            None
        }
    });
    assert!(matches!(ev, SessEvent::Ready(_)), "VEN-310.3 {ev:?}");
    assert_eq!(seen.len(), 2, "VEN-310.3 {seen:?}");
    if let SessEvent::Ready(h) = ev {
        let mut p = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
        p.install(h, None).unwrap();
        net.c()
            .reclaim(p.retire(ConnId(112), hft_net::ErrBits::empty()).unwrap());
    }
    // .5: Sent: one login frame, no answer awaited.
    stub::set_cfg(|c| c.login = LoginMode::Sent);
    let (job, mut peer) = start(&mut drv, &mut net, req(114, ConnRole::Order), Some(A1));
    let ev = run(&mut drv, net.c(), job, &mut peer, 150, |_| None);
    assert!(matches!(ev, SessEvent::Ready(_)), "VEN-310.5 {ev:?}");
    let frames: Vec<_> = drv
        .log()
        .filter(|o| matches!(o, SessionOp::LoginFrame(ConnId(114), _)))
        .collect();
    assert_eq!(frames, vec![SessionOp::LoginFrame(ConnId(114), 0)], "VEN-310.5");
    if let SessEvent::Ready(h) = ev {
        let mut p = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
        p.install(h, None).unwrap();
        net.c()
            .reclaim(p.retire(ConnId(114), hft_net::ErrBits::empty()).unwrap());
    }
    // .4: Rejected(4001).
    stub::set_cfg(|c| c.login = LoginMode::One);
    let free0 = net.c().test_free_slots();
    let (job, mut peer) = start(&mut drv, &mut net, req(113, ConnRole::Order), Some(A1));
    let ev = run(&mut drv, net.c(), job, &mut peer, 200, |t| {
        t.starts_with("LOGIN0|").then(|| "LOGIN_REJ|4001".into())
    });
    stub::set_cfg(|c| c.login = LoginMode::None);
    match ev {
        SessEvent::Failed { conn, code } => assert_eq!((conn, code), (ConnId(113), 4001), "VEN-310.4"),
        e => panic!("VEN-310.4 {e:?}"),
    }
    assert_eq!(net.c().test_free_slots(), free0, "VEN-310.4: reclaimed");
    assert!(drv.log().any(|o| o == SessionOp::Reclaim(ConnId(113))));
}

fn check_prepared_bootstrap_reconnect_authenticates_again() {
    stub::set_cfg(|c| {
        c.bootstrap = true;
        c.login = LoginMode::One;
    });
    let mut net = Net::new(64, 16);
    let mut drv = SessionDriver::<StubProto>::new(scfg(), vec![(A1, cred())]);
    for (i, body) in [b"dummy-listen-one".as_slice(), b"dummy-listen-two"]
        .into_iter()
        .enumerate()
    {
        let r = req(112 + i as u8, ConnRole::UserStream);
        assert!(matches!(
            drv.start(net.c(), r, Some(A1), 1),
            Err(hft_venue::SessErr::Bootstrap)
        ));
        net.c().push_script(ScriptIo::new());
        let job = drv.start_prepared(net.c(), r, Some(A1), 1, Some(body)).unwrap();
        let mut peer = mempeer::server_only(0);
        let mut authenticated = false;
        let event = run(&mut drv, net.c(), job, &mut peer, 10, |frame| {
            if frame.starts_with("LOGIN0|") {
                authenticated = true;
                Some("LOGIN_OK".into())
            } else {
                None
            }
        });
        assert!(authenticated);
        let SessEvent::Ready(h) = event else {
            panic!("{event:?}")
        };
        let mut pool = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
        pool.install(h, None).unwrap();
        drv.retire(net.c(), pool.retire(r.conn, hft_net::ErrBits::empty()).unwrap());
    }
    stub::set_cfg(|c| {
        c.bootstrap = false;
        c.login = LoginMode::None;
    });
}

#[test]
fn ven_312_ready_epfd_register() {
    let mut net = Net::new(64, 16);
    let rp = net.ready_poll();
    let mut cfg = scfg();
    cfg.ready_bbo = Some(rp.handle());
    let mut drv = SessionDriver::<StubProto>::new(cfg, vec![]);
    let (job, mut peer) = start(&mut drv, &mut net, req(3, ConnRole::Md), None);
    let SessEvent::Ready(h) = run(&mut drv, net.c(), job, &mut peer, 10, |_| None) else {
        panic!()
    };
    let log: Vec<SessionOp> = drv.log().collect();
    let add = log
        .iter()
        .position(|o| *o == SessionOp::ReadyAdd(ConnId(3)))
        .unwrap();
    let hand = log
        .iter()
        .position(|o| *o == SessionOp::Handoff(ConnId(3)))
        .unwrap();
    assert!(add < hand, "{log:?}");
    let mut p = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
    p.install(h, None).unwrap();
    assert_eq!(
        p.get(ConnId(3)).unwrap().sys().epoll_ctl,
        0,
        "no epoll_ctl on the hot side"
    );
    let m = p.retire(ConnId(3), hft_net::ErrBits::empty()).unwrap();
    drv.retire(net.c(), m);
    let log: Vec<SessionOp> = drv.log().collect();
    let del = log
        .iter()
        .position(|o| *o == SessionOp::ReadyDel(ConnId(3)))
        .unwrap();
    let rec = log
        .iter()
        .position(|o| *o == SessionOp::Reclaim(ConnId(3)))
        .unwrap();
    assert!(del < rec, "{log:?}");
    assert_eq!(rp.sys().epoll_ctl, 0);
}

#[test]
fn ven_315_credentials_warm_only() {
    // Warm thread: sessions for A1's primary and standby and one BBO connection.
    let warm = std::thread::spawn(|| {
        stub::set_cfg(|c| {
            c.login = LoginMode::One;
            c.session_identity = false;
        });
        let mut net = Net::new(64, 16);
        let mut drv = SessionDriver::<StubProto>::new(scfg(), vec![(A1, cred())]);
        let mut out = Vec::new();
        for (conn, role, acct) in [
            (112u8, ConnRole::Order, Some(A1)),
            (113, ConnRole::Order, Some(A1)),
            (0, ConnRole::Md, None),
        ] {
            let (job, mut peer) = start(&mut drv, &mut net, req(conn, role), acct);
            let ev = run(
                &mut drv,
                net.c(),
                job,
                &mut peer,
                10 + i64::from(conn) * 1000,
                |t| t.starts_with("LOGIN0|").then(|| "LOGIN_OK".into()),
            );
            let SessEvent::Ready(h) = ev else { panic!("{ev:?}") };
            out.push((h, peer));
        }
        let log = stub::take_log();
        (out, log, net)
    });
    let (sessions, warm_log, net) = warm.join().unwrap();
    let warm_id = warm_log.first().unwrap().thread;
    for name in ["login", "endpoint"] {
        let calls: Vec<_> = warm_log.iter().filter(|c| c.name == name).collect();
        assert!(!calls.is_empty(), "{name} called");
        assert!(calls.iter().all(|c| c.thread == warm_id));
    }
    // gw thread: install A1's two order connections, 1000 rounds of orders. The warm net (its
    // connector built the connections) travels with them and comes back for the feed thread.
    let (gw_sessions, md_sessions): (Vec<_>, Vec<_>) = sessions
        .into_iter()
        .partition(|(h, _)| h.role() == ConnRole::Order);
    let gw_t = std::thread::spawn(move || {
        let mut h = gw_with::<StubProto, 2048>(g0(), &[A1], GhOpts::default()).unwrap();
        let mut warm_net = net;
        std::mem::swap(&mut h.net, &mut warm_net);
        let mut t = 1;
        for (ho, peer) in gw_sessions {
            h.poll(t);
            t += 1;
            let b = h.builds.remove(0);
            assert_eq!(b.conn, ho.id());
            h.net.adopt(ho.id(), peer);
            h.handoff.try_push(hft_venue::Handover::Ready(ho)).unwrap();
            h.poll(t);
            t += 1;
        }
        h.cmd(GwCmd::set_epoch(5, false, 0));
        h.poll(t);
        let mut frames = Vec::new();
        for k in 0..1000u64 {
            let c = h.place(A1, (k % 2000) as u32 + 1, 1, S1, false);
            h.cmd(c);
            h.poll(1_000_000 + k * 200_000_000);
            for id in [ConnId(112), ConnId(113)] {
                frames.extend(h.net.sent(&mut h.rt, id).into_iter().map(|f| f.payload));
            }
        }
        let ok = reports(&h.reps())
            .iter()
            .filter(|r| r.status == SendStatus::Written)
            .count();
        // Return the connections to the warm connector, then hand the warm net back.
        h.rt.retire_all(0);
        let mut retire = std::mem::replace(&mut h.retire, spsc(2, Backing::Heap).unwrap().1);
        h.net.reap(&mut retire);
        std::mem::swap(&mut h.net, &mut warm_net);
        (frames, stub::take_log(), ok, warm_net)
    });
    let (frames, gw_log, ok, net) = gw_t.join().unwrap();
    assert!(ok > 0);
    assert!(!frames.is_empty());
    for f in &frames {
        assert!(
            !f.windows(SENTINEL.len()).any(|w| w == SENTINEL),
            "gw frame carries the credential"
        );
    }
    for name in [
        "login",
        "endpoint",
        "session_identity_request",
        "private_subscribe",
    ] {
        assert!(!gw_log.iter().any(|c| c.name == name), "{name} on the gw thread");
    }
    // feed thread: the BBO connection.
    let feed_t = std::thread::spawn(move || {
        let mut h = feed(hft_venue::FeedCfg {
            routes: hft_venue::FeedRoutes::Bbo,
            bbo_conns: 1,
            ..f0()
        });
        let mut warm_net = net;
        std::mem::swap(&mut h.net, &mut warm_net);
        h.sub(S1, streams::BBO);
        h.poll(1);
        for (ho, peer) in md_sessions {
            let b = h.builds.remove(0);
            assert_eq!(b.conn, ho.id());
            h.net.adopt(ho.id(), peer);
            h.handoff.try_push(hft_venue::Handover::Ready(ho)).unwrap();
        }
        let mut frames = Vec::new();
        for k in 0..100u64 {
            h.poll(2 + k * 10_000_000);
            frames.extend(h.net.sent(&mut h.rt, ConnId(0)).into_iter().map(|f| f.payload));
        }
        h.rt.retire_all(0);
        let mut retire = std::mem::replace(&mut h.retire, spsc(2, Backing::Heap).unwrap().1);
        h.net.reap(&mut retire);
        std::mem::swap(&mut h.net, &mut warm_net);
        (frames, stub::take_log())
    });
    let (frames, feed_log) = feed_t.join().unwrap();
    for f in &frames {
        assert!(!f.windows(SENTINEL.len()).any(|w| w == SENTINEL));
    }
    for name in [
        "login",
        "endpoint",
        "session_identity_request",
        "private_subscribe",
    ] {
        assert!(
            !feed_log.iter().any(|c| c.name == name),
            "{name} on the feed thread"
        );
    }
}

#[test]
fn ven_313_session_uid_handover_requires_evidence() {
    for (required, observed, status) in [(false, false, 0), (true, true, 1), (true, false, 2)] {
        stub::set_cfg(|c| {
            c.login = LoginMode::None;
            c.session_identity = required;
        });
        let mut net = Net::new(64, 16);
        let mut drv = SessionDriver::<StubProto>::new(scfg(), vec![(A1, cred())]);
        let (job, mut peer) = start(&mut drv, &mut net, req(112, ConnRole::Order), Some(A1));
        let mut uid = [0; 24];
        uid[..2].copy_from_slice(b"U1");
        if observed {
            drv.verified_identity(job, uid).unwrap();
        }
        let ev = run(&mut drv, net.c(), job, &mut peer, 10, |_| None);
        let SessEvent::Ready(h) = ev else { panic!("{ev:?}") };
        let mut p = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
        p.install(h, None).unwrap();
        let data = p.get(ConnId(112)).unwrap().handoff_data();
        assert_eq!(&data[..8], b"PRIV0001");
        assert_eq!(data[8], status);
        assert_eq!(&data[9..33], if observed { &uid } else { &[0; 24] });
        drv.retire(net.c(), p.retire(ConnId(112), hft_net::ErrBits::empty()).unwrap());
    }
}

#[derive(Default)]
struct SessionHttp {
    active: bool,
    fail: bool,
    attempts: Vec<u64>,
}
impl hft_venue::AcctTransport for SessionHttp {
    fn submit(
        &mut self,
        _: &hft_venue_proto::RestReqBuf,
        _: u64,
        now: u64,
    ) -> Result<ConnId, hft_net::RestErr> {
        assert!(!self.active);
        self.active = true;
        self.attempts.push(now);
        Ok(ConnId(80))
    }
    fn poll(&mut self, _: u64) -> Option<Result<hft_venue::AcctResponse<'_>, hft_net::RestErr>> {
        if !self.active {
            return None;
        }
        self.active = false;
        if self.fail {
            return Some(Err(hft_net::RestErr::Timeout));
        }
        Some(Ok(hft_venue::AcctResponse {
            status: 200,
            body: b"U1",
            conn: ConnId(80),
            ts: Default::default(),
        }))
    }
    fn abort(&mut self) {
        self.active = false;
    }
    fn idle_check(&mut self, _: u64) {}
}
#[test]
fn ven_314_session_uid_failed_after_bounded_backoff() {
    for fail in [false, true] {
        stub::set_cfg(|c| {
            *c = Default::default();
            c.session_identity = true;
        });
        let mut net = Net::new(0, 1);
        let mut drv = SessionDriver::<StubProto, SessionHttp>::new(scfg(), vec![(A1, cred())]);
        drv.set_rest(
            SessionHttp {
                fail,
                ..Default::default()
            },
            hft_venue::GwCfg::default(),
            A1,
            cred(),
        )
        .unwrap();
        net.c().push_script(ScriptIo::new());
        let job = drv
            .start(net.c(), req(112, ConnRole::Order), Some(A1), 1)
            .unwrap();
        let mut peer = mempeer::server_only(0);
        let ev = run(&mut drv, net.c(), job, &mut peer, 10, |_| None);
        let SessEvent::Ready(handoff) = ev else {
            panic!("{ev:?}")
        };
        let mut pool = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
        pool.install(handoff, None).unwrap();
        let data = pool.get(ConnId(112)).unwrap().handoff_data();
        assert_eq!(data[8], if fail { 2 } else { 1 });
        if !fail {
            assert_eq!(&data[9..11], b"U1");
        }
        let a = &drv.rest_mut().unwrap().attempts;
        assert_eq!(a.len(), if fail { 3 } else { 1 });
        if fail {
            assert!(a[1] - a[0] >= 1_000_000_000);
            assert!(a[2] - a[1] >= 2_000_000_000);
        }
        net.c()
            .reclaim(pool.retire(ConnId(112), hft_net::ErrBits::empty()).unwrap());
    }
}
#[test]
fn ven_394_bootstrap_rest_precedes_endpoint() {
    stub::set_cfg(|c| {
        *c = Default::default();
        c.bootstrap = true;
    });
    let mut net = Net::new(0, 1);
    let mut drv = SessionDriver::<StubProto, SessionHttp>::new(scfg(), vec![(A1, cred())]);
    let mut rest = cred();
    rest.api_key = hft_venue_proto::ArrayBuf::default();
    rest.api_key.push(b"bootstrap-rest-test").unwrap();
    drv.set_rest(SessionHttp::default(), hft_venue::GwCfg::default(), A1, rest)
        .unwrap();
    assert!(drv.enqueue(req(121, ConnRole::UserStream), Some(A2)).is_err());
    drv.enqueue(req(120, ConnRole::UserStream), Some(A1)).unwrap();
    assert!(drv.enqueue(req(120, ConnRole::UserStream), Some(A1)).is_err());
    stub::take_log();
    assert!(drv.drive_at(net.c(), 1, 1).is_none());
    assert!(
        stub::calls("bootstrap")
            .iter()
            .any(|c| c.arg.contains("rest_key=true"))
    );
    assert!(stub::calls("endpoint").is_empty());
    net.c().push_script(ScriptIo::new());
    assert!(drv.drive_at(net.c(), 2, 2).is_none());
    let ep = stub::calls("endpoint");
    assert_eq!(ep.len(), 1);
    assert!(ep[0].arg.contains("boot=true"));
    let job = drv.job(ConnId(120)).unwrap();
    // Connector was driven once by drive_at: finish it using its actual TLS peer.
    let mut peer = mempeer::server_only(0);
    peer.feed_handshake(&net.c().script(job).unwrap().take_sent());
    let back = peer.drain();
    net.c().script(job).unwrap().push_recv(RecvStep::data(3, 3, back));
    assert!(drv.drive_at(net.c(), 3, 3).is_none());
    peer.feed_handshake(&net.c().script(job).unwrap().take_sent());
    let back = peer.answer_upgrade();
    net.c().script(job).unwrap().push_recv(RecvStep::data(4, 4, back));
    let mut ev = None;
    for t in [4, 4 + TICKET_WAIT_NS, 5 + TICKET_WAIT_NS] {
        if ev.is_none() {
            ev = drv.drive_at(net.c(), t, t as u64);
        }
    }
    let SessEvent::Ready(h) = ev.unwrap() else {
        panic!("bootstrap session failed")
    };
    let mut pool = ConnPool::<1>::new(PoolMode::Race, 1, RotatePolicy::default());
    pool.install(h, None).unwrap();
    net.c()
        .reclaim(pool.retire(ConnId(120), hft_net::ErrBits::empty()).unwrap());
}

/// Authenticate a scripted socket before handing it to the measured gateway. The login frame
/// really contains the sentinel; only the authenticated connection crosses to the hot runtime.
pub(crate) fn login_handoff(net: &mut Net, r: BuildReq) -> hft_net::Handoff {
    stub::set_cfg(|c| c.login = LoginMode::One);
    let mut drv = SessionDriver::<StubProto>::new(scfg(), vec![(A1, cred())]);
    let (job, mut peer) = start(&mut drv, net, r, Some(A1));
    let mut authenticated = false;
    let event = run(&mut drv, net.c(), job, &mut peer, 100, |text| {
        assert!(text.as_bytes().windows(SENTINEL.len()).any(|w| w == SENTINEL));
        authenticated = true;
        Some("LOGIN_OK".into())
    });
    stub::set_cfg(|c| c.login = LoginMode::None);
    assert!(authenticated);
    let SessEvent::Ready(h) = event else {
        panic!("login failed: {event:?}")
    };
    h
}
