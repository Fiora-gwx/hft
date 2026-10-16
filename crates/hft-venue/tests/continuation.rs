//! Connection lifecycle and stream isolation regressions using the real feed runtime.
use crate::support::{
    feedh::*,
    net::{Ev, Net},
    rd::*,
    stub::{self, StubProto},
};
use hft_event::{ConnStateKind, FeedCmd, FeedMsg};
use hft_net::ConnId;
use hft_types::InstId;
use hft_venue::{FeedCfg, FeedRoutes, streams};

#[test]
fn ven_395_warm_and_account_paths_zero_alloc() {
    crate::snap_pool::measure_pooled_snapshot();
    crate::acct_fetch::measure_account_sequences_and_gateway_apply();
    crate::gw_hold::measure_unmatched_fill_hold_and_timeout();
}

#[test]
fn ven_025_stream_bits() {
    for (mask, name, route, levels) in [
        (streams::BBO, "Bbo", 0, 0),
        (streams::BOOK, "DepthTopN", 1, 5),
        (streams::TRADE, "Trade", 2, 0),
        (streams::FUNDING, "Funding", 2, 0),
        (streams::MARK, "Mark", 2, 0),
        (streams::INDEX, "Index", 2, 0),
        (streams::LIQUIDATION, "Liquidation", 2, 0),
    ] {
        let mut h = feed(f0());
        let mut now = 100;
        stub::take_log();
        h.up(&[S1], mask, &mut now);
        let calls = stub::calls("subscribe");
        assert!(!calls.is_empty());
        assert!(
            calls.iter().all(|c| c.arg.contains(name)
                && c.arg.contains(&format!("route: RouteId({route})"))
                && c.arg.contains(&format!("levels: {levels}"))),
            "{calls:?}"
        );
    }
    let mut h = feed(f0());
    stub::take_log();
    h.sub(S1, 0);
    h.poll(100);
    assert!(h.all_builds.is_empty() && stub::calls("subscribe").is_empty());
    let mut now = 200;
    h.up(&[S1], streams::BBO | streams::TRADE, &mut now);
    let calls = stub::calls("subscribe");
    assert!(calls.iter().any(|c| c.arg.contains("Bbo")));
    assert!(calls.iter().any(|c| c.arg.contains("Trade")));
    assert!(!calls.iter().any(|c| c.arg.contains("Depth")));
}

#[test]
fn ven_026_subscribe_diff() {
    let mut h = feed(f0());
    let mut now = 100;
    h.up(&[S1], streams::BBO | streams::BOOK | streams::TRADE, &mut now);
    h.clear();
    stub::take_log();
    h.sub(S1, streams::BBO | streams::BOOK | streams::TRADE);
    h.poll(now + 1);
    assert!(stub::calls("subscribe").is_empty());
    h.sub(S1, streams::BBO | streams::MARK);
    for _ in 0..4 {
        now += 250_000_000;
        h.poll(now);
        for id in [0, 1, 8, 9] {
            h.ack(ConnId(id));
        }
    }
    let calls = stub::calls("subscribe");
    assert!(
        calls
            .iter()
            .any(|c| c.arg.starts_with("true:") && c.arg.contains("Trade"))
    );
    assert!(
        calls
            .iter()
            .any(|c| c.arg.starts_with("true:") && c.arg.contains("DepthTopN"))
    );
    assert!(
        calls
            .iter()
            .any(|c| c.arg.starts_with("false:") && c.arg.contains("Mark"))
    );
    assert!(!calls.iter().any(|c| c.arg.contains("Bbo")));
    h.clear();
    h.net.push(ConnId(8), b"D|S1|1|1|100 1|101 1");
    h.poll(now + 1);
    assert!(!h.drain_md().iter().any(|e| matches!(e, FeedMsg::Bulk { .. })));
    assert_eq!(h.rt.counters().unsubscribed, 1);
    h.cmd.try_push(FeedCmd::Unsubscribe { inst: S1 }).unwrap();
    for _ in 0..3 {
        now += 250_000_000;
        h.poll(now);
        for id in [0, 1, 8, 9] {
            h.ack(ConnId(id));
        }
    }
    let mut members: Vec<_> = states(&h.drain_bbo())
        .into_iter()
        .filter(|s| s.inst == S1 && s.state == ConnStateKind::Down)
        .map(|s| s.conn)
        .collect();
    members.sort_unstable();
    assert_eq!(members, [0, 1]);
}

#[test]
fn ven_080_rotate_time() {
    const OPEN: u64 = 1_000_000_000;
    for age in [86400, 0] {
        let rd = rd_with(&RdOpts {
            conn_age_s: age,
            ..Default::default()
        });
        let mut h = try_feed::<StubProto>(
            FeedCfg {
                routes: FeedRoutes::Bbo,
                bbo_conns: 1,
                bbo_dead_ms: 2_000_000_000,
                ..f0()
            },
            Caps::default(),
            Net::new(8, 0),
            rd,
        )
        .unwrap();
        h.sub(S1, streams::BBO);
        h.poll(OPEN - 1);
        h.complete(ConnId(0));
        h.poll(OPEN);
        let deadline = OPEN + 82800 * 1_000_000_000;
        h.poll(deadline - 1);
        assert!(h.builds.is_empty());
        h.poll(if age == 0 {
            OPEN + 864000 * 1_000_000_000
        } else {
            deadline
        });
        assert_eq!(h.builds.len(), usize::from(age != 0));
        assert!(h.rt.conn(ConnId(0)).is_some());
    }
}

