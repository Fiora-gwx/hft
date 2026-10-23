//! NET-300–NET-302: `NetConfig` defaults and how they reach the socket and the buffers.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[allow(unused_imports)] // used by the Linux-only tests below
use crate::support;

use hft_net::{NetConfig, Quickack, TlsMode};

#[test]
fn net_300_defaults() {
    let c = NetConfig::default();
    assert!(
        c.endpoints.is_empty() && c.src_ips.is_empty() && c.bind_device.is_empty(),
        "NET-300.1"
    );
    assert_eq!(c.src_port_range, (40_000, 40_999), "NET-300.2");
    assert!(c.rss_plan.is_none() && c.extra_ca.is_none(), "NET-300.3");
    assert!(!c.tx_timestamps, "NET-300.4");
    assert_eq!(
        (c.rx_buf.md, c.rx_buf.order, c.rx_buf.user_stream, c.rx_buf.rest),
        (262_144, 65_536, 65_536, 1_048_576),
        "NET-300.5"
    );
    assert_eq!((c.tx_buf, c.max_msg), (16_384, 262_144), "NET-300.6");
    assert_eq!((c.so_rcvbuf, c.so_sndbuf), (262_144, 65_536), "NET-300.7");
    assert_eq!(c.tcp_user_timeout_ms, 5000, "NET-300.8");
    assert_eq!(
        c.quickack_after_read,
        Quickack {
            md: false,
            order: false
        },
        "NET-300.9"
    );
    assert_eq!(
        (c.connect_timeout_ms, c.tls_timeout_ms, c.upgrade_timeout_ms),
        (3000, 3000, 3000),
        "NET-300.10"
    );
    assert_eq!(c.rotate_stagger_s, 3600, "NET-300.11");
    assert_eq!(c.warm_interval_ns, 0, "NET-300.12");
    assert_eq!(c.rest_timeout_ms, 2000, "NET-300.13");
    assert!(c.ts_expect_hw, "NET-300.14");
    assert_eq!(c.tls_mode, TlsMode::Unbuffered, "NET-300.15");
    assert_eq!(c.rest_max_idle_ms, 0, "NET-300.16");
}

#[cfg(target_os = "linux")]
mod lo {
    use super::support::looppeer::*;
    use hft_net::*;

    #[test]
    fn net_301_tcp_user_timeout() {
        for (row, ms) in [("NET-301.1", 4999u32), ("NET-301.2", 5000), ("NET-301.3", 5001)] {
            let peer = LoopPeer::start(LoopOpts::default());
            let mut cfg = net_cfg(peer.port);
            cfg.tcp_user_timeout_ms = ms;
            let arena = ConnArena::new(&[(ConnRole::Md, 1)], &cfg, clock());
            let mut c = Connector::new(arena, &cfg).unwrap();
            let job = c.submit(req(1, ConnRole::Md)).unwrap();
            let (ev, _) = drive_job(&mut c, job);
            assert!(matches!(ev, BuildEvent::Ready { .. }), "{row}");
            let fd = c.conn_mut(job).unwrap().raw_fd();
            let mut v: i32 = 0;
            let mut l = 4u32;
            let r = unsafe {
                libc::getsockopt(
                    fd,
                    libc::IPPROTO_TCP,
                    libc::TCP_USER_TIMEOUT,
                    (&mut v as *mut i32).cast(),
                    &mut l,
                )
            };
            assert_eq!(r, 0);
            assert_eq!(v as u32, ms, "{row}");
        }
    }

    #[test]
    fn net_302_buffer_sizes_by_role() {
        let rows = [
            ("NET-302.1", ConnRole::Md, (262_144u32, 16_384u32)),
            ("NET-302.2", ConnRole::Order, (65_536, 16_384)),
            ("NET-302.3", ConnRole::UserStream, (65_536, 16_384)),
            ("NET-302.4", ConnRole::Rest, (1_048_576, 16_384)),
        ];
        for (row, role, want) in rows {
            let ws = role != ConnRole::Rest;
            let peer = LoopPeer::start(LoopOpts {
                ws,
                ..LoopOpts::default()
            });
            let mut cfg = net_cfg(peer.port);
            let d = NetConfig::default();
            cfg.rx_buf = d.rx_buf;
            cfg.tx_buf = d.tx_buf;
            let arena = ConnArena::new(&[(role, 1)], &cfg, clock());
            let mut c = Connector::new(arena, &cfg).unwrap();
            let job = c.submit(req(1, role)).unwrap();
            let (ev, _) = drive_job(&mut c, job);
            assert!(matches!(ev, BuildEvent::Ready { .. }), "{row}: {ev:?}");
            assert_eq!(c.conn_mut(job).unwrap().buf_caps(), want, "{row}");
        }
    }
}
