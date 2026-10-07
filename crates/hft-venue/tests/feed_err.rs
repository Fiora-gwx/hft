//! VEN-075–VEN-077: connection error bits.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::ConnStateKind;
use hft_net::sock::{SendKind, SendStep};
use hft_net::{ConnId, ErrBits};
use hft_types::InstId;
use hft_venue::{FeedCfg, FeedRoutes, codes, streams};
use support::feedh::*;
use support::net::{Ev, Net};
use support::rd::*;
use support::stub::StubProto;

#[test]
fn ven_075_fatal_and_race_single() {
    let (mut h, now) = bbo_up(|c| c.bbo_conns = 2, &[S1]);
    h.net.at(ConnId(1), 0, Ev::Eof);
    h.poll(now + 10);
    let v = h.drain_bbo();
    let s = states(&v);
    assert_eq!(s.len(), 2, "{s:?}");
    assert_eq!(
        (s[0].state, s[0].conn, s[0].inst, s[0].reason),
        (ConnStateKind::Down, 1, InstId(0), ErrBits::PEER_EOF.bits())
    );
    assert_eq!(
        (s[1].state, s[1].conn, s[1].inst, s[1].reason),
        (ConnStateKind::Degraded, 0, InstId(0), codes::RACE_SINGLE)
    );
    assert_eq!(h.retired, vec![ConnId(1)]);
    assert_eq!(
        h.builds.iter().map(|b| b.conn).collect::<Vec<_>>(),
        vec![ConnId(1)],
        "rebuilt in the same round"
    );
    h.net.push(ConnId(0), &b("S1", 7));
    h.poll(now + 20);
    assert_eq!(bbos(&h.drain_bbo()).len(), 1);
}

#[test]
fn ven_076_single_conn_no_race_single() {
    let (mut h, now) = bbo_up(|c| c.bbo_conns = 1, &[S1]);
    h.net.at(ConnId(0), 0, Ev::Rst);
    h.poll(now + 10);
    let s = states(&h.drain_bbo());
    assert_eq!(s.len(), 1);
    assert_eq!(
        (s[0].state, s[0].conn, s[0].inst, s[0].reason),
        (ConnStateKind::Down, 0, InstId(0), ErrBits::RESET.bits())
    );
    assert!(!s.iter().any(|x| x.reason == codes::RACE_SINGLE));
}

#[test]
fn ven_077_nonfatal_bits() {
    let cfg = FeedCfg {
        routes: FeedRoutes::Bbo,
        ..f0()
    };
    let mut h = feed_net::<StubProto>(cfg, Caps::default(), Net::expecting_hw());
    let mut now = 100;
    h.up(&[S1], streams::BBO, &mut now);
    h.clear();
    let c0 = ConnId(0);
    // TX_PARTIAL: the probe's sendmsg accepts 1 byte.
    h.rt.conn(c0).unwrap().script().unwrap().push_send(SendStep {
        post_tsc: 0,
        kind: SendKind::Bytes(1),
    });
    now = 1_000_000_200;
    h.poll(now);
    // TS_MISSING: data without a HW stamp on a connection expecting one.
    h.net.push(c0, &b("S1", 1));
    now += 10;
    h.poll(now);
    // KEY_UPDATE: the peer updates its keys.
    let ku = h.net.peer(c0).key_update();
    h.rt.conn(c0)
        .unwrap()
        .script()
        .unwrap()
        .push_recv(hft_net::sock::RecvStep::data(now, now, ku));
    now += 10;
    h.poll(now);
    let v = h.drain_bbo();
    assert!(states(&v).is_empty(), "{v:?}");
    assert!(h.retired.is_empty());
    assert!(h.rt.conn_ids().any(|i| i == c0));
}
