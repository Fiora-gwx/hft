//! SIM-020–SIM-023: book replica and matching view.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod common;

use common::*;
use hft_event::OrderEventKind;
use hft_types::Tif;

#[test]
fn sim_020_topn_full_replace() {
    let mut s = Sim::std();
    s.topn(
        ms(100),
        BTC_S1,
        &[("60000", "0.1"), ("60010", "0.1"), ("60020", "0.1")],
        &[],
    );
    let o1 = s.place(A9, BTC_S1, Buy, "0.5", "61000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    assert_eq!(
        s.fills_of(A9, o1),
        vec![
            (px("60000"), q("0.1")),
            (px("60010"), q("0.1")),
            (px("60020"), q("0.1"))
        ]
    );
    let last = s.events_of(A9, o1).last().copied();
    assert_eq!(
        last.map(|e| (e.what, e.qty, e.cum_filled)),
        Some((OrderEventKind::Expired, q("0.2"), q("0.3")))
    );
    s.topn(ms(200), BTC_S1, &[("60100", "1")], &[]);
    let o2 = s.place(A9, BTC_S1, Buy, "0.1", "60050", Tif::Ioc, ms(200));
    s.run_to(ms(210));
    assert!(s.fills_of(A9, o2).is_empty());
    let last = s.events_of(A9, o2).last().copied();
    assert_eq!(
        last.map(|e| (e.what, e.qty)),
        Some((OrderEventKind::Expired, q("0.1")))
    );
}

#[test]
fn sim_021_bbo_only_one_level() {
    let mut s = Sim::std();
    s.bbo(ms(100), BTC_S1, ("59990", "1"), ("60000", "0.05"));
    let o = s.place(A9, BTC_S1, Buy, "0.1", "61000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    assert_eq!(s.fills_of(A9, o), vec![(px("60000"), q("0.05"))]);
    let last = s.events_of(A9, o).last().copied();
    assert_eq!(
        last.map(|e| (e.what, e.qty)),
        Some((OrderEventKind::Expired, q("0.05")))
    );
    assert_eq!(s.view(A9).counters().top_only_fills, 1);
}

#[test]
fn sim_022_top_from_newer_source() {
    let rows = [
        ("SIM-022.1", ms(101) as i64, ["60005", "60010", "60020"]),
        ("SIM-022.2", ms(99) as i64, ["60000", "60010", "60020"]),
    ];
    for (row, bbo_ex, want) in rows {
        let mut s = Sim::std();
        s.topn_ex(
            ms(100),
            ms(100) as i64,
            BTC_S1,
            &[("60000", "0.1"), ("60010", "0.1"), ("60020", "0.1")],
            &[],
        );
        s.bbo_ex(ms(100) + 1, bbo_ex, BTC_S1, ("59990", "1"), ("60005", "0.1"));
        let o = s.place(A9, BTC_S1, Buy, "0.3", "61000", Tif::Ioc, ms(100) + 1);
        s.run_to(ms(110));
        let want: Vec<_> = want.iter().map(|p| (px(p), q("0.1"))).collect();
        assert_eq!(s.fills_of(A9, o), want, "{row}");
    }
}

#[test]
fn sim_023_book_not_live_rejects() {
    let mut s = Sim::std();
    s.exch_snapshot(ms(50), BTC_S1, 1, &[("60000", "1")], &[("59990", "1")]);
    // Gap: prev_id 5 ≠ last_id 1 → Stale.
    s.l2batch(ms(60), BTC_S1, 5, 6, &[("60010", "1")]);
    let o = s.place(A9, BTC_S1, Buy, "0.1", "61000", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    let ev = s.events_of(A9, o);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].what, OrderEventKind::Reject);
    assert_eq!(ev[0].reject_code, 0x1000_0001);
    assert_eq!(s.view(A9).counters().book_not_live_rejects, 1);
}
