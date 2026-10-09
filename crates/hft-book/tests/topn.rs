//! BOOK-020–BOOK-025: TopN snapshot books.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
use crate::common;

use common::*;
use hft_book::{BookParams, BookState, L2Book, Outcome, StaleReason};
use hft_event::ConnStateKind;

const A: Outcome = Outcome::APPLIED;

fn t11() -> L2Book {
    let mut b = L2Book::new(&P);
    t(&mut b, 11, &[(10000, 4)], &[(10010, 1)]);
    b
}

#[test]
fn book_020_topn_full_replace() {
    let mut b = L2Book::new(&P);
    assert_eq!(
        t(&mut b, 10, &[(10000, 5), (9990, 3)], &[(10010, 2), (10020, 4)]),
        A | Outcome::WENT_LIVE
    );
    assert_eq!(b.meta().origin, 3);
    assert_eq!(t(&mut b, 11, &[(10000, 4)], &[(10010, 1)]), A);
    assert_eq!(side(&b, Buy), [(10000, 4)]);
    assert_eq!(side(&b, Sell), [(10010, 1)]);
    assert_eq!(b.last_id(), 11);
}

#[test]
fn book_021_topn_dedup() {
    let rows = [
        ("BOOK-021.1", 10, Outcome::DUPLICATE, (10000, 4), 11),
        ("BOOK-021.2", 11, Outcome::DUPLICATE, (10000, 4), 11),
        ("BOOK-021.3", 12, A, (10000, 9), 12),
        ("BOOK-021.4", 0, A, (10000, 9), 0),
    ];
    for (row, id, want, bid, last) in rows {
        let mut b = t11();
        assert_eq!(t(&mut b, id, &[(10000, 9)], &[(10010, 9)]), want, "{row}");
        assert_eq!(b.best_bid(), some(bid.0, bid.1), "{row}");
        assert_eq!(b.last_id(), last, "{row}");
        if want == Outcome::DUPLICATE {
            assert_eq!(b.best_ask(), some(10010, 1), "{row}");
        }
    }
}

#[test]
fn book_022_topn_rejects_increments() {
    // BOOK-022.1
    let mut b = t11();
    assert_eq!(
        batch(&mut b, 11, 12, &[(10000, 1)], &[]),
        Outcome::INVALID,
        "BOOK-022.1"
    );
    assert_eq!(b.state(), BookState::Live);
    assert_eq!(b.best_bid(), some(10000, 4));
    assert_eq!(b.last_id(), 11);
    // BOOK-022.2
    let mut b = t11();
    assert_eq!(
        b.apply_delta(&delta(11, 12, Buy, 10000, 1, 0, true)),
        Outcome::INVALID,
        "BOOK-022.2"
    );
    assert_eq!(b.state(), BookState::Live);
    assert_eq!(side(&b, Buy), [(10000, 4)]);
    // BOOK-022.3
    let mut b = t11();
    b.on_conn(ConnStateKind::Down);
    assert_eq!(
        batch(&mut b, 11, 12, &[(10000, 1)], &[]),
        Outcome::INVALID,
        "BOOK-022.3"
    );
    assert_eq!(b.state(), BookState::Stale);
    assert_eq!(b.meta().stale_reason, StaleReason::ConnDown as u8);
}

#[test]
fn book_023_topn_down_then_live() {
    let mut b = t11();
    assert_eq!(b.on_conn(ConnStateKind::Down), Outcome::WENT_STALE);
    assert_eq!(b.state(), BookState::Stale);
    assert_eq!(b.meta().stale_reason, StaleReason::ConnDown as u8);
    assert_eq!(
        t(&mut b, 12, &[(10000, 5)], &[(10010, 2)]),
        A | Outcome::WENT_LIVE
    );
    assert_eq!(b.state(), BookState::Live);
    assert_eq!(b.last_id(), 12);
}

#[test]
fn book_024_topn_base_kept() {
    let rows = [
        ("BOOK-024.1", 10300, 10310, 936),
        ("BOOK-024.2", 10310, 10320, 936),
        ("BOOK-024.3", 10320, 10330, 968),
        ("BOOK-024.4", 9680, 9690, 936),
        ("BOOK-024.5", 9670, 9680, 903),
    ];
    for (row, bid, ask, base) in rows {
        let mut b = L2Book::new(&P);
        t(&mut b, 10, &[(10000, 5)], &[(10010, 2)]);
        assert_eq!(b.meta().base, 936, "{row}");
        assert_eq!(t(&mut b, 11, &[(bid, 1)], &[(ask, 1)]), A, "{row}");
        assert_eq!(b.meta().base, base, "{row}");
    }
}

#[test]
fn book_025_topn_crossed_no_tolerance() {
    let p = BookParams {
        crossed_tolerance: 5,
        ..P
    };
    let mut b = L2Book::new(&p);
    t(&mut b, 10, &[(10000, 5)], &[(10010, 2)]);
    assert_eq!(
        t(&mut b, 11, &[(10010, 1)], &[(10010, 1)]),
        Outcome::INVALID | Outcome::WENT_STALE
    );
    assert_eq!(b.state(), BookState::Stale);
    assert_eq!(b.meta().stale_reason, StaleReason::Crossed as u8);
    assert_eq!(b.best_bid(), some(10000, 5));
    assert_eq!(b.last_id(), 10);
}
