//! SIM-080–SIM-083: spot locks, partial-fill release, transfers, SimReset.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_event::{GwCmd, OrderEventKind, RepMsg};
use hft_types::{Tif, Tsc};

fn step1_080() -> (Sim, hft_types::SlotId) {
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
    let o = s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    (s, o)
}

#[test]
fn sim_080_resting_lock() {
    let (mut s, o) = step1_080();
    let b = s.view(A9).balance(S1, USDT);
    assert_eq!((b.locked, b.free), (usdt("5902.36"), usdt("94097.64")));
    s.cancel(A9, BTC_S1, o, ms(200));
    s.run_to(ms(210));
    assert!(s.whats(A9, o).contains(&OrderEventKind::CancelAck));
    let b = s.view(A9).balance(S1, USDT);
    assert_eq!((b.locked, b.free), (usdt("0"), usdt("100000")));
}

#[test]
fn sim_081_partial_fill_releases_lock() {
    let (mut s, o) = step1_080();
    s.trade(ms(200), BTC_S1, "58990", "0.04");
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("59000"), q("0.04"))]);
    let e = s.events_of(A9, o).last().copied();
    assert_eq!(e.map(|e| e.fee), Some(usdt("0.472")));
    let b = s.view(A9).balance(S1, USDT);
    assert_eq!(
        (b.total, b.locked, b.free),
        (usdt("97639.528"), usdt("3541.416"), usdt("94098.112"))
    );
    assert_eq!(s.total(A9, S1, BTC), q("0.04").0);
}

fn balance_reports(s: &Sim) -> Vec<(hft_types::VenueId, hft_types::AssetId, hft_types::Qty)> {
    s.out
        .iter()
        .filter_map(|r| match r.msg {
            RepMsg::BalanceReport(b) => Some((b.venue, b.asset, b.free)),
            _ => None,
        })
        .collect()
}

#[test]
fn sim_082_transfer_delay() {
    let mut s = Sim::std();
    s.clock(ms(1_000));
    s.ex.on_command(
        &GwCmd::sim_transfer(A9, S1, P2, USDT, usdt("1000")),
        Tsc(ms(1_000)),
    );
    s.run_to(ms(1_000));
    assert_eq!(
        (s.total(A9, S1, USDT), s.total(A9, P2, USDT)),
        (usdt("99000").0, usdt("18000").0),
        "SIM-082.1"
    );
    assert_eq!(balance_reports(&s), vec![(S1, USDT, usdt("99000"))], "SIM-082.1");
    s.out.clear();
    s.run_to(ms(301_000) - 1);
    assert_eq!(s.total(A9, P2, USDT), usdt("18000").0, "SIM-082.2");
    s.out.clear();
    s.run_to(ms(301_000));
    assert_eq!(s.total(A9, P2, USDT), usdt("19000").0, "SIM-082.3");
    let br: Vec<_> = balance_reports(&s).into_iter().filter(|b| b.0 == P2).collect();
    assert_eq!(br, vec![(P2, USDT, usdt("19000"))], "SIM-082.3");
}

#[test]
fn sim_083_sim_reset() {
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
    s.bbo(ms(100), BTC_P2, ("59999.9", "10"), ("60000", "10"));
    s.mark(ms(100), BTC_P2, "60000");
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    let r1 = s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
    let r2 = s.place(A9, BTC_S1, Buy, "0.1", "58000", Tif::Gtc, ms(100));
    s.place(A9, BTC_P2, Buy, "0.01", "60000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    assert!(s.view(A9).memory_entries(BTC_S1, Buy) > 0);
    assert_eq!(s.view(A9).position(BTC_P2).qty, q("0.01"));
    s.out.clear();
    s.clock(ms(200));
    s.ex.on_command(&GwCmd::sim_reset(A9), Tsc(ms(200)));
    s.run_to(ms(200));
    assert!(s.whats(A9, r1).contains(&OrderEventKind::CancelAck));
    assert!(s.whats(A9, r2).contains(&OrderEventKind::CancelAck));
    let v = s.view(A9);
    let bals: Vec<_> = v.balances().map(|(vv, a, b)| (vv, a, b.total)).collect();
    assert_eq!(
        bals,
        vec![
            (S1, USDT, usdt("100000")),
            (S1, BNB, q("1")),
            (P2, USDT, usdt("18000"))
        ]
    );
    assert_eq!(v.positions().count(), 0);
    assert_eq!(v.memory_entries(BTC_S1, Buy), 0);
    let mut br: Vec<_> = balance_reports(&s).iter().map(|b| (b.0, b.1)).collect();
    br.sort();
    let mut want = vec![(S1, USDT), (S1, BTC), (S1, BNB), (P2, USDT)];
    want.sort();
    assert_eq!(br, want);
    let pr = s
        .out
        .iter()
        .filter(|r| matches!(r.msg, RepMsg::PositionReport(p) if p.inst == BTC_P2))
        .count();
    assert_eq!(pr, 1);
    let acr = s
        .out
        .iter()
        .filter(|r| matches!(r.msg, RepMsg::AccountConfigReport(_)))
        .count();
    assert_eq!(acr, 2);
    s.out.clear();
    s.trade(ms(300), BTC_S1, "57000", "1");
    s.run_to(ms(310));
    assert!(s.fills_of(A9, r1).is_empty() && s.fills_of(A9, r2).is_empty());
}
