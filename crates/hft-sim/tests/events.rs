//! SIM-077, SIM-110–SIM-113, SIM-200, SIM-220: produced events, Sim counters and external fills.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_event::{ConnStateKind, LeverageItem, OrderEventKind, RepMsg, SendStatus};
use hft_types::{Rate, SlotId, Tif, Tsc};
use zerocopy::FromBytes;

#[test]
fn sim_077_account_config_report() {
    let mut s = Sim::new(&setup_sim(), &rd_sim());
    s.drain(0);
    let acr: Vec<_> = s
        .out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::AccountConfigReport(h) => Some((h, r.bulk.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(acr.len(), 2);
    for (h, _) in &acr {
        assert_eq!((h.mismatches, h.uid_keys), (0, 0));
    }
    let (s1, _) = acr
        .iter()
        .find(|(h, _)| h.venue == S1)
        .cloned()
        .unwrap_or_else(|| panic!("no S1"));
    assert_eq!(s1.n_lev, 0);
    let (p2, bulk) = acr
        .iter()
        .find(|(h, _)| h.venue == P2)
        .cloned()
        .unwrap_or_else(|| panic!("no P2"));
    assert_eq!(p2.n_lev, 2);
    let items: Vec<_> = bulk
        .chunks_exact(16)
        .filter_map(|c| LeverageItem::read_from_bytes(c).ok())
        .map(|i| (i.inst, i.leverage))
        .collect();
    assert_eq!(
        items,
        vec![(BTC_P2, Rate(5_000_000_000)), (ETH_P2, Rate(5_000_000_000))]
    );
}

#[test]
fn sim_110_order_event_sequence() {
    let mut s = Sim::std();
    s.topn_ex(
        ms(100),
        ms(99) as i64,
        BTC_S1,
        &[("60000", "0.1"), ("60010", "0.2"), ("60020", "0.3")],
        &[],
    );
    s.place(A9, BTC_S1, Buy, "0.25", "60015", Tif::Ioc, ms(100));
    s.run_to(ms(200));
    let ev: Vec<_> = s.order_events(A9).into_iter().map(|(_, e)| e).collect();
    assert_eq!(
        ev.iter().map(|e| e.what).collect::<Vec<_>>(),
        vec![
            OrderEventKind::Ack,
            OrderEventKind::PartialFill,
            OrderEventKind::Fill
        ]
    );
    assert!(ev.iter().all(|e| e.slot == SlotId(1) && e.r#gen == 1));
    assert!(ev[1..].iter().all(|e| e.ex_ts_ns == ms(99) as i64));
    assert!(ev.windows(2).all(|w| w[0].cum_filled <= w[1].cum_filled));
    assert_eq!(ev[2].cum_filled, q("0.25"));
}

#[test]
fn sim_111_truth_report_interval() {
    let mut s = Sim::std();
    s.seed(
        A9,
        &hft_sim::SimSeedState {
            balances: vec![
                (S1, USDT, usdt("100000")),
                (S1, BTC, q("0.1")),
                (P2, USDT, usdt("18000")),
            ],
            positions: vec![(BTC_P2, q("-1.0"), px("60000"))],
            orders: Vec::new(),
            funding: Vec::new(),
        },
    );
    let truth = |s: &Sim| {
        s.out
            .iter()
            .filter(|r| matches!(r.msg, RepMsg::BalanceReport(_) | RepMsg::PositionReport(_)))
            .count()
    };
    s.run_to(60_000_000_000 - 1);
    assert_eq!(truth(&s), 0, "SIM-111.1");
    s.run_to(60_000_000_000);
    let mut bals: Vec<_> = s
        .out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::BalanceReport(b) => Some((b.venue, b.asset, b.free)),
            _ => None,
        })
        .collect();
    bals.sort();
    let mut want = vec![
        (S1, USDT, usdt("100000")),
        (S1, BTC, q("0.1")),
        (P2, USDT, usdt("18000")),
    ];
    want.sort();
    assert_eq!(bals, want, "SIM-111.2");
    let pos: Vec<_> = s
        .out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::PositionReport(p) => Some((p.inst, p.qty, p.entry_px)),
            _ => None,
        })
        .collect();
    assert_eq!(pos, vec![(BTC_P2, q("-1.0"), px("60000"))], "SIM-111.2");
    let v = s.view(A9);
    assert_eq!(v.balance(S1, BTC).free, q("0.1"), "SIM-111.2");
    assert_eq!(v.position(BTC_P2).qty, q("-1.0"), "SIM-111.2");
}

fn conn_states(s: &Sim) -> Vec<(hft_types::VenueId, ConnStateKind, u8)> {
    s.out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::ConnState(c) => Some((c.venue, c.state, c.conn)),
            _ => None,
        })
        .collect()
}

#[test]
fn sim_112_conn_up_once() {
    let mut s = Sim::new(&setup_with_a8(), &rd_sim());
    s.drain(0);
    let cs = conn_states(&s);
    assert_eq!(cs.len(), 2);
    assert!(cs.iter().all(|c| c.1 == ConnStateKind::Up && c.2 == 0));
    s.out.clear();
    s.run_to(ms(600_000));
    assert!(conn_states(&s).is_empty());
}

