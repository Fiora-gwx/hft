//! LED-090–LED-104: funding booking by settle kind, synthesis, push pairing and dedup.

mod common;

use common::*;
use hft_event::{FundingSettle, NoteLevel};
use hft_ledger::notes::code;
use hft_ledger::snapshot::{FundSeenRow, LedgerRowHdr, SECTION_LEDGER_GROUPS, row_kind};
use hft_ledger::{FundingCheck, FundingPush, HFLAG_PUSHED, Ledger, PnlBreakdown, PnlDelta};
use hft_refdata::FundingSettleKind::{self, BalanceOnlyPush, PerSymbolPush, RateOnly};
use hft_types::{GROUP_NONE, INST_NONE, Notional, Px, Qty, Rate};
use zerocopy::IntoBytes;

/// `fund_a1(kind)`: `G1` perp leg −1.0 at 60 000, mark 60 000 at 1 s.
fn fund_a1(kind: FundingSettleKind) -> Ledger {
    fund_a1_cfg(kind, cfg_lf())
}

fn fund_a1_cfg(kind: FundingSettleKind, cfg: hft_ledger::LedgerConfig) -> Ledger {
    let mut l = ledger_with(rd_funding(kind), &cfg);
    boot_a1(&mut l, 0);
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    mark(&mut l, BTC_P2, "60000", s(1));
    l.clear_notes();
    l
}

fn synth(l: &Ledger, ts: i64) -> FundingSettle {
    l.synth_funding(A1, BTC_P2, px("60000"), rate("0.0001"), ts)
        .expect("synth")
}

fn wallet(l: &Ledger) -> Notional {
    l.margin(A1, P2).wallet
}
fn usdt_p2(l: &Ledger) -> Qty {
    cash(l, A1, P2, USDT).total
}

#[test]
fn led_090_per_symbol_push_booked() {
    let mut l = fund_a1(PerSymbolPush);
    let (w0, u0) = (wallet(&l), usdt_p2(&l));
    let r = l.apply_funding(&push(BTC_P2, "6.0", T1), s(10));
    let mut d = PnlBreakdown::zero();
    d.funding = n("6");
    assert_eq!(
        r,
        Some(PnlDelta {
            account: A1,
            group: G1,
            d,
        })
    );
    assert_eq!(l.group(A1, G1).pnl.funding, n("6"));
    assert_eq!(l.group(A1, G1).day.funding, n("6"));
    assert_eq!(l.account(A1).day.funding, n("6"));
    assert_eq!(usdt_p2(&l).0 - u0.0, usdt("6").0);
    assert_eq!(wallet(&l).0 - w0.0, n("6").0);
    let h = l.fund_hist(A1, P2);
    assert_eq!(h.len(), 1);
    assert_eq!(
        (
            h[0].inst,
            h[0].group,
            h[0].hflags,
            h[0].settle_ts_ns,
            h[0].booked,
            h[0].corrected
        ),
        (BTC_P2, G1, HFLAG_PUSHED, T1, n("6"), Notional(0))
    );
}

#[test]
fn led_091_per_symbol_push_dedup() {
    let base = || {
        let mut l = fund_a1(PerSymbolPush);
        l.apply_funding(&push(BTC_P2, "6.0", T1), s(10));
        l
    };
    let mut l = base();
    let (w, g) = (wallet(&l), l.group(A1, G1).pnl.funding);
    assert_eq!(
        l.apply_funding(&push(BTC_P2, "6.0", T1), s(11)),
        None,
        "LED-091.1"
    );
    assert_eq!((wallet(&l), l.group(A1, G1).pnl.funding), (w, g), "LED-091.1");
    let mut l = base();
    assert_eq!(
        l.apply_funding(&push(BTC_P2, "6.0", T1 + s(2)), s(11)),
        None,
        "LED-091.2"
    );
    let mut l = base();
    assert!(
        l.apply_funding(&push(BTC_P2, "5.0", T2), s(11)).is_some(),
        "LED-091.3"
    );
    let h = l.fund_hist(A1, P2);
    assert_eq!(h.len(), 2, "LED-091.3");
    assert_eq!(h[1].settle_ts_ns, T2, "LED-091.3");
    let mut l = base();
    assert!(
        l.apply_funding(&push(ETH_P2, "1.0", T1), s(11)).is_some(),
        "LED-091.4"
    );
}

