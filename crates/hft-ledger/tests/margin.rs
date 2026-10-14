//! LED-150–LED-156, LED-158, LED-160, LED-161: margin report, estimates, liquidation distances,
//! oldest mark time, staleness and `restore_marks`.

use crate::common;

use common::*;
use hft_ledger::views::VenueEquityView;
use hft_ledger::{Ledger, MarginItemRow, lflags};
use hft_types::{Notional, Px, Qty, Rate};

/// `mg_a1`: `A1/P2` wallet 18 000, `G1` short 1.0 at 60 000, mark 60 000, report at 2 s.
fn mg_a1_no_report() -> Ledger {
    let mut l = ledger_lf();
    l.on_run_start(0);
    for (v, a, free) in [
        (S1, USDT, usdt("100000")),
        (S1, BTC, Qty(0)),
        (S1, BNB, qa(BNB, "10")),
        (P2, USDT, usdt("18000")),
    ] {
        l.on_balance(&bal(A1, v, a, free, Qty(0), D), true, 0);
    }
    l.on_ref_px(BTC_S1, px("59999.99"), px("60000.01"), 0);
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), 0);
    l.on_ref_px(USDC_S1, px("0.9999"), px("1.0001"), 0);
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60000", s(1));
    l
}

fn mg_report(l: &mut Ledger, t: i64) {
    let items = margin_items(&[(BTC_P2, px("77600"), 3)]);
    on_margin(
        l,
        margin_hdr(A1, P2, "18000", "300", "6000", "0.016666667", 1),
        &items,
        t,
    );
}

fn mg_a1() -> Ledger {
    let mut l = mg_a1_no_report();
    mg_report(&mut l, s(2));
    l
}

#[test]
fn led_150_est_liq_dist() {
    let rows: [(&str, Option<&str>, &str, Rate, Rate); 5] = [
        (
            "60000",
            Some("0"),
            "18000",
            rate("0.293532338"),
            rate("0.016666667"),
        ),
        (
            "66000",
            Some("-6000"),
            "12000",
            rate("0.175938489"),
            rate("0.0275"),
        ),
        ("54000", None, "24000", rate("0.436464088"), rate("0.0125")),
        ("77610", None, "", rate("0.000025001"), Rate(0)),
        ("78000", None, "0", Rate(0), Rate::MAX),
    ];
    for (k, (m, upnl, emb, dist, ratio)) in rows.iter().enumerate() {
        let id = format!("LED-150.{}", k + 1);
        let mut l = mg_a1();
        mark(&mut l, BTC_P2, m, s(3));
        let ms = l.margin(A1, P2);
        if let Some(u) = upnl {
            assert_eq!(ms.upnl, n(u), "{id}");
        }
        if !emb.is_empty() {
            assert_eq!(ms.est_margin_balance, n(emb), "{id}");
        }
        assert_eq!(ms.est_liq_dist, *dist, "{id}");
        if k != 3 {
            assert_eq!(ms.est_ratio, *ratio, "{id}");
        }
    }
}

/// `boot_a1` + position (`side`, 1.0) + report with `liq_px` + mark 60 000 at 3 s.
fn rep_row(side: hft_types::Side, liq: &str) -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(side, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    let items = margin_items(&[(BTC_P2, px(liq), 1)]);
    on_margin(
        &mut l,
        margin_hdr(A1, P2, "50000", "300", "40000", "0.006", 1),
        &items,
        s(2),
    );
    mark(&mut l, BTC_P2, "60000", s(3));
    l
}

#[test]
fn led_151_rep_liq_dist() {
    let rows = [
        (Buy, "51000", rate("0.15")),
        (Sell, "69000", rate("0.15")),
        (Buy, "61000", Rate(0)),
        (Sell, "59000", Rate(0)),
        (Sell, "0", Rate::MAX),
    ];
    for (k, (side, liq, want)) in rows.iter().enumerate() {
        let l = rep_row(*side, liq);
        let ms = l.margin(A1, P2);
        assert_eq!(ms.rep_liq_dist, *want, "LED-151.{}", k + 1);
        if k == 4 {
            assert!(ms.lflags & lflags::LIQ_PX_MISSING != 0, "LED-151.5");
        }
    }
    let l = mg_a1();
    assert_eq!(l.margin(A1, P2).rep_liq_dist, rate("0.293333333"), "LED-151.6");
}

#[test]
fn led_152_min_over_positions() {
    for (k, (btc_liq, eth_liq, want, missing)) in [
        ("63000", "3450", rate("0.05"), false),
        ("0", "3450", rate("0.15"), true),
    ]
    .iter()
    .enumerate()
    {
        let mut l = booted();
        l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
        l.apply_fill(&f(Sell, ETH_P2, "10.0", "3000", "0", USDT, G3, 0), s(1));
        mark(&mut l, BTC_P2, "60000", s(1));
        mark(&mut l, ETH_P2, "3000", s(1));
        let items = margin_items(&[(BTC_P2, px(btc_liq), 1), (ETH_P2, px(eth_liq), 1)]);
        on_margin(
            &mut l,
            margin_hdr(A1, P2, "50000", "300", "40000", "0.006", 2),
            &items,
            s(2),
        );
        let ms = l.margin(A1, P2);
        assert_eq!(ms.rep_liq_dist, *want, "LED-152.{}", k + 1);
        assert_eq!(
            ms.lflags & lflags::LIQ_PX_MISSING != 0,
            *missing,
            "LED-152.{}",
            k + 1
        );
    }
}

