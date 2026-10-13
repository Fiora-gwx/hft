//! LED-263–LED-265, LED-271–LED-273: slot identity, cumulative-fill dedup, unmapped fills,
//! fees of fill reports whose quantity came earlier.

use crate::common;

use common::journal::*;
use common::*;
use hft_event::OrderEventKind::{self, Ack, CancelReject, Fill, PartialFill};
use hft_ledger::{JournalLedger, Ledger};
use hft_types::{ASSET_NONE, Px, Qty};

const T: i64 = 10_000_000_000;

#[test]
fn led_298_spot_cash_rounds_each_gross_execution_in_hot_and_cold_ledgers() {
    for price in ["59998.1300", "60000.0000"] {
        for side in [Buy, Sell] {
            for fee in ["0", "0.000003"] {
                let mut hot = booted();
                if side == Sell {
                    hot.apply_fill(&f(Buy, BTC_S1, "0.001", "60000", "0", USDT, G1, 0), 0);
                }
                let mut cold = cold_from(&hot);
                clock(&mut cold, T);
                hot.on_run_start(T);
                feed(&mut cold, &epoch_start(7), true);
                let fill = f(side, BTC_S1, "0.00001", price, fee, USDT, G1, 0);
                let before = cash(&hot, A1, S1, USDT).total.0;
                feed(
                    &mut cold,
                    &decision_new(5, 1, A1, S1, BTC_S1, side, G1, fill.qty),
                    true,
                );
                let mut report = order_event(Fill, 7, 5, 1, A1, S1, BTC_S1, fill.qty, fill.px);
                report.qty = fill.qty;
                report.fee = Qty(fill.fee);
                report.fee_asset = USDT;
                hot.apply_fill(&fill, T);
                feed(&mut cold, &report, true);
                // The input amounts are 599981.3 or 600000 raw USDT units.
                // Expected cash comes from those amounts, never either ledger's delta.
                let gross = match (price, side) {
                    ("59998.1300", Buy) => -599_982,
                    ("59998.1300", Sell) => 599_981,
                    (_, Buy) => -600_000,
                    (_, Sell) => 600_000,
                };
                let expected = before + gross - if fee == "0" { 0 } else { 3 };
                assert_eq!(cash(&hot, A1, S1, USDT).total.0, expected);
                assert_eq!(cash(cold.ledger(), A1, S1, USDT).total.0, expected);
                assert_eq!(hot.pos(A1, BTC_S1).qty, cold.ledger().pos(A1, BTC_S1).qty);
                assert_eq!(hot.group(A1, G1).pnl, cold.ledger().group(A1, G1).pnl);
            }
        }
    }
}

/// Snapshot of `boot_a1`, `EpochStart{7}`, `OrderDecision{slot 5, gen 1, Buy BTC/S1, G1, 0.01}`.
fn with_decision() -> JournalLedger {
    let mut j = cold_from(&booted());
    clock(&mut j, T);
    feed(&mut j, &epoch_start(7), true);
    feed(
        &mut j,
        &decision_new(5, 1, A1, S1, BTC_S1, Buy, G1, btc("0.01")),
        true,
    );
    j
}

