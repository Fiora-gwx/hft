//! BOOK-054–BOOK-055: re-centering.
mod common;

use std::collections::BTreeMap;

use common::*;
use hft_book::Outcome;

#[test]
fn book_054_recenter_bounds() {
    let rows = [
        ("BOOK-054.1", 10300, 10310, false, 936),
        ("BOOK-054.2", 10310, 10320, false, 936),
        ("BOOK-054.3", 10320, 10330, true, 968),
        ("BOOK-054.4", 9680, 9690, false, 936),
        ("BOOK-054.5", 9670, 9680, true, 903),
    ];
    for (row, bid, ask, rec, base) in rows {
        let mut b = book_s1(&P);
        let o = batch(&mut b, 100, 101, &[(10000, 0), (bid, 1)], &[(10010, 0), (ask, 1)]);
        assert!(o.contains(Outcome::APPLIED), "{row}");
        assert_eq!(o.contains(Outcome::RECENTERED), rec, "{row}");
        assert_eq!(b.meta().base, base, "{row}");
    }
}

#[test]
fn book_055_large_jump_rebuild() {
    let mut b = book_s0(&P);
    let bids = [(10000, 0), (9990, 0), (11280, 1), (11270, 2)];
    let asks = [(10010, 0), (10020, 0), (11290, 3)];
    // Reference book: S0 then the same message.
    let mut rb: BTreeMap<i64, i64> = S0_BIDS.iter().copied().collect();
    let mut ra: BTreeMap<i64, i64> = S0_ASKS.iter().copied().collect();
    for (m, lv) in [(&mut rb, &bids[..]), (&mut ra, &asks[..])] {
        for &(p, q) in lv {
            if q == 0 {
                m.remove(&p);
            } else {
                m.insert(p, q);
            }
        }
    }
    let o = batch(&mut b, 100, 101, &bids, &asks);
    assert!(o.contains(Outcome::RECENTERED), "{o:?}");
    assert_eq!(b.meta().base, 1064);
    assert_eq!(side(&b, Buy), [(11280, 1), (11270, 2)]);
    assert_eq!(side(&b, Sell), [(11290, 3)]);
    let want_b: Vec<(i64, i64)> = rb.iter().rev().map(|(p, q)| (*p, *q)).collect();
    let want_a: Vec<(i64, i64)> = ra.iter().map(|(p, q)| (*p, *q)).collect();
    assert_eq!(side(&b, Buy), want_b);
    assert_eq!(side(&b, Sell), want_a);
}
