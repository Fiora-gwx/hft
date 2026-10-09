//! SIM-050–SIM-053: fee rates, fee assets, late fees, fee tiers.

#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use crate::common;

use common::*;
use hft_event::OrderEventKind;
use hft_refdata::FeeAssetRule;
use hft_types::{AssetId, Qty, Tif};

struct Row050 {
    row: &'static str,
    rule: FeeAssetRule,
    side: hft_types::Side,
    qty: &'static str,
    px: &'static str,
    bnb: &'static str,
    bnb_market: bool,
    fee: (Qty, AssetId),
}

/// Runs one SIM-050 row; returns the exchange after the fill.
fn run_050(r: &Row050) -> Sim {
    let rd = rd_with(|s| {
        s.caps_mut(S1).fee_asset_rule = r.rule;
        if r.row == "SIM-050.10" {
            s.inst_mut(BTC_S1).min_notional = notional("0.5");
        }
    });
    let mut setup = setup_sim();
    setup.accounts[0].1.initial_balances = vec![
        bal(S1, USDT, usdt("100000")),
        bal(S1, BNB, q(r.bnb)),
        bal(S1, BTC, q("0.1")),
        bal(P2, USDT, usdt("18000")),
    ]
    .into();
    let mut s = Sim::with(&setup, &rd);
    if r.side == Buy {
        s.topn(ms(100), BTC_S1, &[(r.px, "1")], &[]);
    } else {
        s.topn(ms(100), BTC_S1, &[], &[(r.px, "1")]);
    }
    if r.bnb_market {
        s.bbo(ms(100), BNB_S1, ("599.99", "10"), ("600.01", "10"));
    }
    s.place_flags(A9, BTC_S1, r.side, q(r.qty), px(r.px), Tif::Ioc, 0, ms(100));
    s.run_to(ms(110));
    s
}

#[test]
fn sim_050_fee_asset_rules() {
    let bnb_d = FeeAssetRule::ReceivedAssetUnlessDiscount { token: BNB };
    let rows = [
        Row050 {
            row: "SIM-050.1",
            rule: FeeAssetRule::Quote,
            side: Buy,
            qty: "0.1",
            px: "60000",
            bnb: "1",
            bnb_market: true,
            fee: (usdt("2.4"), USDT),
        },
        Row050 {
            row: "SIM-050.2",
            rule: FeeAssetRule::Quote,
            side: Sell,
            qty: "0.1",
            px: "60000",
            bnb: "1",
            bnb_market: true,
            fee: (usdt("2.4"), USDT),
        },
        Row050 {
            row: "SIM-050.3",
            rule: FeeAssetRule::ReceivedAsset,
            side: Buy,
            qty: "0.1",
            px: "60000",
            bnb: "1",
            bnb_market: true,
            fee: (q("0.00004"), BTC),
        },
        Row050 {
            row: "SIM-050.4",
            rule: FeeAssetRule::ReceivedAsset,
            side: Sell,
            qty: "0.1",
            px: "60000",
            bnb: "1",
            bnb_market: true,
            fee: (usdt("2.4"), USDT),
        },
        Row050 {
            row: "SIM-050.5",
            rule: bnb_d,
            side: Buy,
            qty: "0.1",
            px: "60000",
            bnb: "1",
            bnb_market: true,
            fee: (q("0.004"), BNB),
        },
        Row050 {
            row: "SIM-050.6",
            rule: bnb_d,
            side: Buy,
            qty: "0.1",
            px: "60000",
            bnb: "0.003",
            bnb_market: true,
            fee: (q("0.00004"), BTC),
        },
        Row050 {
            row: "SIM-050.7",
            rule: bnb_d,
            side: Buy,
            qty: "0.1",
            px: "60000",
            bnb: "1",
            bnb_market: false,
            fee: (q("0.00004"), BTC),
        },
        Row050 {
            row: "SIM-050.8",
            rule: bnb_d,
            side: Sell,
            qty: "0.1",
            px: "60000",
            bnb: "0.003",
            bnb_market: true,
            fee: (usdt("2.4"), USDT),
        },
        Row050 {
            row: "SIM-050.9",
            rule: FeeAssetRule::Configured(BNB),
            side: Sell,
            qty: "0.1",
            px: "60000",
            bnb: "1",
            bnb_market: true,
            fee: (q("0.004"), BNB),
        },
        Row050 {
            row: "SIM-050.10",
            rule: FeeAssetRule::Quote,
            side: Buy,
            qty: "0.00001",
            px: "60000.01",
            bnb: "1",
            bnb_market: true,
            fee: (usdt("0.000241"), USDT),
        },
    ];
    for r in &rows {
        let s = run_050(r);
        let fills: Vec<_> = s
            .order_events(A9)
            .into_iter()
            .map(|(_, e)| e)
            .filter(|e| e.what == OrderEventKind::Fill)
            .collect();
        assert_eq!(fills.len(), 1, "{}", r.row);
        assert_eq!((fills[0].fee, fills[0].fee_asset), r.fee, "{}", r.row);
        let usdt_d = s.total(A9, S1, USDT) - usdt("100000").0;
        let btc_d = s.total(A9, S1, BTC) - q("0.1").0;
        let bnb_t = s.total(A9, S1, BNB);
        match r.row {
            "SIM-050.1" => assert_eq!((usdt_d, btc_d), (-usdt("6002.4").0, q("0.1").0), "{}", r.row),
            "SIM-050.2" => assert_eq!(usdt_d, usdt("5997.6").0, "{}", r.row),
            "SIM-050.3" => assert_eq!((usdt_d, btc_d), (-usdt("6000").0, q("0.09996").0), "{}", r.row),
            "SIM-050.5" => assert_eq!((bnb_t, btc_d), (q("0.996").0, q("0.1").0), "{}", r.row),
            _ => {}
        }
    }
}

