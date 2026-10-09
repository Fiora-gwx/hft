//! LED-010–LED-018, LED-021–LED-025: net quantity rule, NetAdjust, late fees and fee buckets.

mod common;

use common::*;
use hft_event::{ExternalCause, NoteLevel};
use hft_ledger::notes::code;
use hft_ledger::{FillApplied, Ledger, NetAdjust, bflags};
use hft_refdata::FeeAssetRule;
use hft_types::{ASSET_NONE, GROUP_NONE, Notional, Qty};

/// Values a row of LED-010 compares before/after.
#[derive(Debug, Clone, Copy)]
struct Probe {
    btc_total: i64,
    btc_inv: i64,
    bnb_unassigned: i64,
    usdt_s1: i64,
    pos_s1: i64,
    pos_p2: i64,
    usdt_p2: i64,
    wallet: i64,
    fee_cex: i64,
}

fn probe(l: &Ledger) -> Probe {
    Probe {
        btc_total: cash(l, A1, S1, BTC).total.0,
        btc_inv: l.inventory(A1, S1, BTC, G1).0,
        bnb_unassigned: cash(l, A1, S1, BNB).unassigned.0,
        usdt_s1: cash(l, A1, S1, USDT).total.0,
        pos_s1: l.pos(A1, BTC_S1).qty.0,
        pos_p2: l.pos(A1, BTC_P2).qty.0,
        usdt_p2: cash(l, A1, P2, USDT).total.0,
        wallet: l.margin(A1, P2).wallet.0,
        fee_cex: l.group(A1, G1).pnl.fee_cex.0,
    }
}

/// Boots with `rule` on `S1`, pre-buys 2.0 for sell rows, applies `fill` at 10 s.
fn row(rule: FeeAssetRule, fill: hft_ledger::FillDelta) -> (Probe, Probe, FillApplied) {
    let mut l = booted_with(rd_fee(rule));
    if fill.side == Sell && fill.venue == S1 {
        l.apply_fill(&f(Buy, BTC_S1, "2.0", "60000", "0", USDT, G1, 0), s(1));
    }
    let a = probe(&l);
    let r = l.apply_fill(&fill, s(10));
    (a, probe(&l), r)
}

