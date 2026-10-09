//! LED-190–LED-194: unrealized PnL maintenance, day roll, valuation rules.

mod common;

use common::*;
use hft_ledger::{Ledger, PerpVal, PnlBreakdown, SpotVal};
use hft_types::Notional;

fn short_1() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l
}

fn after_190() -> Ledger {
    let mut l = short_1();
    mark(&mut l, BTC_P2, "60600", s(5));
    mark(&mut l, BTC_P2, "60300", s(6));
    l
}

#[test]
fn led_190_mark_updates_holders() {
    let mut l = short_1();
    mark(&mut l, BTC_P2, "60600", s(5));
    assert_eq!(
        (l.group(A1, G1).day.unrealized, l.account(A1).upnl),
        (n("-600"), n("-600")),
        "step 1"
    );
    mark(&mut l, BTC_P2, "60300", s(6));
    assert_eq!(
        (l.group(A1, G1).day.unrealized, l.account(A1).upnl),
        (n("-300"), n("-300")),
        "step 2"
    );
}

#[test]
fn led_191_only_holders_touched() {
    let mut l = short_1();
    mark(&mut l, BTC_P2, "61000", s(5));
    mark(&mut l, ETH_P2, "3100", s(6));
    assert_eq!(l.group(A1, G3).day.unrealized, Notional(0));
    assert_eq!(l.group(A1, G1).day.unrealized, n("-1000"));
}

#[test]
fn led_192_day_roll_baseline() {
    let mut l = after_190();
    let pnl0 = l.group(A1, G1).pnl;
    l.on_day_roll(s(7));
    assert_eq!(l.group(A1, G1).day, PnlBreakdown::zero(), "step 1");
    assert_eq!(l.account(A1).day, PnlBreakdown::zero(), "step 1");
    assert_eq!(l.group(A1, G1).pnl, pnl0, "step 1");
    mark(&mut l, BTC_P2, "60400", s(8));
    assert_eq!(l.group(A1, G1).day.unrealized, n("-100"), "step 2");
}

#[test]
fn led_193_spot_valuation() {
    for (k, (v, want)) in [(SpotVal::Bid, "100"), (SpotVal::Mid, "200")].iter().enumerate() {
        let mut cfg = cfg_lf();
        cfg.valuation.spot = *v;
        let mut l = ledger_with(rd_lf(), &cfg);
        boot_a1(&mut l, 0);
        l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0", USDT, G1, 0), s(1));
        l.on_ref_px(BTC_S1, px("60100"), px("60300"), s(5));
        assert_eq!(l.group(A1, G1).day.unrealized, n(want), "LED-193.{}", k + 1);
    }
}

#[test]
fn led_194_perp_valuation() {
    let rows = [
        (Sell, PerpVal::Mark, "-600"),
        (Sell, PerpVal::ExitSide, "-700"),
        (Buy, PerpVal::ExitSide, "500"),
    ];
    for (k, (side, v, want)) in rows.iter().enumerate() {
        let mut cfg = cfg_lf();
        cfg.valuation.perp = *v;
        let mut l = ledger_with(rd_lf(), &cfg);
        boot_a1(&mut l, 0);
        l.apply_fill(&f(*side, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
        l.on_ref_px(BTC_P2, px("60500"), px("60700"), s(5));
        mark(&mut l, BTC_P2, "60600", s(5));
        assert_eq!(l.perp_view(A1, BTC_P2).upnl, n(want), "LED-194.{}", k + 1);
    }
}
