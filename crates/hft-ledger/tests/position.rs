//! LED-030–LED-039: average-cost positions, realized PnL, spot cash and inventory, reports.

use crate::common;

use common::*;
use hft_ledger::{BalanceVerdict, Ledger};
use hft_refdata::FeeAssetRule;
use hft_types::{Notional, Qty};

/// LED-030's state: two buys of 1.0 on `BTC/P2` at 60 000 and 62 000.
fn after_030() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(10));
    l.apply_fill(&f(Buy, BTC_P2, "1.0", "62000", "0", USDT, G1, 1), s(11));
    l
}

#[test]
fn led_030_add_same_side() {
    let l = after_030();
    let p = l.pos(A1, BTC_P2);
    assert_eq!(p.qty, btc("2.0"));
    assert_eq!(p.cost, cq("122000"));
    assert_eq!(p.realized_quote, 0);
    assert_eq!(p.last_fill_ens, s(11));
    assert_eq!(l.perp_view(A1, BTC_P2).entry_px, px("61000"));
}

#[test]
fn led_031_partial_reduce() {
    let mut l = after_030();
    let (u0, w0) = (cash(&l, A1, P2, USDT).total, l.margin(A1, P2).wallet);
    l.apply_fill(&f(Sell, BTC_P2, "0.5", "63000", "0", USDT, G1, 1), s(12));
    let p = l.pos(A1, BTC_P2);
    // removed = 122 000 × 0.5 / 2.0 = 30 500
    assert_eq!(p.cost, cq("91500"));
    assert_eq!(p.realized_quote, cq("1000"));
    assert_eq!(p.qty, btc("1.5"));
    assert_eq!(l.group(A1, G1).pnl.price_realized, n("1000"));
    assert_eq!(cash(&l, A1, P2, USDT).total.0 - u0.0, usdt("1000").0);
    assert_eq!(l.margin(A1, P2).wallet.0 - w0.0, n("1000").0);
}

#[test]
fn led_032_removed_cost_truncation() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_P2, "0.01", "60000.0001", "0", USDT, G1, 1), s(10));
    l.apply_fill(&f(Buy, BTC_P2, "0.02", "60000.0000", "0", USDT, G1, 1), s(10));
    assert_eq!(l.pos(A1, BTC_P2).cost, 1_800_000_001_000_000, "step 1");
    l.apply_fill(&f(Sell, BTC_P2, "0.01", "60000.0000", "0", USDT, G1, 1), s(11));
    let p = l.pos(A1, BTC_P2);
    assert_eq!(p.cost, 1_200_000_000_666_667, "step 2 cost");
    assert_eq!(p.realized_quote, -333_333, "step 2 realized");
    l.apply_fill(&f(Sell, BTC_P2, "0.02", "60000.0000", "0", USDT, G1, 1), s(12));
    let p = l.pos(A1, BTC_P2);
    assert_eq!(
        (p.qty, p.cost, p.realized_quote),
        (Qty(0), 0, -1_000_000),
        "step 3"
    );
    assert_eq!(l.group(A1, G1).pnl.price_realized, Notional(0), "step 3 USDT");
}

fn long_1() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l
}

#[test]
fn led_033_cross_zero() {
    let mut l = long_1();
    l.apply_fill(&f(Sell, BTC_P2, "1.5", "61000", "0", USDT, G1, 1), s(10));
    let p = l.pos(A1, BTC_P2);
    assert_eq!(p.realized_quote, cq("1000"));
    assert_eq!(p.qty, btc("-0.5"));
    assert_eq!(p.cost, cq("30500"));
    assert_eq!(l.perp_view(A1, BTC_P2).entry_px, px("61000"));
}

#[test]
fn led_034_full_close() {
    let mut l = long_1();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "59000", "0", USDT, G1, 1), s(10));
    let p = l.pos(A1, BTC_P2);
    assert_eq!(
        (p.qty, p.cost, p.realized_quote, p.last_fill_ens),
        (Qty(0), 0, cq("-1000"), s(10))
    );
    assert_eq!(l.margin(A1, P2).mark_oldest_ens, i64::MAX);
}

