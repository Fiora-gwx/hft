//! NET-212, NET-286: system calls of the hot entries.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::support;

use hft_net::sock::{RecvKind, RecvStep, ScriptIo, SendKind, SendStep};
use hft_net::{
    BuildEvent, BuildReq, ConnArena, ConnId, ConnPool, ConnRole, Connector, EndpointId, ErrBits, MaskedFrame,
    NetConfig, Opcode, PoolMode, RotatePolicy, SysCount,
};
use hft_types::VenueId;
use support::mempeer::{self, ws_build};
use support::ws_ref::{server_frame, text};

fn cfg() -> NetConfig {
    NetConfig {
        endpoints: vec![hft_net::Endpoint {
            host: "a.test".into(),
            sni: "a.test".into(),
            port: 443,
            path: "/ws".into(),
            ips: vec![std::net::Ipv4Addr::LOCALHOST],
            upgrade_headers: Vec::new(),
        }],
        src_ips: vec![std::net::Ipv4Addr::LOCALHOST],
        bind_device: vec![(std::net::Ipv4Addr::LOCALHOST, "lo".into())],
        extra_ca: Some(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/certs/ca.pem").into()),
        endpoint_override: true,
        ts_expect_hw: false,
        ..NetConfig::default()
    }
}

fn forbidden(s: &SysCount) -> [u64; 5] {
    [s.connect, s.shutdown, s.close, s.getaddrinfo, s.epoll_ctl]
}

#[test]
fn net_212_hot_never_connects() {
    let cfg = cfg();
    let arena = ConnArena::new(&[(ConnRole::Md, 1)], &cfg, mempeer::clock());
    let mut c = Connector::new(arena, &cfg).unwrap();
    c.push_script(ScriptIo {
        connect: 0,
        ..ScriptIo::new()
    });
    let job = c
        .submit(BuildReq {
            venue: VenueId(1),
            conn: ConnId(9),
            role: ConnRole::Md,
            endpoint: EndpointId(0),
            src_ip_idx: 0,
            rx_queue: 0,
            deadline_ns: i64::MAX,
            account: hft_types::AccountId(0),
        })
        .unwrap();
    let mut peer = mempeer::server_only(0);
    let ev = ws_build(&mut c, job, &mut peer, 1_000);
    assert!(matches!(ev, BuildEvent::Ready { .. }), "{ev:?}");
    let h = c.handoff(job).unwrap();
    let mut pool: ConnPool<2> = ConnPool::new(PoolMode::PrimaryStandby, 1, RotatePolicy::default());
    let id = pool.install(h, None).unwrap();
    let before = forbidden(pool.get(id).unwrap().sys());
    let mut f = MaskedFrame::<2048>::new();
    let mut frames = 0u32;
    for round in 0..10_000u32 {
        let conn = pool.get(id).unwrap();
        // Script: data (frames and a Ping), EAGAIN, short writes; a Close then EOF at the end.
        match round % 4 {
            0 => {
                let b = peer.frames(&[text(b"md"), server_frame(true, 0x9, b"p")]);
                conn.script().unwrap().push_recv(RecvStep::data(1, 2, b));
            }
            1 => conn.script().unwrap().push_recv(RecvStep::eagain(3)),
            2 => conn.script().unwrap().push_send(SendStep {
                post_tsc: 4,
                kind: SendKind::Bytes(7),
            }),
            _ => {}
        }
        if round == 9_000 {
            let b = peer.frames(&[server_frame(true, 0x8, &[0x03, 0xE8])]);
            conn.script().unwrap().push_recv(RecvStep::data(5, 6, b));
        }
        if round == 9_500 {
            conn.script().unwrap().push_recv(RecvStep {
                pre_tsc: 7,
                post_tsc: 8,
                kind: RecvKind::Eof,
            });
        }
        conn.poll_rx(0);
        while conn.next_frame().is_some() {
            frames += 1;
        }
        f.unseal();
        f.body_mut()[..3].copy_from_slice(b"ord");
        f.seal(Opcode::Text, 3, round);
        let _ = conn.send_frame(&f);
        let _ = conn.send_text(b"sub", 0);
        let _ = conn.send_ping(round as u64);
        conn.finish_poll(0);
        let _ = conn.flush();
        // Keep the server's view consistent while the connection is alive.
        let sent = conn.script().unwrap().take_sent();
        if conn.phase() == hft_net::ConnPhase::Open {
            peer.feed_client(&sent);
        }
    }
    assert_eq!(frames, 2_251, "every data frame before the Close was read");
    assert_eq!(
        peer.client_frames().iter().filter(|f| f.opcode == 0xA).count(),
        2_251
    );
    let after = forbidden(pool.get(id).unwrap().sys());
    assert_eq!(after, before, "connect, shutdown, close, getaddrinfo, epoll_ctl");
    let m = pool.retire(id, ErrBits::PEER_EOF).unwrap();
    assert_eq!(m.id(), id);
    assert_eq!(c.sys().getaddrinfo, 0);
}

#[cfg(target_os = "linux")]
#[test]
fn net_286_feed_round_syscalls() {
    use support::looppeer::*;
    let peer = LoopPeer::start(LoopOpts::default());
    let mut c = connector(peer.port, &[(ConnRole::Md, 12)]);
    let mut rp = hft_net::ReadyPoll::new().unwrap();
    let hd = rp.handle();
    let mut p = pool();
    let mut pcs = Vec::new();
    for id in 1..=12u8 {
        let (h, pc) = open(&mut c, &peer, id, ConnRole::Md);
        let id = p.install(h, None).unwrap();
        hd.add(p.get(id).unwrap()).unwrap();
        pcs.push(pc);
    }
    let mut x = support::xorshift::XorShift64Star(0x5EED);
    let mut out = hft_net::ConnSet::default();
    let mut sent = 0u64;
    let mut got = 0u64;
    for _ in 0..1000 {
        let k = x.next_u32() % 4;
        for _ in 0..k {
            let i = (x.next_u32() % 12) as usize;
            pcs[i].text(b"tick");
            sent += 1;
        }
        std::thread::sleep(std::time::Duration::from_micros(300));
        let s0 = *rp.sys();
        let before: Vec<SysCount> = (1..=12u8).map(|i| *p.get(ConnId(i)).unwrap().sys()).collect();
        let n = rp.probe(&mut out);
        assert_eq!(rp.sys().epoll_pwait, s0.epoll_pwait + 1);
        assert_eq!(rp.sys().last_epoll_timeout, 0);
        for id in out.iter() {
            let conn = p.get(id).unwrap();
            conn.poll_rx(0);
            while conn.next_frame().is_some() {
                got += 1;
            }
            conn.finish_poll(0);
        }
        let mut recv = 0;
        for (k, i) in (1..=12u8).enumerate() {
            let a = p.get(ConnId(i)).unwrap().sys();
            recv += a.recvmsg - before[k].recvmsg;
            assert_eq!(a.sendmsg, before[k].sendmsg);
            assert_eq!(a.setsockopt, before[k].setsockopt);
            assert_eq!(a.total() - before[k].total(), a.recvmsg - before[k].recvmsg);
        }
        assert_eq!(recv, n as u64, "one recvmsg per ready connection");
    }
    // Drain the rest.
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(1));
        rp.probe(&mut out);
        for id in out.iter() {
            let conn = p.get(id).unwrap();
            conn.poll_rx(0);
            while conn.next_frame().is_some() {
                got += 1;
            }
        }
        if got == sent {
            break;
        }
    }
    assert_eq!(got, sent);
}