#[test]
fn sim_113_order_conn_down() {
    let mut s = Sim::std();
    s.topn(
        ms(100),
        BTC_S1,
        &[("60000", "0.1"), ("60010", "0.2"), ("60020", "0.3")],
        &[],
    );
    s.ex.set_order_conn(A9, S1, false, Tsc(ms(200)));
    let o1 = s.place(A9, BTC_S1, Buy, "0.1", "60015", Tif::Ioc, ms(201));
    s.cancel(A9, BTC_S1, o1, ms(201));
    s.query(A9, BTC_S1, o1, ms(201));
    s.run_to(ms(250));
    assert_eq!(conn_states(&s), vec![(S1, ConnStateKind::Down, 0)]);
    let srs: Vec<_> = s
        .out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::SendReport(x) => Some(x.status),
            _ => None,
        })
        .collect();
    assert_eq!(srs, vec![SendStatus::NoConn; 3]);
    assert!(s.order_events(A9).is_empty());
    s.out.clear();
    s.ex.set_order_conn(A9, S1, true, Tsc(ms(300)));
    let o2 = s.place(A9, BTC_S1, Buy, "0.1", "60015", Tif::Ioc, ms(301));
    s.run_to(ms(350));
    assert_eq!(conn_states(&s), vec![(S1, ConnStateKind::Up, 0)]);
    assert_eq!(s.whats(A9, o2).first(), Some(&OrderEventKind::Ack));
    assert_eq!(s.fills_of(A9, o2), vec![(px("60000"), q("0.1"))]);
}

#[test]
fn sim_200_sim_counters() {
    // SIM-200.1: SIM-021.
    let mut s = Sim::std();
    s.bbo(ms(100), BTC_S1, ("59990", "1"), ("60000", "0.05"));
    s.place(A9, BTC_S1, Buy, "0.1", "61000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    assert_eq!(s.view(A9).counters().top_only_fills, 1, "SIM-200.1");
    // SIM-200.2: SIM-034.2.
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "0.1")], &[]);
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    let before = s.view(A9).counters().liquidity_memory_hits;
    s.topn(ms(602), BTC_S1, &[("60000", "0.1")], &[]);
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(600));
    s.run_to(ms(700));
    assert_eq!(
        s.view(A9).counters().liquidity_memory_hits,
        before + 1,
        "SIM-200.2"
    );
    // SIM-200.3: SIM-072.2.
    let mut s = Sim::std();
    s.seed(A9, &seed9_state("18000", &[(BTC_P2, "-1.0", "60000")]));
    s.mark(0, BTC_P2, "60000");
    s.mark(ms(1), BTC_P2, "77611.9403");
    s.run_to(ms(10));
    assert_eq!(s.view(A9).counters().liquidations, 1, "SIM-200.3");
}

#[test]
fn sim_220_external_fill() {
    let mut s = Sim::std();
    s.seed(A9, &seed9_state("18000", &[(BTC_P2, "-1.0", "60000")]));
    s.drain(ms(1));
    s.out.clear();
    assert_eq!(
        s.ex.external_fill(A9, BTC_P2, Buy, q("0.4"), px("59000")),
        Some(usdt("400"))
    );
    assert_eq!(s.view(A9).position(BTC_P2).qty, q("-0.6"));
    assert_eq!(s.view(A9).balance(P2, USDT).total, usdt("18400"));
    let (b0, u0) = (
        s.view(A9).balance(S1, BTC).total.0,
        s.view(A9).balance(S1, USDT).total.0,
    );
    assert_eq!(
        s.ex.external_fill(A9, BTC_S1, Buy, q("0.1"), px("60000")),
        Some(q("0"))
    );
    assert_eq!(s.view(A9).balance(S1, BTC).total.0 - b0, q("0.1").0);
    assert_eq!(s.view(A9).balance(S1, USDT).total.0 - u0, -usdt("6000").0);
    s.drain(ms(2));
    assert!(s.out.is_empty());
}

#[test]
fn sim_221_partial_close_rounds_toward_zero() {
    let mut s = Sim::std();
    s.seed(A9, &seed9_state("18000", &[]));
    assert_eq!(
        s.ex.external_fill(A9, BTC_P2, Sell, q("0.1"), px("60000")),
        Some(q("0"))
    );
    assert_eq!(
        s.ex.external_fill(A9, BTC_P2, Sell, q("0.2"), px("60000.0001")),
        Some(q("0"))
    );
    assert_eq!(
        s.ex.external_fill(A9, BTC_P2, Buy, q("0.1"), px("60000.0002")),
        Some(hft_types::Qty(-13))
    );
    assert_eq!(s.view(A9).position(BTC_P2).qty, q("-0.2"));
    // removed = 18 000 000 002 × 10^6 / 3 → 6 000 000 006 666 666 (toward zero); realised
    // −13 333 334 × 10^-12 USDT → −13 × 10^-6 (toward zero).
    assert_eq!(s.view(A9).balance(P2, USDT).total.0, usdt("18000").0 - 13);
}
