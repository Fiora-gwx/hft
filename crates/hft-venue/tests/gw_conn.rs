//! VEN-230, VEN-231, VEN-233: order connections.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::{ConnRole, ConnStateKind, SendStatus};
use hft_net::{ConnId, ErrBits};
use support::gwh::*;
use support::net::Ev;
use support::rd::*;
use support::stub::StubProto;

#[test]
fn ven_230_primary_fatal_failover() {
    let mut h = gw::<StubProto>();
    for s in [1, 2] {
        let c = h.place(A1, s, 1, S1, false);
        h.cmd(c);
    }
    h.poll(3_000_000);
    h.reps();
    assert_eq!(h.rt.inflight(ConnId(112)), 2);
    h.net.at(ConnId(112), 0, Ev::Rst);
    h.poll(3_000_001);
    let v = h.reps();
    let d: Vec<_> = conn_states(&v)
        .into_iter()
        .filter(|s| s.state == ConnStateKind::Down)
        .collect();
    assert_eq!(d.len(), 1);
    assert_eq!(
        (d[0].account, d[0].role, d[0].conn, d[0].reason),
        (A1, ConnRole::Order, 112, ErrBits::RESET.bits())
    );
    assert_eq!(h.rt.primary(A1), Some(ConnId(113)));
    assert_eq!(h.builds.len(), 1, "a replacement is requested");
    let c = h.place(A1, 3, 1, S1, false);
    h.cmd(c);
    h.poll(3_000_002);
    let r = reports(&h.reps());
    assert_eq!((r[0].status, r[0].conn), (SendStatus::Written, ConnId(113)));
}

#[test]
fn ven_231_no_standby() {
    let mut h = gw_with::<StubProto, 2048>(g0(), &[A1], GhOpts::default()).unwrap();
    h.poll(1_000);
    h.complete(ConnId(112));
    h.poll(1_001);
    h.cmd(hft_event::GwCmd::set_epoch(5, false, 0));
    h.poll(1_002);
    assert_eq!(
        h.builds.iter().map(|b| b.conn).collect::<Vec<_>>(),
        vec![ConnId(113)],
        "standby build left open"
    );
    h.reps();
    h.net.at(ConnId(112), 0, Ev::Eof);
    h.poll(2_000);
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    h.poll(2_001);
    let v = h.reps();
    assert!(
        conn_states(&v)
            .iter()
            .any(|s| s.state == ConnStateKind::Down && s.conn == 112)
    );
    assert_eq!(statuses(&v), vec![SendStatus::NoConn]);
    // Step 2: the open build fails, the next one (114) is handed over.
    h.fail(ConnId(113));
    h.poll(2_002);
    assert_eq!(
        h.builds.iter().map(|b| b.conn).collect::<Vec<_>>(),
        vec![ConnId(114)]
    );
    h.complete(ConnId(114));
    h.poll(2_003);
    let c = h.place(A1, 2, 1, S1, false);
    h.cmd(c);
    h.poll(2_004);
    let v = h.reps();
    assert!(conn_states(&v).iter().any(|s| s.state == ConnStateKind::Up
        && s.account == A1
        && s.role == ConnRole::Order
        && s.conn == 114));
    let r = reports(&v);
    assert_eq!((r[0].status, r[0].conn), (SendStatus::Written, ConnId(114)));
}

#[test]
fn ven_233_up_on_install() {
    let mut h = gw_with::<StubProto, 2048>(g0(), &[A1], GhOpts::default()).unwrap();
    h.poll(1_000);
    h.complete(ConnId(112));
    h.poll(1_001);
    h.complete(ConnId(113));
    h.poll(1_002);
    let up: Vec<_> = conn_states(&h.reps())
        .into_iter()
        .filter(|s| s.state == ConnStateKind::Up)
        .map(|s| (s.account, s.role, s.conn))
        .collect();
    assert_eq!(up, vec![(A1, ConnRole::Order, 112), (A1, ConnRole::Order, 113)]);
}
