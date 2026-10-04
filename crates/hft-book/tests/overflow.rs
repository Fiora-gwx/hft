//! BOOK-050–BOOK-053: overflow area outside the window.
#![allow(clippy::type_complexity)]
mod common;

use common::*;
use hft_book::Outcome;

const A: Outcome = Outcome::APPLIED;

#[test]
fn book_050_overflow_visible() {
    let mut b = book_s0(&P);
    assert_eq!(batch(&mut b, 100, 101, &[(9000, 1), (9350, 2)], &[(10700, 3)]), A);
    assert_eq!(side(&b, Buy), [(10000, 5), (9990, 3), (9350, 2), (9000, 1)]);
    assert_eq!(side(&b, Sell), [(10010, 2), (10020, 4), (10700, 3)]);
}

#[test]
fn book_051_overflow_eviction() {
    let mut b = book_s1(&P);
    let rows: [(&str, i64, i64, Outcome, &[i64], u32); 7] = [
        ("BOOK-051.1", 9000, 1, A, &[9000], 0),
        ("BOOK-051.2", 8990, 1, A, &[9000, 8990], 0),
        ("BOOK-051.3", 8980, 1, A, &[9000, 8990, 8980], 0),
        ("BOOK-051.4", 8970, 1, A, &[9000, 8990, 8980, 8970], 0),
        (
            "BOOK-051.5",
            8960,
            1,
            A | Outcome::TRUNCATED,
            &[9000, 8990, 8980, 8970],
            1,
        ),
        (
            "BOOK-051.6",
            9010,
            1,
            A | Outcome::TRUNCATED,
            &[9010, 9000, 8990, 8980],
            2,
        ),
        ("BOOK-051.7", 8960, 0, A, &[9010, 9000, 8990, 8980], 2),
    ];
    for (k, (row, px, q, want, ovf, trunc)) in rows.into_iter().enumerate() {
        let k = 100 + k as u64;
        assert_eq!(batch(&mut b, k, k + 1, &[(px, q)], &[]), want, "{row}");
        let got: Vec<i64> = b.iter(Buy).skip(1).map(|l| l.px.0).collect();
        assert_eq!(got, ovf, "{row}");
        assert_eq!(b.meta().truncated, trunc, "{row}");
    }
}

#[test]
fn book_052_best_in_overflow_recenters() {
    let mut b = book_s1(&P);
    let o = batch(&mut b, 100, 101, &[(10000, 0), (9000, 1)], &[]);
    assert!(o.contains(A | Outcome::BID_TOP | Outcome::RECENTERED), "{o:?}");
    assert_eq!(b.best_bid(), some(9000, 1));
    assert_eq!(b.meta().base, 886);
}

#[test]
fn book_053_out_of_window_better_level() {
    let mut b = book_s1(&P);
    let o = batch(
        &mut b,
        100,
        101,
        &[(10000, 0), (10700, 1)],
        &[(10010, 0), (10710, 1)],
    );
    assert_eq!(b.best_bid(), some(10700, 1));
    assert_eq!(b.best_ask(), some(10710, 1));
    assert!(o.contains(Outcome::RECENTERED), "{o:?}");
    assert_eq!(b.meta().base, 1006);
}
