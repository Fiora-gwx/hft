//! LED-075–LED-079: the cold reader's handling of ExternalFill notes, AUTO_RECON notes and
//! OMS foreign-fill notes.

mod common;

use common::journal::*;
use common::*;
use hft_event::{ExternalCause, NoteSource, ReconcileReport};
use hft_ledger::notes::code;
use hft_ledger::{KEY_ORDER_FILL, Ledger};
use hft_types::{ASSET_NONE, AccountId, InstId, Px, Qty, Side, VenueId};
use zerocopy::{FromZeros, IntoBytes};

const T: i64 = 10_000_000_000;

#[test]
fn led_075_plain_note_not_booked() {
    let base = booted();
    let mut c = cold_from(&base);
    let mut h = base.clone();
    h.on_run_start(T);
    let e = ext_tid(ExternalCause::Liquidation, Buy, BTC_P2, "0.4", "60000", 910);
    h.clear_notes();
    h.apply_external(&e, false, T);
    let n = notes_of(&h, code::EXTERNAL_FILL)[0];
    assert_eq!((n.v[0] >> 24) & 0xFF, 0, "aflags 0");
    clock(&mut c, T);
    let q0 = c.ledger().pos(A1, BTC_P2).qty;
    feed(&mut c, &e, false);
    feed(&mut c, &n, true);
    assert_eq!(c.ledger().pos(A1, BTC_P2).qty.0 - q0.0, btc("0.4").0);
    assert_snap_eq(c.ledger(), &h);
}

fn long_10() -> Ledger {
    let mut l = booted();
    l.apply_fill(&f(Buy, BTC_P2, "10.0", "60000", "0", USDT, G1, 1), s(1));
    l.clear_notes();
    l
}

#[test]
fn led_076_auto_recon_note_consumed() {
    let base = long_10();
    let mut c = cold_from(&base);
    let mut h = base.clone();
    h.on_run_start(T);
    let auto = Ledger::auto_external_fill(A1, P2, BTC_P2, btc("4.0"), px("60500"), 4242);
    h.apply_external(&auto, true, T);
    let n = notes_of(&h, code::EXTERNAL_FILL)[0];
    // The cold reader's reconstruction: note fields plus the Input's report_id.
    let qty = n.v[3] as i64;
    let d = if n.v[2] == Side::Sell as u64 { qty } else { -qty };
    let rebuilt = Ledger::auto_external_fill(
        AccountId((n.v[0] >> 8 & 0xFF) as u8),
        VenueId((n.v[0] & 0xFF) as u8),
        InstId(n.v[1] as u32),
        Qty(d),
        Px(n.v[4] as i64),
        4242,
    );
    assert_eq!(rebuilt.as_bytes(), auto.as_bytes());
    clock(&mut c, T);
    let mut rr = ReconcileReport::new_zeroed();
    rr.report_id = 4242;
    feed(&mut c, &rr, false);
    feed(&mut c, &n, true);
    assert_snap_eq(c.ledger(), &h);
    assert_eq!(h.auto_held(A1, BTC_P2, Sell), btc("4.0"));
    assert_eq!(c.ledger().auto_held(A1, BTC_P2, Sell), btc("4.0"));
}

/// LED-077's base note (`path` and `trade_id` vary by row).
fn n77(path: u64, trade_id: u64) -> hft_event::Note {
    oms_foreign_fill(
        P2,
        A1,
        BTC_P2,
        Buy,
        path,
        USDT,
        btc("0.3"),
        px("60100"),
        usdt("0.09"),
        99,
        trade_id,
        btc("0.5"),
    )
}

#[test]
fn led_077_external_from_oms_note() {
    let e = Ledger::external_from_oms_note(&n77(2, 0)).expect("LED-077.1");
    assert_eq!(
        (
            e.venue,
            e.account,
            e.side,
            e.cause,
            e.inst,
            e.qty,
            e.px,
            e.fee,
            e.fee_asset
        ),
        (
            P2,
            A1,
            Buy,
            ExternalCause::Unknown,
            BTC_P2,
            btc("0.3"),
            px("60100"),
            usdt("0.09"),
            USDT
        ),
        "LED-077.1"
    );
    assert_eq!(
        (e.exch_order_id, e.trade_id, e.fill_seq, e.ex_ts_ns, e.rx_tsc.0),
        (99, 0, 50_000_000, 0, 0),
        "LED-077.1"
    );
    let e2 = Ledger::external_from_oms_note(&n77(1, 0)).expect("LED-077.2");
    assert_eq!(e2.as_bytes(), e.as_bytes(), "LED-077.2");
    let e3 = Ledger::external_from_oms_note(&n77(2, 12_345)).expect("LED-077.3");
    assert_eq!((e3.trade_id, e3.fill_seq), (12_345, 0), "LED-077.3");
    let mut n4 = n77(2, 0);
    n4.code = code::OMS_STALE_GEN_FILL;
    assert!(Ledger::external_from_oms_note(&n4).is_none(), "LED-077.4");
    let mut n5 = n77(2, 0);
    n5.source = NoteSource::Ledger;
    n5.code = code::EXTERNAL_FILL;
    assert!(Ledger::external_from_oms_note(&n5).is_none(), "LED-077.5");
}

#[test]
fn led_078_oms_note_dedup() {
    let mut l = booted();
    let mut e = ext(ExternalCause::Unknown, Buy, BTC_P2, "0.3", "60100");
    e.exch_order_id = 99;
    e.fill_seq = btc("0.5").0 as u64;
    assert!(l.apply_external(&e, false, s(5)).is_some());
    let before = snap(&l);
    let x = Ledger::external_from_oms_note(&n77(2, 0)).expect("note");
    assert!(l.apply_external(&x, false, s(10)).is_none());
    assert_eq!(snap(&l), before);
}

#[test]
fn led_079_oms_note_two_increments() {
    let mut l = booted();
    let q0 = l.pos(A1, BTC_P2).qty;
    for (q, cum) in [("0.2", "0.2"), ("0.3", "0.5")] {
        let n = oms_foreign_fill(
            P2,
            A1,
            BTC_P2,
            Buy,
            1,
            ASSET_NONE,
            btc(q),
            px("60000"),
            Qty(0),
            99,
            0,
            btc(cum),
        );
        let x = Ledger::external_from_oms_note(&n).expect("note");
        assert!(l.apply_external(&x, false, s(10)).is_some());
    }
    assert_eq!(l.pos(A1, BTC_P2).qty.0 - q0.0, btc("0.5").0);
    let bs: Vec<u64> = l
        .ext_seen(A1, P2)
        .iter()
        .filter(|k| k.kind == KEY_ORDER_FILL)
        .map(|k| k.b)
        .collect();
    assert_eq!(bs, [btc("0.2").0 as u64, btc("0.5").0 as u64]);
}
