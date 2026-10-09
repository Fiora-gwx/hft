//! VEN-120–VEN-129: BBO connection health (README §3a).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{ConnRole, ConnState, ConnStateKind, FeedCmd, FeedMsg};
use hft_net::ConnId;
use hft_types::{AccountId, InstId};
use hft_venue::{FeedCfg, FeedRoutes, codes, streams};
use hft_venue_proto::{PingKind, StreamKind};
use support::feedh::*;
use support::net::Ev;
use support::rd::*;
use support::stub;

const C0: ConnId = ConnId(0);
const C1: ConnId = ConnId(1);

/// Answers every WS Ping (and `PING|` app ping) sent on `c` with a pong `delay` later.
fn answer_pings(h: &mut Fh, c: ConnId, now: u64, delay: u64) -> (usize, usize) {
    let mut ws = 0;
    let mut app = 0;
    for f in h.net.sent(&mut h.rt, c) {
        if f.opcode == 9 {
            ws += 1;
            h.net.at(c, now + delay, Ev::Pong(f.payload.clone()));
        } else if f.payload.starts_with(b"PING|") {
            app += 1;
            h.net.at(c, now + delay, Ev::Text(b"PONG|1".to_vec()));
        }
    }
    (ws, app)
}

#[test]
fn ven_120_probe_period() {
    for (row, data, probe) in [
        ("VEN-120.1", true, PingKind::WsPing),
        ("VEN-120.2", false, PingKind::WsPing),
        ("VEN-120.3", false, PingKind::AppText),
    ] {
        stub::set_cfg(|c| c.probe = probe);
        let mut h = feed(FeedCfg {
            routes: FeedRoutes::Bbo,
            bbo_conns: 1,
            bbo_ping_ms: 1000,
            ..f0()
        });
        h.sub(S1, streams::BBO);
        h.poll(0);
        h.complete(C0);
        h.poll(0);
        h.ack(C0);
        let mut ws_t = Vec::new();
        let mut app_n = 0;
        let mut u = 0;
        for ms in 1..=10_000u64 {
            let t = ms * 1_000_000;
            if data && ms % 10 == 0 {
                u += 1;
                h.net.at(C0, t, Ev::Text(b("S1", u)));
            }
            h.poll(t);
            for f in h.net.sent(&mut h.rt, C0) {
                if f.opcode == 9 {
                    assert_eq!(f.payload, t.to_le_bytes().to_vec(), "{row}: payload = TSC");
                    ws_t.push(t);
                    h.net.at(C0, t + 1, Ev::Pong(f.payload.clone()));
                } else if f.payload.starts_with(b"PING|") {
                    app_n += 1;
                    h.net.at(C0, t + 1, Ev::Text(b"PONG|1".to_vec()));
                }
            }
        }
        stub::set_cfg(|c| c.probe = PingKind::WsPing);
        if probe == PingKind::WsPing {
            let want: Vec<u64> = (1..=10).map(|k| k * 1_000_000_000).collect();
            assert_eq!(ws_t, want, "{row}");
            assert_eq!(app_n, 0, "{row}");
        } else {
            assert_eq!((app_n, ws_t.len()), (10, 0), "{row}");
        }
    }
}

#[test]
fn ven_121_conn_and_member_events() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        ..f0()
    });
    h.sub(S1, streams::BBO);
    h.sub(S2, streams::BBO);
    h.poll(0);
    h.complete(C0);
    h.poll(100);
    let s = states(&h.drain_bbo());
    assert_eq!(s.len(), 1);
    assert_eq!(
        (s[0].state, s[0].role, s[0].conn, s[0].inst, s[0].rx_tsc),
        (ConnStateKind::Up, ConnRole::Md, 0, InstId(0), 100)
    );
    h.ack(C0);
    h.poll(200);
    let v = h.drain_bbo();
    assert!(has_state(&v, 0, ConnStateKind::Up, S1, 0) && has_state(&v, 0, ConnStateKind::Up, S2, 0));
    h.cmd.try_push(FeedCmd::Unsubscribe { inst: S2 }).unwrap();
    // The unsubscribe frame waits for the subscription pacing (one frame per 250 ms here).
    h.poll(300);
    h.poll(300_000_000);
    h.ack(C0);
    h.poll(300_000_100);
    let s = states(&h.drain_bbo());
    assert_eq!(
        s.iter().map(|x| (x.state, x.conn, x.inst)).collect::<Vec<_>>(),
        vec![(ConnStateKind::Down, 0, S2)]
    );
}

/// Connection 0 (and 1) up; connection 0's last published record at 1e9 (a `Bbo` received then).
fn published_at_1s() -> Fh {
    let (mut h, _) = bbo_up(|c| c.bbo_ping_ms = 1000, &[S1]);
    h.net.at(C0, 1_000_000_000, Ev::Text(b("S1", 1)));
    h.poll(1_000_000_000);
    h.clear();
    h
}

