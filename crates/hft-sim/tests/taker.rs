//! SIM-030–SIM-036: taker matching, IOC/FOK, take_avail_bp, consumed-liquidity memory, GTC rest.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod common;

use common::*;
use hft_event::OrderEventKind;
use hft_sim::SimSetup;
use hft_types::{Px, Qty, Tif};

fn book030(s: &mut Sim, t: u64) {
    s.topn(
        t,
        BTC_S1,
        &[("60000", "0.1"), ("60010", "0.2"), ("60020", "0.3")],
        &[],
    );
}

#[test]
fn sim_030_walk_levels_to_limit() {
    let mut s = Sim::std();
    book030(&mut s, ms(100));
    let o = s.place(A9, BTC_S1, Buy, "0.25", "60015", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    let ev = s.events_of(A9, o);
    assert_eq!(ev.len(), 3);
    assert_eq!(ev[0].what, OrderEventKind::Ack);
    assert_eq!(ev[1].what, OrderEventKind::PartialFill);
    assert_eq!(
        (ev[1].cum_filled, ev[1].avg_px, ev[1].fee, ev[1].fee_asset),
        (q("0.1"), px("60000"), usdt("2.4"), USDT)
    );
    assert_eq!(ev[2].what, OrderEventKind::Fill);
    assert_eq!(
        (ev[2].cum_filled, ev[2].avg_px, ev[2].fee),
        (q("0.25"), px("60006"), usdt("3.6006"))
    );
}

#[test]
fn sim_031_ioc_remainder_expired() {
    let mut s = Sim::std();
    book030(&mut s, ms(100));
    let o = s.place(A9, BTC_S1, Buy, "0.7", "60015", Tif::Ioc, ms(100));
    s.run_to(ms(110));
    assert_eq!(
        s.fills_of(A9, o),
        vec![(px("60000"), q("0.1")), (px("60010"), q("0.2"))]
    );
    let last = s.events_of(A9, o).last().copied();
    assert_eq!(
        last.map(|e| (e.what, e.cum_filled)),
        Some((OrderEventKind::Expired, q("0.3")))
    );
}

#[test]
fn sim_032_fok() {
    // SIM-032.1
    let mut s = Sim::std();
    book030(&mut s, ms(100));
    let before = s.total(A9, S1, USDT);
    let o = s.place(A9, BTC_S1, Buy, "0.7", "61000", Tif::Fok, ms(100));
    s.run_to(ms(110));
    assert_eq!(
        s.whats(A9, o),
        vec![OrderEventKind::Ack, OrderEventKind::Expired],
        "SIM-032.1"
    );
    assert_eq!(
        s.events_of(A9, o).last().map(|e| e.cum_filled),
        Some(Qty(0)),
        "SIM-032.1"
    );
    assert_eq!(s.total(A9, S1, USDT), before, "SIM-032.1");
    // SIM-032.2
    let mut s = Sim::std();
    book030(&mut s, ms(100));
    let o = s.place(A9, BTC_S1, Buy, "0.6", "61000", Tif::Fok, ms(100));
    s.run_to(ms(110));
    assert_eq!(s.fills_of(A9, o).len(), 3, "SIM-032.2");
    let last = s.events_of(A9, o).last().copied();
    assert_eq!(
        last.map(|e| (e.what, e.cum_filled)),
        Some((OrderEventKind::Fill, q("0.6"))),
        "SIM-032.2"
    );
}

#[test]
fn sim_033_take_avail_bp() {
    let rows: [(&str, u16, Vec<(&str, &str)>); 3] = [
        ("SIM-033.1", 5_000, vec![("60000", "0.05"), ("60010", "0.1")]),
        ("SIM-033.2", 10_000, vec![("60000", "0.1"), ("60010", "0.15")]),
        (
            "SIM-033.3",
            3_333,
            vec![("60000", "0.03333"), ("60010", "0.06666")],
        ),
    ];
    for (row, bp, want) in rows {
        let mut setup = setup_sim();
        setup.tuning.take_avail_bp = bp;
        let mut s = Sim::with(&setup, &rd_sim());
        book030(&mut s, ms(100));
        let o = s.place(A9, BTC_S1, Buy, "0.25", "60015", Tif::Ioc, ms(100));
        s.run_to(ms(110));
        let want: Vec<(Px, Qty)> = want.iter().map(|(p, x)| (px(p), q(x))).collect();
        assert_eq!(s.fills_of(A9, o), want, "{row}");
        if row == "SIM-033.1" {
            let last = s.events_of(A9, o).last().copied();
            assert_eq!(
                last.map(|e| (e.what, e.qty)),
                Some((OrderEventKind::Expired, q("0.1"))),
                "{row}"
            );
        }
    }
}

fn setup_decay(decay: u32) -> SimSetup {
    let mut setup = setup_sim();
    setup.tuning.consume_decay_ms = decay;
    setup
}

/// First order eats 0.1 at 102 ms; later orders arrive at `102 + d` ms; returns their fills.
fn run_034(later: &[u64]) -> Vec<Qty> {
    let mut s = Sim::with(&setup_decay(1_000), &rd_sim());
    s.topn(ms(100), BTC_S1, &[("60000", "0.1")], &[]);
    s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    let mut slots = Vec::new();
    for &d in later {
        let arrive = ms(102 + d);
        s.topn(arrive, BTC_S1, &[("60000", "0.1")], &[]);
        slots.push(s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, arrive - ms(2)));
    }
    s.run_to(ms(5_000));
    slots
        .iter()
        .map(|o| Qty(s.fills_of(A9, *o).iter().map(|f| f.1.0).sum()))
        .collect()
}

