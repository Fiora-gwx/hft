//! SIM-010–SIM-012: matching after arrival, total order, delivery order.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_event::OrderEventKind;
use hft_sim::LatencyModel;
use hft_types::{Px, Qty, SlotId, Tif, Tsc};

#[test]
fn sim_010_match_after_arrive() {
    let mut s = Sim::std();
    s.bbo(ms(100), BTC_S1, ("59990", "1"), ("60000", "1"));
    let slot = s.place(A9, BTC_S1, Buy, "0.1", "60030", Tif::Ioc, ms(100));
    s.bbo(ms(102), BTC_S1, ("59990", "1"), ("60010", "1"));
    s.bbo(ms(102) + 1, BTC_S1, ("59990", "1"), ("60020", "1"));
    s.drain(ms(104) - 1);
    assert!(s.order_events(A9).is_empty(), "first drain_due: no order replies");
    s.drain(ms(104));
    let ev = s.order_events(A9);
    assert_eq!(ev.len(), 2);
    assert_eq!(ev[0].1.what, OrderEventKind::Ack);
    assert_eq!(ev[1].1.what, OrderEventKind::Fill);
    assert_eq!(ev[1].1.slot, slot);
    assert_eq!(ev[1].1.px, px("60010"));
    assert_eq!(ev[1].1.cum_filled, q("0.1"));
    assert_eq!(ev[1].1.avg_px, px("60010"));
    assert_eq!(ev[1].1.ex_ts_ns, ms(102) as i64);
    assert!(ev.iter().all(|(d, _)| *d == ms(104)));
}

#[test]
fn sim_011_total_order() {
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "0.1")], &[]);
    let s1 = s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    let s2 = s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    assert_eq!(s.fills_of(A9, s1), vec![(px("60000"), q("0.1"))]);
    assert!(s.fills_of(A9, s2).is_empty());
    let last = s.events_of(A9, s2).last().map(|e| (e.what, e.cum_filled));
    assert_eq!(last, Some((OrderEventKind::Expired, Qty(0))));
}

#[test]
fn sim_012_drain_order() {
    let mut setup = setup_with_a8();
    setup.latency_models = vec![
        LatencyModel::Fixed {
            entry_us: 2_000,
            resp_us: 2_000,
        },
        LatencyModel::Fixed {
            entry_us: 1_000,
            resp_us: 1_000,
        },
    ];
    setup.accounts[1].1.latency_model = 1;
    let mut s = Sim::with(&setup, &rd_sim());
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.place(A8, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.advance(ms(110));
    // The flat account's startup margin report precedes the first order response.
    assert_eq!(s.ex.next_due(), Some(Tsc(ms(101))));
    s.drain(ms(110));
    assert!(s.out.windows(2).all(|w| w[0].due <= w[1].due));
    let accts: Vec<_> = s
        .out
        .iter()
        .filter(|r| matches!(r.msg, hft_event::RepMsg::OrderEvent(_, _)))
        .map(|r| (r.account, r.due))
        .collect();
    let n8 = accts.iter().filter(|(a, _)| *a == A8).count();
    assert!(n8 >= 2);
    assert!(
        accts[..n8].iter().all(|(a, d)| *a == A8 && *d == ms(102)),
        "{accts:?}"
    );
    assert!(
        accts[n8..].iter().all(|(a, d)| *a == A9 && *d == ms(104)),
        "{accts:?}"
    );
    assert_eq!(s.ex.next_due(), None);
    let _ = (Px(0), SlotId(0));
}
