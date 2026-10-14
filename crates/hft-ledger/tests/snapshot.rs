//! LED-230, LED-232, LED-235–LED-237, LED-247: section order, row order, versions, i128 words,
//! engine-time fields.

use crate::common;

use common::*;
use hft_event::ExternalCause;
use hft_ledger::snapshot::{
    FundHistRow, LEDGER_GROUPS_VERSION, LEDGER_INVENTORY_VERSION, POSITIONS_VERSION, PositionRow,
    SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY, SECTION_POSITIONS, row_kind,
};
use hft_ledger::{FundingReconItem, Ledger};
use hft_refdata::FundingSettleKind;
use hft_strategy_sdk::{RestoreError, SnapshotReader};
use hft_types::{AccountId, INST_NONE, Qty};
use zerocopy::FromBytes;

const G9: hft_types::GroupId = gid(I9, 1);

/// Activity of account `a` (group `g`) touching every row kind.
fn activity(l: &mut Ledger, a: AccountId, g: hft_types::GroupId, base_tid: u64) {
    l.apply_fill(&fa(a, Buy, BTC_S1, "0.1", "60000", "0", USDT, g, 0), s(2));
    l.apply_fill(&fa(a, Sell, BTC_P2, "0.1", "60000", "0", USDT, g, 1), s(2));
    for k in 0..2 {
        let mut e = ext_tid(ExternalCause::Manual, Sell, BTC_P2, "0.01", "60000", base_tid + k);
        e.account = a;
        l.apply_external(&e, false, s(3));
    }
    let auto = Ledger::auto_external_fill(a, P2, BTC_P2, btc("0.5"), px("60000"), base_tid + 100);
    l.apply_external(&auto, true, s(4));
    // Pending pair (synth only), a pushed wallet record and a no-history item.
    if let Some(mut x) = l.synth_funding(a, BTC_P2, px("60000"), rate("0.0001"), T2) {
        x.account = a;
        l.apply_funding(&x, s(5));
    }
    let mut p = push(INST_NONE, "1.0", T1);
    p.account = a;
    l.apply_funding_push(&p, s(6));
    let it = FundingReconItem {
        account: a,
        venue: P2,
        inst: INST_NONE,
        settle_ts_ns: D - 8 * H,
        exch_amount: n("1"),
        engine_amount: n("0"),
        report_id: 1,
    };
    l.reconcile_funding(&it, D + H, s(7));
}

fn rich() -> Ledger {
    let mut l = ledger_with(rd_funding(FundingSettleKind::BalanceOnlyPush), &cfg_lf());
    l.on_groups_changed(I9, &table(&[spec(G9, BTC, &[(BTC_S1, S1), (BTC_P2, P2)])]));
    boot_a1(&mut l, 0);
    for v in [S1, P2] {
        l.on_balance(&bal(A9, v, USDT, usdt("10000"), Qty(0), D), true, 0);
    }
    l.on_balance(&bal(A9, S1, BTC, Qty(0), Qty(0), D), true, 0);
    activity(&mut l, A1, G1, 1_000);
    activity(&mut l, A9, G9, 2_000);
    let items = margin_items(&[(BTC_P2, px("80000"), 2)]);
    on_margin(
        &mut l,
        margin_hdr(A1, P2, "50000", "300", "40000", "0.006", 1),
        &items,
        s(8),
    );
    on_margin(
        &mut l,
        margin_hdr(A9, P2, "10000", "30", "9000", "0.003", 1),
        &items,
        s(8),
    );
    l
}

/// Ordering key of a row: (account, venue, id, settle time, inventory group).
type RowKey = (u8, u8, u32, i64, u32);

fn rank(kind: u8) -> u8 {
    match kind {
        row_kind::ACCOUNT => 0,
        row_kind::RATE => 1,
        row_kind::PX => 2,
        row_kind::GROUP | row_kind::PENDING => 3,
        row_kind::MARGIN => 4,
        row_kind::FUND_PENDING => 5,
        row_kind::FUND_MARK | row_kind::FUND_HIST => 6,
        row_kind::FUND_NO_HIST => 7,
        row_kind::FUND_SEEN => 8,
        row_kind::EXT_SEEN => 9,
        row_kind::AUTO_HELD => 10,
        row_kind::CASH => 11,
        row_kind::INV => 12,
        k => panic!("kind {k}"),
    }
}