#[test]
fn ven_122_alive_threshold() {
    for (row, at, alive) in [
        ("VEN-122.1", 1_499_999_999u64, false),
        ("VEN-122.2", 1_500_000_000, true),
        ("VEN-122.3", 1_500_000_001, true),
    ] {
        let mut h = published_at_1s();
        h.net.at(C0, at, Ev::Pong(vec![0; 8]));
        h.poll(at);
        let s: Vec<ConnState> = states(&h.drain_bbo())
            .into_iter()
            .filter(|s| s.state == ConnStateKind::Alive)
            .collect();
        if alive {
            assert_eq!(s.len(), 1, "{row}");
            assert_eq!(
                (s[0].role, s[0].conn, s[0].inst, s[0].account, s[0].rx_tsc),
                (ConnRole::Md, 0, InstId(0), AccountId(0), at),
                "{row}"
            );
        } else {
            assert!(s.is_empty(), "{row}");
        }
    }
}

#[test]
fn ven_123_quiet_book_alive() {
    let (mut h, _) = bbo_up(|c| c.bbo_ping_ms = 1000, &[S1, S2, S3]);
    let mut evid: Vec<Vec<u64>> = vec![Vec::new(), Vec::new()];
    let mut alive = [0usize; 2];
    let mut all = Vec::new();
    for ms in 1..=10_060u64 {
        let t = ms * 1_000_000;
        h.poll(t);
        for c in [C0, C1] {
            answer_pings(&mut h, c, t, 50_000_000);
        }
        all.extend(h.drain_bbo());
    }
    for m in &all {
        if let FeedMsg::ConnState(s) = m
            && (s.state == ConnStateKind::Alive || s.state == ConnStateKind::Up)
        {
            evid[s.conn as usize].push(s.rx_tsc);
            if s.state == ConnStateKind::Alive {
                alive[s.conn as usize] += 1;
            }
        }
    }
    assert_eq!(alive, [10, 10]);
    assert!(bbos(&all).is_empty());
    assert!(!states(&all).iter().any(|s| s.state == ConnStateKind::Degraded));
    for (c, ev) in evid.iter().enumerate() {
        assert!(ev.windows(2).all(|w| w[1] >= w[0]), "conn {c}");
        for w in ev.windows(2) {
            assert!(w[1] - w[0] <= 1_050_000_000, "conn {c}: gap {}", w[1] - w[0]);
        }
    }
}

#[test]
fn ven_124_busy_no_alive() {
    let (mut h, _) = bbo_up(|c| c.bbo_conns = 1, &[S1]);
    let mut all = Vec::new();
    let mut u = 0;
    for ms in 1..=5_000u64 {
        let t = ms * 1_000_000;
        if ms % 10 == 0 {
            u += 1;
            h.net.at(C0, t, Ev::Text(b("S1", u)));
        }
        h.poll(t);
        answer_pings(&mut h, C0, t, 1);
        all.extend(h.drain_bbo());
    }
    assert!(!states(&all).iter().any(|s| s.state == ConnStateKind::Alive));
    assert_eq!(bbos(&all).len(), 500);
}

#[test]
fn ven_125_dup_only_conn_alive() {
    let (mut h, _) = bbo_up(|_| {}, &[S1]);
    let mut all = Vec::new();
    let mut u = 0;
    for ms in 1..=5_100u64 {
        let t = ms * 1_000_000;
        if ms % 10 == 0 {
            u += 1;
            h.net.at(C0, t, Ev::Text(b("S1", u)));
            h.net.at(C1, t + 1_000_000, Ev::Text(b("S1", u)));
        }
        h.poll(t);
        for c in [C0, C1] {
            answer_pings(&mut h, c, t, 1);
        }
        all.extend(h.drain_bbo());
    }
    let alive = |c: u8| {
        states(&all)
            .iter()
            .filter(|s| s.state == ConnStateKind::Alive && s.conn == c)
            .count()
    };
    assert_eq!(alive(1), 5);
    assert_eq!(alive(0), 0);
}

