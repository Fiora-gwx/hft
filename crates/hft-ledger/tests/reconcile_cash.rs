//! Exact execution identity owns a reconciliation hold, including across restart.
use crate::common::*;
use hft_event::{ExternalCause, ExternalFill};
use hft_ledger::{Ledger, SettlementError};
use hft_strategy_sdk::SnapshotReader;
use hft_types::{Qty, Side, Tsc};

fn receipt(side: Side, id: u64) -> ExternalFill {
    let mut e = ext_tid(ExternalCause::Adl, side, BTC_P2, "0.001", "60027.5", id);
    e.exch_order_id = 800 + id;
    e.cash_qty = e.qty;
    e.cash_amount = Qty(if side == Buy { -8_600 } else { 8_600 });
    e.fee_asset = USDT;
    e.fee = Qty(37);
    e
}

#[test]
fn led_292_exact_receipts_survive_restart_and_do_not_offset_other_fills() {
    for side in [Buy, Sell] {
        for use_trade in [true, false] {
            let mut l = booted();
            l.apply_fill(
                &f(side.opposite(), BTC_P2, "0.004", "60018.9", "0", USDT, G1, 1),
                s(1),
            );
            let mut e = receipt(side, 51);
            if !use_trade {
                e.fill_seq = 51;
                e.trade_id = 0;
            }
            let initial = cash(&l, A1, P2, USDT).total.0;
            assert!(l.apply_reconciled_external(&e, s(2)).unwrap().is_some());
            let held = snap(&l);
            assert_eq!(section(&held, 13).version, 9);
            assert_eq!(l.auto_held(A1, BTC_P2, side), e.qty);
            assert!(l.apply_reconciled_external(&e, s(20)).unwrap().is_none());
            assert_eq!(snap(&l), held);
            l = restore(&held).unwrap();
            assert_eq!(snap(&l), held);
            // Same direction, price and amount, different execution: it must be booked.
            let other = receipt(side, 52);
            assert!(l.apply_settled_external(&other, s(3)).unwrap().is_some());
            assert_eq!(l.auto_held(A1, BTC_P2, side), e.qty);
            assert_eq!(
                cash(&l, A1, P2, USDT).total.0,
                initial + 2 * (e.cash_amount.0 - e.fee.0)
            );
            let pos = *l.pos(A1, BTC_P2);
            let wallet = cash(&l, A1, P2, USDT).total;
            e.rx_tsc = Tsc(99_999); // transport timestamp is not durable receipt identity
            assert!(l.apply_settled_external(&e, s(4)).unwrap().is_none());
            assert_eq!(l.auto_held(A1, BTC_P2, side), Qty(0));
            assert_eq!(*l.pos(A1, BTC_P2), pos);
            assert_eq!(cash(&l, A1, P2, USDT).total, wallet);
            assert!(
                l.notes()
                    .iter()
                    .any(|n| n.code == hft_ledger::notes::code::AUTO_HELD_OFFSET)
            );
            let confirmed = snap(&l);
            assert_eq!(section(&confirmed, 13).version, 8);
            l = restore(&confirmed).unwrap();
            assert!(l.apply_settled_external(&e, s(5)).unwrap().is_none());
            assert_eq!(snap(&l), confirmed);
        }
    }
}

#[test]
fn led_302_receipt_confirmation_ignores_delivery_timestamps() {
    for (row, ex_ts, rx) in [("LED-302.1", 1, 0), ("LED-302.2", 0, 77), ("LED-302.3", 5, 9)] {
        let mut l = booted();
        let e = receipt(Sell, 51);
        l.apply_reconciled_external(&e, s(1)).unwrap();
        let wallet = cash(&l, A1, P2, USDT).total;
        let pos = *l.pos(A1, BTC_P2);
        l.clear_notes();
        let mut push = e;
        push.ex_ts_ns += ex_ts;
        push.rx_tsc = Tsc(rx);
        push.cause = ExternalCause::Unknown;
        assert_eq!(l.apply_settled_external(&push, s(2)), Ok(None), "{row}");
        assert_eq!(l.auto_held(A1, BTC_P2, Sell), Qty(0), "{row}");
        assert_eq!(cash(&l, A1, P2, USDT).total, wallet, "{row}");
        assert_eq!(*l.pos(A1, BTC_P2), pos, "{row}");
        assert!(
            !l.notes()
                .iter()
                .any(|n| n.code == hft_ledger::notes::code::RECEIPT_MISMATCH),
            "{row}"
        );
    }
}