#[test]
fn led_230_section_and_row_order() {
    let l = rich();
    let bytes = snap(&l);
    // Sections 1, 13, 22, each exactly once.
    let mut r = SnapshotReader::new(&bytes);
    let mut order = Vec::new();
    while let Some(s) = r.next_section().expect("framing") {
        order.push(s.section);
    }
    assert_eq!(
        order,
        [SECTION_POSITIONS, SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY]
    );
    // Section 1 by (account, inst).
    let s1 = section(&bytes, SECTION_POSITIONS);
    let keys: Vec<(u8, u32)> = (0..s1.count as usize)
        .map(|k| PositionRow::read_from_bytes(&s1.body[k * 64..k * 64 + 64]).expect("row"))
        .map(|r| (r.account.0, r.inst.0))
        .collect();
    assert!(keys.windows(2).all(|w| w[0] < w[1]), "{keys:?}");
    assert!(keys.iter().any(|k| k.0 == A9.0));
    // Section 13: kind ranks non-decreasing; keys ascending within a kind (FundSeen/ExtSeen
    // in booking order, FundHist by (inst, settle_ts) inside its FundMark block).
    for sec in [SECTION_LEDGER_GROUPS, SECTION_LEDGER_INVENTORY] {
        let rows = ledger_rows(&section(&bytes, sec));
        let ranks: Vec<u8> = rows.iter().map(|(h, _)| rank(h.kind)).collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "section {sec}: {ranks:?}");
        let mut prev: Option<(u8, RowKey)> = None;
        for (h, b) in &rows {
            if matches!(h.kind, row_kind::FUND_SEEN | row_kind::EXT_SEEN) {
                prev = None;
                continue;
            }
            let extra = match h.kind {
                row_kind::FUND_HIST => FundHistRow::read_from_bytes(b).expect("hist").settle_ts_ns,
                row_kind::FUND_PENDING | row_kind::FUND_NO_HIST => {
                    i64::from_le_bytes(b[..8].try_into().expect("ts"))
                }
                _ => 0,
            };
            let grp = if h.kind == row_kind::INV {
                u32::from_le_bytes(b[..4].try_into().expect("g"))
            } else {
                0
            };
            let key = (h.account.0, h.venue.0, h.id, extra, grp);
            let kind = if h.kind == row_kind::FUND_MARK {
                row_kind::FUND_HIST
            } else {
                h.kind
            };
            if h.kind == row_kind::FUND_MARK {
                prev = None;
                continue;
            }
            if let Some((pk, pkey)) = prev
                && pk == kind
            {
                assert!(pkey < key, "section {sec} kind {kind}: {pkey:?} !< {key:?}");
            }
            prev = Some((kind, key));
        }
        for kind in if sec == SECTION_LEDGER_GROUPS {
            vec![
                row_kind::ACCOUNT,
                row_kind::RATE,
                row_kind::GROUP,
                row_kind::MARGIN,
                row_kind::FUND_PENDING,
                row_kind::FUND_MARK,
                row_kind::FUND_HIST,
                row_kind::FUND_NO_HIST,
                row_kind::FUND_SEEN,
                row_kind::EXT_SEEN,
                row_kind::AUTO_HELD,
            ]
        } else {
            vec![row_kind::CASH, row_kind::INV]
        } {
            for a in [A1, A9] {
                if kind == row_kind::RATE {
                    continue;
                }
                assert!(
                    rows.iter().any(|(h, _)| h.kind == kind && h.account == a),
                    "section {sec}: no kind {kind} row for {a:?}"
                );
            }
        }
    }
}

#[test]
fn led_232_positions_only_nonzero() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(1));
    l.apply_fill(&f(Sell, BTC_P2, "1.0", "60000", "0", USDT, G1, 1), s(2));
    l.apply_fill(&f(Buy, BTC_S1, "0.1", "60000", "0", USDT, G1, 0), s(3));
    // A position that is untouched since boot is not written. The flat BTC/P2 keeps its row:
    // its last fill time feeds the BalanceReport quiet test and must survive a restore.
    let bytes = snap(&l);
    let s1 = section(&bytes, SECTION_POSITIONS);
    assert_eq!(s1.count, 2);
    let rows: Vec<PositionRow> = (0..2)
        .map(|k| PositionRow::read_from_bytes(&s1.body[k * 64..k * 64 + 64]).expect("row"))
        .collect();
    let keys: Vec<_> = rows.iter().map(|r| (r.account, r.inst)).collect();
    assert!(
        keys.contains(&(A1, BTC_S1)) && keys.contains(&(A1, BTC_P2)),
        "{keys:?}"
    );
    let flat = rows.iter().find(|r| r.inst == BTC_P2).expect("flat row");
    assert_eq!(flat.qty, Qty(0));
    assert_eq!(flat.last_fill_ens, l.pos(A1, BTC_P2).last_fill_ens);
    assert_ne!(flat.last_fill_ens, 0);
    let r = restore(&bytes).expect("restore");
    assert_eq!(r.pos(A1, BTC_P2), l.pos(A1, BTC_P2));
    assert_eq!(snap(&r), bytes);
}

