//! BOOK-080–BOOK-082: `export` and `state_hash`.
#![allow(clippy::type_complexity)]
mod common;

use common::*;
use hft_book::{L2Book, StaleReason};
use hft_event::Level;

fn s0_overflow() -> L2Book {
    let mut b = book_s0(&P);
    batch(&mut b, 100, 101, &[(9000, 1), (9350, 2)], &[(10700, 3)]);
    b
}

#[test]
fn book_080_export_round_trip() {
    let b = s0_overflow();
    assert_eq!(b.last_id(), 101);
    let mut out = vec![lv(0, 0); 300];
    let (nb, na, tr) = b.export(&mut out);
    assert_eq!((nb, na, tr), (4, 3, false));
    let px = |s: &[Level]| s.iter().map(|l| l.px.0).collect::<Vec<_>>();
    assert_eq!(px(&out[..4]), [10000, 9990, 9350, 9000]);
    assert_eq!(px(&out[4..7]), [10010, 10020, 10700]);
    let mut c = L2Book::new(&P);
    let h = snap_hdr(EXCHANGE, 101, 4, 3);
    c.apply_snapshot(&h, &out[..7]);
    assert_eq!(c.state_hash(), b.state_hash());
}

#[test]
fn book_081_export_truncation() {
    let rows: [(&str, usize, bool, (u16, u16, bool), &[i64]); 5] = [
        ("BOOK-081.1", 4, true, (2, 2, false), &[10000, 9990, 10010, 10020]),
        ("BOOK-081.2", 3, true, (1, 1, true), &[10000, 10010]),
        ("BOOK-081.3", 2, true, (1, 1, true), &[10000, 10010]),
        ("BOOK-081.4", 0, true, (0, 0, true), &[]),
        ("BOOK-081.5", 0, false, (0, 0, false), &[]),
    ];
    for (row, cap, with_s0, want, px) in rows {
        let b = if with_s0 { book_s0(&P) } else { L2Book::new(&P) };
        let mut out = vec![lv(0, 0); cap];
        let got = b.export(&mut out);
        assert_eq!(got, want, "{row}");
        let n = usize::from(got.0 + got.1);
        let p: Vec<i64> = out[..n].iter().map(|l| l.px.0).collect();
        assert_eq!(p, px, "{row}");
    }
}

#[test]
fn book_082_state_hash_scope() {
    let x = {
        let mut b = L2Book::new(&P);
        t(&mut b, 100, S0_BIDS, S0_ASKS);
        assert_eq!(b.meta().base, 936);
        b
    };
    // BOOK-082.1
    let mut y = L2Book::new(&P);
    t(&mut y, 50, &[(10300, 1)], &[(10310, 1)]);
    assert_eq!(y.meta().base, 966);
    t(&mut y, 100, S0_BIDS, S0_ASKS);
    assert_eq!(y.meta().base, 966, "BOOK-082.1");
    assert_eq!(x.state_hash(), y.state_hash(), "BOOK-082.1");
    // BOOK-082.2
    let mut y = L2Book::new(&P);
    t(&mut y, 100, S0_BIDS, S0_ASKS);
    y.mark_stale(StaleReason::Refdata);
    assert_ne!(x.state_hash(), y.state_hash(), "BOOK-082.2");
    // BOOK-082.3
    let mut y = L2Book::new(&P);
    t(&mut y, 101, S0_BIDS, S0_ASKS);
    assert_ne!(x.state_hash(), y.state_hash(), "BOOK-082.3");
    // BOOK-082.4
    let mut y = L2Book::new(&P);
    t(&mut y, 100, &[(10000, 5), (9990, 4)], S0_ASKS);
    assert_ne!(x.state_hash(), y.state_hash(), "BOOK-082.4");
}