#[test]
fn ven_081_rotate_cmd() {
    let (mut h, now) = bbo_up(|_| {}, &[S1]);
    h.cmd.try_push(FeedCmd::Rotate { conn: ConnId(1) }).unwrap();
    h.poll(now + 1);
    assert_eq!(h.builds.iter().map(|b| b.conn).collect::<Vec<_>>(), [ConnId(2)]);
    assert!(h.rt.conn(ConnId(1)).is_some());
    assert!(
        !states(&h.drain_bbo())
            .iter()
            .any(|s| s.state == ConnStateKind::Down)
    );
}

#[test]
fn ven_082_race_replacement_retires_oldest() {
    let (mut h, now) = bbo_up(|_| {}, &[S1]);
    h.cmd.try_push(FeedCmd::Rotate { conn: ConnId(0) }).unwrap();
    h.poll(now + 1);
    h.complete(ConnId(2));
    h.poll(now + 2);
    h.ack(ConnId(2));
    h.poll(now + 3);
    assert_eq!(
        h.rt.conn_ids().collect::<Vec<_>>(),
        [ConnId(0), ConnId(1), ConnId(2)]
    );
    assert!(
        states(&h.drain_bbo())
            .iter()
            .any(|s| s.conn == 2 && s.inst == S1 && s.state == ConnStateKind::Up)
    );
    assert!(h.rt.conn(ConnId(0)).is_some());
    h.net.push(ConnId(2), &b("S1", 10));
    h.poll(now + 4);
    assert!(h.retired.contains(&ConnId(0)));
    assert_eq!(h.rt.conn_ids().collect::<Vec<_>>(), [ConnId(1), ConnId(2)]);
    let states = states(&h.drain_bbo());
    assert!(
        states
            .iter()
            .any(|s| s.conn == 0 && s.inst == InstId(0) && s.state == ConnStateKind::Down)
    );
    assert!(!states.iter().any(|s| s.reason == hft_venue::codes::RACE_SINGLE));
}

#[test]
fn ven_083_server_shutdown_hint() {
    let (mut h, now) = bbo_up(|_| {}, &[S1]);
    h.net.push(ConnId(1), b"BYE");
    h.poll(now + 1);
    assert_eq!(h.builds.len(), 1);
    assert!(h.rt.conn(ConnId(1)).is_some());
    assert!(h.drain_bbo().is_empty());
}

#[test]
fn ven_087_frame_events_over_is_decode_error() {
    for count in [63, 64, 65] {
        let mut h = feed(FeedCfg {
            routes: FeedRoutes::Md,
            ..f0()
        });
        let mut now = 100;
        h.up(&[S1], streams::TRADE, &mut now);
        h.clear();
        h.net
            .push(ConnId(8), format!("T|S1|1|100|1|B|{count}").as_bytes());
        h.poll(now + 1);
        let events = h.drain_md();
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, FeedMsg::Trade(_, _)))
                .count(),
            if count == 65 { 0 } else { count }
        );
        assert_eq!(h.rt.counters().frame_events_over, u64::from(count == 65));
        h.net.push(ConnId(8), b"T|S1|66|100|1|B");
        h.poll(now + 2);
        assert_eq!(
            h.drain_md()
                .iter()
                .filter(|e| matches!(e, FeedMsg::Trade(_, _)))
                .count(),
            1
        );
    }
}

#[test]
fn ven_102_streams_independent() {
    let mut h = feed(f0());
    let mut now = 100;
    h.up(&[S1, S2], streams::BBO | streams::TRADE, &mut now);
    h.clear();
    let mut got = Vec::new();
    for (conn, body) in [
        (0, b("S1", 50)),
        (8, b"T|S1|50|100|1|B".to_vec()),
        (1, b("S2", 50)),
        (8, b"T|S1|49|100|1|B".to_vec()),
        (0, b("S1", 51)),
    ] {
        h.net.push(ConnId(conn), &body);
        now += 1;
        h.poll(now);
        got.extend(h.drain_bbo());
        got.extend(h.drain_md());
    }
    assert_eq!(got.len(), 4);
    assert!(matches!(got[0], FeedMsg::Bbo(e, _) if e.h.inst == S1 && e.update_id == 50));
    assert!(matches!(got[1], FeedMsg::Trade(e, _) if e.h.inst == S1 && e.trade_id == 50));
    assert!(matches!(got[2], FeedMsg::Bbo(e, _) if e.h.inst == S2 && e.update_id == 50));
    assert!(matches!(got[3], FeedMsg::Bbo(e, _) if e.update_id == 51));
}

#[test]
fn ven_131_no_snap_when_pushes_first() {
    stub::set_cfg(|c| c.bbo_snapshot = false);
    stub::take_log();
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        ..f0()
    });
    let mut now = 100;
    h.up(&[S1], streams::BBO, &mut now);
    assert_eq!(stub::calls("bbo_snapshot_request").len(), 1);
    for ms in (100..=10_000u64).step_by(100) {
        let now = ms * 1_000_000;
        h.poll(now);
        for id in [ConnId(0), ConnId(1)] {
            for frame in h.net.sent(&mut h.rt, id) {
                if frame.opcode == 9 {
                    h.net.at(id, now + 1, Ev::Pong(frame.payload));
                }
            }
        }
        assert!(h.snapreq.as_mut().unwrap().try_pop().is_none());
    }
    assert_eq!(stub::calls("bbo_snapshot_request").len(), 1);
}
