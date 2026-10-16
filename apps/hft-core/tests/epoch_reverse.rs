//! Fresh boot epoch through the real gateway, protocol decoder, sequencer, OMS and exec.
#[path = "../../../crates/hft-sequencer/tests/common/mod.rs"]
mod common;
mod net_support;
use common::{risk_kit::*, *};
use hft_event::{GwOp, OrderEventKind, RepMsg};
use hft_net::ConnId;
use hft_sequencer::OutRing;
use hft_strategy_sdk::{EventRef, HedgeKind, LegLinkOpt, LinkRole, OnFail, PxMode};
use hft_types::{Qty, Tsc};

#[test]
fn core_303_fresh_boot_gateway_fill_triggers_simultaneous_reverse_and_replays() {
    reset_script();
    let mut h = b0().boot_self_contained();
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
    // Forward the assembly's real boot output; do not synthesize SetEpoch in this test.
    for (ring, cmd) in &h.io_ref().out {
        if *ring == OutRing::Venue(A) {
            gateway.command(*cmd);
        }
    }
    for _ in 0..8 {
        gateway.poll(h.now());
    }
    gateway.deliver(&mut h);
    let before = h.io_ref().out.len();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50003")
        {
            let mut it = pair_first(hx, q("0.01"));
            let mut link = *it.link.get().unwrap();
            link.hedge.kind = HedgeKind::Simultaneous;
            link.hedge.on_fail = OnFail::Reverse;
            link.hedge.px_mode = PxMode::Limit;
            link.hedge.limit_px = px("50010");
            it.link = LegLinkOpt::some(link);
            let _ = ctx.place(it);
        }
    });
    let mark = h.mark();
    h.quote(A1, "50000", "50003");
    h.run();
    let pair = decs_since(&h, mark);
    assert_eq!(pair.len(), 2);
    assert!(pair.iter().all(|d| d.reject == 0));
    let first = pair.iter().find(|d| d.inst == A1).unwrap();
    let second = pair.iter().find(|d| d.inst == B1).unwrap();
    for (ring, cmd) in &h.io_ref().out[before..] {
        if *ring == OutRing::Venue(A) {
            gateway.command(*cmd);
        }
    }
    gateway.poll(h.now());
    gateway.deliver(&mut h);
    let frames = gateway.net.sent_text(&mut gateway.runtime, ConnId(112));
    let wire = frames.iter().find(|f| f.contains("order.place")).unwrap();
    let cid = wire
        .split("\"newClientOrderId\":\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    // A private fill carries the client id actually encoded with the gateway's boot epoch.
    let fill = format!(
        r#"{{"e":"executionReport","E":1790000000000,"s":"BTCUSDT","c":"{cid}","S":"BUY","o":"LIMIT","f":"IOC","q":"0.01","p":"50003","x":"TRADE","X":"FILLED","i":1234,"l":"0.01","z":"0.01","L":"50003","n":"0","N":"USDT","T":1790000000000,"t":1,"Z":"500.03"}}"#
    );
    let fill = format!(r#"{{"subscriptionId":1,"event":{fill}}}"#);
    gateway.net.push(ConnId(112), fill.as_bytes());
    gateway.poll(h.now());
    let reports = gateway.deliver(&mut h);
    assert!(
        reports.iter().any(|r| matches!(r, RepMsg::OrderEvent(e, _)
        if e.what == OrderEventKind::Fill && e.epoch == h.epoch() && e.slot == first.slot)),
        "cid={cid}; reports={reports:?}"
    );
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, q("0.01"));
    expire_order(&mut h, second);
    let reverse: Vec<_> = decs_since(&h, mark)
        .into_iter()
        .filter(|d| d.link_role == LinkRole::Reverse as u8)
        .collect();
    assert_eq!(reverse.len(), 1, "{reverse:?}");
    let reverse = reverse[0];
    assert_eq!((reverse.inst, reverse.qty, reverse.reject), (A1, q("0.01"), 0));
    assert_eq!(reverse.side, hft_types::Side::Sell);
    assert!(
        h.io_ref()
            .out
            .iter()
            .any(|(_, c)| c.op == GwOp::Place && c.slot == reverse.slot)
    );
    h.fill(&reverse, reverse.qty, reverse.qty, reverse.px);
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, Qty(0));
    assert_eq!(h.s.engine().ledger.pos(AC1, B1).qty, Qty(0));
    let mismatches = h.verify();
    clear_hook();
    assert!(mismatches.is_empty(), "{mismatches:?}");
}