fn ev(what: OrderEventKind, slot: u32, r#gen: u32, cum: &str, avg: Px) -> hft_event::OrderEvent {
    order_event(what, 7, slot, r#gen, A1, S1, BTC_S1, btc(cum), avg)
}

#[test]
fn led_263_fills_from_journal() {
    let mut j = with_decision();
    feed(&mut j, &ev(PartialFill, 5, 1, "0.004", px("60000.00")), true);
    feed(&mut j, &ev(Fill, 5, 1, "0.01", px("60001.00")), true);
    let l = j.ledger();
    assert_eq!(l.pos(A1, BTC_S1).qty, btc("0.01"));
    assert_eq!(l.inventory(A1, S1, BTC, G1), btc("0.01"));
    // 0.004 × 60 000 + (60 001 × 0.01 − 60 000 × 0.004) = 240 + 360.01
    assert_eq!(cash(l, A1, S1, USDT).total, usdt("99399.99"));
    assert_eq!(j.stats().unmapped_fills, 0);
}

#[test]
fn led_299_original_spot_notional_is_independent_of_rounded_price_and_errors_are_atomic() {
    use hft_ledger::SettlementError;
    for side in [Buy, Sell] {
        let mut l = booted();
        l.apply_fill(&f(Buy, BTC_S1, "0.01", "60000", "0", USDT, G1, 0), 0);
        let fill = f(side, BTC_S1, "0.006", "60001.6667", "0.000003", USDT, G1, 0);
        let before = cash(&l, A1, S1, USDT).total.0;
        l.apply_spot_fill(&fill, 360_010_000_000_000, T).unwrap();
        let signed = if side == Buy { -360_010_000 } else { 360_010_000 };
        assert_eq!(cash(&l, A1, S1, USDT).total.0, before + signed - 3);
        assert_eq!(
            l.pos(A1, BTC_S1).qty,
            btc(if side == Buy { "0.016" } else { "0.004" })
        );
    }
    let mut l = booted();
    let fill = f(Buy, BTC_S1, "0.006", "60001.6667", "0", USDT, G1, 0);
    for row in 0..8 {
        let mut invalid = fill;
        let mut notional = 360_010_000_000_000;
        let expected = match row {
            0 => {
                invalid.inst = hft_types::InstId(u32::MAX);
                SettlementError::UnknownInstrument
            }
            1 => {
                invalid.inst = BTC_P2;
                SettlementError::NotSpot
            }
            2 => {
                invalid.venue = P2;
                SettlementError::VenueMismatch
            }
            3 => {
                invalid.account = hft_types::AccountId(0);
                SettlementError::AccountVenue
            }
            4 => {
                invalid.qty = Qty(0);
                SettlementError::InvalidQuantity
            }
            5 => {
                invalid.px = Px(0);
                SettlementError::InvalidPrice
            }
            6 => {
                notional = 0;
                SettlementError::InvalidNotional
            }
            _ => {
                notional = -1;
                SettlementError::InvalidNotional
            }
        };
        let before = snap(&l);
        assert_eq!(
            l.apply_spot_fill(&invalid, notional, T),
            Err(expected),
            "row {row}"
        );
        assert_eq!(snap(&l), before, "invalid input advanced state, row {row}");
    }
}

#[test]
fn led_264_cum_filled_dedup() {
    let rows: [(&[(OrderEventKind, &str)], &str); 4] = [
        (&[(PartialFill, "0.004"), (PartialFill, "0.004")], "0.004"),
        (&[(PartialFill, "0.006"), (PartialFill, "0.004")], "0.006"),
        (&[(PartialFill, "0.004"), (Fill, "0.01"), (Fill, "0.01")], "0.01"),
        (&[(Ack, "0"), (Fill, "0.01")], "0.01"),
    ];
    for (k, (seq, want)) in rows.iter().enumerate() {
        let mut j = with_decision();
        for (what, cum) in seq.iter() {
            feed(&mut j, &ev(*what, 5, 1, cum, px("60000")), true);
        }
        assert_eq!(j.ledger().pos(A1, BTC_S1).qty, btc(want), "LED-264.{}", k + 1);
    }
}

#[test]
fn led_265_unmapped_and_foreign_note() {
    let base = booted();
    let mut j = cold_from(&base);
    clock(&mut j, T);
    feed(&mut j, &epoch_start(7), true);
    let q0 = j.ledger().pos(A1, BTC_S1).qty;
    let mut e = ev(Fill, 9, 3, "0.002", px("60000"));
    e.exch_order_id = 77;
    feed(&mut j, &e, false);
    assert_eq!(j.stats().unmapped_fills, 1);
    assert_eq!(j.ledger().pos(A1, BTC_S1).qty, q0);
    let n = oms_foreign_fill(
        S1,
        A1,
        BTC_S1,
        Buy,
        1,
        ASSET_NONE,
        btc("0.002"),
        px("60000"),
        Qty(0),
        77,
        0,
        btc("0.002"),
    );
    feed(&mut j, &n, true);
    assert_eq!(j.ledger().pos(A1, BTC_S1).qty.0 - q0.0, btc("0.002").0);
    let mut h = base.clone();
    h.on_run_start(T);
    h.apply_external(&Ledger::external_from_oms_note(&n).expect("note"), false, T);
    assert_snap_eq(j.ledger(), &h);
}

#[test]
fn led_271_fee_of_fill_after_non_fill_report() {
    let mut j = with_decision();
    feed(&mut j, &ev(CancelReject, 5, 1, "0.01", px("60000")), true);
    assert_eq!(j.ledger().pos(A1, BTC_S1).qty, btc("0.01"));
    assert_eq!(cash(j.ledger(), A1, S1, USDT).total, usdt("99400"));
    let mut f = ev(Fill, 5, 1, "0.01", px("60000"));
    f.fee = usdt("0.5");
    f.fee_asset = USDT;
    feed(&mut j, &f, true);
    assert_eq!(j.ledger().pos(A1, BTC_S1).qty, btc("0.01"));
    assert_eq!(cash(j.ledger(), A1, S1, USDT).total, usdt("99399.5"));
    feed(&mut j, &f, true);
    assert_eq!(cash(j.ledger(), A1, S1, USDT).total, usdt("99399.5"));
}

#[test]
fn led_272_slot_snapshot_fee_watermark_versions() {
    use hft_ledger::{OMS_SECTION_SLOTS, OMS_SLOT_ROW_LEN};
    use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};

    let mut before = with_decision();
    feed(&mut before, &ev(CancelReject, 5, 1, "0.01", px("60000")), true);
    let mut fill = ev(Fill, 5, 1, "0.01", px("60000"));
    fill.fee = usdt("0.5");
    fill.fee_asset = USDT;
    // A single OMS row using the section-2 contract. v1 has no fee watermark and assumes the
    // filled quantity's fee is already booked; v2 can preserve a still-outstanding fee.
    for (version, fee_cum, cash_after) in [
        (1, 0, "99400"),
        (2, 0, "99399.5"),
        (2, btc("0.01").0, "99400"),
        (3, 0, "99399.5"),
        (3, btc("0.01").0, "99400"),
        (4, 0, "99400"),
    ] {
        let mut w = SnapshotWriter::new(64 << 20);
        before.ledger().snapshot(&mut w);
        w.begin_section(OMS_SECTION_SLOTS, version);
        let mut row = vec![0u8; if version == 1 { 160 } else { OMS_SLOT_ROW_LEN }];
        row[6] = S1.0;
        row[7] = A1.0;
        row[8..12].copy_from_slice(&1u32.to_le_bytes());
        row[12..16].copy_from_slice(&5u32.to_le_bytes());
        row[16..20].copy_from_slice(&BTC_S1.0.to_le_bytes());
        row[1] = Buy as u8;
        row[22..24].copy_from_slice(&7u16.to_le_bytes());
        row[24..28].copy_from_slice(&G1.0.to_le_bytes());
        row[40..48].copy_from_slice(&btc("0.01").0.to_le_bytes());
        if version != 1 {
            row[160..168].copy_from_slice(&fee_cum.to_le_bytes());
        }
        w.put_bytes(&row);
        w.add_rows(1);
        w.end_section();
        assert!(w.status().is_ok());
        let mut r = SnapshotReader::new(w.as_bytes());
        let restored = JournalLedger::from_snapshot(&mut r, rd_lf(), &cfg_lf(), &caps_lf());
        if version == 4 {
            assert!(matches!(restored, Err(RestoreError::VersionUnsupported)));
            continue;
        }
        let mut j = restored.unwrap();
        clock(&mut j, T);
        feed(&mut j, &fill, true);
        assert_eq!(cash(j.ledger(), A1, S1, USDT).total, usdt(cash_after));
        feed(&mut j, &fill, true);
        assert_eq!(cash(j.ledger(), A1, S1, USDT).total, usdt(cash_after));
        assert_eq!(j.ledger().pos(A1, BTC_S1).qty, btc("0.01"));
    }
}

