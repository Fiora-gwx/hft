//! SIM-040–SIM-041: TradeThrough fills of resting orders; SIM-228: Queue position.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

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

/// `Sim` with `maker_fill = Queue`; bid level 59000 shows 1 BTC; account 9 rests a buy 0.1 @ 59000.
fn queue_rig() -> (Sim, hft_types::SlotId) {
    let mut setup = setup_sim();
    setup.tuning.maker_fill = hft_sim::MakerFill::Queue;
    let mut s = Sim::with(&setup, &rd_sim());
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[("59000", "1")]);
    let o = s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    (s, o)
}

#[test]
fn sim_228_queue_position() {
    use hft_types::Side;
    // .1 a trade at the price consumes the 1 BTC ahead before the order: 0.4 (level shown first,
    // then the trade, as the exchange publishes them), then 0.5, then 0.15 → 0.05 filled.
    let (mut s, o) = queue_rig();
    s.topn(ms(200), BTC_S1, &[("60000", "1")], &[("59000", "0.6")]);
    s.trade_by(ms(220), BTC_S1, "59000", "0.4", Side::Sell);
    s.topn(ms(300), BTC_S1, &[("60000", "1")], &[("59000", "0.1")]);
    s.trade_by(ms(320), BTC_S1, "59000", "0.5", Side::Sell);
    s.run_to(ms(330));
    assert!(s.fills_of(A9, o).is_empty(), "SIM-228.1");
    s.trade_by(ms(400), BTC_S1, "59000", "0.15", Side::Sell);
    s.run_to(ms(410));
    assert_eq!(s.fills_of(A9, o), vec![(px("59000"), q("0.05"))], "SIM-228.1");
    // .2 a decrease without a trade within 500 ms is a cancellation behind the order: the queue
    // ahead only shrinks to the displayed level (0.5), so a 0.55 trade fills 0.05.
    let (mut s, o) = queue_rig();
    s.topn(ms(200), BTC_S1, &[("60000", "1")], &[("59000", "0.5")]);
    s.topn(ms(800), BTC_S1, &[("60000", "1")], &[("59000", "0.5")]);
    s.trade_by(ms(900), BTC_S1, "59000", "0.55", Side::Sell);
    s.run_to(ms(910));
    assert_eq!(s.fills_of(A9, o), vec![(px("59000"), q("0.05"))], "SIM-228.2");
    // .3 a trade at the price whose taker is on the order's own side does not touch the queue.
    let (mut s, o) = queue_rig();
    s.trade_by(ms(200), BTC_S1, "59000", "5", Side::Buy);
    s.run_to(ms(210));
    assert!(s.fills_of(A9, o).is_empty(), "SIM-228.3");
    // .4 TradeThrough ignores trades at the price.
    let mut s = Sim::std();
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[("59000", "1")]);
    let o = s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    s.trade_by(ms(200), BTC_S1, "59000", "5", Side::Sell);
    s.run_to(ms(210));
    assert!(s.fills_of(A9, o).is_empty(), "SIM-228.4");
}

/// `rd_binary()` with `maker_fill = Queue`: Up shows a 50-share bid at 0.45; account 9 rests a buy
/// of 100 Up @ 0.45.
fn binary_rig() -> (Sim, hft_types::SlotId) {
    let mut setup = setup_sim();
    setup.tuning.maker_fill = hft_sim::MakerFill::Queue;
    let mut a9 = cfg_a9();
    a9.initial_balances = vec![bal(B5, USDT, usdt("1000"))].into();
    setup.accounts = vec![(A9, a9)];
    let mut s = Sim::with(&setup, &rd_binary());
    s.topn(ms(100), UP_B5, &[("0.47", "50")], &[("0.45", "50")]);
    let o = s.place(A9, UP_B5, Buy, "100", "0.45", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    (s, o)
}

#[test]
fn sim_229_binary_complement_prints() {
    use hft_types::Side;
    // .1 buyers of Down at 0.55 are sellers of Up at 0.45: 30 then 40 shares consume the 50 ahead,
    // then fill 20.
    let (mut s, o) = binary_rig();
    s.trade_by(ms(200), DN_B5, "0.55", "30", Side::Buy);
    s.run_to(ms(210));
    assert!(s.fills_of(A9, o).is_empty(), "SIM-229.1");
    s.trade_by(ms(300), DN_B5, "0.55", "40", Side::Buy);
    s.run_to(ms(310));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("20"))], "SIM-229.1");
    // .2 sellers of Down at 0.55 are buyers of Up: no fill.
    let (mut s, o) = binary_rig();
    s.trade_by(ms(200), DN_B5, "0.55", "500", Side::Sell);
    s.run_to(ms(210));
    assert!(s.fills_of(A9, o).is_empty(), "SIM-229.2");
    // .3 a Down buy at 0.60 trades through the Up bid (mirrored at 0.40 < 0.45).
    let (mut s, o) = binary_rig();
    s.trade_by(ms(200), DN_B5, "0.60", "10", Side::Buy);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("10"))], "SIM-229.3");
    // .4 the same-token rule is unchanged: Up sellers at 0.45 consume the queue first.
    let (mut s, o) = binary_rig();
    s.trade_by(ms(200), UP_B5, "0.45", "60", Side::Sell);
    s.run_to(ms(210));
    assert_eq!(s.fills_of(A9, o), vec![(px("0.45"), q("10"))], "SIM-229.4");
}