#[test]
fn ven_126_silent() {
    for (row, now, silent) in [
        ("VEN-126.1", 4_000_000_000u64, false),
        ("VEN-126.2", 4_000_000_001, true),
    ] {
        let mut h = feed(FeedCfg {
            routes: FeedRoutes::Bbo,
            bbo_conns: 1,
            bbo_dead_ms: 3000,
            ..f0()
        });
        h.sub(S1, streams::BBO);
        h.poll(900_000_000);
        h.complete(C0);
        h.poll(900_000_000);
        h.ack(C0);
        h.poll(950_000_000);
        h.net.at(C0, 1_000_000_000, Ev::Text(b("S1", 1)));
        h.poll(1_000_000_000);
        h.clear();
        h.builds.clear();
        h.poll(now);
        let s = states(&h.drain_bbo());
        if silent {
            assert_eq!(s.len(), 2, "{row} {s:?}");
            assert_eq!(
                (s[0].state, s[0].conn, s[0].reason),
                (ConnStateKind::Degraded, 0, codes::BBO_SILENT),
                "{row}"
            );
            assert_eq!(
                (s[1].state, s[1].conn, s[1].inst),
                (ConnStateKind::Down, 0, InstId(0)),
                "{row}"
            );
            assert_eq!(h.retired, vec![C0], "{row}");
            assert_eq!(h.builds.len(), 1, "{row}");
        } else {
            assert!(s.is_empty(), "{row}");
        }
    }
}

#[test]
fn ven_127_silent_from_install() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        bbo_conns: 1,
        bbo_dead_ms: 3000,
        ..f0()
    });
    h.sub(S1, streams::BBO);
    h.poll(5_000_000_000);
    h.complete(C0);
    h.poll(5_000_000_000);
    h.clear();
    for (t, silent) in [
        (5_000_000_001u64, false),
        (8_000_000_000, false),
        (8_000_000_001, true),
    ] {
        h.poll(t);
        let s = states(&h.drain_bbo());
        assert_eq!(
            s.iter()
                .any(|x| x.state == ConnStateKind::Degraded && x.reason == codes::BBO_SILENT),
            silent,
            "t={t}"
        );
        assert_eq!(s.iter().any(|x| x.state == ConnStateKind::Down), silent, "t={t}");
    }
}

/// Connections 0, 1 up with S1, S2 members; then `rst` on `drop` and replacements completed.
fn refresh_case(drop: &[u8], ack: &[u8]) -> (Fh, Vec<FeedMsg>) {
    let (mut h, mut now) = bbo_up(|_| {}, &[S1, S2]);
    for &c in drop {
        h.net.at(ConnId(c), 0, Ev::Rst);
    }
    now += 10;
    h.poll(now);
    h.clear();
    let ids = h.complete_builds();
    now += 10;
    h.poll(now);
    let mut all = Vec::new();
    for &c in ack {
        assert!(ids.contains(&ConnId(c)) || !drop.contains(&c));
        h.ack(ConnId(c));
        now += 10;
        h.poll(now);
        all.extend(h.drain_bbo());
    }
    (h, all)
}

#[test]
fn ven_128_refresh_when_all_down() {
    let (_h, v) = refresh_case(&[0], &[0]);
    assert!(
        !states(&v).iter().any(|s| s.state == ConnStateKind::Resync),
        "VEN-128.1"
    );
    let (_h, v) = refresh_case(&[0, 1], &[0]);
    let r: Vec<_> = states(&v)
        .into_iter()
        .filter(|s| s.state == ConnStateKind::Resync)
        .collect();
    assert_eq!(
        r.iter()
            .map(|s| (s.conn, s.inst, s.reason, s.role))
            .collect::<Vec<_>>(),
        vec![(0, S1, 6, ConnRole::Md), (0, S2, 6, ConnRole::Md)],
        "VEN-128.2"
    );
    let (_h, v) = refresh_case(&[0, 1], &[0, 1]);
    let r = states(&v)
        .into_iter()
        .filter(|s| s.state == ConnStateKind::Resync)
        .count();
    assert_eq!(r, 2, "VEN-128.3: the second replacement adds no Resync");
}

#[test]
fn ven_129_refresh_ends_on_quote() {
    for (row, first, fwd) in [
        ("VEN-129.1", 200u64, true),
        ("VEN-129.2", 199, false),
        ("VEN-129.3", 201, true),
    ] {
        let (mut h, _) = refresh_case(&[0, 1], &[0]);
        h.rt.set_last_fwd(S1, StreamKind::Bbo, 200);
        h.net.push(C0, &b("S1", first));
        h.poll(10_000_000);
        let got: Vec<u64> = bbos(&h.drain_bbo()).iter().map(|x| x.update_id).collect();
        assert_eq!(got.len(), usize::from(fwd), "{row}");
        if row == "VEN-129.1" {
            h.net.push(C0, &b("S1", 200));
            h.poll(10_000_001);
            assert!(
                bbos(&h.drain_bbo()).is_empty(),
                "{row}: refresh over, 200 is a duplicate now"
            );
        }
        if row == "VEN-129.2" {
            h.net.push(C0, &b("S1", 200));
            h.poll(10_000_001);
            assert_eq!(bbos(&h.drain_bbo()).len(), 1, "{row}: still refreshing");
        }
    }
}