#[test]
fn led_092_rate_only_synth_booked() {
    let mut l = fund_a1(RateOnly);
    let x = synth(&l, T1 + 300_000_000);
    assert_eq!(
        (x.inst, x.amount, x.rate, x.pos_qty, x.settle_ts_ns, x.synth),
        (BTC_P2, n("6"), rate("0.0001"), btc("-1.0"), T1, 1)
    );
    let (w0, u0) = (wallet(&l), usdt_p2(&l));
    l.apply_funding(&x, s(10));
    assert_eq!(l.group(A1, G1).pnl.funding, n("6"));
    assert_eq!(usdt_p2(&l).0 - u0.0, usdt("6").0);
    assert_eq!(wallet(&l).0 - w0.0, n("6").0);
    let h = l.fund_hist(A1, P2);
    assert_eq!((h[0].booked, h[0].hflags & HFLAG_PUSHED), (n("6"), 0));
}

/// RateOnly ledger with `BTC/P2` position `q` (built by one fill).
fn rate_only_pos(q: &str) -> Ledger {
    let mut l = booted_with(rd_funding(RateOnly));
    if q != "0" {
        let side = if q.starts_with('-') { Sell } else { Buy };
        let mut fl = f(side, BTC_P2, q.trim_start_matches('-'), "60000", "0", USDT, G1, 1);
        fl.qty = Qty(fl.qty.0.abs());
        fl.net_qty = fl.qty;
        l.apply_fill(&fl, s(1));
    }
    l
}

#[test]
fn led_093_synth_rounding() {
    let m = Px(600_000_001);
    let r = Rate(123_457);
    let l = rate_only_pos("-0.001");
    assert_eq!(
        l.synth_funding(A1, BTC_P2, m, r, T1).map(|x| x.amount),
        Some(Notional(7_407)),
        "LED-093.1"
    );
    let l = rate_only_pos("0.001");
    assert_eq!(
        l.synth_funding(A1, BTC_P2, m, r, T1).map(|x| x.amount),
        Some(Notional(-7_408)),
        "LED-093.2"
    );
}

#[test]
fn led_094_synth_zero_position() {
    let m = px("60000");
    let r = rate("0.0001");
    let l = rate_only_pos("0");
    assert_eq!(l.synth_funding(A1, BTC_P2, m, r, T1), None, "LED-094.1");
    let l = rate_only_pos("-0.00000001");
    assert_eq!(
        l.synth_funding(A1, BTC_P2, m, r, T1).map(|x| x.amount),
        Some(Notional(0)),
        "LED-094.2"
    );
    let l = rate_only_pos("0.00000001");
    assert_eq!(
        l.synth_funding(A1, BTC_P2, m, r, T1).map(|x| x.amount),
        Some(Notional(-1)),
        "LED-094.3"
    );
}

fn after_095() -> Ledger {
    let mut l = fund_a1(BalanceOnlyPush);
    let x = synth(&l, T1);
    l.apply_funding(&x, s(10));
    l
}

#[test]
fn led_095_balance_only_synth_no_wallet() {
    let l0 = fund_a1(BalanceOnlyPush);
    let (w0, u0) = (wallet(&l0), usdt_p2(&l0));
    let l = after_095();
    assert_eq!(l.group(A1, G1).pnl.funding, n("6"));
    assert_eq!((wallet(&l), usdt_p2(&l)), (w0, u0));
    assert_eq!(l.account(A1).funding_residual, n("-6"));
    assert_eq!(l.fund_pending(A1, P2), [(T1, n("6"), Notional(0), 1)]);
}

fn after_096() -> (Ledger, FundingPush) {
    let mut l = after_095();
    let r = l.apply_funding_push(&push(INST_NONE, "6.04", T1 + 1_500_000_000), s(11));
    (l, r)
}

fn wallet_rec(l: &Ledger, ts: i64) -> Option<hft_ledger::FundHistView> {
    l.fund_hist(A1, P2)
        .into_iter()
        .find(|h| h.inst == INST_NONE && h.settle_ts_ns == ts)
}

#[test]
fn led_096_balance_only_push_pairs() {
    let l0 = after_095();
    let (w0, u0) = (wallet(&l0), usdt_p2(&l0));
    let (l, r) = after_096();
    assert_eq!(
        r,
        FundingPush::Booked {
            check: FundingCheck::Ok
        }
    );
    assert_eq!(wallet(&l).0 - w0.0, n("6.04").0);
    assert_eq!(usdt_p2(&l).0 - u0.0, usdt("6.04").0);
    assert_eq!(wallet_rec(&l, T1).map(|h| h.booked), Some(n("6.04")));
    assert_eq!(l.account(A1).funding_residual, n("0.04"));
    assert!(l.fund_pending(A1, P2).is_empty());
}