#[test]
fn sim_051_maker_fee() {
    let mut s = Sim::std();
    let o = s.place(A9, BTC_S1, Buy, "0.1", "59000", Tif::Gtc, ms(100));
    s.run_to(ms(110));
    s.trade(ms(200), BTC_S1, "58990", "1");
    s.run_to(ms(210));
    let e = s.events_of(A9, o).last().copied();
    assert_eq!(
        e.map(|e| (e.what, e.cum_filled, e.avg_px, e.fee, e.fee_asset)),
        Some((OrderEventKind::Fill, q("0.1"), px("59000"), usdt("1.18"), USDT))
    );
}

#[test]
fn sim_052_late_fee() {
    let mut setup = setup_sim();
    setup.tuning.late_fee = true;
    let mut s = Sim::with(&setup, &rd_sim());
    s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
    let o = s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
    s.advance(ms(153));
    assert_eq!(
        s.total(A9, S1, USDT),
        usdt("94000").0,
        "fee not yet charged before FeeOnly"
    );
    s.advance(ms(154));
    assert_eq!(s.total(A9, S1, USDT), usdt("93997.6").0);
    s.drain(ms(200));
    let ev: Vec<_> = s
        .order_events(A9)
        .into_iter()
        .filter(|(_, e)| e.slot == o)
        .collect();
    let fill = ev.iter().find(|(_, e)| e.what == OrderEventKind::Fill).copied();
    let fo = ev
        .iter()
        .find(|(_, e)| e.what == OrderEventKind::FeeOnly)
        .copied();
    assert_eq!(fill.map(|(d, e)| (d, e.fee)), Some((ms(104), Qty(0))));
    assert_eq!(
        fo.map(|(d, e)| (d, e.fee, e.fee_asset)),
        Some((ms(154), usdt("2.4"), USDT))
    );
}

#[test]
fn sim_053_fee_tier_row() {
    for (row, tier, want) in [("SIM-053.1", 1u8, "2.4"), ("SIM-053.2", 2u8, "1.8")] {
        let mut setup = setup_sim();
        setup.accounts[0].1.fee_tier = tier;
        let mut s = Sim::with(&setup, &rd_sim());
        s.topn(ms(100), BTC_S1, &[("60000", "1")], &[]);
        let o = s.place(A9, BTC_S1, Buy, "0.1", "60000", Tif::Ioc, ms(100));
        s.run_to(ms(110));
        let e = s.events_of(A9, o).last().copied();
        assert_eq!(e.map(|e| e.fee), Some(usdt(want)), "{row}");
    }
}