#[test]
fn sim_034_memory_decay() {
    assert_eq!(run_034(&[0]), vec![q("0")], "SIM-034.1");
    assert_eq!(run_034(&[500]), vec![q("0.05")], "SIM-034.2");
    assert_eq!(run_034(&[999]), vec![q("0.0999")], "SIM-034.3");
    assert_eq!(run_034(&[1_000]), vec![q("0.1")], "SIM-034.4");
    assert_eq!(run_034(&[500, 600]), vec![q("0.05"), q("0.015")], "SIM-034.5");
}

#[test]
fn sim_035_memory_ring_overwrite() {
    let mut s = Sim::with(&setup_decay(60_000), &rd_sim());
    let prices: Vec<String> = (0..65).map(|k| format!("60000.{k:02}")).collect();
    let levels: Vec<(&str, &str)> = prices.iter().map(|p| (p.as_str(), "0.001")).collect();
    s.topn(ms(100), BTC_S1, &levels, &[]);
    let mut t = ms(100);
    for p in &prices {
        s.place(A9, BTC_S1, Buy, "0.001", p, Tif::Ioc, t);
        t += ms(1);
    }
    s.run_to(t + ms(10));
    let v = s.view(A9);
    assert_eq!(v.memory_entries(BTC_S1, Buy), 64);
    assert_eq!(
        v.memory_at(BTC_S1, Buy, px(&prices[0])),
        Qty(0),
        "level 1 overwritten"
    );
    assert_eq!(
        v.memory_at(BTC_S1, Buy, px(&prices[1])),
        q("0.001"),
        "level 2 kept"
    );
    assert_eq!(
        v.memory_at(BTC_S1, Buy, px(&prices[64])),
        q("0.001"),
        "level 65 stored"
    );
    let o = s.place(A9, BTC_S1, Buy, "0.001", &prices[0], Tif::Ioc, t + ms(20));
    s.run_to(t + ms(40));
    assert_eq!(s.fills_of(A9, o), vec![(px(&prices[0]), q("0.001"))]);
    assert_eq!(s.view(A9).memory_entries(BTC_S1, Buy), 64);
}

#[test]
fn sim_036_gtc_rests_remainder() {
    let mut s = Sim::std();
    book030(&mut s, ms(100));
    let o = s.place(A9, BTC_S1, Buy, "0.5", "60010", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    assert_eq!(
        s.fills_of(A9, o),
        vec![(px("60000"), q("0.1")), (px("60010"), q("0.2"))]
    );
    assert!(!s.whats(A9, o).contains(&OrderEventKind::Expired));
    let ov = s
        .view(A9)
        .order(o)
        .map(|x| (x.resting, x.status, x.qty.0 - x.filled.0));
    assert_eq!(ov, Some((true, 2, q("0.2").0)));
    assert_eq!(s.view(A9).balance(S1, USDT).locked, usdt("12006.8008"));
}
