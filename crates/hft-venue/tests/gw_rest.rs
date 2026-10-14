use crate::support::{
    gwh::*,
    mempeer, net,
    rd::*,
    stub::{self, Stub, V_REST_CANCEL_ALL, V_SWEEP},
};
use hft_event::{GwOp, OrderEventKind, RepMsg, SendStatus};
use hft_net::{ConnId, EndpointId, RestPool};
fn setup<const V: u32>() -> Gh<Stub<2048, V>> {
    stub::set_cfg(|c| c.rest_query = true);
    let mut h = gw_full::<Stub<2048, V>>(g0(), &[A1], GhOpts::default());
    let pool = RestPool::new(b"a.test", &net::net_cfg(), mempeer::clock());
    h.rt.set_rest_pools(A1, vec![(EndpointId(0), pool)]).unwrap();
    h.open(3000, 2);
    h.reps();
    h
}
fn response<const V: u32>(h: &mut Gh<Stub<2048, V>>, status: u16, now: u64) {
    let msg = format!("HTTP/1.1 {status} Result\r\nContent-Length: 2\r\n\r\nok");
    let wire = h.net.peer(ConnId(114)).records(&[msg.as_bytes()]);
    mempeer::deliver(h.rt.conn(ConnId(114)).unwrap(), now, now, wire);
    for n in 0..8 {
        h.poll(now + n);
    }
}

#[test]
fn ven_210_wait_fds_only_pending_rest() {
    use hft_rt::PollTask;
    let mut h = setup::<0>();
    let p = h.place(A1, 7, 3, S1, false);
    h.cmd(p);
    h.poll(4000);
    h.reps();
    let q = h.targeted(GwOp::Query, A1, 7);
    h.cmd(q);
    h.poll(5000);
    let mut fds = hft_rt::WaitFds::new();
    h.rt.wait_fds(&mut fds);
    let mut expected = [112, 113, 114]
        .map(|i| h.rt.conn(ConnId(i)).unwrap().raw_fd())
        .to_vec();
    let mut got = fds.as_slice().to_vec();
    got.sort();
    expected.sort();
    assert_eq!(got, expected);
    response(&mut h, 200, 6000);
    fds.clear();
    h.rt.wait_fds(&mut fds);
    let mut got = fds.as_slice().to_vec();
    got.sort();
    let mut expected = [112, 113]
        .map(|i| h.rt.conn(ConnId(i)).unwrap().raw_fd())
        .to_vec();
    expected.sort();
    assert_eq!(got, expected);
}

#[test]
fn ven_236_deadman_covers_live_smoke_and_account_scope() {
    use hft_venue_proto::{DeadManSpec, Via};
    let mut h = setup::<0>();
    let a = h.place(A1, 1, 1, S1, false);
    let mut b = h.place(A1, 2, 1, S2, false);
    b.oflags |= hft_event::OrderFlags::SMOKE.0;
    h.cmd(a);
    h.cmd(b);
    h.poll(4000);
    h.reps();
    h.net.sent_text(&mut h.rt, ConnId(112));
    h.cmd(hft_event::GwCmd {
        arg: 30_000,
        ..base(GwOp::DeadManRefresh, A1)
    });
    h.poll(5000);
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(112))
            .iter()
            .any(|s| s == "DM|30000|701,702")
    );
    assert!(reports(&h.reps()).is_empty());
    let mut h = setup::<0>();
    h.cmd(base(GwOp::DeadManRefresh, A1));
    h.poll(4000);
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(112))
            .iter()
            .all(|s| !s.starts_with("DM|"))
    );
    stub::set_cfg(|c| {
        c.dead_man = DeadManSpec::CountdownAccount {
            min_ms: 5000,
            max_ms: 60000,
            timeout_ms: 30000,
            refresh_ms: 10000,
            via: Via::Rest,
        }
    });
    h.cmd(base(GwOp::DeadManRefresh, A1));
    h.poll(5000);
    response(&mut h, 500, 6000);
    assert!(
        conn_states(&h.reps())
            .iter()
            .any(|s| s.reason == hft_venue::codes::DEADMAN)
    );
    stub::set_cfg(|c| c.dead_man = DeadManSpec::None);
    h.cmd(base(GwOp::DeadManRefresh, A1));
    h.poll(7000);
    assert!(
        conn_states(&h.reps())
            .iter()
            .any(|s| s.reason == hft_venue::codes::DEADMAN)
    );
}

