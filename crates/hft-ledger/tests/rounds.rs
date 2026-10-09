//! LED-200–LED-203: round start/close against `round_dust_notional`, RoundClosed contents.

mod common;

use common::*;
use hft_ledger::notes::code;
use hft_ledger::{Ledger, LegAgg};

/// Round cases' common pre-state (spot valued at bid 50 000).
fn pre() -> Ledger {
    let mut l = booted();
    l.on_ref_px(BTC_S1, px("50000"), px("50000.01"), 0);
    l.clear_notes();
    l
}

fn buy(l: &mut Ledger, q: &str, p: &str, t: i64, seq: u64) {
    let mut fl = f(Buy, BTC_S1, q, p, "0", USDT, G1, 0);
    fl.seq = seq;
    l.apply_fill(&fl, t);
}

fn sell(l: &mut Ledger, q: &str, p: &str, t: i64) {
    l.apply_fill(&f(Sell, BTC_S1, q, p, "0", USDT, G1, 0), t);
}

#[test]
fn led_200_round_start_boundary() {
    for (k, (q, started)) in [("0.00009999", false), ("0.0001", true), ("0.00010001", true)]
        .iter()
        .enumerate()
    {
        let mut l = pre();
        buy(&mut l, q, "50000", s(10), 1_000);
        let r = l.group(A1, G1).round;
        if *started {
            assert_eq!(r.round_id, 1, "LED-200.{}", k + 1);
            if k == 1 {
                assert_eq!((r.open_ens, r.open_seq), (s(10), 1_000), "LED-200.2");
            }
        } else {
            assert_eq!((r.round_id, r.open_ens), (0, 0), "LED-200.1");
        }
    }
}

#[test]
fn led_201_round_close() {
    let mut l = pre();
    buy(&mut l, "0.0001", "50000", s(10), 1_000);
    l.clear_notes();
    sell(&mut l, "0.0001", "51000", s(70));
    let ns = notes_of(&l, code::ROUND_CLOSED);
    assert_eq!(ns.len(), 1);
    let x = &ns[0];
    assert_eq!(x.group, G1);
    assert_eq!(x.v[0] >> 32, 0, "leg 0");
    assert_eq!(x.v[1], s(60) as u64, "duration");
    assert_eq!(
        x.v[2..7],
        [
            btc("0.0001").0 as u64,
            n("5").0 as u64,
            btc("0.0001").0 as u64,
            n("5.1").0 as u64,
            n("0.1").0 as u64
        ]
    );
    assert_eq!(l.group(A1, G1).round.legs, [LegAgg::ZERO; 4]);
}

#[test]
fn led_202_round_close_boundary() {
    for (k, (q, closes)) in [("0.0001", false), ("0.00010001", true)].iter().enumerate() {
        let mut l = pre();
        buy(&mut l, "0.0002", "50000", s(10), 1);
        l.clear_notes();
        sell(&mut l, q, "50000", s(20));
        assert_eq!(
            notes_of(&l, code::ROUND_CLOSED).len(),
            usize::from(*closes),
            "LED-202.{}",
            k + 1
        );
    }
}

#[test]
fn led_203_round_avg_from_paired_fills() {
    let mut l = pre();
    buy(&mut l, "0.001", "50000", s(10), 1);
    buy(&mut l, "0.001", "52000", s(11), 2);
    sell(&mut l, "0.001", "53000", s(12));
    l.clear_notes();
    sell(&mut l, "0.001", "54000", s(13));
    let ns = notes_of(&l, code::ROUND_CLOSED);
    assert_eq!(ns.len(), 1);
    assert_eq!(
        ns[0].v[2..6],
        [
            btc("0.002").0 as u64,
            n("102").0 as u64,
            btc("0.002").0 as u64,
            n("107").0 as u64
        ]
    );
}
