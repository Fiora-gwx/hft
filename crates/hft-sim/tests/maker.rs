//! SIM-040–SIM-041: TradeThrough fills of resting orders.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

mod common;

use common::*;
use hft_types::Tif;

#[test]
fn sim_040_trade_through() {
    type Row = (&'static str, fn(&mut Sim), Option<(&'static str, &'static str)>);
    let rows: [Row; 4] = [
        ("SIM-040.1", |s| s.trade(ms(200), BTC_S1, "59000", "1"), None),
        (
            "SIM-040.2",
            |s| s.trade(ms(200), BTC_S1, "58990", "0.04"),
            Some(("0.04", "0.472")),
        ),
        (
            "SIM-040.3",
            |s| s.bbo(ms(200), BTC_S1, ("58990", "1"), ("58999.99", "1")),
            Some(("0.1", "1.18")),
        ),
        (
            "SIM-040.4",
            |s| s.bbo(ms(200), BTC_S1, ("58990", "1"), ("59000", "1")),
            None,
        ),
    ];
    for (row, md, want) in rows {
        let mut s = Sim::std();
        let o = s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
        s.run_to(ms(110));
        md(&mut s);
        s.run_to(ms(210));
        let fills = s.fills_of(A9, o);
        match want {
            None => assert!(fills.is_empty(), "{row}: {fills:?}"),
            Some((qty, fee)) => {
                assert_eq!(fills, vec![(px("59000"), q(qty))], "{row}");
                let e = s.events_of(A9, o).last().copied();
                assert_eq!(e.map(|e| (e.fee, e.fee_asset)), Some((usdt(fee), USDT)), "{row}");
            }
        }
    }
}

#[test]
fn sim_041_trade_through_sell() {
    let mut setup = setup_sim();
    let mut b = setup.accounts[0].1.initial_balances.to_vec();
    b.push(bal(S1, BTC, q("0.1")));
    setup.accounts[0].1.initial_balances = b.into();
    let mut s = Sim::with(&setup, &rd_sim());
    let o = s.place(A9, BTC_S1, Sell, "0.1", "61000", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    s.trade(ms(200), BTC_S1, "61000.01", "0.2");
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("61000"), q("0.1"))]);
}