#[test]
fn ven_237_maintenance_uses_private_owner_and_rest_session_status() {
    use crate::support::stub::V_USER_STREAM;
    use hft_event::{ConnRole, ConnStateKind};
    for (status, body, supported) in [
        (200, "ok", true),
        (400, "bad", true),
        (200, "ended", true),
        (200, "ok", false),
    ] {
        stub::set_cfg(|c| {
            c.keepalive = supported;
            c.private_channels = 1;
        });
        let mut h = setup::<V_USER_STREAM>();
        h.net.push(ConnId(114), b"PACK|0|ok");
        h.poll(3500);
        let owner = h.rt.private_owner(A1).unwrap();
        h.reps();
        h.net.sent_text(&mut h.rt, owner);
        h.net.sent_text(&mut h.rt, ConnId(113));
        h.cmd(base(GwOp::Resubscribe, A1));
        h.poll(4000);
        assert!(
            h.net
                .sent_text(&mut h.rt, owner)
                .iter()
                .any(|s| s.starts_with("PSUB|"))
        );
        assert!(h.net.sent_text(&mut h.rt, ConnId(113)).is_empty());
        h.net.push(owner, b"PACK|0|ok");
        h.poll(4100);
        h.reps();
        h.cmd(base(GwOp::SessionKeepalive, A1));
        h.poll(5000);
        if supported {
            let msg = format!(
                "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let wire = h.net.peer(ConnId(115)).records(&[msg.as_bytes()]);
            mempeer::deliver(h.rt.conn(ConnId(115)).unwrap(), 6000, 6000, wire);
            for n in 0..8 {
                h.poll(6000 + n);
            }
        }
        let reps = h.reps();
        assert!(reports(&reps).is_empty());
        let states = conn_states(&reps);
        if body == "ended" {
            assert!(states.iter().any(|s| s.role == ConnRole::UserStream
                && s.state == ConnStateKind::Down
                && s.conn == owner.0));
            assert!(h.retired.contains(&owner));
            assert!(h.rt.conn(ConnId(115)).is_some());
        } else if status == 400 {
            assert!(
                states
                    .iter()
                    .any(|s| s.role == ConnRole::UserStream && s.reason == hft_venue::codes::DEADMAN)
            );
        } else {
            assert!(states.is_empty());
        }
        assert_eq!(h.rt.counters().keepalive_unsupported, u64::from(!supported));
        h.cmd(hft_event::GwCmd::set_epoch(6, true, 5));
        h.poll(7000);
        assert!(reports(&h.reps()).is_empty());
        let c = h.place(A1, 7, 1, S1, false);
        h.cmd(c);
        h.poll(8000);
        assert_eq!(h.rt.mirror(7).unwrap().epoch, 6);
    }
}

#[test]
fn ven_247_deadman_only_on_explicit_command() {
    let mut h = setup::<0>();
    let c = h.place(A1, 7, 1, S1, false);
    h.cmd(c);
    h.poll(4000);
    h.net.sent_text(&mut h.rt, ConnId(112));
    for second in 1..=600 {
        h.poll(second * 1_000_000_000);
        h.reps();
    }
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(112))
            .iter()
            .all(|s| !s.starts_with("DM|"))
    );
    assert!(stub::calls("decode_rest").is_empty());
}
#[test]
fn ven_228_query_uses_persistent_rest_and_stable_origin() {
    let mut h = setup::<0>();
    let p = h.place(A1, 7, 3, S1, false);
    h.cmd(p);
    h.poll(4000);
    h.reps();
    let q = h.targeted(GwOp::Query, A1, 7);
    h.cmd(q);
    h.poll(5000);
    let r = reports(&h.reps());
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].status, r[0].conn), (SendStatus::Written, ConnId(114)));
    response(&mut h, 200, 6000);
    let v = h.reps();
    assert!(reports(&v).is_empty(), "one send report, at submission");
    assert!(v.iter().any(|m|matches!(m,RepMsg::OrderEvent(e) if e.what==OrderEventKind::QueryResult && e.epoch==5 && e.slot.0==7 && e.r#gen==3 && e.cum_filled.0==3)));
    assert_eq!(stub::calls("encode_rest").len(), 1);
    assert_eq!(stub::calls("encode_rest_signed").len(), 1);
}
#[test]
fn ven_228_rest_identity_survives_epoch_change_and_slot_reuse() {
    for status in [200, 503] {
        for accept_prev in [false, true] {
            let mut h = setup::<0>();
            let old = h.place(A1, 7, 3, S1, false);
            h.cmd(old);
            h.poll(4000);
            h.reps();
            let q = h.targeted(GwOp::Query, A1, 7);
            h.cmd(q);
            h.poll(5000);
            h.reps();
            h.cmd(hft_event::GwCmd::set_epoch(70, accept_prev, 5));
            h.poll(5100);
            // Even reusing the same generation cannot redirect a response to the new epoch.
            let new = h.place(A1, 7, 3, S1, false);
            h.cmd(new);
            h.poll(5200);
            h.reps();
            response(&mut h, status, 6000);
            let orders = events(&h.reps());
            assert_eq!(orders.len(), 1);
            let e = orders[0];
            assert_eq!(e.epoch, 5, "the original request owns the response");
            if status == 200 && !accept_prev {
                assert_eq!(e.slot, hft_types::SlotId::UNRESOLVED);
            } else {
                assert_eq!((e.slot.0, e.r#gen), (7, 3));
            }
            let mirror = h.rt.mirror(7).unwrap();
            assert_eq!((mirror.epoch, mirror.r#gen, mirror.live), (70, 3, true));
        }
    }
}
#[test]
fn ven_228_rest_failures_preserve_unknown_outcome() {
    for status in [429, 503] {
        let mut h = setup::<0>();
        let p = h.place(A1, 7, 3, S1, false);
        h.cmd(p);
        h.poll(4000);
        h.reps();
        let q = h.targeted(GwOp::Query, A1, 7);
        h.cmd(q);
        h.poll(5000);
        h.reps();
        response(&mut h, status, 6000);
        let v = h.reps();
        assert!(v.iter().any(
            |m| matches!(m,RepMsg::OrderEvent(e) if e.what==OrderEventKind::OutcomeUnknown && e.slot.0==7)
        ));
        assert!(
            !v.iter()
                .any(|m| matches!(m,RepMsg::OrderEvent(e) if e.what==OrderEventKind::Reject))
        );
    }
}
#[test]
fn ven_392_cancel_all_uses_declared_rest_channel() {
    let mut h = setup::<V_REST_CANCEL_ALL>();
    stub::take_log();
    h.cmd(base(GwOp::CancelAll, A1));
    h.poll(4000);
    assert_eq!(statuses(&h.reps()), vec![SendStatus::Written]);
    assert_eq!(stub::calls("encode_rest").len(), 1);
    assert!(stub::calls("encode").is_empty());
}
#[test]
fn ven_391_cancel_all_sweep_decodes_owned_open_orders() {
    stub::set_cfg(|c| {
        c.account_cancel_all = false;
        c.inst_cancel_all = false;
    });
    let mut h = setup::<V_SWEEP>();
    h.cmd(base(GwOp::CancelAll, A1));
    h.poll(4000);
    assert_eq!(statuses(&h.reps()), vec![SendStatus::Written]);
    assert_eq!(stub::calls("ro_request_signed").len(), 1);
    response(&mut h, 200, 5000);
    assert!(
        h.reps()
            .iter()
            .any(|m| matches!(m,RepMsg::OrderEvent(e) if e.what==OrderEventKind::QueryResult && e.slot.0==7))
    );
}

#[test]
fn ven_228_rest_keepalive_borrows_account_credential() {
    let mut h = setup::<0>();
    stub::take_log();
    h.cmd(base(GwOp::SessionKeepalive, A1));
    h.poll(4000);
    assert_eq!(stub::calls("session_keepalive_request_signed").len(), 1);
    assert!(
        h.reps()
            .iter()
            .all(|m| !matches!(m, RepMsg::ConnState(c) if c.state == hft_event::ConnStateKind::Degraded))
    );
}
#[test]
fn ven_393_rest_polling_reads_only_pending_lanes() {
    let mut h = setup::<0>();
    let p = h.place(A1, 7, 3, S1, false);
    h.cmd(p);
    h.poll(4000);
    h.reps();
    let q = h.targeted(GwOp::Query, A1, 7);
    h.cmd(q);
    h.poll(5000);
    h.reps();
    let before = h.rt.conn(ConnId(114)).unwrap().sys().recvmsg;
    let idle = h.rt.conn(ConnId(115)).unwrap().sys().recvmsg;
    for n in 0..80 {
        h.poll(6000 + n);
    }
    assert_eq!(h.rt.conn(ConnId(114)).unwrap().sys().recvmsg - before, 10);
    assert_eq!(h.rt.conn(ConnId(115)).unwrap().sys().recvmsg, idle);
}

#[test]
fn ven_209_rest_idle_check() {
    let mut h = setup::<0>();
    let first = ConnId(114);
    let other = ConnId(115);
    let a = h.rt.conn(first).unwrap().sys().recvmsg;
    let b = h.rt.conn(other).unwrap().sys().recvmsg;
    for _ in 0..8 {
        h.poll(999_999_999);
    }
    assert_eq!(h.rt.conn(first).unwrap().sys().recvmsg, a);
    assert_eq!(h.rt.conn(other).unwrap().sys().recvmsg, b);
    for _ in 0..8 {
        h.poll(1_000_000_000);
    }
    assert_eq!(h.rt.conn(first).unwrap().sys().recvmsg, a + 1);
    assert_eq!(h.rt.conn(other).unwrap().sys().recvmsg, b + 1);
    for _ in 0..16 {
        h.poll(1_000_000_000);
    }
    assert_eq!(h.rt.conn(first).unwrap().sys().recvmsg, a + 1);
    h.net.at(first, 2_000_000_000, crate::support::net::Ev::Eof);
    for _ in 0..8 {
        h.poll(2_000_000_000);
    }
    assert!(h.retired.contains(&first));
    assert!(h.rt.conn(other).is_some());
    assert_eq!(h.rt.conn(other).unwrap().sys().recvmsg, b + 2);
    assert!(h.builds.iter().any(|r| r.role == hft_net::ConnRole::Rest));
}