#[test]
fn led_273_slot_identity_includes_venue_and_account() {
    use hft_ledger::{OMS_SECTION_SLOTS, OMS_SLOT_ROW_LEN};
    use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};

    let mut base = booted();
    boot_a2(&mut base, 0, "100000", "100000");
    // Same epoch/slot/generation, first across venues, then across accounts on one venue.
    let rows = [
        (A1, S1, BTC_S1, G1, "0.01", "0.5", "99399.5"),
        (A1, P2, BTC_P2, G1, "0.02", "0.7", "49999.3"),
        (A2, P2, BTC_P2, G7, "0.03", "0.9", "99999.1"),
    ];
    for version in 0..=2u16 {
        let mut j = if version == 0 {
            cold_from(&base)
        } else {
            let mut w = SnapshotWriter::new(64 << 20);
            base.snapshot(&mut w);
            w.begin_section(OMS_SECTION_SLOTS, version);
            for &(a, v, inst, group, qty, _, _) in &rows {
                let mut row = vec![0u8; if version == 1 { 160 } else { OMS_SLOT_ROW_LEN }];
                row[0] = 1; // PendingNew, no fill or fee booked yet.
                row[1] = Buy as u8;
                row[6] = v.0;
                row[7] = a.0;
                row[8..12].copy_from_slice(&1u32.to_le_bytes());
                row[12..16].copy_from_slice(&5u32.to_le_bytes());
                row[16..20].copy_from_slice(&inst.0.to_le_bytes());
                row[20..22].copy_from_slice(&((group.0 >> 24) as u16).to_le_bytes());
                row[22..24].copy_from_slice(&7u16.to_le_bytes());
                row[24..28].copy_from_slice(&group.0.to_le_bytes());
                row[32..40].copy_from_slice(&btc(qty).0.to_le_bytes());
                row[48..56].copy_from_slice(&px("60000").0.to_le_bytes());
                w.put_bytes(&row);
                w.add_rows(1);
            }
            w.end_section();
            assert!(w.status().is_ok());
            let mut r = SnapshotReader::new(w.as_bytes());
            JournalLedger::from_snapshot(&mut r, rd_lf(), &cfg_lf(), &caps_lf()).unwrap()
        };
        clock(&mut j, T);
        feed(&mut j, &epoch_start(7), true);
        if version == 0 {
            for &(a, v, inst, group, qty, _, _) in &rows {
                feed(
                    &mut j,
                    &decision_new(5, 1, a, v, inst, Buy, group, btc(qty)),
                    true,
                );
            }
        }
        for &(a, v, inst, _, qty, _, _) in &rows {
            let e = order_event(CancelReject, 7, 5, 1, a, v, inst, btc(qty), px("60000"));
            feed(&mut j, &e, true);
        }
        // Reverse the reports, then repeat them: both the quantity and fee watermarks belong
        // to the full identity and must not suppress or transfer another table's fee.
        for _ in 0..2 {
            for &(a, v, inst, _, qty, fee, _) in rows.iter().rev() {
                let mut e = order_event(Fill, 7, 5, 1, a, v, inst, btc(qty), px("60000"));
                e.fee = usdt(fee);
                e.fee_asset = USDT;
                feed(&mut j, &e, true);
            }
        }
        for &(a, v, inst, _, qty, _, balance) in &rows {
            assert_eq!(j.ledger().pos(a, inst).qty, btc(qty), "LED-273.{}", version + 1);
            assert_eq!(
                cash(j.ledger(), a, v, USDT).total,
                usdt(balance),
                "LED-273.{}",
                version + 1
            );
        }
        assert_eq!(j.stats().unmapped_fills, 0, "LED-273.{}", version + 1);
        assert_eq!(j.stats().map_evicted, 0, "LED-273.{}", version + 1);
    }
}
