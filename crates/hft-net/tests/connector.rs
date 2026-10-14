//! NET-190–NET-213: `Connector` — build stages, failures, timeouts, `submit` checks, socket
//! options, NAPI/MSS reads, source address, trust, ClientHello. Scripted cases run everywhere;
//! loopback cases (`LoopPeer`) are Linux-only.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::support;

use std::net::Ipv4Addr;

use hft_net::sock::{ScriptIo, errno};
use hft_net::{
    BuildErr, BuildEvent, BuildFail, BuildReq, ConnArena, ConnId, ConnPhase, ConnRole, Connector, Endpoint,
    EndpointId, NetConfig,
};
use hft_types::VenueId;
use support::mempeer::{self, MemPeer};

const T0: i64 = 1_000_000_000_000;

fn script_cfg() -> NetConfig {
    NetConfig {
        endpoints: vec![
            Endpoint {
                host: "a.test".into(),
                sni: "a.test".into(),
                port: 443,
                path: "/ws".into(),
                ips: vec![Ipv4Addr::LOCALHOST],
                upgrade_headers: Vec::new(),
            },
            Endpoint {
                host: "localhost".into(),
                sni: "a.test".into(),
                port: 443,
                path: "/ws".into(),
                ips: Vec::new(),
                upgrade_headers: Vec::new(),
            },
        ],
        src_ips: vec![Ipv4Addr::new(10, 0, 0, 5)],
        bind_device: vec![(Ipv4Addr::new(10, 0, 0, 5), "eth1".into())],
        extra_ca: Some(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/certs/ca.pem").into()),
        endpoint_override: true,
        connect_timeout_ms: 3000,
        tls_timeout_ms: 3000,
        upgrade_timeout_ms: 3000,
        ts_expect_hw: false,
        ..NetConfig::default()
    }
}

fn sreq(conn: u8, role: ConnRole, deadline_ns: i64) -> BuildReq {
    BuildReq {
        venue: VenueId(1),
        conn: ConnId(conn),
        role,
        endpoint: EndpointId(0),
        src_ip_idx: 0,
        rx_queue: 0,
        deadline_ns,
    }
}

fn script_connector(cfg: &NetConfig, plan: &[(ConnRole, u16)]) -> Connector {
    let arena = ConnArena::new(plan, cfg, mempeer::clock());
    Connector::new(arena, cfg).unwrap()
}

fn connect_now() -> ScriptIo {
    ScriptIo {
        connect: 0,
        ..ScriptIo::new()
    }
}

#[test]
fn net_208_reconnect_backoff_is_bounded_and_isolated() {
    let mut cfg = script_cfg();
    cfg.reconnect_backoff_ms = (100, 250);
    cfg.endpoints[1] = cfg.endpoints[0].clone();
    let mut c = script_connector(&cfg, &[(ConnRole::Md, 2)]);
    let mut now = T0;
    c.push_script(ScriptIo {
        connect: 111,
        ..ScriptIo::new()
    });
    c.submit(sreq(1, ConnRole::Md, i64::MAX)).unwrap();
    assert!(matches!(c.drive(now), Some(BuildEvent::Failed { .. })));
    for delay in [100_000_000, 200_000_000, 250_000_000] {
        c.push_script(ScriptIo {
            connect: 111,
            ..ScriptIo::new()
        });
        c.submit(sreq(1, ConnRole::Md, i64::MAX)).unwrap();
        assert_eq!(c.drive(now + delay - 1), None);
        now += delay;
        assert!(matches!(c.drive(now), Some(BuildEvent::Failed { .. })));
    }
    c.push_script(ScriptIo {
        connect: 111,
        ..ScriptIo::new()
    });
    let mut other = sreq(2, ConnRole::Md, i64::MAX);
    other.endpoint = EndpointId(1);
    c.submit(other).unwrap();
    assert!(matches!(c.drive(now), Some(BuildEvent::Failed { .. })));
}

/// Moves the client's bytes to the server and the server's answer back into the script.
fn exchange(c: &mut Connector, job: hft_net::JobId, peer: &mut MemPeer) {
    let out = c.script(job).unwrap().take_sent();
    peer.feed_handshake(&out);
    let back = peer.drain();
    if !back.is_empty() {
        c.script(job)
            .unwrap()
            .push_recv(hft_net::sock::RecvStep::data(0, 0, back));
    }
}

#[test]
fn net_199_stage_timeouts() {
    let rows = [
        ("NET-199.1", ConnPhase::TcpConnecting, 2_999_999_999i64, false),
        ("NET-199.2", ConnPhase::TcpConnecting, 3_000_000_000, true),
        ("NET-199.3", ConnPhase::TlsHandshake, 2_999_999_999, false),
        ("NET-199.4", ConnPhase::TlsHandshake, 3_000_000_000, true),
        ("NET-199.5", ConnPhase::WsUpgrade, 2_999_999_999, false),
        ("NET-199.6", ConnPhase::WsUpgrade, 3_000_000_000, true),
    ];
    for (row, stage, off, fires) in rows {
        let cfg = script_cfg();
        let mut c = script_connector(&cfg, &[(ConnRole::Md, 1)]);
        let script = match stage {
            ConnPhase::TcpConnecting => ScriptIo {
                connect: errno::EINPROGRESS,
                writable_after: u32::MAX,
                ..ScriptIo::new()
            },
            _ => connect_now(),
        };
        c.push_script(script);
        let job = c.submit(sreq(1, ConnRole::Md, T0 + 60_000_000_000)).unwrap();
        assert_eq!(c.drive(T0), None, "{row}");
        let start = if stage == ConnPhase::WsUpgrade {
            let mut peer = mempeer::server_only(0);
            exchange(&mut c, job, &mut peer);
            let t1 = T0 + 1_000;
            assert_eq!(c.drive(t1), None, "{row}");
            assert_eq!(c.phase(job), Some(ConnPhase::WsUpgrade), "{row}");
            t1
        } else {
            T0
        };
        assert_eq!(c.phase(job), Some(stage), "{row}");
        let ev = c.drive(start + off);
        if fires {
            assert_eq!(
                ev,
                Some(BuildEvent::Failed {
                    job,
                    stage,
                    why: BuildFail::Timeout
                }),
                "{row}"
            );
        } else {
            assert_eq!(ev, None, "{row}");
        }
    }
}

#[test]
fn net_200_deadline_first() {
    let cfg = script_cfg();
    let mut c = script_connector(&cfg, &[(ConnRole::Md, 1)]);
    c.push_script(ScriptIo {
        connect: errno::EINPROGRESS,
        writable_after: u32::MAX,
        ..ScriptIo::new()
    });
    let job = c.submit(sreq(1, ConnRole::Md, T0 + 1_000_000_000)).unwrap();
    assert_eq!(c.drive(T0), None);
    assert_eq!(c.drive(T0 + 999_999_999), None);
    assert_eq!(
        c.drive(T0 + 1_000_000_000),
        Some(BuildEvent::Failed {
            job,
            stage: ConnPhase::TcpConnecting,
            why: BuildFail::Timeout
        })
    );
}

#[test]
fn net_202_submit_errors() {
    let mk = |cfg: &NetConfig, slots: u16| script_connector(cfg, &[(ConnRole::Md, slots)]);
    // NET-202.1 / .6: 16 jobs.
    let cfg = script_cfg();
    let mut c = mk(&cfg, 32);
    for k in 0..15 {
        assert!(c.submit(sreq(k, ConnRole::Md, T0)).is_ok());
    }
    assert!(c.submit(sreq(15, ConnRole::Md, T0)).is_ok(), "NET-202.6");
    assert_eq!(
        c.submit(sreq(16, ConnRole::Md, T0)),
        Err(BuildErr::JobsFull),
        "NET-202.1"
    );
    // NET-202.2: no free slot.
    let mut c = mk(&cfg, 0);
    assert_eq!(
        c.submit(sreq(1, ConnRole::Md, T0)),
        Err(BuildErr::ArenaFull),
        "NET-202.2"
    );
    // NET-202.3: endpoint 2 of 2.
    let mut c = mk(&cfg, 32);
    let mut r = sreq(1, ConnRole::Md, T0);
    r.endpoint = EndpointId(2);
    assert_eq!(c.submit(r), Err(BuildErr::BadEndpoint), "NET-202.3");
    // NET-202.4: src_ip_idx out of range.
    let mut r = sreq(1, ConnRole::Md, T0);
    r.src_ip_idx = 1;
    assert_eq!(c.submit(r), Err(BuildErr::BadSrcIp), "NET-202.4");
    // NET-202.5: the source address has no bind_device entry.
    let mut cfg5 = script_cfg();
    cfg5.src_ips = vec![Ipv4Addr::new(10, 0, 0, 6)];
    let mut c = mk(&cfg5, 32);
    assert_eq!(
        c.submit(sreq(1, ConnRole::Md, T0)),
        Err(BuildErr::BadSrcIp),
        "NET-202.5"
    );
}

#[test]
fn net_203_not_ready() {
    let cfg = script_cfg();
    let mut c = script_connector(&cfg, &[(ConnRole::Md, 1)]);
    c.push_script(connect_now());
    let job = c.submit(sreq(1, ConnRole::Md, T0 + 60_000_000_000)).unwrap();
    assert_eq!(c.drive(T0), None);
    assert_eq!(c.phase(job), Some(ConnPhase::TlsHandshake));
    assert!(c.conn_mut(job).is_none());
    assert_eq!(c.handoff(job).unwrap_err(), BuildErr::NotReady);
}

#[test]
fn net_204_ticket_wait() {
    for (row, tickets, at, ready) in [
        ("NET-204.1", 1usize, 1_000_000i64, true),
        ("NET-204.2", 0, 19_999_999, false),
        ("NET-204.3", 0, 20_000_000, true),
    ] {
        let cfg = script_cfg();
        let mut c = script_connector(&cfg, &[(ConnRole::Rest, 1)]);
        c.push_script(connect_now());
        let mut peer = mempeer::server_only(tickets);
        let job = c.submit(sreq(1, ConnRole::Rest, T0 + 60_000_000_000)).unwrap();
        assert_eq!(c.drive(T0 - 1_000), None, "{row}");
        exchange(&mut c, job, &mut peer);
        // The handshake completes at t0: Open.
        assert_eq!(c.drive(T0), None, "{row}");
        assert_eq!(c.phase(job), Some(ConnPhase::Open), "{row}");
        // The server reads the client Finished and (row 1) sends its ticket right away.
        exchange(&mut c, job, &mut peer);
        let ev = c.drive(T0 + at);
        assert_eq!(
            matches!(ev, Some(BuildEvent::Ready { .. })),
            ready,
            "{row}: {ev:?}"
        );
        if !ready {
            assert_eq!(ev, None, "{row}");
        }
    }
}

#[test]
fn net_208_no_events_after_failed() {
    let cfg = script_cfg();
    let mut c = script_connector(&cfg, &[(ConnRole::Md, 1)]);
    c.push_script(ScriptIo {
        connect: errno::ECONNREFUSED,
        ..ScriptIo::new()
    });
    let job = c.submit(sreq(1, ConnRole::Md, T0 + 60_000_000_000)).unwrap();
    assert_eq!(
        c.drive(T0),
        Some(BuildEvent::Failed {
            job,
            stage: ConnPhase::TcpConnecting,
            why: BuildFail::Errno(111)
        })
    );
    for k in 0..10 {
        assert_eq!(c.drive(T0 + k), None);
    }
    assert!(c.conn_mut(job).is_none());
    assert_eq!(c.handoff(job).unwrap_err(), BuildErr::NotReady);
}

#[test]
fn net_213_pinned_ips_no_dns() {
    for (row, ep) in [("NET-213.1", 0u8), ("NET-213.2", 1)] {
        let cfg = script_cfg();
        let mut c = script_connector(&cfg, &[(ConnRole::Md, 1)]);
        c.push_script(ScriptIo {
            connect: errno::EINPROGRESS,
            writable_after: u32::MAX,
            ..ScriptIo::new()
        });
        let mut r = sreq(1, ConnRole::Md, T0 + 60_000_000_000);
        r.endpoint = EndpointId(ep);
        let job = c.submit(r).unwrap();
        assert_eq!(c.drive(T0), None, "{row}");
        let target = c.test_conn(job).unwrap().sys().connect_addr.map(|a| a.0);
        if ep == 0 {
            assert_eq!(c.sys().getaddrinfo, 0, "{row}");
            assert_eq!(target, Some(Ipv4Addr::LOCALHOST), "{row}");
        } else {
            assert_eq!(c.sys().getaddrinfo, 1, "{row}");
            let want = std::net::ToSocketAddrs::to_socket_addrs(&("localhost", 443))
                .unwrap()
                .find_map(|a| match a {
                    std::net::SocketAddr::V4(v) => Some(*v.ip()),
                    _ => None,
                });
            assert_eq!(target, want, "{row}");
        }
    }
}

// ---------------------------------------------------------------------- loopback (Linux)

#[cfg(target_os = "linux")]
mod lo {
    use super::support::looppeer::*;
    use super::support::ws_ref;
    use hft_net::sock::opt;
    use hft_net::*;
    use std::net::Ipv4Addr;

    fn tls13_x25519() -> LoopOpts {
        LoopOpts {
            versions: vec![&rustls::version::TLS13],
            suites: Some(vec![
                rustls::crypto::aws_lc_rs::cipher_suite::TLS13_AES_128_GCM_SHA256,
            ]),
            kx_x25519_only: true,
            // The peer answers the Upgrade 20 ms late so that at least one drive sees `WsUpgrade`
            // (on a fast host the 101 can otherwise arrive within the drive that sent the request).
            upgrade_delay_ms: 20,
            ..LoopOpts::default()
        }
    }

    #[test]
    fn net_190_ws_happy_path() {
        let peer = LoopPeer::start(tls13_x25519());
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, phases) = drive_job(&mut c, job);
        let want = [
            ConnPhase::TcpConnecting,
            ConnPhase::TlsHandshake,
            ConnPhase::WsUpgrade,
            ConnPhase::Open,
        ];
        let mut it = phases.iter();
        for w in want {
            assert!(it.any(|p| *p == w), "phases {phases:?} miss {w:?}: {ev:?}");
        }
        // Loopback has no NAPI instance (0); WSL's mirrored `loopback0` has one.
        let napi = match ev {
            BuildEvent::Ready { napi_id, .. } if wsl_mirrored() => napi_id,
            _ => 0,
        };
        assert_eq!(
            ev,
            BuildEvent::Ready {
                job,
                tls: TlsInfo {
                    version: 0x0304,
                    suite: 0x1301,
                    group: 0x001D
                },
                napi_id: napi
            }
        );
        assert!(c.drive(now_ns()).is_none());
        assert!(c.handoff(job).is_ok());
    }

    #[test]
    fn net_191_rest_no_upgrade() {
        let peer = LoopPeer::start(LoopOpts {
            ws: false,
            ..LoopOpts::default()
        });
        let mut c = connector(peer.port, &[(ConnRole::Rest, 1)]);
        let job = c.submit(req(1, ConnRole::Rest)).unwrap();
        let (ev, phases) = drive_job(&mut c, job);
        assert!(matches!(ev, BuildEvent::Ready { .. }), "{ev:?}");
        assert!(!phases.contains(&ConnPhase::WsUpgrade), "{phases:?}");
        let mut pc = peer.accept();
        pc.s.sock
            .set_read_timeout(Some(std::time::Duration::from_millis(50)))
            .unwrap();
        let mut b = [0u8; 64];
        let n = std::io::Read::read(&mut pc.s, &mut b).unwrap_or(0);
        assert_eq!(n, 0, "no application data before Ready");
    }

    #[test]
    fn net_192_upgrade_request() {
        let peer = LoopPeer::start(LoopOpts::default());
        let mut cfg = net_cfg(peer.port);
        cfg.endpoints[0].path = "/ws?x=1".into();
        cfg.endpoints[0].upgrade_headers = vec![("X-MBX-APIKEY".into(), "k1".into())];
        let arena = ConnArena::new(&[(ConnRole::Md, 1)], &cfg, clock());
        let mut c = Connector::new(arena, &cfg).unwrap();
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        assert!(matches!(ev, BuildEvent::Ready { .. }), "{ev:?}");
        let pc = peer.accept();
        let t = String::from_utf8(pc.request.clone()).unwrap();
        let lines: Vec<&str> = t.split("\r\n").collect();
        assert_eq!(lines[0], "GET /ws?x=1 HTTP/1.1");
        for h in [
            "Host: a.test",
            "Upgrade: websocket",
            "Connection: Upgrade",
            "Sec-WebSocket-Version: 13",
            "X-MBX-APIKEY: k1",
        ] {
            assert!(lines.contains(&h), "missing {h}: {t}");
        }
        let key = lines
            .iter()
            .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
            .unwrap();
        assert_eq!(key.len(), 24);
        assert_eq!(b64_decode(key).len(), 16);
        assert!(!t.contains("Sec-WebSocket-Extensions"));
    }

    fn b64_decode(s: &str) -> Vec<u8> {
        let t = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut bits = 0u32;
        let mut n = 0;
        let mut out = Vec::new();
        for ch in s.bytes().filter(|c| *c != b'=') {
            let v = t.iter().position(|x| *x == ch).unwrap() as u32;
            bits = bits << 6 | v;
            n += 6;
            if n >= 8 {
                n -= 8;
                out.push((bits >> n) as u8);
            }
        }
        out
    }

    #[test]
    fn net_193_accept_check() {
        let rows = [
            (
                "NET-193.1",
                Upgrade::Accept("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=".into()),
                None,
            ),
            (
                "NET-193.2",
                Upgrade::Accept("s3pPLMBiTxaQ9kYGzzhZRbK+xOp=".into()),
                Some(BuildFail::BadAccept),
            ),
            ("NET-193.3", Upgrade::NoAccept, Some(BuildFail::BadAccept)),
        ];
        for (row, up, fail) in rows {
            let peer = LoopPeer::start(LoopOpts {
                upgrade: up,
                ..LoopOpts::default()
            });
            let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
            c.test_fix_ws_key(Some(*b"the sample nonce"));
            let job = c.submit(req(1, ConnRole::Md)).unwrap();
            let (ev, _) = drive_job(&mut c, job);
            match fail {
                None => assert!(matches!(ev, BuildEvent::Ready { .. }), "{row}: {ev:?}"),
                Some(why) => assert_eq!(
                    ev,
                    BuildEvent::Failed {
                        job,
                        stage: ConnPhase::WsUpgrade,
                        why
                    },
                    "{row}"
                ),
            }
        }
    }

    #[test]
    fn net_194_connect_refused() {
        let port = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut c = connector(port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        assert_eq!(
            ev,
            BuildEvent::Failed {
                job,
                stage: ConnPhase::TcpConnecting,
                why: BuildFail::Errno(111)
            }
        );
    }

    #[test]
    fn net_195_tls_alert() {
        let peer = LoopPeer::start(LoopOpts {
            alert_after_hello: Some(40),
            ..LoopOpts::default()
        });
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        assert_eq!(
            ev,
            BuildEvent::Failed {
                job,
                stage: ConnPhase::TlsHandshake,
                why: BuildFail::TlsAlert(40)
            }
        );
    }

    #[test]
    fn net_196_untrusted_cert() {
        let peer = LoopPeer::start(LoopOpts::default());
        let mut cfg = net_cfg(peer.port);
        cfg.extra_ca = None;
        let arena = ConnArena::new(&[(ConnRole::Md, 1)], &cfg, clock());
        let mut c = Connector::new(arena, &cfg).unwrap();
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        assert_eq!(
            ev,
            BuildEvent::Failed {
                job,
                stage: ConnPhase::TlsHandshake,
                why: BuildFail::Tls
            }
        );
    }

    #[test]
    fn net_197_upgrade_non_101() {
        for (row, code) in [("NET-197.1", 403u16), ("NET-197.2", 200), ("NET-197.3", 101)] {
            let up = if code == 101 {
                Upgrade::Ok
            } else {
                Upgrade::Status(code)
            };
            let peer = LoopPeer::start(LoopOpts {
                upgrade: up,
                ..LoopOpts::default()
            });
            let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
            let job = c.submit(req(1, ConnRole::Md)).unwrap();
            let (ev, _) = drive_job(&mut c, job);
            if code == 101 {
                assert!(matches!(ev, BuildEvent::Ready { .. }), "{row}: {ev:?}");
            } else {
                assert_eq!(
                    ev,
                    BuildEvent::Failed {
                        job,
                        stage: ConnPhase::WsUpgrade,
                        why: BuildFail::HttpStatus(code)
                    },
                    "{row}"
                );
            }
        }
    }

    #[test]
    fn net_198_extensions_rejected() {
        let peer = LoopPeer::start(LoopOpts {
            upgrade: Upgrade::Extensions,
            ..LoopOpts::default()
        });
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        assert_eq!(
            ev,
            BuildEvent::Failed {
                job,
                stage: ConnPhase::WsUpgrade,
                why: BuildFail::Extensions
            }
        );
    }

    #[test]
    fn net_201_reclaim_closes_and_frees() {
        let peer = LoopPeer::start(LoopOpts::default());
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let (h, mut pc) = open(&mut c, &peer, 1, ConnRole::Md);
        let mut p = pool();
        let id = p.install(h, None).unwrap();
        let m = p.retire(id, ErrBits::PEER_EOF).unwrap();
        c.reclaim(m);
        assert!(pc.sees_eof(100), "peer reads EOF within 100 ms");
        assert!(c.submit(req(2, ConnRole::Md)).is_ok());
    }

    fn try_getsockopt_i32(fd: i32, level: i32, name: i32) -> Option<i32> {
        let mut v: i32 = -1;
        let mut l = 4u32;
        let r = unsafe { libc::getsockopt(fd, level, name, (&mut v as *mut i32).cast(), &mut l) };
        if r == 0 {
            Some(v)
        } else {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ENOPROTOOPT)
            );
            None
        }
    }

    fn getsockopt_i32(fd: i32, level: i32, name: i32) -> i32 {
        let mut v: i32 = -1;
        let mut l = 4u32;
        let r = unsafe { libc::getsockopt(fd, level, name, (&mut v as *mut i32).cast(), &mut l) };
        assert_eq!(r, 0, "getsockopt({level}, {name})");
        v
    }

    #[test]
    fn net_205_socket_options() {
        let peer = LoopPeer::start(LoopOpts {
            ws: true,
            ..LoopOpts::default()
        });
        let mut cfg = net_cfg(peer.port);
        cfg.tcp_user_timeout_ms = 5000;
        cfg.so_rcvbuf = 131_072;
        cfg.so_sndbuf = 65_536;
        let arena = ConnArena::new(&[(ConnRole::Md, 1), (ConnRole::Order, 1)], &cfg, clock());
        let mut c = Connector::new(arena, &cfg).unwrap();
        let mut conns = Vec::new();
        for (id, role) in [(1u8, ConnRole::Md), (2, ConnRole::Order)] {
            conns.push(open(&mut c, &peer, id, role));
        }
        let rpeer = LoopPeer::start(LoopOpts {
            ws: false,
            ..LoopOpts::default()
        });
        let mut rcfg = net_cfg(rpeer.port);
        rcfg.tcp_user_timeout_ms = 5000;
        rcfg.so_rcvbuf = 131_072;
        rcfg.so_sndbuf = 65_536;
        let rarena = ConnArena::new(&[(ConnRole::Rest, 1)], &rcfg, clock());
        let mut rc = Connector::new(rarena, &rcfg).unwrap();
        conns.push(open(&mut rc, &rpeer, 3, ConnRole::Rest));
        let mut p = pool();
        for (h, _) in &conns {
            p.install(*h, None).unwrap();
        }
        for id in [1u8, 2, 3] {
            let conn = p.get(ConnId(id)).unwrap();
            let fd = conn.raw_fd();
            let fl = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert_ne!(fl & libc::O_NONBLOCK, 0, "NET-205.1 conn {id}");
            assert_eq!(
                getsockopt_i32(fd, libc::IPPROTO_TCP, libc::TCP_NODELAY),
                1,
                "NET-205.2"
            );
            assert_eq!(
                getsockopt_i32(fd, libc::IPPROTO_TCP, libc::TCP_USER_TIMEOUT),
                5000,
                "NET-205.3"
            );
            assert_eq!(
                getsockopt_i32(fd, libc::SOL_SOCKET, opt::SO_BUSY_POLL),
                0,
                "NET-205.4"
            );
            let busy: Vec<i32> = conn
                .sys()
                .setsockopt_values(opt::SOL_SOCKET, opt::SO_BUSY_POLL)
                .collect();
            assert_eq!(busy, vec![0], "NET-205.4 explicit setsockopt");
            for o in [opt::SO_PREFER_BUSY_POLL, opt::SO_BUSY_POLL_BUDGET] {
                // Kernels before getsockopt support for the budget answer ENOPROTOOPT (default 0).
                if let Some(v) = try_getsockopt_i32(fd, libc::SOL_SOCKET, o) {
                    assert_eq!(v, 0, "NET-205.5");
                }
                assert_eq!(
                    conn.sys().setsockopt_values(opt::SOL_SOCKET, o).count(),
                    0,
                    "NET-205.5"
                );
            }
            let ts = getsockopt_i32(fd, libc::SOL_SOCKET, opt::SO_TIMESTAMPING);
            assert_eq!(ts, opt::TS_RX_FLAGS, "NET-205.6");
            assert_eq!(
                ts & (opt::TS_TX_SOFTWARE | opt::TS_TX_SCHED | opt::TS_OPT_ID),
                0,
                "NET-205.6"
            );
            assert_eq!(
                getsockopt_i32(fd, libc::SOL_SOCKET, libc::SO_REUSEADDR),
                1,
                "NET-205.7"
            );
            assert_eq!(
                (
                    getsockopt_i32(fd, libc::SOL_SOCKET, libc::SO_RCVBUF),
                    getsockopt_i32(fd, libc::SOL_SOCKET, libc::SO_SNDBUF)
                ),
                (262_144, 131_072),
                "NET-205.8"
            );
            let mut dev = [0u8; 32];
            let mut l = 32u32;
            let r = unsafe {
                libc::getsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_BINDTODEVICE,
                    dev.as_mut_ptr().cast(),
                    &mut l,
                )
            };
            assert_eq!(r, 0);
            assert_eq!(&dev[..lo_dev().len()], lo_dev().as_bytes(), "NET-205.9");
            assert_eq!(
                conn.sys()
                    .setsockopt_values(libc::IPPROTO_TCP, opt::TCP_QUICKACK)
                    .count(),
                0,
                "NET-205.10"
            );
        }
    }

    #[test]
    fn net_206_napi_and_mss_read_once() {
        let peer = LoopPeer::start(LoopOpts::default());
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        let BuildEvent::Ready { napi_id, .. } = ev else {
            panic!("{ev:?}")
        };
        let conn = c.conn_mut(job).unwrap();
        let napi: Vec<u8> = conn
            .sys()
            .getsockopt_phases(opt::SOL_SOCKET, opt::SO_INCOMING_NAPI_ID)
            .collect();
        let mss: Vec<u8> = conn
            .sys()
            .getsockopt_phases(opt::IPPROTO_TCP, opt::TCP_MAXSEG)
            .collect();
        assert_eq!(napi.len(), 1);
        assert_eq!(mss.len(), 1);
        for p in napi.iter().chain(mss.iter()) {
            assert!(
                *p == ConnPhase::WsUpgrade as u8 || *p == ConnPhase::Open as u8,
                "read after the TLS handshake (phase {p})"
            );
        }
        if !wsl_mirrored() {
            assert_eq!(napi_id, 0);
        }
        assert_eq!(conn.napi_id(), napi_id);
        let before = c.sys().total();
        assert_eq!(c.napi_ifindex(0), None);
        assert_eq!(c.sys().total(), before);
    }

    #[test]
    fn net_207_source_address() {
        let peer = LoopPeer::start(LoopOpts::default());
        let mut cfg = net_cfg(peer.port);
        cfg.src_ips = vec![Ipv4Addr::new(127, 0, 0, 2), Ipv4Addr::new(127, 0, 0, 3)];
        cfg.bind_device = vec![
            (Ipv4Addr::new(127, 0, 0, 2), "lo".into()),
            (Ipv4Addr::new(127, 0, 0, 3), "lo".into()),
        ];
        let arena = ConnArena::new(&[(ConnRole::Md, 1)], &cfg, clock());
        let mut c = Connector::new(arena, &cfg).unwrap();
        let mut r = req(1, ConnRole::Md);
        r.src_ip_idx = 1;
        let job = c.submit(r).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        assert!(matches!(ev, BuildEvent::Ready { .. }), "{ev:?}");
        let conn = c.conn_mut(job).unwrap();
        assert_eq!(conn.local_addr().unwrap().0, Ipv4Addr::new(127, 0, 0, 3));
        assert_eq!(conn.sys().bind_addr.map(|a| a.1), Some(0));
    }

    #[test]
    fn net_209_extra_ca_only_with_override() {
        for (row, ov, ok) in [("NET-209.1", true, true), ("NET-209.2", false, false)] {
            let peer = LoopPeer::start(LoopOpts::default());
            let mut cfg = net_cfg(peer.port);
            cfg.endpoint_override = ov;
            let arena = ConnArena::new(&[(ConnRole::Md, 1)], &cfg, clock());
            let mut c = Connector::new(arena, &cfg).unwrap();
            let job = c.submit(req(1, ConnRole::Md)).unwrap();
            let (ev, _) = drive_job(&mut c, job);
            if ok {
                assert!(matches!(ev, BuildEvent::Ready { .. }), "{row}: {ev:?}");
            } else {
                assert_eq!(
                    ev,
                    BuildEvent::Failed {
                        job,
                        stage: ConnPhase::TlsHandshake,
                        why: BuildFail::Tls
                    },
                    "{row}"
                );
            }
        }
    }

    /// TLS 1.3 cipher suites and supported_groups of a raw ClientHello record.
    fn hello_fields(rec: &[u8]) -> (Vec<u16>, Vec<u16>) {
        let h = &rec[5..];
        assert_eq!(h[0], 1, "ClientHello");
        let mut p = 4 + 2 + 32;
        let sid = h[p] as usize;
        p += 1 + sid;
        let cl = u16::from_be_bytes([h[p], h[p + 1]]) as usize;
        let suites: Vec<u16> = h[p + 2..p + 2 + cl]
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        p += 2 + cl;
        let comp = h[p] as usize;
        p += 1 + comp;
        let el = u16::from_be_bytes([h[p], h[p + 1]]) as usize;
        p += 2;
        let end = p + el;
        let mut groups = Vec::new();
        while p + 4 <= end {
            let t = u16::from_be_bytes([h[p], h[p + 1]]);
            let l = u16::from_be_bytes([h[p + 2], h[p + 3]]) as usize;
            if t == 10 {
                let gl = u16::from_be_bytes([h[p + 4], h[p + 5]]) as usize;
                groups = h[p + 6..p + 6 + gl]
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
            }
            p += 4 + l;
        }
        (suites, groups)
    }

    #[test]
    fn net_210_client_hello() {
        // Row 1.
        let peer = LoopPeer::start(LoopOpts::default());
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        assert!(matches!(ev, BuildEvent::Ready { .. }), "{ev:?}");
        let pc = peer.accept();
        let (suites, groups) = hello_fields(&pc.client_hello);
        let tls13: Vec<u16> = suites.iter().copied().filter(|s| *s >> 8 == 0x13).collect();
        assert_eq!(tls13, vec![0x1301, 0x1302, 0x1303], "NET-210.1");
        assert!(groups.contains(&0x11EC), "NET-210.1 groups {groups:x?}");
        // Row 2: TLS 1.2 only.
        let peer = LoopPeer::start(LoopOpts {
            versions: vec![&rustls::version::TLS12],
            ..LoopOpts::default()
        });
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        let BuildEvent::Ready { tls, .. } = ev else {
            panic!("NET-210.2 {ev:?}")
        };
        assert_eq!(tls.version, 0x0303, "NET-210.2");
        // The TLS 1.2 record layer carries frames both ways.
        let mut pc = peer.accept();
        let h = c.handoff(job).unwrap();
        let mut p = pool();
        p.install(h, None).unwrap();
        pc.text(b"over tls12");
        let conn = p.get(ConnId(1)).unwrap();
        let mut got = None;
        for _ in 0..5000 {
            conn.poll_rx(0);
            if let Some(f) = conn.next_frame() {
                got = Some(f.payload.to_vec());
                break;
            }
            std::thread::sleep(std::time::Duration::from_micros(200));
        }
        assert_eq!(got.as_deref(), Some(&b"over tls12"[..]), "NET-210.2");
        conn.send_text(b"back", 0).unwrap();
        let mut acc = Vec::new();
        let fr = pc.read_frames(1, &mut acc);
        assert_eq!(fr[0].payload, b"back", "NET-210.2");
        // Row 3: TLS 1.3 with AES-256-GCM only.
        let peer = LoopPeer::start(LoopOpts {
            versions: vec![&rustls::version::TLS13],
            suites: Some(vec![
                rustls::crypto::aws_lc_rs::cipher_suite::TLS13_AES_256_GCM_SHA384,
            ]),
            ..LoopOpts::default()
        });
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let job = c.submit(req(1, ConnRole::Md)).unwrap();
        let (ev, _) = drive_job(&mut c, job);
        let BuildEvent::Ready { tls, .. } = ev else {
            panic!("NET-210.3 {ev:?}")
        };
        assert_eq!(tls.suite, 0x1302, "NET-210.3");
    }

    #[test]
    fn net_211_ws_key_random() {
        let peer = LoopPeer::start(LoopOpts::default());
        let mut c = connector(peer.port, &[(ConnRole::Md, 1)]);
        let mut p = pool();
        let mut keys = std::collections::BTreeSet::new();
        for i in 0..100u32 {
            let (h, pc) = open(&mut c, &peer, 1, ConnRole::Md);
            let t = String::from_utf8(pc.request.clone()).unwrap();
            let k = t
                .lines()
                .find_map(|l| l.strip_prefix("Sec-WebSocket-Key: "))
                .unwrap()
                .to_string();
            assert!(keys.insert(k), "duplicate key at {i}");
            let id = p.install(h, None).unwrap();
            c.reclaim(p.retire(id, ErrBits::PEER_EOF).unwrap());
        }
        assert_eq!(keys.len(), 100);
        let _ = ws_ref::text(b"");
    }
}
