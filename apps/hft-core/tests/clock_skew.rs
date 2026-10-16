#[path = "../../../crates/hft-sequencer/tests/common/mod.rs"]
mod common;
mod net_support;
use common::{risk_kit::*, *};
use hft_event::{EventKind, GwOp, OrderEventKind, OrderFlags, RepMsg};
use hft_net::ConnId;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::IntentClass;
use hft_types::{RejectBits, Side, Tsc};

fn deliver(gateway: &mut net_support::Gateway, h: &mut H) -> Vec<RepMsg> {
    let mut out = Vec::new();
    while let Some(msg) = gateway.reports.try_pop() {
        hft_core::io::rep_parts(&msg, &[], |kind, parts| {
            h.push_bytes(RingId::Rep(0), kind, parts.concat());
        });
        out.push(msg);
    }
    h.run();
    out
}

#[test]
fn fp_8_clock_skew_reject_and_clockbad_blocks_open() {
    reset_script();
    let mut h = b0().boot_self_contained();
    h.steady();
    h.open_position(AC1, A1, Side::Buy, "0.01");
    let hedge = place_in(&mut h, S1, G1, B1, Side::Sell, q("0.01"), IntentClass::Hedge);
    assert_eq!(hedge.reject, 0);
    h.ack(&hedge);
    h.fill(&hedge, hedge.qty, hedge.qty, hedge.px);
    clear_hook();
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
    gateway.command(hft_event::GwCmd::set_epoch(h.epoch(), false, 0));
    for _ in 0..6 {
        gateway.poll(h.now());
    }
    deliver(&mut gateway, &mut h);
    let decision = place_in(&mut h, S1, G1, A1, Side::Buy, q("0.01"), IntentClass::Open);
    assert_eq!(decision.reject, 0);
    let cmd = h
        .io_ref()
        .out
        .iter()
        .rev()
        .find(|(_, c)| c.slot == decision.slot && c.r#gen == decision.r#gen && c.op == GwOp::Place)
        .unwrap()
        .1;
    gateway.command(cmd);
    gateway.poll(h.now());
    deliver(&mut gateway, &mut h);
    let frames = gateway.net.sent_text(&mut gateway.runtime, ConnId(112));
    assert!(
        frames.iter().any(|frame| frame.contains("\"recvWindow\":5000")),
        "{frames:?}"
    );
    let req = venue_binance::encode_req_id(GwOp::Place, decision.slot.0, decision.r#gen).unwrap();
    let req = std::str::from_utf8(&req).unwrap();
    let response = format!(
        "{{\"id\":\"{req}\",\"status\":400,\"error\":{{\"code\":-1021,\"msg\":\"outside recvWindow\"}}}}"
    );
    // The same request id arriving on a different connection must neither bind to
    // this slot nor consume the real outstanding request on the primary.
    gateway.net.push(ConnId(113), response.as_bytes());
    for _ in 0..8 {
        gateway.poll(h.now());
    }
    let unmatched = deliver(&mut gateway, &mut h);
    assert!(
        unmatched
            .iter()
            .any(|m| matches!(m, RepMsg::OrderEvent(e, _) if e.slot == hft_types::SlotId::UNRESOLVED))
    );
    assert_eq!(gateway.runtime.inflight(ConnId(112)), 1);
    assert_eq!(
        h.s.engine()
            .oms
            .raw_slot(A, AC1, decision.slot.0)
            .unwrap()
            .0
            .state,
        hft_oms::OrderState::PendingNew
    );
    gateway.net.push(ConnId(112), response.as_bytes());
    clear_log();
    let rejected = std::rc::Rc::new(std::cell::Cell::new(false));
    let observed = rejected.clone();
    set_hook(move |hx, event, _| {
        if hx.s.strategy == S1
            && let hft_strategy_sdk::EventRef::Order { u, .. } = event
            && let hft_strategy_sdk::OrderUpdateKind::Rejected { bits, venue_code } = u.kind
        {
            assert_eq!(bits.0, 0);
            assert_eq!(venue_code, 0xFFFF_FC03);
            observed.set(true);
        }
    });
    gateway.poll(h.now());
    let reports = deliver(&mut gateway, &mut h);
    clear_hook();
    assert!(rejected.get(), "owner must receive the exchange reject code");
    assert_eq!(
        h.s.engine()
            .oms
            .raw_slot(A, AC1, decision.slot.0)
            .unwrap()
            .0
            .state,
        hft_oms::OrderState::Rejected
    );
    assert!(reports.iter().any(|m| matches!(m, RepMsg::OrderEvent(e, _) if e.what == OrderEventKind::Reject && e.reject_code == 0xFFFF_FC03)));
    assert!(reports.iter().any(|m| matches!(m, RepMsg::ConnState(c) if c.reason == 9 && c.state == hft_event::ConnStateKind::Degraded)));
    assert!(log().iter().any(|x| x.s == S1 && x.kind == "Conn" && !x.model));
    assert!(h.s.engine().oms.venue_ready(A, AC1));
    gateway.net.push(ConnId(112), response.as_bytes());
    gateway.poll(h.now());
    let duplicate = deliver(&mut gateway, &mut h);
    assert!(
        duplicate
            .iter()
            .any(|m| matches!(m, RepMsg::OrderEvent(e, _) if e.slot == hft_types::SlotId::UNRESOLVED))
    );
    assert_eq!(
        h.s.engine()
            .oms
            .raw_slot(A, AC1, decision.slot.0)
            .unwrap()
            .0
            .state,
        hft_oms::OrderState::Rejected
    );
    let version = h.s.engine().risk.mode.ctl_state_version();
    h.sys(9); // ClockBad
    assert_eq!(h.s.engine().risk.mode.ctl_state_version(), version + 1);
    assert!(
        h.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .any(|c| c.source == hft_types::CapSource::CLOCK_BAD
                && c.cflags & hft_event::CapRecord::CONDITIONAL != 0)
    );
    let blocked = place_in(&mut h, S1, G1, A1, Side::Buy, q("0.01"), IntentClass::Open);
    assert_eq!(blocked.reject, RejectBits::CLOCK.0);
    let reduce = place_in(&mut h, S1, G1, A1, Side::Sell, q("0.005"), IntentClass::Reduce);
    assert_eq!(reduce.reject, 0);
    assert_ne!(reduce.oflags & OrderFlags::RISK_REDUCING.0, 0);
    expire_order(&mut h, &reduce);
    h.sys(8); // ClockOk
    assert!(
        !h.s.engine()
            .risk
            .mode
            .caps()
            .iter()
            .any(|c| c.source == hft_types::CapSource::CLOCK_BAD)
    );
    assert_eq!(
        place_in(&mut h, S1, G1, A1, Side::Buy, q("0.001"), IntentClass::Open).reject,
        0
    );
    assert!(
        h.recs()
            .iter()
            .filter(|r| r.kind == EventKind::ORDER_EVENT)
            .all(|r| r
                .read::<hft_event::OrderEvent>()
                .is_none_or(|e| e.what != OrderEventKind::OutcomeUnknown))
    );
}
