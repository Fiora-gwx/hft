//! LED-040–LED-046: balance initialization, drift check against dust, baselines.

mod common;

use common::*;
use hft_ledger::notes::code;
use hft_ledger::{BalanceVerdict, BaselineResult, FixedVec, Ledger, bflags};
use hft_types::{AssetId, Qty};

fn fresh() -> Ledger {
    let mut l = ledger_lf();
    l.on_run_start(0);
    l
}

#[test]
fn led_040_first_report_initializes() {
    let mut l = fresh();
    let v1 = l.on_balance(&bal(A1, S1, USDT, usdt("90000"), usdt("10000"), D), false, s(1));
    let v2 = l.on_balance(&bal(A1, S1, BNB, qa(BNB, "10"), Qty(0), D), false, s(1));
    assert_eq!(
        (v1, v2),
        (BalanceVerdict::Initialized, BalanceVerdict::Initialized)
    );
    assert_eq!(notes_of(&l, code::BALANCE_INIT).len(), 2);
    let u = cash(&l, A1, S1, USDT);
    assert_eq!((u.total, u.unassigned), (usdt("100000"), Qty(0)));
    assert_eq!(u.bflags & bflags::UNINIT, 0);
    let b = cash(&l, A1, S1, BNB);
    assert_eq!(
        (b.total, b.unassigned, b.baseline),
        (qa(BNB, "10"), qa(BNB, "10"), Qty(0))
    );
}

#[test]
fn led_041_equity_valid_after_all_venues() {
    let mut l = fresh();
    for (a, free, locked) in [
        (USDT, usdt("90000"), usdt("10000")),
        (BNB, qa(BNB, "10"), Qty(0)),
        (BTC, Qty(0), Qty(0)),
    ] {
        l.on_balance(&bal(A1, S1, a, free, locked, D), false, s(1));
    }
    l.on_ref_px(BTC_S1, px("59999.99"), px("60000.01"), s(1));
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), s(1));
    l.on_ref_px(USDC_S1, px("0.9999"), px("1.0001"), s(1));
    assert!(!l.equity(A1).valid, "step 1: P2 not initialized");
    l.on_balance(&bal(A1, P2, USDT, usdt("50000"), Qty(0), D), false, s(2));
    assert!(l.equity(A1).valid, "step 2");
}

#[test]
fn led_042_not_quiet_no_check() {
    let mut l = booted();
    let v = l.on_balance(&bal(A1, S1, USDT, usdt("70000"), Qty(0), D + s(5)), false, s(5));
    assert_eq!(v, BalanceVerdict::Reported);
    let u = cash(&l, A1, S1, USDT);
    assert_eq!(
        (u.rep_free, u.rep_locked, u.rep_ens, u.total),
        (usdt("70000"), Qty(0), s(5), usdt("100000"))
    );
}

#[test]
fn led_043_drift_dust_boundary() {
    let rows = [
        ("100000.009999", BalanceVerdict::Ok),
        (
            "100000.01",
            BalanceVerdict::Drift {
                asset: USDT,
                diff: usdt("0.01"),
            },
        ),
        (
            "100000.010001",
            BalanceVerdict::Drift {
                asset: USDT,
                diff: usdt("0.010001"),
            },
        ),
        (
            "99999.99",
            BalanceVerdict::Drift {
                asset: USDT,
                diff: usdt("-0.01"),
            },
        ),
    ];
    for (k, (rep, want)) in rows.iter().enumerate() {
        let mut l = booted();
        let v = l.on_balance(&bal(A1, S1, USDT, usdt(rep), Qty(0), D + s(5)), true, s(5));
        assert_eq!(v, *want, "LED-043.{}", k + 1);
        assert_eq!(
            cash(&l, A1, S1, USDT).total,
            usdt("100000"),
            "LED-043.5 after row {}",
            k + 1
        );
    }
}

fn baseline(
    l: &mut Ledger,
    a: hft_types::AccountId,
    v: hft_types::VenueId,
    seq: u64,
) -> (BaselineResult, Vec<AssetId>) {
    let mut out = FixedVec::new();
    let r = l.set_balance_baseline(a, v, seq, &mut out);
    (r, out.iter().copied().collect())
}

#[test]
fn led_044_set_baseline() {
    let mut l = booted();
    l.on_balance(&bal(A1, S1, BNB, qa(BNB, "10.5"), Qty(0), D + s(5)), false, s(5));
    l.clear_notes();
    let (r, out) = baseline(&mut l, A1, S1, 77);
    assert_eq!(
        r,
        BaselineResult::Set {
            n_assets: 1,
            notional: n("6300"),
        }
    );
    assert_eq!(out, [BNB]);
    let b = cash(&l, A1, S1, BNB);
    assert_eq!(
        (b.baseline, b.unassigned, b.total),
        (qa(BNB, "10.5"), Qty(0), qa(BNB, "10.5"))
    );
    assert!(b.bflags & bflags::NON_STRATEGY != 0);
    assert_eq!(l.account(A1).day.adjust, n("300"));
    let ns = notes_of(&l, code::BASELINE_SET);
    assert_eq!(ns.len(), 1);
    assert_eq!((ns[0].v[0], ns[0].v[1]), (1, 6_300_000_000));
}

#[test]
fn led_045_baseline_skips_quote_and_group_assets() {
    let mut l = ledger_lf();
    boot_a1_init(&mut l, 0, "0.05", "10");
    l.apply_fill(&f(Buy, BTC_S1, "0.1", "60000", "0", USDT, G1, 0), s(1));
    l.on_balance(&bal(A1, S1, BNB, qa(BNB, "10.5"), Qty(0), D + s(5)), false, s(5));
    l.on_balance(&bal(A1, S1, USDT, usdt("100500"), Qty(0), D + s(5)), false, s(5));
    l.on_balance(&bal(A1, S1, BTC, btc("0.2"), Qty(0), D + s(5)), false, s(5));
    let usdt0 = cash(&l, A1, S1, USDT).total;
    let (_, out) = baseline(&mut l, A1, S1, 77);
    assert_eq!(out, [BNB]);
    assert_eq!(cash(&l, A1, S1, USDT).total, usdt0);
    let b = cash(&l, A1, S1, BTC);
    assert_eq!((b.baseline, b.unassigned), (Qty(0), btc("0.05")));
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("0.1"));
}

#[test]
fn led_046_baseline_needs_balance_view() {
    let mut l = booted();
    l.clear_notes();
    let before = snap(&l);
    let (r, out) = baseline(&mut l, A2, S3, 78);
    assert_eq!(r, BaselineResult::NoBalanceView);
    assert!(out.is_empty());
    assert_eq!(snap(&l), before);
    assert!(notes_of(&l, code::BASELINE_SET).is_empty());
}
