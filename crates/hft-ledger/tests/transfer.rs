//! LED-185–LED-188: transfers and simulated-account reset.

use crate::common;

use common::*;
use hft_event::ExternalCause;
use hft_ledger::notes::code;
use hft_ledger::snapshot::{
    PositionRow, SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY, SECTION_POSITIONS, row_kind,
};
use hft_ledger::{Ledger, PnlBreakdown, RoundBook, TransferDelta, TransferKind};
use hft_refdata::{AccountKind, FundingSettleKind, SimBalance};
use hft_types::{AccountId, INST_NONE, Qty};
use zerocopy::FromBytes;

/// Every row of `bytes` that belongs to account `a` (sections 1, 13 and 22).
fn rows_of_account(bytes: &[u8], a: AccountId) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let s1 = section(bytes, SECTION_POSITIONS);
    for k in 0..s1.count as usize {
        let row = &s1.body[k * 64..k * 64 + 64];
        if PositionRow::read_from_bytes(row).map(|r| r.account) == Ok(a) {
            out.push(row.to_vec());
        }
    }
    for sec in [SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY] {
        for (h, b) in ledger_rows(&section(bytes, sec)) {
            if h.account == a {
                out.push(b);
            }
        }
    }
    out
}

fn sim_init() -> Vec<SimBalance> {
    match &rd_lf().account(A9).kind {
        AccountKind::Sim(c) => c.initial_balances.to_vec(),
        other => panic!("A9 is not Sim: {other:?}"),
    }
}

fn boot_a9(l: &mut Ledger, t: i64) {
    for v in [S1, P2] {
        l.on_balance(&bal(A9, v, USDT, usdt("10000"), Qty(0), D), true, t);
    }
    l.on_balance(&bal(A9, S1, BTC, Qty(0), Qty(0), D), true, t);
    l.on_balance(&bal(A9, S1, BNB, Qty(0), Qty(0), D), true, t);
}

#[test]
fn led_185_record_transfer() {
    let mut l = booted();
    let (eq0, a0, g0) = (l.equity(A1).total, l.account(A1).day, *l.group(A1, G1));
    l.apply_transfer(
        &TransferDelta {
            account: A1,
            from_venue: S1,
            to_venue: P2,
            asset: USDT,
            amount: usdt("1000"),
            kind: TransferKind::Record,
        },
        s(10),
    );
    assert_eq!(cash(&l, A1, S1, USDT).total, usdt("99000"));
    assert_eq!(cash(&l, A1, P2, USDT).total, usdt("51000"));
    assert_eq!(l.margin(A1, P2).wallet, n("51000"));
    assert_eq!((l.account(A1).day, *l.group(A1, G1)), (a0, g0));
    assert_eq!(l.equity(A1).total, eq0);
}

#[test]
fn led_186_sim_transfer() {
    let mut l = booted();
    boot_a9(&mut l, 0);
    let a1_rows = rows_of_account(&snap(&l), A1);
    let a9_day = l.account(A9).day;
    l.apply_transfer(
        &TransferDelta {
            account: A9,
            from_venue: S1,
            to_venue: P2,
            asset: USDT,
            amount: usdt("2500"),
            kind: TransferKind::Sim,
        },
        s(10),
    );
    assert_eq!(cash(&l, A9, S1, USDT).total, usdt("7500"));
    assert_eq!(cash(&l, A9, P2, USDT).total, usdt("12500"));
    assert_eq!(l.account(A9).day, a9_day);
    assert_eq!(rows_of_account(&snap(&l), A1), a1_rows);
}

const G9: hft_types::GroupId = gid(I9, 1);

/// LED-187's pre-state on a BalanceOnlyPush `P2` (so `A9` can hold a `FundSeen` entry).
fn pre_187() -> (Ledger, hft_event::ExternalFill) {
    let mut l = ledger_with(rd_funding(FundingSettleKind::BalanceOnlyPush), &cfg_lf());
    set_groups(&mut l, &[]);
    l.on_groups_changed(I9, &table(&[spec(G9, BTC, &[(BTC_S1, S1), (BTC_P2, P2)])]));
    boot_a1(&mut l, 0);
    boot_a9(&mut l, 0);
    l.apply_fill(&f(Buy, BTC_P2, "0.5", "60000", "0", USDT, G1, 1), s(1));
    l.apply_fill(&fa(A9, Buy, BTC_S1, "0.1", "60000", "0", USDT, G9, 0), s(2));
    l.apply_fill(&fa(A9, Sell, BTC_P2, "0.1", "60000", "0", USDT, G9, 1), s(2));
    let mut e = ext_tid(ExternalCause::Manual, Sell, BTC_P2, "0.01", "60000", 77);
    e.account = A9;
    assert!(l.apply_external(&e, false, s(3)).is_some());
    let auto = Ledger::auto_external_fill(A9, P2, BTC_P2, btc("1.0"), px("60000"), 4300);
    assert!(l.apply_external(&auto, true, s(4)).is_some());
    let mut p = push(INST_NONE, "1.0", T1);
    p.account = A9;
    l.apply_funding_push(&p, s(5));
    assert_eq!(l.ext_seen(A9, P2).len(), 2);
    assert_eq!(l.fund_seen(A9, P2).len(), 1);
    assert_eq!(l.auto_held(A9, BTC_P2, Sell), btc("1.0"));
    assert!(l.group(A9, G9).round.round_id != 0);
    l.clear_notes();
    (l, e)
}

fn after_187() -> (Ledger, hft_event::ExternalFill) {
    let (mut l, e) = pre_187();
    l.sim_reset(A9, &sim_init(), s(50));
    (l, e)
}

#[test]
fn led_187_sim_reset() {
    let (l0, _) = pre_187();
    let a1_rows = rows_of_account(&snap(&l0), A1);
    let (l, _) = after_187();
    for i in [BTC_S1, BTC_P2] {
        assert_eq!(*l.pos(A9, i), hft_ledger::InstPos::ZERO, "{i:?}");
    }
    assert!(l.venue_book(A9, S1).inv.is_empty());
    for e in l.venue_book(A9, S1).cash.iter() {
        assert_eq!((e.unassigned, e.baseline), (Qty(0), Qty(0)), "{:?}", e.asset);
    }
    let g = l.group(A9, G9);
    assert_eq!((g.pnl, g.day), (PnlBreakdown::zero(), PnlBreakdown::zero()));
    assert_eq!(g.round.legs, RoundBook::ZERO.legs);
    assert_eq!(g.round.pnl, PnlBreakdown::zero());
    assert_eq!(l.account(A9).day, PnlBreakdown::zero());
    assert_eq!(cash(&l, A9, S1, USDT).total, usdt("10000"));
    assert_eq!(cash(&l, A9, P2, USDT).total, usdt("10000"));
    let bytes = snap(&l);
    let n9 = ledger_rows(&section(&bytes, SECTION_LEDGER_GROUPS))
        .iter()
        .filter(|(h, _)| {
            h.account == A9
                && matches!(
                    h.kind,
                    row_kind::EXT_SEEN | row_kind::FUND_SEEN | row_kind::AUTO_HELD
                )
        })
        .count();
    assert_eq!(n9, 0);
    assert_eq!(notes_of(&l, code::SIM_RESET).len(), 1);
    assert_eq!(rows_of_account(&bytes, A1), a1_rows);
}

#[test]
fn led_188_reset_clears_dedup() {
    let (mut l, e) = after_187();
    assert!(l.apply_external(&e, false, s(60)).is_some());
}