#[test]
fn led_010_net_qty_rules() {
    let b = |v| btc(v).0;
    let u = |v| usdt(v).0;
    let nn = |v| n(v).0;
    // .1
    let (a, z, r) = row(
        FeeAssetRule::Quote,
        f(Buy, BTC_S1, "1.0", "60000", "6", USDT, G1, 0),
    );
    assert_eq!(z.pos_s1 - a.pos_s1, b("1.0"), "LED-010.1");
    assert_eq!(
        (z.btc_total - a.btc_total, z.btc_inv - a.btc_inv),
        (b("1.0"), b("1.0")),
        "LED-010.1"
    );
    assert_eq!(z.usdt_s1 - a.usdt_s1, -u("60006"), "LED-010.1");
    assert_eq!(z.fee_cex - a.fee_cex, -nn("6"), "LED-010.1");
    assert_eq!(r.net_adjust, None, "LED-010.1");
    // .2
    let (a, z, _) = row(
        FeeAssetRule::Quote,
        f(Sell, BTC_S1, "1.0", "60000", "6", USDT, G1, 0),
    );
    assert_eq!(
        (z.btc_total - a.btc_total, z.btc_inv - a.btc_inv),
        (-b("1.0"), -b("1.0")),
        "LED-010.2"
    );
    assert_eq!(z.usdt_s1 - a.usdt_s1, u("59994"), "LED-010.2");
    // .3
    let (a, z, r) = row(
        FeeAssetRule::ReceivedAsset,
        f(Buy, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0),
    );
    assert_eq!(z.pos_s1 - a.pos_s1, b("0.999"), "LED-010.3");
    assert_eq!(
        (z.btc_total - a.btc_total, z.btc_inv - a.btc_inv),
        (b("0.999"), b("0.999")),
        "LED-010.3"
    );
    assert_eq!(z.usdt_s1 - a.usdt_s1, -u("60000"), "LED-010.3");
    assert_eq!(z.fee_cex - a.fee_cex, -nn("60"), "LED-010.3");
    assert_eq!(r.net_adjust, None, "LED-010.3");
    // .4
    let (a, z, _) = row(
        FeeAssetRule::ReceivedAsset,
        f(Sell, BTC_S1, "1.0", "60000", "6", USDT, G1, 0),
    );
    assert_eq!(a.pos_s1 - z.pos_s1, b("1.0"), "LED-010.4");
    assert_eq!(z.usdt_s1 - a.usdt_s1, u("59994"), "LED-010.4");
    // .5
    let disc = FeeAssetRule::ReceivedAssetUnlessDiscount { token: BNB };
    let (a, z, _) = row(disc, f(Buy, BTC_S1, "1.0", "60000", "0.01", BNB, G1, 0));
    assert_eq!(z.pos_s1 - a.pos_s1, b("1.0"), "LED-010.5");
    assert_eq!(z.btc_total - a.btc_total, b("1.0"), "LED-010.5");
    assert_eq!(
        (a.bnb_unassigned, z.bnb_unassigned),
        (qa(BNB, "10").0, qa(BNB, "9.99").0),
        "LED-010.5"
    );
    assert_eq!(z.fee_cex - a.fee_cex, -nn("6"), "LED-010.5");
    // .6
    let (a, z, _) = row(disc, f(Buy, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0));
    assert_eq!(z.pos_s1 - a.pos_s1, b("0.999"), "LED-010.6");
    // .7
    let (a, z, _) = row(
        FeeAssetRule::Configured(BNB),
        f(Sell, BTC_S1, "1.0", "60000", "0.01", BNB, G1, 0),
    );
    assert_eq!(a.pos_s1 - z.pos_s1, b("1.0"), "LED-010.7");
    assert_eq!(
        z.bnb_unassigned - a.bnb_unassigned,
        -qa(BNB, "0.01").0,
        "LED-010.7"
    );
    // .8: the reported fee asset wins over the rule.
    let (a, z, _) = row(
        FeeAssetRule::Configured(BNB),
        f(Buy, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0),
    );
    assert_eq!(z.pos_s1 - a.pos_s1, b("0.999"), "LED-010.8");
    // .9
    let (a, z, _) = row(
        FeeAssetRule::Configured(BTC),
        f(Sell, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0),
    );
    assert_eq!(a.pos_s1 - z.pos_s1, b("1.001"), "LED-010.9");
    assert_eq!(
        (z.btc_total - a.btc_total, z.btc_inv - a.btc_inv),
        (-b("1.001"), -b("1.001")),
        "LED-010.9"
    );
    assert_eq!(z.usdt_s1 - a.usdt_s1, u("60000"), "LED-010.9");
    // .10
    let (a, z, _) = row(
        FeeAssetRule::Quote,
        f(Buy, BTC_P2, "1.0", "60000", "30", USDT, G1, 1),
    );
    assert_eq!(z.pos_p2 - a.pos_p2, b("1.0"), "LED-010.10");
    assert_eq!(z.usdt_p2 - a.usdt_p2, -u("30"), "LED-010.10");
    assert_eq!(z.wallet - a.wallet, -nn("30"), "LED-010.10");
}

fn mismatch_notes(l: &Ledger) -> Vec<hft_event::Note> {
    notes_of(l, code::NET_QTY_MISMATCH)
}

#[test]
fn led_011_consistent_net_no_adjust() {
    let mut l = booted_with(rd_fee(FeeAssetRule::ReceivedAsset));
    let fl = f(Buy, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0);
    assert_eq!(fl.net_qty, btc("0.999"));
    l.clear_notes();
    let r = l.apply_fill(&fl, s(10));
    assert_eq!(r.net_adjust, None);
    assert!(mismatch_notes(&l).is_empty());
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("0.999"));
}

#[test]
fn led_012_buy_net_mismatch() {
    let mut l = booted_with(rd_fee(FeeAssetRule::ReceivedAsset));
    let mut fl = f(Buy, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0);
    fl.net_qty = btc("1.0");
    l.clear_notes();
    let r = l.apply_fill(&fl, s(10));
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("0.999"));
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("0.999"));
    assert_eq!(cash(&l, A1, S1, BTC).total, btc("0.999"));
    assert_eq!(
        r.net_adjust,
        Some(NetAdjust {
            account: A1,
            venue: S1,
            group: G1,
            inst: BTC_S1,
            delta: btc("-0.001"),
        })
    );
    let ns = mismatch_notes(&l);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].level, NoteLevel::P1);
}

#[test]
fn led_013_sell_net_mismatch_signed() {
    let mut l = booted_with(rd_fee(FeeAssetRule::Configured(BTC)));
    l.apply_fill(&f(Buy, BTC_S1, "2.0", "60000", "0", USDT, G1, 0), s(1));
    let mut fl = f(Sell, BTC_S1, "1.0", "60000", "0.001", BTC, G1, 0);
    assert_eq!(fl.net_qty, btc("1.001"), "recomputed net");
    fl.net_qty = btc("1.0");
    l.clear_notes();
    let r = l.apply_fill(&fl, s(10));
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("0.999"));
    assert_eq!(r.net_adjust.map(|x| x.delta), Some(btc("-0.001")));
    let ns = mismatch_notes(&l);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].level, NoteLevel::P1);
}

