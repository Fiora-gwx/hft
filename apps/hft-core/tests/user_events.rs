//! C12's transport/OMS behavior on the supported Spot adapter. The frozen
//! OKX protocol matrix remains separate acceptance work; Venue diagnostics reach the journal.
#[path = "../../../crates/hft-sequencer/tests/common/mod.rs"]
mod common;
mod net_support;
use common::{risk_kit::*, *};
use hft_event::{ConnRole, ConnStateKind, GwOp, RepMsg};
use hft_strategy_sdk::IntentClass;
use hft_types::{Side, Tif, Tsc};

#[test]
fn c12_ignorable_and_unknown_user_events_only_counted() {
    reset_script();
    let mut config = b0();
    config
        .rd
        .caps
        .iter_mut()
        .find(|c| c.venue == A)
        .unwrap()
        .ignorable_user_events = vec!["evt_ignorable".into()].into_boxed_slice();
    let mut h = config.boot_self_contained();
    h.steady();
    let clock = hft_rt::TscClock::from_hz_milli(
        HZ,
        hft_rt::CalibPair {
            tsc: Tsc(T0),
            realtime_ns: UTC0,
            ..Default::default()
        },
    );
    let rd = Box::leak(Box::new(h.rd.clone()));
    let mut gateway = net_support::Gateway::new(rd, clock);
    for _ in 0..8 {
        gateway.poll(h.now());
    }
    let subscribed = gateway.net.sent_text(&mut gateway.runtime, hft_net::ConnId(112));
    assert!(subscribed.iter().any(|f| f.contains("userDataStream.subscribe")));
    gateway.net.push(
        hft_net::ConnId(112),
        br#"{"id":"u0000000000000000","status":200,"result":{"subscriptionId":1}}"#,
    );
    gateway.poll(h.now());
    gateway.deliver(&mut h);
    // Spot private subscriptions share the authenticated order connection.
    let conn = hft_net::ConnId(112);
    let builds = gateway.builds;
    let mut live = Vec::new();
    for _ in 0..3 {
        let d = place_tif(
            &mut h,
            S1,
            G1,
            A1,
            Side::Buy,
            q("0.001"),
            IntentClass::Open,
            Tif::Gtc,
        );
        assert_eq!(d.reject, 0);
        h.ack(&d);
        live.push(d);
    }
    let first = h.io_ref().out.len();
    for event in ["evt_ignorable", "evt_new_type"] {
        let frame = format!("{{\"subscriptionId\":1,\"event\":{{\"e\":\"{event}\"}}}}");
        for _ in 0..10_000 {
            gateway.net.push(conn, frame.as_bytes());
            gateway.poll(h.now());
            let reports = gateway.deliver(&mut h);
            assert!(!reports.iter().any(|m| matches!(m, RepMsg::ConnState(_))));
        }
    }
    let count = gateway.runtime.counters();
    assert_eq!(count.user_evt_ignored, 10_000);
    assert_eq!(count.user_evt_unknown, 10_000);
    let notes: Vec<_> = h
        .recs()
        .iter()
        .filter(|r| r.kind == hft_event::EventKind::NOTE)
        .filter_map(|r| r.read::<hft_event::Note>())
        .filter(|n| n.code == hft_venue::codes::UNKNOWN_USER_EVENT)
        .collect();
    assert_eq!(notes.len(), 11);
    for note in notes {
        assert_eq!(note.source, hft_event::NoteSource::Venue);
        let bytes: Vec<_> = note.v[2..6].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(&bytes[..12], b"evt_new_type");
    }
    assert_eq!(gateway.builds, builds);
    assert!(!h.io_ref().out[first..].iter().any(|(_, c)| c.op == GwOp::Query));
    for d in &live {
        let s = h.s.engine().oms.raw_slot(A, AC1, d.slot.0).unwrap().0;
        assert_eq!(s.state, hft_oms::OrderState::Live);
        assert_eq!(s.sflags.0 & hft_oms::SlotFlags::VERIFY, 0);
    }
    gateway
        .net
        .push(conn, br#"{"subscriptionId":1,"event":{"e":"executionReport","#);
    gateway.poll(h.now());
    let reports = gateway.deliver(&mut h);
    assert!(reports.iter().any(|m| matches!(m, RepMsg::ConnState(c) if c.role == ConnRole::Order && c.state == ConnStateKind::Degraded && c.reason == hft_venue::codes::REPORT_DECODE)));
    for d in &live {
        let s = h.s.engine().oms.raw_slot(A, AC1, d.slot.0).unwrap().0;
        assert_ne!(s.sflags.0 & hft_oms::SlotFlags::VERIFY, 0);
        assert_eq!(
            h.io_ref().out[first..]
                .iter()
                .filter(|(_, c)| c.op == GwOp::Query && c.target_slot == d.slot)
                .count(),
            1
        );
    }
}