#[test]
fn led_097_push_synth_order_independent() {
    // .1 synth → push
    let mut a = fund_a1(BalanceOnlyPush);
    let w0 = wallet(&a);
    let x = synth(&a, T1);
    a.apply_funding(&x, s(10));
    let r = a.apply_funding_push(&push(INST_NONE, "6.04", T1), s(11));
    assert_eq!(
        r,
        FundingPush::Booked {
            check: FundingCheck::Ok
        },
        "LED-097.1"
    );
    assert_eq!(a.account(A1).funding_residual, n("0.04"), "LED-097.1");
    assert_eq!(wallet(&a).0 - w0.0, n("6.04").0, "LED-097.1");
    assert_eq!(a.group(A1, G1).pnl.funding, n("6"), "LED-097.1");
    // .2 push → synth
    let mut b = fund_a1(BalanceOnlyPush);
    let r = b.apply_funding_push(&push(INST_NONE, "6.04", T1), s(10));
    assert_eq!(
        r,
        FundingPush::Booked {
            check: FundingCheck::Ok
        },
        "LED-097.2"
    );
    b.apply_funding(&x, s(11));
    assert_eq!(b.account(A1).funding_residual, n("0.04"), "LED-097.2");
    assert_eq!(wallet(&b).0 - w0.0, n("6.04").0, "LED-097.2");
    assert_eq!(b.group(A1, G1).pnl.funding, n("6"), "LED-097.2");
    assert!(notes_of(&b, code::FUNDING_MISMATCH).is_empty(), "LED-097.2");
    // .3
    assert_eq!(snap(&a), snap(&b), "LED-097.3");
}

#[test]
fn led_098_mismatch_tolerance() {
    let synth_s = |amount: &str| fs(BTC_P2, amount, T1, 1);
    let rows = [
        ("6.00", "6.05", FundingCheck::Ok),
        (
            "6.00",
            "6.050001",
            FundingCheck::Mismatch {
                residual: n("0.050001"),
            },
        ),
        ("19.9995", "20.10", FundingCheck::Ok),
        (
            "19.999499",
            "20.10",
            FundingCheck::Mismatch {
                residual: n("0.100501"),
            },
        ),
    ];
    for (k, (sv, pv, want)) in rows.iter().enumerate() {
        let mut l = fund_a1(BalanceOnlyPush);
        l.apply_funding(&synth_s(sv), s(10));
        let r = l.apply_funding_push(&push(INST_NONE, pv, T1), s(11));
        assert_eq!(r, FundingPush::Booked { check: *want }, "LED-098.{}", k + 1);
    }
    // .5 push first, synth second: the ledger writes the mismatch note itself.
    let mut l = fund_a1(BalanceOnlyPush);
    let r = l.apply_funding_push(&push(INST_NONE, "6.10", T1), s(10));
    assert_eq!(
        r,
        FundingPush::Booked {
            check: FundingCheck::Ok
        },
        "LED-098.5"
    );
    l.apply_funding(&synth_s("6.00"), s(11));
    let ns = notes_of(&l, code::FUNDING_MISMATCH);
    assert_eq!(ns.len(), 1, "LED-098.5");
    assert_eq!(ns[0].level, NoteLevel::P1, "LED-098.5");
}

#[test]
fn led_099_push_replay_duplicate() {
    let (mut l, _) = after_096();
    let before = (
        wallet(&l),
        usdt_p2(&l),
        wallet_rec(&l, T1).map(|h| h.booked),
        l.account(A1).funding_residual,
    );
    assert_eq!(
        l.apply_funding_push(&push(INST_NONE, "6.04", T1), s(12)),
        FundingPush::Duplicate
    );
    assert_eq!(
        (
            wallet(&l),
            usdt_p2(&l),
            wallet_rec(&l, T1).map(|h| h.booked),
            l.account(A1).funding_residual
        ),
        before
    );
    assert_eq!(before.2, Some(n("6.04")));
}

#[test]
fn led_100_push_same_ts_other_amount() {
    let (mut l, _) = after_096();
    let w0 = wallet(&l);
    let r = l.apply_funding_push(&push(INST_NONE, "0.5", T1), s(12));
    assert!(matches!(r, FundingPush::Booked { .. }));
    assert_eq!(wallet_rec(&l, T1).map(|h| h.booked), Some(n("6.54")));
    assert_eq!(wallet(&l).0 - w0.0, n("0.5").0);
}