/// LED-014's state: fee-less buy of 1.0 under `ReceivedAsset`.
fn after_014() -> (Ledger, FillApplied) {
    let mut l = booted_with(rd_fee(FeeAssetRule::ReceivedAsset));
    let fl = f(Buy, BTC_S1, "1.0", "60000", "0", ASSET_NONE, G1, 0);
    assert_eq!(fl.net_qty, btc("1.0"));
    l.clear_notes();
    let r = l.apply_fill(&fl, s(10));
    (l, r)
}

#[test]
fn led_014_fee_absent_uses_gross() {
    let (l, r) = after_014();
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("1.0"));
    assert_eq!(l.group(A1, G1).pnl.fee_cex, Notional(0));
    assert_eq!(r.net_adjust, None);
    assert!(mismatch_notes(&l).is_empty());
}

#[test]
fn led_015_late_base_fee_buy() {
    let (mut l, _) = after_014();
    let r = l.apply_fee(&fee_delta(Buy, BTC_S1, G1, "0.001", BTC), s(11));
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("0.999"));
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("0.999"));
    assert_eq!(cash(&l, A1, S1, BTC).total, btc("0.999"));
    assert_eq!(l.group(A1, G1).pnl.fee_cex, n("-60"));
    assert_eq!(
        r.net_adjust,
        Some(NetAdjust {
            account: A1,
            venue: S1,
            group: G1,
            inst: BTC_S1,
            delta: btc("-0.001"),
        })
    );
}

#[test]
fn led_016_late_base_fee_sell() {
    let mut l = booted_with(rd_fee(FeeAssetRule::Configured(BTC)));
    l.apply_fill(&f(Buy, BTC_S1, "2.0", "60000", "0", USDT, G1, 0), s(1));
    l.apply_fill(&f(Sell, BTC_S1, "1.0", "60000", "0", ASSET_NONE, G1, 0), s(2));
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("1.0"));
    let r = l.apply_fee(&fee_delta(Sell, BTC_S1, G1, "0.001", BTC), s(11));
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("0.999"));
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("0.999"));
    assert_eq!(r.net_adjust.map(|x| x.delta), Some(btc("-0.001")));
}

#[test]
fn led_017_late_quote_fee() {
    let (mut l, _) = after_014();
    let before = cash(&l, A1, S1, USDT).total;
    let r = l.apply_fee(&fee_delta(Buy, BTC_S1, G1, "6", USDT), s(11));
    assert_eq!(cash(&l, A1, S1, USDT).total.0 - before.0, -usdt("6").0);
    assert_eq!(l.group(A1, G1).pnl.fee_cex, n("-6"));
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("1.0"));
    assert_eq!(r.net_adjust, None);
}

#[test]
fn led_018_unattributed_fee_account_only() {
    let mut l = ledger_lf();
    set_groups(&mut l, &[g4()]);
    boot_a1(&mut l, 0);
    let groups = [G1, G3, G4].map(|g| *l.group(A1, g));
    let usdt0 = cash(&l, A1, S1, USDT).total;
    let r = l.apply_fee(&fee_delta(Buy, BTC_S1, GROUP_NONE, "6", USDT), s(11));
    assert_eq!(l.account(A1).day.fee_cex, n("-6"));
    assert_eq!(cash(&l, A1, S1, USDT).total.0 - usdt0.0, -usdt("6").0);
    for (g, before) in [G1, G3, G4].iter().zip(groups) {
        assert_eq!(l.group(A1, *g).pnl, before.pnl);
        assert_eq!(l.group(A1, *g).day, before.day);
    }
    assert_eq!(r.pnl.group, GROUP_NONE);
}

fn disc() -> &'static hft_refdata::RefData {
    rd_fee(FeeAssetRule::ReceivedAssetUnlessDiscount { token: BNB })
}

#[test]
fn led_021_discount_fee_from_unassigned() {
    let mut l = booted_with(disc());
    l.clear_notes();
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0.01", BNB, G1, 0), s(10));
    let c = cash(&l, A1, S1, BNB);
    assert_eq!(
        (c.unassigned, c.baseline, c.total),
        (qa(BNB, "9.99"), Qty(0), qa(BNB, "9.99"))
    );
    for g in [G1, G3, G4] {
        assert_eq!(l.inventory(A1, S1, BNB, g), Qty(0));
    }
    assert_eq!(l.group(A1, G1).pnl.fee_cex, n("-6"));
    assert!(notes_of(&l, code::INV_SHORTFALL).is_empty());
}

