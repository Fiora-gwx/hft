#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{NoteLevel, OrderEventKind};
use hft_oms::notes;
use hft_oms::*;
use hft_strategy_sdk::OrderRef;
use hft_types::{Qty, Side, StrategyId, Tif};

fn sim_it(tif: Tif) -> ApprovedIntent {
    intent(VS, S1, X, Side::Buy, tif, p("60000.00"), q("0.01"), g3())
}

#[test]
fn oms_285_sim_reset_preconditions() {
    for (row, tif, st, ok) in [
        ("OMS-285.1", Tif::Gtc, OrderState::Unknown, false),
        ("OMS-285.2", Tif::Ioc, OrderState::Live, false),
        ("OMS-285.3", Tif::Fok, OrderState::PendingNew, false),
        ("OMS-285.4", Tif::Gtc, OrderState::Live, true),
    ] {
        let mut h = H::new();
        let s = h.place_ok(&sim_it(tif), 0);
        h.force(s, st, true, OrderState::Live);
        let r = h.o.sim_reset(S1);
        if ok {
            assert!(r.is_ok(), "{row}");
        } else {
            assert_eq!(r, Err(OpError::NotIdle), "{row}");
            assert_eq!(h.state(s), st, "{row}");
        }
    }
}

/// s1@VS: two old tombs, q (GTC Live, gen 3), smoke k (Live), g3 leg 0 net 0.02 / residual 0.0001.
fn setup() -> (H, OrderRef, OrderRef) {
    let mut h = H::with(|c| c.slots.set(VS, S1, 3));
    let mut t = 0;
    for _ in 0..2 {
        let r = h.place_ok(&sim_it(Tif::Gtc), t);
        h.what(r, OrderEventKind::CancelAck, t);
        h.sweep(t + 5001);
        t += 5002;
    }
    let qs = loop {
        let r = h.place_ok(&sim_it(Tif::Gtc), t);
        if r.r#gen == 3 {
            break r;
        }
        h.what(r, OrderEventKind::CancelAck, t);
        h.sweep(t + 5001);
        t += 5002;
    };
    h.ack(qs, 1, t);
    let mut sm = sim_it(Tif::PostOnly);
    sm.intent.group = hft_types::GROUP_NONE;
    sm.origin = Origin::Command;
    sm.smoke_ttl_ms = 60_000;
    sm.strategy = StrategyId(3);
    let k = h.place_ok(&sm, t);
    h.ack(k, 2, t);
    h.o.test_set_net(S1, g3(), X, q("0.02"));
    h.o.test_set_residual(S1, g3(), X, q("0.0001"));
    (h, qs, k)
}

#[test]
fn oms_286_sim_reset() {
    let (mut h, qs, k) = setup();
    let tombs_before = (1..=3u32)
        .flat_map(|s| (1..=5u32).map(move |g| (s, g)))
        .filter(|(s, g)| h.o.tomb(VS, S1, *s, *g).is_some())
        .count();
    assert!(tombs_before >= 2);
    h.o.sim_reset(S1).unwrap();
    for r in [qs, k] {
        assert_eq!(h.state(r), OrderState::Free);
        assert!(h.o.tomb(VS, S1, r.slot, r.r#gen).is_some());
    }
    let tombs_after = (1..=3u32)
        .flat_map(|s| (1..=5u32).map(move |g| (s, g)))
        .filter(|(s, g)| h.o.tomb(VS, S1, *s, *g).is_some())
        .count();
    assert_eq!(tombs_after, 2);
    assert_eq!(h.o.table_view(VS, S1).unwrap().smoke_slot, 0);
    let st = h.legs(S1, g3());
    assert_eq!((st.view.leg[0].net, st.view.leg[0].residual), (Qty(0), Qty(0)));
    let ib = h.o.inflight();
    assert_eq!(
        (ib.open_buy(S1, X), ib.imm_buy(S1, X), ib.buy_notional(S1, VS).0),
        (Qty(0), Qty(0), 0)
    );
    let mut e = h.oe(qs, OrderEventKind::CancelAck);
    e.r#gen = qs.r#gen;
    h.ev(&e, 1);
    assert!(h.out.is_empty());
}

#[test]
fn oms_287_fill_after_sim_reset() {
    let (mut h, qs, _k) = setup();
    h.o.sim_reset(S1).unwrap();
    let mut e = h.oe(qs, OrderEventKind::PartialFill);
    e.r#gen = qs.r#gen;
    e.cum_filled = q("0.003");
    e.avg_px = p("60000.00");
    h.ev(&e, 1);
    let f = h.out.fills[0];
    assert!(f.flags & fill_flags::LATE != 0);
    assert_eq!(f.qty, q("0.003"));
    assert_eq!(h.notes(notes::LATE_FILL)[0].level, NoteLevel::P0);
}