#[test]
fn led_101_push_seen_capacity() {
    let mut cfg = cfg_lf();
    cfg.funding_push_seen_cap = 2;
    let mut l = fund_a1_cfg(BalanceOnlyPush, cfg);
    let p = |a: &str| push(INST_NONE, a, T1);
    assert!(matches!(
        l.apply_funding_push(&p("1.0"), s(10)),
        FundingPush::Booked { .. }
    ));
    assert!(matches!(
        l.apply_funding_push(&p("2.0"), s(11)),
        FundingPush::Booked { .. }
    ));
    assert_eq!(
        l.apply_funding_push(&p("1.0"), s(12)),
        FundingPush::Duplicate,
        "LED-101.1"
    );
    assert!(
        matches!(l.apply_funding_push(&p("3.0"), s(13)), FundingPush::Booked { .. }),
        "LED-101.2"
    );
    assert_eq!(l.fund_seen(A1, P2), [(T1, n("2")), (T1, n("3"))], "LED-101.2");
    let b0 = wallet_rec(&l, T1).map(|h| h.booked.0).unwrap_or(0);
    assert!(
        matches!(l.apply_funding_push(&p("1.0"), s(14)), FundingPush::Booked { .. }),
        "LED-101.3"
    );
    assert_eq!(
        wallet_rec(&l, T1).map(|h| h.booked.0 - b0),
        Some(n("1").0),
        "LED-101.3"
    );
}

#[test]
fn led_102_pairing_table_capacity() {
    let mut l = fund_a1(BalanceOnlyPush);
    for k in 1..=17 {
        let x = synth(&l, D + k * 8 * H);
        l.apply_funding(&x, s(10 + k));
    }
    let p = l.fund_pending(A1, P2);
    assert_eq!(p.len(), 16);
    let ts: Vec<i64> = p.iter().map(|x| x.0).collect();
    assert_eq!(ts, (2..=17).map(|k| D + k * 8 * H).collect::<Vec<_>>());
    assert_eq!(l.account(A1).funding_residual, n("-102"), "k = 1 still counted");
    let w0 = wallet(&l);
    let r = l.apply_funding_push(&push(INST_NONE, "6.0", D + 8 * H), s(30));
    assert_eq!(
        r,
        FundingPush::Booked {
            check: FundingCheck::Ok
        }
    );
    assert_eq!(wallet(&l).0 - w0.0, n("6").0);
    assert_eq!(l.fund_pending(A1, P2).len(), 16);
}

#[test]
fn led_103_funding_account_level() {
    let mut l = booted();
    boot_a2(&mut l, 0, "10000", "10000");
    let mut e = ext(hft_event::ExternalCause::Liquidation, Sell, ETH_P2, "0.5", "3000");
    e.account = A2;
    l.apply_external(&e, false, s(1));
    assert_eq!(l.pos(A2, ETH_P2).qty, qa(ETH, "-0.5"));
    let w0 = l.margin(A2, P2).wallet;
    let mut x = fs(ETH_P2, "1.5", T1, 0);
    x.account = A2;
    let r = l.apply_funding(&x, s(10)).expect("Some");
    assert_eq!((r.account, r.group, r.d.funding), (A2, GROUP_NONE, n("1.5")));
    assert_eq!(l.account(A2).day.funding, n("1.5"));
    assert_eq!(l.margin(A2, P2).wallet.0 - w0.0, n("1.5").0);
    assert_eq!(l.fund_hist(A2, P2)[0].group, GROUP_NONE);
}

#[test]
fn led_104_push_seen_survives_restore() {
    let (l, _) = after_096();
    let bytes = snap(&l);
    let body = section(&bytes, SECTION_LEDGER_GROUPS).body;
    let mut want = LedgerRowHdr {
        kind: row_kind::FUND_SEEN,
        account: A1,
        venue: P2,
        _pad: 0,
        id: 0,
    }
    .as_bytes()
    .to_vec();
    want.extend_from_slice(
        FundSeenRow {
            settle_ts_ns: T1,
            amount: n("6.04"),
        }
        .as_bytes(),
    );
    assert!(body.windows(want.len()).any(|w| w == want.as_slice()));
    let hist_hdr = LedgerRowHdr {
        kind: row_kind::FUND_HIST,
        account: A1,
        venue: P2,
        _pad: 0,
        id: INST_NONE.0,
    };
    assert!(body.windows(8).any(|w| w == hist_hdr.as_bytes()));
    let mut r = restore_with(&bytes, rd_funding(BalanceOnlyPush)).expect("restore");
    assert_eq!(
        r.apply_funding_push(&push(INST_NONE, "6.04", T1), s(20)),
        FundingPush::Duplicate
    );
}
