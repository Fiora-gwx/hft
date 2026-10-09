#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::OrderEventKind;
use hft_oms::*;
use hft_types::{Tif, Tsc};

#[test]
fn oms_305_quiescent_bits() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 0);
    assert_eq!(h.o.quiescent(A1), 0, "OMS-305.1");
    let mut h = H::new();
    let (f, _l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    assert_eq!(h.o.quiescent(A1), quiesce::LINKS_ACTIVE, "OMS-305.2");
    let mut h = H::new();
    let mut it = buy_x();
    it.intent.tif = Tif::Ioc;
    let s = h.place_ok(&it, 0);
    h.ack(s, 1, 0);
    assert_eq!(h.o.quiescent(A1), quiesce::IOC_NONTERMINAL, "OMS-305.3");
    let mut h = H::new();
    it.intent.tif = Tif::Fok;
    h.place_ok(&it, 0);
    assert_eq!(h.o.quiescent(A1), quiesce::IOC_NONTERMINAL, "OMS-305.4");
    let mut h = H::new();
    let (f, l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    h.fill(f, q("0.012"), p("60000.00"), 0);
    h.out.clear();
    h.o.hedge_gated(
        l,
        HedgeGate::Px(HedgePxErr::StaleQuote),
        Tsc(0),
        ms(1),
        1,
        &mut h.out,
    );
    assert_eq!(
        h.o.quiescent(A1),
        quiesce::LINKS_ACTIVE | quiesce::HEDGE_RETRY_PENDING,
        "OMS-305.5"
    );
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.what(s, OrderEventKind::OutcomeUnknown, 0);
    assert_eq!(h.o.quiescent(A1), quiesce::UNKNOWN, "OMS-305.6");
    let mut h = H::new();
    let s = h.place_ok(
        &intent(
            VS,
            A2,
            X,
            hft_types::Side::Buy,
            Tif::Gtc,
            p("60000.00"),
            q("0.01"),
            hft_types::GROUP_NONE,
        ),
        0,
    );
    h.what(s, OrderEventKind::OutcomeUnknown, 0);
    assert_eq!(h.o.quiescent(A1), 0, "OMS-305.7");
}
