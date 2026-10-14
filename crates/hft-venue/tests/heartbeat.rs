//! VEN-280, VEN-283: heartbeats (README §5).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

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

fn order_hb(hb_spec: HeartbeatSpec) -> support::gwh::Gh<support::stub::StubProto> {
    use support::gwh::*;
    stub::set_cfg(|c| c.hb_order = hb_spec);
    let mut h = gw_with::<support::stub::StubProto, 2048>(g0(), &[A1], GhOpts::default()).unwrap();
    h.poll(0);
    h.complete(ConnId(112));
    h.poll(0);
    h.complete(ConnId(113));
    h.poll(0);
    h.reps();
    h
}

#[test]
fn ven_281_app_heartbeat() {
    for (unconditional, busy, expected) in [(false, true, 0), (true, true, 3), (false, false, 1)] {
        let mut h = order_hb(HeartbeatSpec {
            ping: PingKind::AppText,
            interval_ms: 20_000,
            unconditional,
            pong_timeout_ms: 10_000,
            idle_timeout_ms: 0,
        });
        let mut pings = Vec::new();
        for sec in 1..=if busy { 60u64 } else { 20 } {
            let now = sec * 1_000_000_000;
            if busy {
                h.net.push(ConnId(112), b"E|listenKeyExtended");
            }
            h.poll(now);
            for id in [ConnId(112), ConnId(113)] {
                for f in h.net.sent(&mut h.rt, id) {
                    if f.payload.starts_with(b"PING|") {
                        if id == ConnId(112) {
                            pings.push(sec);
                        }
                        h.net.at(id, now + 1, Ev::Text(b"PONG|1".to_vec()));
                    } else if f.opcode == 9 {
                        h.net.at(id, now + 1, Ev::Pong(f.payload));
                    }
                }
            }
            h.poll(now + 1);
            h.reps();
        }
        assert_eq!(
            pings.len(),
            expected,
            "unconditional={unconditional} busy={busy} pings={pings:?}"
        );
        if expected == 3 {
            assert_eq!(pings, [20, 40, 60]);
        }
        if expected == 1 {
            assert_eq!(pings, [20]);
        }
    }
    // A receive just before the timer must defer only until last_data + interval.
    let mut h = order_hb(HeartbeatSpec {
        ping: PingKind::AppText,
        interval_ms: 1000,
        unconditional: false,
        pong_timeout_ms: 0,
        idle_timeout_ms: 0,
    });
    h.net.push(ConnId(112), b"E|listenKeyExtended");
    h.poll(900_000_000);
    h.poll(1_000_000_000);
    assert!(
        !h.net
            .sent(&mut h.rt, ConnId(112))
            .iter()
            .any(|f| f.payload.starts_with(b"PING|"))
    );
    h.poll(1_899_999_999);
    assert!(
        !h.net
            .sent(&mut h.rt, ConnId(112))
            .iter()
            .any(|f| f.payload.starts_with(b"PING|"))
    );
    h.poll(1_900_000_000);
    assert!(
        h.net
            .sent(&mut h.rt, ConnId(112))
            .iter()
            .any(|f| f.payload.starts_with(b"PING|"))
    );
}

#[test]
fn ven_282_rtt_ping() {
    let mut gw = order_hb(stub::HB_OFF);
    stub::set_cfg(|c| c.hb_md = stub::HB_OFF);
    let mut md = feed(FeedCfg {
        routes: FeedRoutes::Md,
        ..f0()
    });
    md.sub(S1, streams::BOOK);
    md.poll(0);
    md.complete(ConnId(8));
    md.poll(0);
    md.ack(ConnId(8));
    md.poll(1);
    let mut counts = [0, 0];
    for sec in 1..=20u64 {
        let now = sec * 1_000_000_000;
        gw.poll(now);
        md.poll(now);
        for frame in gw.net.sent(&mut gw.rt, ConnId(112)) {
            if frame.opcode == 9 {
                counts[0] += 1;
                assert_eq!(sec % 5, 0);
                gw.net.at(ConnId(112), now + 3_000_000, Ev::Pong(frame.payload));
            }
        }
        for frame in md.net.sent(&mut md.rt, ConnId(8)) {
            if frame.opcode == 9 {
                counts[1] += 1;
                assert_eq!(sec % 5, 0);
                md.net.at(ConnId(8), now + 3_000_000, Ev::Pong(frame.payload));
            }
        }
        gw.poll(now + 3_000_000);
        md.poll(now + 3_000_000);
        gw.reps();
        md.drain_md();
    }
    assert_eq!(counts, [4, 4]);
    assert_eq!(gw.rt.conn(ConnId(112)).unwrap().stats().rtt.count, 4);
    assert_eq!(md.rt.conn(ConnId(8)).unwrap().stats().rtt.count, 4);
    for hist in [
        gw.rt.conn(ConnId(112)).unwrap().stats().rtt,
        md.rt.conn(ConnId(8)).unwrap().stats().rtt,
    ] {
        assert_eq!((hist.min_ns, hist.max_ns), (3_000_000, 3_000_000));
    }
}

#[test]
fn ven_134_md_conn_alive() {
    stub::set_cfg(|c| c.hb_md = stub::HB_OFF);
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::All,
        ..f0()
    });
    let mut now = 100;
    h.up(&[S1], streams::BOOK, &mut now);
    h.clear();
    let ping_at = 5_000_000_000 + now;
    h.poll(ping_at);
    let ping = h
        .net
        .sent(&mut h.rt, ConnId(8))
        .into_iter()
        .find(|f| f.opcode == 9)
        .unwrap();
    let pong_at = ping_at + 3_000_000;
    h.net.at(ConnId(8), pong_at, Ev::Pong(ping.payload));
    h.poll(pong_at);
    let events = h.drain_md();
    assert!(events.iter().any(|m| matches!(m,hft_event::FeedMsg::ConnState(s)
        if s.state == ConnStateKind::Alive && s.role == hft_event::ConnRole::Md
        && s.conn == 8 && s.inst == InstId(0) && s.rx_tsc == pong_at)));
    assert!(h.drain_bbo().is_empty());
    h.net.push(ConnId(8), b"D|S1|1|1|100 1|101 1");
    h.net.push(ConnId(8), b"PONG|1");
    h.poll(pong_at + 1_000_000);
    assert!(
        !states(&h.drain_md())
            .iter()
            .any(|s| s.state == ConnStateKind::Alive)
    );
}
