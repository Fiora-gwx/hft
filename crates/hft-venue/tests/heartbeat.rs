//! VEN-280, VEN-283: heartbeats (README §5).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::ConnStateKind;
use hft_net::{ConnId, ErrBits};
use hft_types::InstId;
use hft_venue::{FeedCfg, FeedRoutes, streams};
use hft_venue_proto::{HeartbeatSpec, PingKind};
use support::feedh::*;
use support::net::Ev;
use support::rd::*;
use support::stub;

#[test]
fn ven_280_server_ping() {
    let (mut h, now) = bbo_up(|_| {}, &[S1]);
    let c0 = ConnId(0);
    let before = h.rt.conn(c0).unwrap().last_rx_tsc();
    h.net.sent(&mut h.rt, c0);
    h.net.at(c0, 0, Ev::Ping(b"hi".to_vec()));
    h.poll(now + 1000);
    let sent = h.net.sent(&mut h.rt, c0);
    assert!(
        sent.iter().any(|f| f.opcode == 10 && f.payload == b"hi"),
        "{sent:?}"
    );
    assert!(h.drain_bbo().is_empty());
    assert!(h.rt.conn(c0).unwrap().last_rx_tsc() > before);
}

const MD_HB: HeartbeatSpec = HeartbeatSpec {
    ping: PingKind::AppJson,
    interval_ms: 1000,
    unconditional: true,
    pong_timeout_ms: 5000,
    idle_timeout_ms: 30_000,
};

/// `routes = Md`, S1 trades on connection 8 (installed at `t_inst`); returns the harness.
fn md8(t_inst: u64) -> Fh {
    stub::set_cfg(|c| c.hb_md = MD_HB);
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Md,
        ..f0()
    });
    h.sub(S1, streams::TRADE);
    h.poll(t_inst);
    h.complete(ConnId(8));
    h.poll(t_inst);
    h.ack(ConnId(8));
    stub::set_cfg(|c| c.hb_md = support::stub::HB_OFF);
    h
}

fn downs(h: &mut Fh) -> Vec<hft_event::ConnState> {
    states(&h.drain_md())
        .into_iter()
        .filter(|s| s.state == ConnStateKind::Down)
        .collect()
}

#[test]
fn ven_283_timeouts() {
    // .1/.2: the first app ping goes out at t0 = install + 1 s and is never answered.
    let mut h = md8(0);
    let t0 = 1_000_000_000u64;
    h.poll(t0);
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(8))
            .iter()
            .any(|t| t.starts_with("PING|")),
        "app ping at t0"
    );
    let mut t = t0;
    while t + 100_000_000 < t0 + 5_000_000_000 {
        t += 100_000_000;
        h.poll(t);
    }
    h.poll(t0 + 5_000_000_000);
    assert!(downs(&mut h).is_empty(), "VEN-283.1");
    h.poll(t0 + 5_000_000_001);
    let d = downs(&mut h);
    assert_eq!(d.len(), 1, "VEN-283.2");
    assert_eq!((d[0].conn, d[0].inst), (8, InstId(0)));
    assert_ne!(d[0].reason & ErrBits::PING_TIMEOUT.bits(), 0, "VEN-283.2 reason");
    // .3: pongs answered, the last data frame at t_last.
    let mut h = md8(0);
    let t_last = 2_000_000_000u64;
    h.net
        .at(ConnId(8), t_last, Ev::Text(b"T|S1|1|100.00|1.000|B".to_vec()));
    let mut t = 0u64;
    let end = t_last + 30_000_000_000;
    while t < end {
        t = (t + 100_000_000).min(end);
        h.poll(t);
        for s in h.net.sent_text(&mut h.rt, ConnId(8)) {
            if s.starts_with("PING|") {
                h.net.at(ConnId(8), t + 1, Ev::Text(b"PONG|1".to_vec()));
            }
        }
    }
    assert!(downs(&mut h).is_empty(), "VEN-283.3 at t_last + 30 s");
    h.poll(end + 1);
    let d = downs(&mut h);
    assert_eq!(d.len(), 1, "VEN-283.3");
    assert_ne!(d[0].reason & ErrBits::IDLE_TIMEOUT.bits(), 0);
}
