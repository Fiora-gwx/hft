//! Funding synthesis (TESTS.md "资金费合成": SEQ-192–SEQ-196, SEQ-361).

use hft_refdata::FundingSettleKind;
use hft_types::{Side, Tsc};

use crate::common::*;

const H8: i64 = 8 * 3_600_000_000_000;

/// 08:00 of the B0 day (UTC0 is 00:30).
fn t0800() -> i64 {
    UTC0 - 30 * 60_000_000_000 + H8
}

fn rate_only() -> H {
    reset_script();
    let mut b = b0();
    b.rd.caps[1].funding_settle = FundingSettleKind::RateOnly;
    let mut h = b.boot();
    h.steady();
    h
}

#[test]
fn seq_192_next_funding_settle() {
    // .3 PerSymbolPush: never synthesized.
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.funding_md(B1, "0.0001", t0800(), 28_800);
    assert_eq!(h.s.view().next_funding_settle(), None);
    // RateOnly with a position: (B, 08:00).
    let mut h = rate_only();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.funding_md(B1, "0.0001", t0800(), 28_800);
    assert_eq!(h.s.view().next_funding_settle(), Some((B, t0800())));
}

#[test]
fn seq_193_timer_triggered_synthesis() {
    let mut h = rate_only();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.open_position(AC2, B1, Side::Buy, "0.020");
    h.funding_md(B1, "0.0001", t0800(), 28_800);
    h.mark_md(B1, "50000");
    h.trace_on();
    let m = h.mark();
    h.at(T0 + (t0800() - UTC0) as u64);
    h.sys(15 | (u32::from(B.0) << 8));
    let s = settles(&h.since(m));
    assert_eq!(s.len(), 2, "{s:?}");
    assert_eq!(s[0].account, AC1);
    assert_eq!(s[1].account, AC2);
    assert!(s.iter().all(|x| x.synth == 1 && x.settle_ts_ns == t0800() && x.rx_tsc == Tsc(0)));
    assert_eq!(s[0].amount.0.abs(), 50_000);
    assert_eq!(s[1].amount.0.abs(), 100_000);
    let tr = h.trace();
    assert_eq!(tr.iter().filter(|x| **x == "ledger.synth_funding").count(), 2);
    assert_eq!(h.s.engine().market.row(B1).unwrap().settled_cut_ns, t0800());
}

#[test]
fn seq_194_synthesis_once() {
    // .1 Timer first, then Funding pushing the next time: synthesized once.
    let mut h = rate_only();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.funding_md(B1, "0.0001", t0800(), 28_800);
    h.mark_md(B1, "50000");
    let m = h.mark();
    h.at(T0 + (t0800() - UTC0) as u64);
    h.sys(15 | (u32::from(B.0) << 8));
    let r0 = *h.s.engine().market.row(B1).unwrap();
    h.funding_md(B1, "0.0001", t0800() + H8, 28_800);
    let st = settles(&h.since(m));
    assert_eq!(st.len(), 1, "{st:?} cut {} next {}", r0.settled_cut_ns, r0.next_funding_ns);
    // .2 Funding first (past the unsettled 08:00): synthesized there; the Timer adds nothing.
    let mut h = rate_only();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.funding_md(B1, "0.0001", t0800(), 28_800);
    h.mark_md(B1, "50000");
    let m = h.mark();
    h.funding_md(B1, "0.0001", t0800() + H8, 28_800);
    assert_eq!(settles(&h.since(m)).len(), 1);
    h.at(T0 + (t0800() - UTC0) as u64);
    h.sys(15 | (u32::from(B.0) << 8));
    assert_eq!(settles(&h.since(m)).len(), 1);
    // .3 Funding not past 08:00: nothing.
    let m = h.mark();
    h.funding_md(B1, "0.0002", t0800() + H8, 28_800);
    assert_eq!(settles(&h.since(m)).len(), 0);
}

#[test]
fn seq_195_per_symbol_push_not_synthesized() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.funding_md(B1, "0.0001", t0800(), 28_800);
    let m = h.mark();
    h.funding_md(B1, "0.0001", t0800() + H8, 28_800);
    assert!(settles(&h.since(m)).is_empty());
}

#[test]
fn seq_361_synth_rx_tsc_zero() {
    let mut h = rate_only();
    h.open_position(AC1, B1, Side::Sell, "0.010");
    h.funding_md(B1, "0.0001", t0800(), 28_800);
    h.mark_md(B1, "50000");
    let m = h.mark();
    h.adv(5_000);
    h.at(T0 + (t0800() - UTC0) as u64);
    h.sys(15 | (u32::from(B.0) << 8));
    let s = settles(&h.since(m));
    assert!(!s.is_empty());
    assert!(s.iter().all(|x| x.rx_tsc == Tsc(0)));
    let v = h.verify();
    assert!(v.is_empty(), "{v:?}");
}
