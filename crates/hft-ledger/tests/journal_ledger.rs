//! LED-263–LED-265: `JournalLedger` slot map, cumulative-fill dedup, unmapped fills.

mod common;

use common::journal::*;
use common::*;
use hft_event::OrderEventKind::{self, Ack, Fill, PartialFill};
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