#[test]
fn led_235_row_versions() {
    let l = rich();
    let bytes = snap(&l);
    assert_eq!(
        [
            section(&bytes, SECTION_POSITIONS).version,
            section(&bytes, SECTION_LEDGER_GROUPS).version,
            section(&bytes, SECTION_LEDGER_INVENTORY).version
        ],
        [POSITIONS_VERSION, LEDGER_GROUPS_VERSION, LEDGER_INVENTORY_VERSION],
        "LED-235.1"
    );
    assert_eq!(
        [POSITIONS_VERSION, LEDGER_GROUPS_VERSION, LEDGER_INVENTORY_VERSION],
        [2, 8, 2]
    );
    for (k, (sec, v)) in [(SECTION_LEDGER_GROUPS, 10u16), (SECTION_POSITIONS, 3)]
        .iter()
        .enumerate()
    {
        let mut b = bytes.clone();
        let at = section_at(&b, *sec) + 2;
        b[at..at + 2].copy_from_slice(&v.to_le_bytes());
        let r = restore_with(&b, rd_funding(FundingSettleKind::BalanceOnlyPush));
        assert_eq!(
            r.err(),
            Some(RestoreError::VersionUnsupported),
            "LED-235.{}",
            k + 2
        );
    }
    // Version 9 is valid only when the snapshot actually contains pending receipts.
    let mut bad = bytes.clone();
    let at = section_at(&bad, SECTION_LEDGER_GROUPS) + 2;
    bad[at..at + 2].copy_from_slice(&9u16.to_le_bytes());
    assert_eq!(
        restore_with(&bad, rd_funding(FundingSettleKind::BalanceOnlyPush)).err(),
        Some(RestoreError::BadValue),
        "LED-235.4"
    );
}

#[test]
fn led_236_i128_little_endian() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_P2, "0.01", "60000.0001", "0", USDT, G1, 1), s(10));
    l.apply_fill(&f(Buy, BTC_P2, "0.02", "60000.0000", "0", USDT, G1, 1), s(10));
    l.apply_fill(&f(Sell, BTC_P2, "0.01", "60000.0000", "0", USDT, G1, 1), s(11));
    l.apply_fill(&f(Sell, BTC_P2, "0.02", "60000.0000", "0", USDT, G1, 1), s(12));
    assert_eq!(l.pos(A1, BTC_P2).realized_quote, -1_000_000);
    let bytes = snap(&l);
    let s1 = section(&bytes, SECTION_POSITIONS);
    let row = (0..s1.count as usize)
        .map(|k| PositionRow::read_from_bytes(&s1.body[k * 64..k * 64 + 64]).expect("row"))
        .find(|r| r.inst == BTC_P2)
        .expect("BTC/P2 row");
    assert_eq!(row.realized_quote, [0xFFFF_FFFF_FFF0_BDC0, 0xFFFF_FFFF_FFFF_FFFF]);
    let r = restore(&bytes).expect("restore");
    assert_eq!(r.pos(A1, BTC_P2).realized_quote, -1_000_000);
}

#[test]
fn led_237_engine_ns_not_shifted() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "0.1", "60000", "0", USDT, G1, 0), s(10));
    l.on_balance(&bal(A1, S1, USDT, usdt("94000"), Qty(0), D + s(12)), false, s(12));
    l.on_ref_px(BNB_S1, px("599.99"), px("600.01"), s(12));
    let probe = |l: &Ledger| {
        (
            l.pos(A1, BTC_S1).last_fill_ens,
            l.group(A1, G1).round.open_ens,
            cash(l, A1, S1, USDT).rep_ens,
            l.rate(BNB).ens,
        )
    };
    let before = probe(&l);
    assert_eq!(before, (s(10), s(10), s(12), s(12)));
    let mut r = restore(&snap(&l)).expect("restore");
    assert_eq!(probe(&r), before);
    r.on_run_start(s(500));
    r.apply_fill(&f(Buy, BTC_S1, "0.1", "60000", "0", USDT, G1, 0), s(501));
    assert_eq!(r.pos(A1, BTC_S1).last_fill_ens, s(501));
    assert_eq!(r.group(A1, G1).round.open_ens, s(10));
}

#[test]
fn led_247_engine_time_continuous() {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_S1, "0.1", "60000", "0", USDT, G1, 0), s(10));
    let mut r = restore(&snap(&l)).expect("restore");
    r.on_run_start(s(20));
    r.apply_fill(&f(Buy, BTC_S1, "0.1", "60000", "0", USDT, G1, 0), s(25));
    assert_eq!(r.pos(A1, BTC_S1).last_fill_ens, s(25));
    let g = r.group_pos(A1, G1);
    assert_eq!((g.last_fill_ens, g.round_start_ens), (s(25), s(10)));
}
