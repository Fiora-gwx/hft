use crate::support::{
    gwh::*,
    net::Ev,
    rd::*,
    stub::{self, Stub, StubProto, V_USER_STREAM},
};
use hft_event::{ConnRole, ConnStateKind, GwOp, RepMsg};
use hft_net::ConnId;
fn setup() -> Gh<StubProto> {
    stub::set_cfg(|c| c.private_channels = 3);
    let mut h = gw_full::<StubProto>(g0(), &[A1], GhOpts::default());
    h.net.push(ConnId(112), b"PACK|0|ok");
    h.net.push(ConnId(112), b"PACK|1|ok");
    h.poll(3000);
    h.reps();
    h
}
fn rotate(h: &mut Gh<StubProto>, now: u64) {
    h.cmd(base(GwOp::Rotate, A1));
    h.poll(now);
}
#[test]
fn ven_270_standby_never_subscribes() {
    let mut h = setup();
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(112))
            .iter()
            .any(|s| s.starts_with("PSUB|"))
    );
    assert!(
        !h.net
            .sent_text(&mut h.rt, ConnId(113))
            .iter()
            .any(|s| s.starts_with("PSUB|"))
    );
    h.net.at(ConnId(113), 0, Ev::Rst);
    for n in 0..8 {
        h.poll(4000 + n);
    }
    let replacement = h.builds[0].conn;
    h.complete(replacement);
    h.poll(5000);
    assert!(
        !h.net
            .sent_text(&mut h.rt, replacement)
            .iter()
            .any(|s| s.starts_with("PSUB|"))
    );
    assert_eq!(h.rt.private_owner(A1), Some(ConnId(112)));
}
#[test]
fn ven_271_owner_by_shape_and_warm_frames() {
    let mut h = setup();
    assert_eq!(h.rt.private_owner(A1), Some(ConnId(112)));
    stub::take_log();
    rotate(&mut h, 4000);
    assert!(stub::calls("private_subscribe").is_empty());
    stub::set_cfg(|c| c.private_channels = 0);
    let mut h = gw_full::<Stub<2048, V_USER_STREAM>>(g0(), &[A1], GhOpts::default());
    let owner = h.rt.private_owner(A1).unwrap();
    assert_ne!(owner, ConnId(112));
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(112))
            .iter()
            .all(|s| !s.starts_with("PSUB|"))
    );
}
#[test]
fn ven_272_make_before_break_shared_dedup() {
    let mut h = setup();
    rotate(&mut h, 4000);
    assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
    h.net.push(ConnId(113), b"PACK|0|ok");
    h.poll(4001);
    assert_eq!(h.rt.private_owner(A1), Some(ConnId(112)));
    h.net.push(ConnId(113), b"PACK|1|ok");
    h.poll(4002);
    assert_eq!(h.rt.private_owner(A1), Some(ConnId(113)));
    assert_eq!(h.rt.primary(A1), Some(ConnId(113)));
    let id = hft_types::ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot: 7,
        r#gen: 1,
    };
    let frame = format!("O|{}|EX-7|Fill|3|3|100|0|0|77", stub::cid_text(&id));
    h.net.push(ConnId(112), frame.as_bytes());
    h.net.push(ConnId(113), frame.as_bytes());
    let mut events = Vec::new();
    for n in 0..8 {
        h.poll(5000 + n);
        events.extend(h.reps());
    }
    assert_eq!(
        events
            .iter()
            .filter(|m| matches!(m,RepMsg::OrderEvent(e) if e.trade_id==77))
            .count(),
        1
    );
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(112))
            .iter()
            .any(|s| s.starts_with("PUNSUB|"))
    );
    h.poll(1_000_004_002);
    assert!(h.retired.contains(&ConnId(112)));
}
#[test]
fn ven_273_switch_rejection_and_incomplete_ack_keep_old_owner() {
    for reject in [false, true] {
        let mut h = setup();
        rotate(&mut h, 4000);
        h.net
            .push(ConnId(113), if reject { b"PACK|0|no" } else { b"PACK|0|ok" });
        h.poll(4001);
        h.net.push(ConnId(113), b"PACK|0|ok");
        h.poll(4002); // duplicate ACK cannot satisfy bit 1
        h.poll(5_000_004_000);
        assert_eq!(h.rt.private_owner(A1), Some(ConnId(112)));
        assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
        assert!(
            h.net
                .sent_text(&mut h.rt, ConnId(113))
                .iter()
                .any(|s| s.starts_with("PUNSUB|"))
        );
        assert!(
            !conn_states(&h.reps())
                .iter()
                .any(|s| s.state == ConnStateKind::Rotated)
        );
    }
}
#[test]
fn ven_274_forced_switch_waits_for_all_private_ack() {
    let mut h = setup();
    h.net.at(ConnId(112), 0, Ev::Rst);
    h.poll(4000);
    assert_eq!(h.rt.private_owner(A1), None);
    let states = conn_states(&h.reps());
    assert!(
        states
            .iter()
            .any(|s| s.role == ConnRole::Order && s.state == ConnStateKind::Down)
    );
    assert!(
        states
            .iter()
            .any(|s| s.role == ConnRole::UserStream && s.state == ConnStateKind::Down)
    );
    h.net.push(ConnId(113), b"PACK|0|ok");
    h.net.push(ConnId(113), b"PACK|1|ok");
    h.poll(4001);
    assert_eq!(h.rt.private_owner(A1), Some(ConnId(113)));
    assert!(
        conn_states(&h.reps())
            .iter()
            .any(|s| s.role == ConnRole::UserStream && s.state == ConnStateKind::Up)
    );
}
#[test]
fn ven_275_push_not_owner_is_raw_only() {
    let mut h = setup();
    h.net.push(ConnId(113), b"X|liquidation");
    for n in 0..8 {
        h.poll(4000 + n);
    }
    assert_eq!(h.rt.counters().private_unowned, 1);
    assert!(!h.reps().iter().any(|m| matches!(m, RepMsg::ExternalFill(_))));
}
#[test]
fn ven_276_user_stream_replacement_waits_for_first_event() {
    stub::set_cfg(|c| c.private_channels = 0);
    let mut h = gw_full::<Stub<2048, V_USER_STREAM>>(g0(), &[A1], GhOpts::default());
    let old = h.rt.private_owner(A1).unwrap();
    h.rt.conn(old).unwrap().mark(hft_net::ErrBits::SERVER_HINT);
    h.poll(4000);
    let new = h
        .builds
        .iter()
        .find(|b| b.role == ConnRole::UserStream)
        .unwrap()
        .conn;
    h.complete(new);
    h.poll(4001);
    assert_eq!(h.rt.private_owner(A1), Some(old));
    h.net.push(new, b"E|listStatus");
    h.poll(4002);
    assert_eq!(h.rt.private_owner(A1), Some(new));
    assert!(h.retired.contains(&old));
    assert!(
        !conn_states(&h.reps())
            .iter()
            .any(|s| s.role == ConnRole::UserStream && s.state == ConnStateKind::Down)
    );
}

#[test]
fn ven_248_stream_end_resubscribes_order_connection() {
    let mut h = setup();
    h.net.sent_text(&mut h.rt, ConnId(112));
    h.net.push(ConnId(112), b"E|ENDED");
    h.poll(4000);
    assert_eq!(h.rt.primary(A1), Some(ConnId(112)));
    assert_eq!(h.rt.private_owner(A1), None);
    assert!(
        conn_states(&h.reps())
            .iter()
            .any(|s| s.role == ConnRole::UserStream && s.state == ConnStateKind::Down)
    );
    assert!(
        h.net
            .sent_text(&mut h.rt, ConnId(112))
            .iter()
            .any(|s| s.starts_with("PSUB|"))
    );
    h.net.push(ConnId(112), b"PACK|0|ok");
    h.net.push(ConnId(112), b"PACK|1|ok");
    h.poll(4001);
    assert_eq!(h.rt.private_owner(A1), Some(ConnId(112)));
    assert!(!h.retired.contains(&ConnId(112)));
}