#[test]
fn led_153_no_perp_position() {
    let mut l = booted();
    on_margin(
        &mut l,
        margin_hdr(A1, P2, "50000", "0", "50000", "0", 0),
        &[],
        s(2),
    );
    let ms = l.margin(A1, P2);
    assert_eq!(
        (ms.rep_liq_dist, ms.est_liq_dist, ms.mark_oldest_ens),
        (Rate::MAX, Rate::MAX, i64::MAX)
    );
}

#[test]
fn led_154_no_report_no_estimate() {
    let mut l = mg_a1_no_report();
    mark(&mut l, BTC_P2, "66000", s(3));
    let ms = l.margin(A1, P2);
    assert!(!ms.has_report);
    assert_eq!(ms.est_ratio, Rate(0));
    assert_eq!(ms.est_liq_dist, Rate::MAX);
    assert_eq!(ms.rep_liq_dist, Rate::MAX);
    assert!(ms.lflags & lflags::LIQ_PX_MISSING != 0);
    assert_eq!(ms.upnl, n("-6000"));
}

#[test]
fn led_155_mark_oldest() {
    let mut l = booted();
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60000", s(10));
    assert_eq!(l.margin(A1, P2).mark_oldest_ens, s(10), "LED-155.1");
    l.apply_fill(&f(Sell, ETH_P2, "1.0", "3000", "0", USDT, G3, 0), s(11));
    assert_eq!(l.margin(A1, P2).mark_oldest_ens, 0, "LED-155.2");
    mark(&mut l, ETH_P2, "3000", s(12));
    assert_eq!(l.margin(A1, P2).mark_oldest_ens, s(10), "LED-155.3");
    mark(&mut l, BTC_P2, "60000", s(13));
    assert_eq!(l.margin(A1, P2).mark_oldest_ens, s(12), "LED-155.4");
}

#[test]
fn led_156_touched_only_holders() {
    let mut l = booted();
    boot_a2(&mut l, 0, "10000", "10000");
    l.apply_fill(&f(Sell, ETH_P2, "1.0", "3000", "0", USDT, G3, 0), s(1));
    assert_eq!(mark(&mut l, ETH_P2, "3000", s(5)), [(A1, P2)]);
    assert!(mark(&mut l, BTC_P2, "60000", s(6)).is_empty());
}

fn flags(l: &Ledger, v: hft_types::VenueId) -> u16 {
    l.venue_book(A1, v).summary.flags
}

#[test]
fn led_158_stale_before_run_start() {
    let mut l = mg_a1();
    l.on_run_start(s(100));
    assert!(flags(&l, S1) & VenueEquityView::RATE_STALE != 0, "step 1 S1");
    assert!(flags(&l, P2) & VenueEquityView::MARGIN_STALE != 0, "step 1 P2");
    assert!(!l.equity(A1).valid, "step 1");
    l.on_ref_px(BTC_S1, px("59999.99"), px("60000.01"), s(101));
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), s(101));
    l.on_ref_px(USDC_S1, px("0.9999"), px("1.0001"), s(101));
    assert_eq!(flags(&l, S1) & VenueEquityView::RATE_STALE, 0, "step 2");
    mg_report(&mut l, s(102));
    assert_eq!(flags(&l, P2) & VenueEquityView::MARGIN_STALE, 0, "step 3");
    assert!(l.equity(A1).valid, "step 3");
}

#[test]
fn led_160_restore_marks_no_delta() {
    let mut l = mg_a1();
    mark(&mut l, BTC_P2, "61000", s(5));
    assert_eq!(l.group(A1, G1).day.unrealized, n("-1000"));
    let before = (*l.group(A1, G1), l.account(A1).day);
    let mut r = restore(&snap(&l)).expect("restore");
    r.clear_notes();
    r.restore_marks(&[(BTC_P2, px("61000"), s(5))]);
    assert_eq!(r.group(A1, G1).day.unrealized, n("-1000"));
    assert_eq!(r.account(A1).upnl, n("-1000"));
    assert_eq!(
        (r.group(A1, G1).pnl, r.group(A1, G1).day, r.account(A1).day),
        (before.0.pnl, before.0.day, before.1)
    );
    assert!(r.notes().is_empty());
}

#[test]
fn led_161_on_margin_stores_report() {
    let l = mg_a1();
    let ms = l.margin(A1, P2);
    assert!(ms.has_report);
    assert_eq!(
        (
            ms.rep_margin_balance,
            ms.rep_maint,
            ms.rep_avail,
            ms.rep_ratio,
            ms.rep_ens,
            ms.rep_perp_notional
        ),
        (
            n("18000"),
            n("300"),
            n("6000"),
            rate("0.016666667"),
            s(2),
            n("60000")
        )
    );
    let items: Vec<MarginItemRow> = ms.items.iter().copied().collect();
    assert_eq!(
        items,
        [MarginItemRow {
            inst: BTC_P2,
            liq_px: px("77600"),
            adl_quantile: 3,
        }]
    );
    let p = l.perp_view(A1, BTC_P2);
    assert_eq!(
        (p.qty, p.entry_px, p.upnl, p.liq_px, p.adl_quantile),
        (btc("-1.0"), px("60000"), Notional(0), px("77600"), 3)
    );
    let e = l.perp_view(A1, ETH_P2);
    assert_eq!((e.liq_px, e.adl_quantile), (Px(0), 0));
}
