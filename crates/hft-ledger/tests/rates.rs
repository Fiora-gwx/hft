//! LED-210–LED-215: USDT conversion rates, par assets, staleness and fallback.

use crate::common;

use common::*;
use hft_ledger::views::VenueEquityView;
use hft_ledger::{Ledger, RateView};
use hft_refdata::FeeAssetRule;

fn s3_value(l: &Ledger) -> hft_types::Notional {
    l.equity(A2)
        .by_venue
        .iter()
        .find(|(v, _)| *v == S3)
        .map(|x| x.1)
        .expect("S3 in equity(A2)")
}

#[test]
fn led_210_rate_is_mid_floor() {
    let rd = rd_fee(FeeAssetRule::ReceivedAssetUnlessDiscount { token: BNB });
    // .1
    let mut l = booted_with(rd);
    boot_a2(&mut l, 0, "100000", "10000");
    l.on_ref_px(USDC_S1, px("0.9999"), px("1.0002"), s(5));
    assert_eq!(
        l.rate(USDC),
        RateView {
            bid: px("0.9999"),
            ask: px("1.0002"),
            ens: s(5),
            stale: false,
        },
        "LED-210.1"
    );
    assert_eq!(s3_value(&l), n("100000"), "LED-210.1");
    // .2
    let mut l = booted_with(rd);
    boot_a2(&mut l, 0, "100000", "10000");
    l.on_ref_px(BNB_S1, px("599.99"), px("600.02"), s(5));
    l.apply_fill(&f(Buy, BTC_S1, "0.01", "60000", "1.0", BNB, G1, 0), s(6));
    assert_eq!(l.group(A1, G1).pnl.fee_cex, n("-600.005"), "LED-210.2");
}

#[test]
fn led_211_par_asset() {
    let mut l = ledger_lf();
    l.on_run_start(s(1000));
    assert!(!l.rate(USDT).stale);
    l.apply_fee(&fee_delta(Buy, BTC_S1, G1, "6", USDT), s(1000));
    assert_eq!(l.group(A1, G1).pnl.fee_cex, n("-6"));
}

fn refresh_all(l: &mut Ledger, t: i64) {
    l.on_ref_px(BTC_S1, px("59999.99"), px("60000.01"), t);
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), t);
    l.on_ref_px(USDC_S1, px("0.9999"), px("1.0001"), t);
}

#[test]
fn led_212_rate_age_boundary() {
    for (k, (t, stale)) in [(s(70) - 1, false), (s(70), false), (s(70) + 1, true)]
        .iter()
        .enumerate()
    {
        let mut l = booted();
        refresh_all(&mut l, s(10));
        mark(&mut l, BTC_P2, "60000", *t);
        assert_eq!(l.rate(BNB).stale, *stale, "LED-212.{}", k + 1);
        if *stale {
            assert!(
                l.venue_book(A1, S1).summary.flags & VenueEquityView::RATE_STALE != 0,
                "LED-212.3"
            );
        }
    }
}

#[test]
fn led_213_stale_fallback() {
    let mut l = booted();
    boot_a2(&mut l, 0, "100000", "10000");
    l.on_ref_px(USDC_S1, px("0.9000"), px("0.9002"), s(10));
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), s(10));
    l.on_ref_px(BTC_S1, px("59999.99"), px("60000.01"), s(10));
    mark(&mut l, BTC_P2, "60000", s(200));
    assert_eq!(s3_value(&l), n("100000"), "LED-213.1");
    assert!(l.rate(USDC).stale, "LED-213.1");
    let b = l.rate(BNB);
    assert!(b.stale, "LED-213.2");
    assert_eq!(
        (b.bid, b.ask),
        (px("599.99"), px("600.01")),
        "LED-213.2: old value kept"
    );
    // The BNB fee conversion still uses 600.00.
    l.apply_fill(&f(Buy, BTC_S1, "0.01", "60000", "0.01", BNB, G1, 0), s(200));
    assert_eq!(l.group(A1, G1).pnl.fee_cex, n("-6"), "LED-213.2");
    assert!(!l.equity(A1).valid, "LED-213.3");
}

#[test]
fn led_214_stale_before_run_start() {
    let mut l = ledger_lf();
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), s(95));
    l.on_run_start(s(100));
    assert!(l.rate(BNB).stale);
}

const G7_SPEC: fn() -> hft_event::GroupSpec = || spec(G7, BTC, &[(BTCUSDC_S3, S3)]);

#[test]
fn led_215_realized_not_revalued() {
    let mut l = booted();
    boot_a2(&mut l, 0, "100000", "10000");
    l.on_groups_changed(I2, &table(&[G7_SPEC()]));
    l.on_ref_px(USDC_S1, px("0.9989"), px("0.9991"), s(1));
    l.apply_fill(&fa(A2, Buy, BTCUSDC_S3, "0.1", "60000", "0", USDC, G7, 0), s(2));
    l.apply_fill(&fa(A2, Sell, BTCUSDC_S3, "0.1", "61000", "0", USDC, G7, 0), s(3));
    assert_eq!(l.group(A2, G7).pnl.price_realized, n("99.9"), "step 1");
    l.on_ref_px(USDC_S1, px("1.0009"), px("1.0011"), s(4));
    assert_eq!(l.group(A2, G7).pnl.price_realized, n("99.9"), "step 2");
    // USDC cash 100 100 at 1.0010.
    assert_eq!(l.venue_book(A2, S3).summary.quote_cash, n("100200.1"), "step 2");
}