#[test]
fn led_303_section13_v8_rejects_pending_receipt_rows() {
    let mut l = booted();
    l.apply_reconciled_external(&receipt(Buy, 51), s(1)).unwrap();
    let mut bytes = snap(&l);
    assert_eq!(section(&bytes, 13).version, 9);
    assert!(restore(&bytes).is_ok(), "LED-303.1");
    let at = section_at(&bytes, 13);
    bytes[at + 2..at + 4].copy_from_slice(&8u16.to_le_bytes());
    assert_eq!(
        restore(&bytes).err(),
        Some(hft_strategy_sdk::RestoreError::BadValue),
        "LED-303.2"
    );
}

#[test]
fn led_293_receipt_conflicts_capacity_and_eviction_are_atomic() {
    use hft_ledger::receipt_field as rf;
    let e = receipt(Buy, 51);
    for (mutation, bit) in [rf::CASH, rf::FEE, rf::QTY, rf::SIDE, rf::PX, rf::IDENTITY]
        .into_iter()
        .enumerate()
    {
        let mut l = booted();
        l.apply_reconciled_external(&e, s(1)).unwrap();
        l.clear_notes();
        let before = snap(&l);
        let mut bad = e;
        match mutation {
            0 => bad.cash_amount.0 += 1,
            1 => bad.fee.0 += 1,
            2 => {
                bad.qty.0 += 1;
                bad.cash_qty = bad.qty;
            }
            3 => bad.side = Sell,
            4 => bad.px.0 += 1,
            _ => bad.exch_order_id += 1,
        }
        // A money/identity mismatch with the pending receipt books nothing and keeps it.
        assert_eq!(
            l.apply_settled_external(&bad, s(90)),
            Ok(None),
            "mutation {mutation}"
        );
        assert_eq!(snap(&l), before, "mutation {mutation}");
        let n: Vec<_> = l
            .notes()
            .iter()
            .filter(|n| n.code == hft_ledger::notes::code::RECEIPT_MISMATCH)
            .collect();
        assert_eq!(n.len(), 1, "mutation {mutation}");
        assert_eq!(n[0].level, hft_event::NoteLevel::P0);
        assert_eq!(n[0].v[5] & u64::from(bit), u64::from(bit), "mutation {mutation}");
        assert_eq!(l.auto_held(A1, BTC_P2, Buy), e.qty);
        assert!(l.apply_reconciled_external(&bad, s(90)).is_err());
        assert_eq!(snap(&l), before);
    }
    let mut cfg = cfg_lf();
    cfg.ext_dedup_cap = 2;
    let mut l = ledger_with(rd_lf(), &cfg);
    boot_a1(&mut l, 0);
    for id in [51, 52] {
        l.apply_reconciled_external(&receipt(Buy, id), s(1)).unwrap();
    }
    let before = snap(&l);
    assert_eq!(
        l.apply_reconciled_external(&receipt(Buy, 53), s(99)),
        Err(SettlementError::ReceiptCapacity)
    );
    assert_eq!(snap(&l), before);
    // Ordinary keys can wrap, but pending original receipts must not be evicted.
    for id in [71, 72, 73] {
        l.apply_settled_external(&receipt(Sell, id), s(2)).unwrap();
    }
    let wallet = cash(&l, A1, P2, USDT).total;
    let bytes = snap(&l);
    l = Ledger::restore(&mut SnapshotReader::new(&bytes), rd_lf(), &cfg, &caps_lf()).unwrap();
    for id in [51, 52] {
        assert!(
            l.apply_settled_external(&receipt(Buy, id), s(3))
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(l.auto_held(A1, BTC_P2, Buy), Qty(0));
    assert_eq!(cash(&l, A1, P2, USDT).total, wallet);
    // The confirmation puts the key back in the ordinary FIFO.
    assert!(
        l.apply_settled_external(&receipt(Buy, 52), s(4))
            .unwrap()
            .is_none()
    );
}
