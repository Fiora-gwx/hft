//! `feed.depth_conns ≥ 2` (README §3c): DepthTopN connections of a group back each other up.
use crate::support::{feedh::*, net::Ev, rd::*};
use hft_event::{ConnStateKind, FeedMsg};
use hft_net::ConnId;
use hft_venue::{FeedCfg, FeedRoutes, InitErr, streams};

fn depth_cfg(n: u8) -> FeedCfg {
    FeedCfg {
        routes: FeedRoutes::Md,
        depth_conns: n,
        ..f0()
    }
}

fn snapshots(v: &[FeedMsg]) -> usize {
    v.iter().filter(|m| matches!(m, FeedMsg::Bulk { .. })).count()
}

#[test]
fn ven_397_depth_conns_back_each_other_up() {
    let mut h = feed(depth_cfg(2));
    let mut t = 100;
    h.up(&[S1], streams::BOOK, &mut t);
    // Step 1: two connections of group 0, both subscribed.
    let mut ids: Vec<ConnId> = h.rt.conn_ids().collect();
    ids.sort_unstable();
    assert_eq!(ids, [ConnId(8), ConnId(9)]);
    h.clear();
    // Step 2: the same snapshot on both connections is forwarded once.
    for c in [8, 9] {
        h.net.push(ConnId(c), b"D|S1|20|1|100 1|101 1");
    }
    t += 1;
    h.poll(t);
    let md = h.drain_md();
    assert_eq!(snapshots(&md), 1);
    assert!(has_state(&md, 8, ConnStateKind::Up, S1, 0) || has_state(&md, 9, ConnStateKind::Up, S1, 0));
    assert_eq!(h.rt.counters().dup, 1);
    // Step 3: connection 8 closes; S1 stays live on 9, the group runs on one connection.
    h.net.at(ConnId(8), 0, Ev::Eof);
    t += 1;
    h.poll(t);
    let md = h.drain_md();
    assert!(
        states(&md)
            .iter()
            .any(|s| s.conn == 8 && s.state == ConnStateKind::Down && s.inst.0 == 0)
    );
    assert!(has_state(
        &md,
        9,
        ConnStateKind::Degraded,
        hft_types::InstId(0),
        5
    ));
    assert!(!states(&md).iter().any(|s| s.state == ConnStateKind::Resync));
    // Step 4: data on 9 keeps flowing; a replacement is requested.
    h.net.push(ConnId(9), b"D|S1|21|1|100 1|101 1");
    t += 1;
    h.poll(t);
    assert_eq!(snapshots(&h.drain_md()), 1);
    assert_eq!(h.builds.len(), 1);
    let r = h.builds[0].conn;
    h.complete(r);
    for _ in 0..10_000 {
        t += 1_000;
        h.poll(t);
        if h.ack(r) > 0 {
            break;
        }
    }
    t += 1;
    h.poll(t);
    h.clear();
    // Step 5: 9 closes while the replacement holds S1: no resync.
    h.net.at(ConnId(9), 0, Ev::Eof);
    t += 1;
    h.poll(t);
    let md = h.drain_md();
    assert!(!states(&md).iter().any(|s| s.state == ConnStateKind::Resync));
    assert!(has_state(
        &md,
        r.0,
        ConnStateKind::Degraded,
        hft_types::InstId(0),
        5
    ));
    // Step 6: the last connection holding S1 closes: Resync{CONN_LOST}.
    h.builds.clear();
    h.net.at(r, 0, Ev::Eof);
    t += 1;
    h.poll(t);
    let md = h.drain_md();
    assert!(has_state(&md, r.0, ConnStateKind::Resync, S1, 15));
}

#[test]
fn ven_398_depth_conns_limits() {
    for (n, ok) in [(0u8, false), (1, true), (2, true), (56, false)] {
        let r = try_feed::<crate::support::stub::StubProto>(
            depth_cfg(n),
            Caps::default(),
            crate::support::net::Net::new(64, 4),
            rd_stub(),
        );
        match r {
            Ok(_) => assert!(ok, "{n}"),
            Err(e) => {
                assert!(!ok, "{n}");
                assert_eq!(e, InitErr::Cfg("feed.depth_conns"));
            }
        }
    }
}
