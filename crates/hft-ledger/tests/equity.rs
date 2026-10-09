//! LED-220–LED-226: total equity, valuation switches, validity, per-venue summary and views.

use crate::common;

use common::*;
use hft_event::ExternalCause;
use hft_ledger::views::{CashView, GroupPosView, InventoryView, VenueEquityView};
use hft_ledger::{BalanceVerdict, FixedVec, Ledger};
use hft_types::{Notional, Qty};

#[test]
fn led_220_equity_total() {
    let l = booted();
    let e = l.equity(A1);
    assert_eq!(e.total, n("155999.9"));
    assert_eq!(
        e.by_venue.iter().copied().collect::<Vec<_>>(),
        [(S1, n("105999.9")), (P2, n("50000"))]
    );
    assert!(e.valid);
}

#[test]
fn led_221_include_switches() {
    let rows = [
        (false, true, "3599.94"),
        (true, true, "5999.9"),
        (false, false, "0"),
        (true, false, "2399.96"),
    ];
    for (k, (inc_base, inc_un, want)) in rows.iter().enumerate() {
        let mut cfg = cfg_lf();
        cfg.valuation.include_baseline = *inc_base;
        cfg.valuation.include_unassigned = *inc_un;
        let mut l = ledger_with(rd_lf(), &cfg);
        boot_a1_bnb(&mut l, 0, "4");
        let mut out = FixedVec::new();
        l.set_balance_baseline(A1, S1, 1, &mut out);
        l.apply_external(
            &ext_tid(ExternalCause::Manual, Buy, BNB_S1, "6.0", "600", 1),
            false,
            s(1),
        );
        let c = cash(&l, A1, S1, BNB);
        assert_eq!(
            (c.baseline, c.unassigned),
            (qa(BNB, "4"), qa(BNB, "6")),
            "LED-221.{}",
            k + 1
        );
        // BTC is 0, so the S1 spot value is the BNB contribution.
        assert_eq!(
            l.venue_book(A1, S1).summary.spot_value,
            n(want),
            "LED-221.{}",
            k + 1
        );
    }
}

#[test]
fn led_222_equity_validity() {
    // .1
    let mut l = ledger_lf();
    l.on_run_start(0);
    for (a, free) in [(USDT, usdt("100000")), (BTC, Qty(0)), (BNB, qa(BNB, "10"))] {
        l.on_balance(&bal(A1, S1, a, free, Qty(0), D), false, 0);
    }
    l.on_ref_px(BTC_S1, px("59999.99"), px("60000.01"), 0);
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), 0);
    l.on_ref_px(USDC_S1, px("0.9999"), px("1.0001"), 0);
    assert!(!l.equity(A1).valid, "LED-222.1");
    // .2
    let mut l = booted();
    mark(&mut l, BTC_P2, "60000", s(61));
    assert!(!l.equity(A1).valid, "LED-222.2");
    // .3
    let mut l = booted();
    mark(&mut l, BTC_P2, "60000", s(30));
    assert!(l.equity(A1).valid, "LED-222.3");
}

#[test]
fn led_223_venue_summary() {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60600", s(5));
    let p = l.venue_book(A1, P2).summary;
    assert_eq!(
        (
            p.equity,
            p.quote_cash,
            p.spot_value,
            p.perp_upnl,
            p.margin_balance
        ),
        (n("49400"), n("50000"), Notional(0), n("-600"), n("49400"))
    );
    let s1 = l.venue_book(A1, S1).summary;
    assert_eq!(
        (s1.equity, s1.quote_cash, s1.spot_value),
        (n("105999.9"), n("100000"), n("5999.9"))
    );
    let v = l.on_balance(&bal(A1, S1, USDT, usdt("99000"), Qty(0), D + s(6)), true, s(6));
    assert!(matches!(v, BalanceVerdict::Drift { .. }));
    assert!(l.venue_book(A1, S1).summary.flags & VenueEquityView::BALANCE_UNRECONCILED != 0);
    let v = l.on_balance(&bal(A1, S1, USDT, usdt("100000"), Qty(0), D + s(7)), true, s(7));
    assert_eq!(v, BalanceVerdict::Ok);
    assert_eq!(
        l.venue_book(A1, S1).summary.flags & VenueEquityView::BALANCE_UNRECONCILED,
        0
    );
}

/// `A1/S1` USDC with `total` and `rep_locked` as given (the fee in USDC lowers `total`).
fn usdc_row(total: &str, locked: &str, fee: &str) -> Ledger {
    let mut l = booted();
    let t = qa(USDC, total).0 + qa(USDC, fee).0;
    let free = Qty((t - qa(USDC, locked).0).max(0));
    l.on_balance(&bal(A1, S1, USDC, free, qa(USDC, locked), D), false, s(1));
    if fee != "0" {
        l.apply_fee(&fee_delta(Buy, BTC_S1, G1, fee, USDC), s(2));
    }
    assert_eq!(cash(&l, A1, S1, USDC).total, qa(USDC, total));
    l
}

#[test]
fn led_224_cash_view() {
    let rows = [
        ("100", "30", "0", "70"),
        ("30", "30", "0", "0"),
        ("20", "30", "10", "0"),
    ];
    for (k, (total, locked, fee, free)) in rows.iter().enumerate() {
        let l = usdc_row(total, locked, fee);
        assert_eq!(
            l.cash_view(A1, S1, USDC),
            CashView {
                free: qa(USDC, free),
                locked: qa(USDC, locked),
            },
            "LED-224.{}",
            k + 1
        );
    }
}

#[test]
fn led_225_inventory_views() {
    let mut l = ledger_lf();
    boot_a1_init(&mut l, 0, "0.05", "10");
    l.apply_fill(&f(Buy, BTC_S1, "0.3", "60000", "0", USDT, G1, 0), s(1));
    assert_eq!(
        l.inventory_view(A1, S1, BTC),
        InventoryView {
            total: btc("0.35"),
            assigned: btc("0.3"),
            unassigned: btc("0.05"),
            baseline: Qty(0),
        }
    );
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("0.3"));
    assert_eq!(l.inventory_of(A1, S1, G1), btc("0.3"));
}

#[test]
fn led_226_group_pos_view() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0", USDT, G1, 0), s(10));
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60100", "0", USDT, G1, 1), s(11));
    mark(&mut l, BTC_P2, "60100", s(11));
    l.on_ref_px(BTC_S1, px("60000"), px("60000.01"), s(11));
    let g = l.group_pos(A1, G1);
    assert_eq!(g.n_legs, 2);
    assert_eq!([g.legs[0].qty, g.legs[1].qty], [btc("1.0"), btc("-1.0")]);
    assert_eq!(
        [g.legs[0].entry_px, g.legs[1].entry_px],
        [px("60000"), px("60100")]
    );
    assert_eq!([g.legs[0].upnl, g.legs[1].upnl], [Notional(0), Notional(0)]);
    assert_eq!(g.held_notional, n("120100"));
    assert_eq!((g.round_start_ens, g.last_fill_ens), (s(10), s(11)));
    assert_eq!(g.flags & GroupPosView::ENTRY_ADOPTED, 0);
}
