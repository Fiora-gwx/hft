//! VEN-230, VEN-231, VEN-233: order connections.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{ConnRole, ConnStateKind, SendStatus};
use hft_net::{ConnId, ErrBits};
use support::gwh::*;
use support::net::Ev;
use support::rd::*;
use support::stub::StubProto;

#[test]
fn ven_232_standby_and_user_fatal() {
    use support::stub::{Stub, V_USER_STREAM};
    for user in [false, true] {
        let mut cfg = g0();
        cfg.aux_every = 1;
        let mut h = gw_full::<Stub<2048, V_USER_STREAM>>(cfg, &[A1], GhOpts::default());
        let id = if user {
            h.rt.private_owner(A1).unwrap()
        } else {
            h.rt.standby(A1).unwrap()
        };
        h.net.at(id, 0, Ev::Rst);
        h.poll(3000);
        assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
        assert!(h.retired.contains(&id));
        assert!(!h.builds.is_empty());
        assert!(
            conn_states(&h.reps())
                .iter()
                .any(|s| s.state == ConnStateKind::Down
                    && s.conn == id.0
                    && s.role
                        == if user {
                            ConnRole::UserStream
                        } else {
                            ConnRole::Order
                        })
        );
    }
}

#[test]
fn ven_234_scheduled_switch_replaces_expiring_standby_first() {
    use support::stub::{Stub, V_USER_STREAM};
    for response_at in [0, 2, 60, 120] {
        let cfg = hft_venue::GwCfg {
            aux_every: 1,
            inflight_timeout_ms: 120_000,
            ..g0()
        };
        let mut h = gw_full::<Stub<2048, V_USER_STREAM>>(cfg, &[A1], GhOpts::default());
        // The default 24 h age minus 1 h margin puts primary rotation at 23 h. The pool
        // staggers its original standby another hour earlier; that socket must be replaced.
        let tr = 82_800_000_001_000;
        let old_standby = h.rt.standby(A1).unwrap();
        h.poll(tr - 3_600_000_000_000);
        assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
        assert!(h.retired.contains(&old_standby));
        assert!(
            !conn_states(&h.reps())
                .iter()
                .any(|s| s.state == ConnStateKind::Rotated)
        );
        let replacement = h.builds.iter().find(|r| r.role == ConnRole::Order).unwrap().conn;
        h.complete(replacement);
        h.poll(tr - 3_600_000_000_000 + 1);
        h.reps();
        if response_at != 0 {
            let c = h.place(A1, 1, 1, S1, false);
            h.cmd(c);
            h.poll(tr - 1_000_000_000);
            h.reps();
        }
        h.poll(tr);
        if response_at == 120 {
            h.poll(tr + 60_000_000_000);
            h.poll(tr + 119_000_000_001);
            assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
        }
        let now = tr + response_at * 1_000_000_000;
        if response_at == 2 || response_at == 60 {
            h.net.push(ConnId(112), b"R|1-1-1-5|0");
        }
        if response_at != 0 {
            h.poll(now);
        }
        assert_eq!(h.rt.primary(A1), Some(replacement), "{response_at}");
        assert!(
            conn_states(&h.reps())
                .iter()
                .any(|s| s.state == ConnStateKind::Rotated && s.reason == ErrBits::ROTATE_DUE.bits())
        );
        h.net.push(ConnId(112), b"R|8-1-1-5|-2010");
        h.poll(now + 1);
        assert_eq!(events(&h.reps()).len(), 1);
        h.poll(now + 1_000_000_000);
        assert!(h.rt.conn(ConnId(112)).is_none());
    }
}

#[test]
fn ven_235_early_switch_waits_for_inflight() {
    rotation_wait_boundary_and_old_report_drain();
    use support::stub::{Stub, V_USER_STREAM};
    for (server, busy) in [(false, false), (true, false), (false, true)] {
        let mut cfg = g0();
        cfg.aux_every = 1;
        let mut h = gw_full::<Stub<2048, V_USER_STREAM>>(cfg, &[A1], GhOpts::default());
        let standby = h.rt.standby(A1).unwrap();
        if busy {
            let c = h.place(A1, 1, 1, S1, false);
            h.cmd(c);
            h.poll(3000);
            h.reps();
        }
        if server {
            h.net.push(ConnId(112), b"BYE");
        } else {
            h.cmd(base(hft_event::GwOp::Rotate, A1));
        }
        h.poll(4000);
        if busy {
            assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
            assert!(reports(&h.reps()).is_empty());
            h.net.push(ConnId(112), b"R|1-1-1-5|0");
            h.poll(4001);
        }
        assert_eq!(h.rt.primary(A1), Some(standby), "server={server} busy={busy}");
        let v = h.reps();
        assert!(reports(&v).is_empty());
        assert!(conn_states(&v).iter().any(|s| s.state == ConnStateKind::Rotated
            && s.conn == standby.0
            && s.reason == if server { ErrBits::SERVER_HINT.bits() } else { 0 }));
    }
}

pub(crate) fn rotation_wait_boundary_and_old_report_drain() {
    use support::stub::{Stub, V_USER_STREAM};
    for response_at in [0, 2, 60, 120] {
        let mut cfg = g0();
        cfg.aux_every = 1;
        cfg.inflight_timeout_ms = 120_000;
        let mut h = gw_full::<Stub<2048, V_USER_STREAM>>(cfg, &[A1], GhOpts::default());
        let standby = h.rt.standby(A1).unwrap();
        let tr = 2_000_001_000;
        if response_at != 0 {
            let c = h.place(A1, 1, 1, S1, false);
            h.cmd(c);
            h.poll(tr - 1_000_000_000);
            h.reps();
        }
        h.cmd(base(hft_event::GwOp::Rotate, A1));
        h.poll(tr);
        if response_at == 120 {
            h.poll(tr + 60_000_000_000);
            h.poll(tr + 119_000_000_001);
            assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
        }
        let now = tr + response_at * 1_000_000_000;
        if response_at != 0 {
            h.net.push(ConnId(112), b"R|1-1-1-5|0");
            h.poll(now);
        }
        assert_eq!(h.rt.primary(A1), Some(standby), "{response_at}");
        h.reps();
        h.net.push(ConnId(112), b"R|8-1-1-5|-2010");
        h.poll(now + 1);
        assert_eq!(events(&h.reps()).len(), 1);
        h.poll(now + 1_000_000_000);
        assert!(h.rt.conn(ConnId(112)).is_none());
    }
}

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
        .filter(|s| s.state == ConnStateKind::Down && s.role == ConnRole::Order)
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
    assert_eq!(
        up,
        vec![
            (A1, ConnRole::Order, 112),
            (A1, ConnRole::UserStream, 112),
            (A1, ConnRole::Order, 113)
        ]
    );
}
