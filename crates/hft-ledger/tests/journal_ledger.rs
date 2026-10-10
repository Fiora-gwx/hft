//! LED-263–LED-265, LED-271: `JournalLedger` slot map, cumulative-fill dedup, unmapped fills,
//! fees of fill reports whose quantity came earlier.

use crate::common;

use common::journal::*;
use common::*;
use hft_event::OrderEventKind::{self, Ack, CancelReject, Fill, PartialFill};
use hft_ledger::{JournalLedger, Ledger};
use hft_types::{ASSET_NONE, Px, Qty};

const T: i64 = 10_000_000_000;

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
        (3, 0, "99400"),
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
        if version == 3 {
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