/// LED-022/023 construction: BNB initialized to `init`, baselined, then an unattributed
/// external buy of `ext_qty` BNB.
fn baselined_bnb(init: &str, ext_qty: &str) -> Ledger {
    let mut l = ledger_with(disc(), &cfg_lf());
    boot_a1_bnb(&mut l, 0, init);
    let mut out = hft_ledger::FixedVec::new();
    l.set_balance_baseline(A1, S1, 1, &mut out);
    l.apply_external(
        &ext_tid(ExternalCause::Manual, Buy, BNB_S1, ext_qty, "600", 1),
        false,
        s(1),
    );
    l.clear_notes();
    l
}

#[test]
fn led_022_discount_fee_spills_to_baseline() {
    let mut l = baselined_bnb("5", "0.005");
    let c = cash(&l, A1, S1, BNB);
    assert_eq!(
        (c.baseline, c.unassigned, c.total),
        (qa(BNB, "5"), qa(BNB, "0.005"), qa(BNB, "5.005"))
    );
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0.01", BNB, G1, 0), s(10));
    let c = cash(&l, A1, S1, BNB);
    assert_eq!(
        (c.unassigned, c.baseline, c.total),
        (Qty(0), qa(BNB, "4.995"), qa(BNB, "4.995"))
    );
    assert!(c.bflags & bflags::NON_STRATEGY != 0);
    assert!(notes_of(&l, code::INV_SHORTFALL).is_empty());
}

#[test]
fn led_023_discount_fee_shortfall() {
    let mut l = baselined_bnb("0.003", "0.002");
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0.01", BNB, G1, 0), s(10));
    let c = cash(&l, A1, S1, BNB);
    assert_eq!(
        (c.baseline, c.unassigned, c.total),
        (Qty(0), qa(BNB, "-0.005"), qa(BNB, "-0.005"))
    );
    let ns = notes_of(&l, code::INV_SHORTFALL);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].level, NoteLevel::P1);
}

#[test]
fn led_024_fee_never_touches_other_group_inv() {
    let mut l = ledger_with(disc(), &cfg_lf());
    set_groups(&mut l, &[g4()]);
    boot_a1_bnb(&mut l, 0, "0");
    l.apply_fill(&f(Buy, BNB_S1, "1.0", "600", "0", USDT, G4, 0), s(1));
    assert_eq!(l.inventory(A1, S1, BNB, G4), qa(BNB, "1.0"));
    l.clear_notes();
    l.apply_fill(&f(Buy, BTC_S1, "1.0", "60000", "0.01", BNB, G1, 0), s(10));
    let c = cash(&l, A1, S1, BNB);
    assert_eq!(l.inventory(A1, S1, BNB, G4), qa(BNB, "1.0"));
    assert_eq!(c.unassigned, qa(BNB, "-0.01"));
    assert_eq!(notes_of(&l, code::INV_SHORTFALL).len(), 1);
    let inv_sum: i64 = l
        .venue_book(A1, S1)
        .inv
        .iter()
        .filter(|e| e.asset == BNB)
        .map(|e| e.qty.0)
        .sum();
    assert_eq!(c.total.0, c.baseline.0 + c.unassigned.0 + inv_sum);
    assert_eq!(c.total, qa(BNB, "0.99"));
}

#[test]
fn led_025_spot_sell_beyond_group_inv() {
    let mut l = ledger_lf();
    boot_a1_init(&mut l, 0, "0.2", "10");
    l.apply_fill(&f(Buy, BTC_S1, "0.5", "60000", "0", USDT, G1, 0), s(1));
    let c = cash(&l, A1, S1, BTC);
    assert_eq!(
        (l.inventory(A1, S1, BTC, G1), c.unassigned, c.total),
        (btc("0.5"), btc("0.2"), btc("0.7"))
    );
    l.clear_notes();
    l.apply_fill(&f(Sell, BTC_S1, "0.6", "60000", "0", USDT, G1, 0), s(10));
    let c = cash(&l, A1, S1, BTC);
    assert_eq!(
        (l.inventory(A1, S1, BTC, G1), c.unassigned, c.total),
        (Qty(0), btc("0.1"), btc("0.1"))
    );
    let ns = notes_of(&l, code::INV_SHORTFALL);
    assert_eq!(ns.len(), 1);
    assert_eq!(ns[0].level, NoteLevel::P1);
}