#[test]
fn led_035_short_reduce() {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(10));
    let p = l.pos(A1, BTC_P2);
    assert_eq!((p.qty, p.cost), (btc("-1.0"), cq("60000")), "step 1");
    l.apply_fill(&f(Buy, BTC_P2, "0.4", "58000", "0", USDT, G1, 1), s(11));
    let p = l.pos(A1, BTC_P2);
    // removed 24 000
    assert_eq!(
        (p.realized_quote, p.cost, p.qty),
        (cq("800"), cq("36000"), btc("-0.6")),
        "step 2"
    );
}

#[test]
fn led_036_spot_cost_net() {
    let mut l = booted_with(rd_fee(FeeAssetRule::ReceivedAsset));
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0), s(10));
    let p = *l.pos(A1, BTC_S1);
    assert_eq!((p.qty, p.cost), (btc("0.999"), cq("59940")));
    assert_eq!(l.group(A1, G1).pnl.fee_cex, n("-60"));
    // Same cost as a fee-less fill followed by the late fee.
    let mut m = booted_with(rd_fee(FeeAssetRule::ReceivedAsset));
    m.apply_fill(
        &f(Buy, BTC_S1, "1.0", "60000", "0", hft_types::ASSET_NONE, G1, 0),
        s(10),
    );
    m.apply_fee(&fee_delta(Buy, BTC_S1, G1, "0.001", BTC), s(11));
    assert_eq!((m.pos(A1, BTC_S1).qty, m.pos(A1, BTC_S1).cost), (p.qty, p.cost));
}

#[test]
fn led_037_realized_into_three_books() {
    let mut l = after_030();
    let u0 = cash(&l, A1, P2, USDT).total;
    l.apply_fill(&f(Sell, BTC_P2, "0.5", "63000", "31.5", USDT, G1, 1), s(12));
    let g = l.group(A1, G1);
    for (name, b) in [("pnl", g.pnl), ("day", g.day), ("round", g.round.pnl)] {
        assert_eq!(b.price_realized, n("1000"), "{name}");
        assert_eq!(b.fee_cex, n("-31.5"), "{name}");
    }
    let a = l.account(A1).day;
    assert_eq!((a.price_realized, a.fee_cex), (n("1000"), n("-31.5")));
    assert_eq!(cash(&l, A1, P2, USDT).total.0 - u0.0, usdt("968.5").0);
}

fn after_038_step1() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "0.1", "60000", "6", USDT, G1, 0), s(10));
    l
}

#[test]
fn led_038_spot_cash_and_inventory() {
    let mut l = after_038_step1();
    assert_eq!(cash(&l, A1, S1, USDT).total, usdt("93994"), "step 1");
    assert_eq!(cash(&l, A1, S1, BTC).total, btc("0.1"), "step 1");
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("0.1"), "step 1");
    l.apply_fill(&f(Sell, BTC_S1, "0.1", "61000", "6.1", USDT, G1, 0), s(11));
    assert_eq!(cash(&l, A1, S1, USDT).total, usdt("100087.9"), "step 2");
    assert_eq!(cash(&l, A1, S1, BTC).total, Qty(0), "step 2");
    assert_eq!(l.inventory(A1, S1, BTC, G1), Qty(0), "step 2");
    let g = l.group(A1, G1).pnl;
    assert_eq!((g.price_realized, g.fee_cex), (n("100"), n("-12.1")), "step 2");
}

#[test]
fn led_039_reports_never_change_position() {
    let mut l = after_038_step1();
    let before = (
        *l.pos(A1, BTC_S1),
        cash(&l, A1, S1, BTC).total,
        l.inventory(A1, S1, BTC, G1),
    );
    let v = l.on_balance(&bal(A1, S1, BTC, btc("5"), Qty(0), D + s(20)), true, s(20));
    assert_eq!(
        v,
        BalanceVerdict::Drift {
            asset: BTC,
            diff: btc("4.9"),
        }
    );
    assert_eq!(
        (
            *l.pos(A1, BTC_S1),
            cash(&l, A1, S1, BTC).total,
            l.inventory(A1, S1, BTC, G1)
        ),
        before
    );
    let c = cash(&l, A1, S1, BTC);
    assert_eq!((c.rep_free, c.rep_ens), (btc("5"), s(20)));
}
