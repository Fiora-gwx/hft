//! SIM-100–SIM-104: cancel, amend, cancel-all, query, order-table capacity.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_event::{OrderEvent, OrderEventKind};
use hft_types::{InstId, SlotId, Tif};

/// SIM-080 step 1 (slot 1 resting buy 0.1 @ 59 000) plus slot 2: an IOC filled 0.1 @ 60 000.
fn base() -> Sim {
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
    s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    s.out.clear();
    s
}

fn evs(s: &Sim) -> Vec<OrderEvent> {
    s.order_events(A9).into_iter().map(|(_, e)| e).collect()
}

#[test]
fn sim_100_cancel() {
    let mut s = base();
    s.cancel(A9, BTC_S1, SlotId(1), ms(200));
    s.run_to(ms(210));
    assert_eq!(
        evs(&s).iter().map(|e| e.what).collect::<Vec<_>>(),
        vec![OrderEventKind::CancelAck],
        "SIM-100.1"
    );
    assert_eq!(s.view(A9).balance(S1, USDT).locked, usdt("0"), "SIM-100.1");

    let mut s = base();
    let before = s.view(A9).balance(S1, USDT);
    s.cancel(A9, BTC_S1, SlotId(2), ms(200));
    s.run_to(ms(210));
    assert_eq!(
        evs(&s).iter().map(|e| e.what).collect::<Vec<_>>(),
        vec![OrderEventKind::CancelReject],
        "SIM-100.2"
    );
    assert_eq!(s.view(A9).balance(S1, USDT), before, "SIM-100.2");

    let mut s = base();
    s.cancel(A9, BTC_S1, SlotId(99), ms(200));
    s.run_to(ms(210));
    assert_eq!(
        evs(&s).iter().map(|e| e.what).collect::<Vec<_>>(),
        vec![OrderEventKind::CancelReject],
        "SIM-100.3"
    );
}

#[test]
fn sim_101_amend() {
    let mut s = base();
    s.amend(A9, BTC_S1, SlotId(1), "59500", "0.1", ms(200));
    s.run_to(ms(210));
    assert_eq!(
        evs(&s).last().map(|e| e.what),
        Some(OrderEventKind::AmendAck),
        "SIM-101.1"
    );
    assert_eq!(s.view(A9).balance(S1, USDT).locked, usdt("5952.38"), "SIM-101.1");

    let mut s = base();
    s.amend(A9, BTC_S1, SlotId(1), "59500.005", "0.1", ms(200));
    s.run_to(ms(210));
    assert_eq!(
        evs(&s).last().map(|e| e.what),
        Some(OrderEventKind::AmendReject),
        "SIM-101.2"
    );
    assert_eq!(
        s.view(A9).order(SlotId(1)).map(|o| o.px),
        Some(px("59000")),
        "SIM-101.2"
    );
    assert_eq!(s.view(A9).balance(S1, USDT).locked, usdt("5902.36"), "SIM-101.2");

    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
    s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::PostOnly, ms(100));
    s.run_to(ms(110));
    s.out.clear();
    s.amend(A9, BTC_S1, SlotId(1), "60000", "0.1", ms(200));
    s.run_to(ms(210));
    assert_eq!(
        evs(&s).last().map(|e| e.what),
        Some(OrderEventKind::AmendReject),
        "SIM-101.3"
    );

    let mut s = base();
    s.amend(A9, BTC_S1, SlotId(2), "59500", "0.1", ms(200));
    s.run_to(ms(210));
    assert_eq!(
        evs(&s).last().map(|e| e.what),
        Some(OrderEventKind::AmendReject),
        "SIM-101.4"
    );
}

fn three_resting() -> Sim {
    let mut s = Sim::std();
    s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
    s.place(A9, BTC_S1, Buy, "0.1", "58000", Tif::Gtc, ms(100));
    s.place(A9, BNB_S1, Buy, "0.1", "500", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    assert_eq!(s.view(A9).resting_count(), 3);
    s.out.clear();
    s
}

fn acks(s: &Sim) -> usize {
    evs(s)
        .iter()
        .filter(|e| e.what == OrderEventKind::CancelAck)
        .count()
}

#[test]
fn sim_102_cancel_all() {
    let mut s = three_resting();
    s.cancel_all(A9, BTC_S1, ms(200));
    s.run_to(ms(210));
    assert_eq!(acks(&s), 2, "SIM-102.1");
    assert_eq!(
        s.view(A9).order(SlotId(3)).map(|o| o.resting),
        Some(true),
        "SIM-102.1"
    );

    let mut s = three_resting();
    s.cancel_all(A9, InstId(0), ms(200));
    s.run_to(ms(210));
    assert_eq!(acks(&s), 3, "SIM-102.2");

    let mut s = Sim::std();
    s.cancel_all(A9, InstId(0), ms(200));
    s.run_to(ms(210));
    assert!(evs(&s).is_empty(), "SIM-102.3");
}

#[test]
fn sim_103_query() {
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "0.13")], &[]);
    s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100)); // slot 1
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100)); // slot 2
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100)); // slot 3
    s.place(A9, BTC_S1, Buy, "0.000015", "60000", Tif::Ioc, ms(100)); // slot 4 (lot reject)
    s.place(A9, BTC_S1, Buy, "0.1", "57000", Tif::Gtc, ms(100)); // slot 5
    s.run_to(ms(110));
    s.trade(ms(150), BTC_S1, "58990", "0.04");
    s.run_to(ms(160));
    let rows: [(&str, u32, u8, &str, &str); 6] = [
        ("SIM-103.1", 1, 2, "0.04", "59000"),
        ("SIM-103.2", 2, 3, "0.1", "60000"),
        ("SIM-103.3", 3, 6, "0.03", "60000"),
        ("SIM-103.4", 4, 5, "0", "0"),
        ("SIM-103.5", 99, 7, "0", "0"),
        ("SIM-103.6", 5, 1, "0", "0"),
    ];
    for (row, slot, status, cum, avg) in rows {
        s.out.clear();
        s.query(A9, BTC_S1, SlotId(slot), ms(200));
        s.run_to(ms(300));
        let e = evs(&s);
        assert_eq!(e.len(), 1, "{row}");
        assert_eq!(
            (e[0].what, e[0].detail, e[0].cum_filled, e[0].avg_px),
            (OrderEventKind::QueryResult, status, q(cum), px(avg)),
            "{row}"
        );
    }
}

#[test]
fn sim_104_order_table_capacity() {
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "1000")], &[]);
    for k in 0..1_025u64 {
        s.place(A9, BTC_S1, Buy, "0.001", "60000", Tif::Ioc, ms(100) + k * ms(10));
        s.run_to(ms(100) + k * ms(10) + ms(5));
    }
    s.out.clear();
    s.query(A9, BTC_S1, SlotId(1), ms(20_000));
    s.query(A9, BTC_S1, SlotId(2), ms(20_000));
    s.run_to(ms(20_010));
    let e = evs(&s);
    assert_eq!(e.iter().map(|e| e.detail).collect::<Vec<_>>(), vec![7, 3]);
    assert_eq!(s.view(A9).order_count(), 1_024);
}
