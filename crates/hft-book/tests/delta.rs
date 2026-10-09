//! BOOK-030–BOOK-041: incremental levels (`apply_delta`).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)]
use crate::common;

use common::*;
use hft_book::{BookState, L2Book, Outcome, StaleReason};
use hft_event::{ConnStateKind, L2Delta};
use hft_types::Side;

const A: Outcome = Outcome::APPLIED;

fn stale(b: &L2Book, r: StaleReason) -> bool {
    b.state() == BookState::Stale && b.meta().stale_reason == r as u8
}

#[test]
fn book_030_single_level_message() {
    let mut b = book_s0(&P);
    assert_eq!(
        b.apply_delta(&delta(100, 101, Sell, 10010, 1, 0, true)),
        A | Outcome::ASK_TOP
    );
    assert_eq!(b.best_ask(), some(10010, 1));
    assert_eq!(b.last_id(), 101);
}

#[test]
fn book_031_multi_level_message() {
    let mut b = book_s0(&P);
    assert_eq!(b.apply_delta(&delta(100, 101, Buy, 9990, 0, 0, false)), A);
    assert_eq!(b.last_id(), 100);
    assert_eq!(b.apply_delta(&delta(100, 101, Sell, 10020, 7, 1, false)), A);
    assert_eq!(b.last_id(), 100);
    assert_eq!(
        b.apply_delta(&delta(100, 101, Buy, 10000, 6, 2, true)),
        A | Outcome::BID_TOP
    );
    assert_eq!(b.last_id(), 101);
    assert_eq!(side(&b, Buy), [(10000, 6)]);
    assert_eq!(side(&b, Sell), [(10010, 2), (10020, 7)]);
}

#[test]
fn book_032_gap() {
    let rows = [("BOOK-032.1", 100u64), ("BOOK-032.2", 101), ("BOOK-032.3", 99)];
    for (row, prev) in rows {
        let mut b = book_s0(&P);
        let o = b.apply_delta(&delta(prev, 105, Buy, 10000, 1, 0, true));
        if prev == 100 {
            assert_eq!(o, A | Outcome::BID_TOP, "{row}");
            assert_eq!(b.state(), BookState::Live, "{row}");
            assert_eq!(b.last_id(), 105, "{row}");
        } else {
            assert_eq!(o, Outcome::GAP | Outcome::WENT_STALE, "{row}");
            assert!(stale(&b, StaleReason::Gap), "{row}");
            assert_eq!(b.best_bid(), some(10000, 5), "{row}");
            assert_eq!(b.last_id(), 100, "{row}");
        }
    }
}

#[test]
fn book_033_duplicate_message() {
    let mut b = book_s0(&P);
    let m = [
        delta(100, 101, Buy, 10000, 6, 0, false),
        delta(100, 101, Sell, 10010, 1, 1, true),
    ];
    for d in &m {
        assert!(b.apply_delta(d).contains(A));
    }
    for d in &m {
        assert_eq!(b.apply_delta(d), Outcome::DUPLICATE);
    }
    assert_eq!(
        b.apply_delta(&delta(99, 100, Buy, 9990, 9, 0, true)),
        Outcome::DUPLICATE
    );
    assert_eq!(b.last_id(), 101);
    assert_eq!(b.best_bid(), some(10000, 6));
    assert_eq!(side(&b, Buy), [(10000, 6), (9990, 3)]);
    assert_eq!(
        b.apply_delta(&delta(101, 102, Buy, 10000, 7, 0, true)),
        A | Outcome::BID_TOP
    );
    assert_eq!(b.last_id(), 102);
}

#[test]
fn book_034_split() {
    let d0 = delta(100, 101, Buy, 10000, 6, 0, false);
    let rows: Vec<(&str, Vec<L2Delta>, bool)> = vec![
        (
            "BOOK-034.1",
            vec![delta(100, 101, Buy, 10000, 6, 1, false)],
            false,
        ),
        (
            "BOOK-034.2",
            vec![d0, delta(100, 101, Sell, 10010, 1, 2, false)],
            false,
        ),
        (
            "BOOK-034.3",
            vec![d0, delta(100, 102, Sell, 10010, 1, 1, false)],
            false,
        ),
        (
            "BOOK-034.4",
            vec![d0, delta(101, 102, Buy, 9990, 1, 0, true)],
            false,
        ),
        (
            "BOOK-034.5",
            vec![d0, delta(100, 101, Sell, 10010, 1, 1, true)],
            true,
        ),
    ];
    for (row, seq, ok) in rows {
        let mut b = book_s0(&P);
        let mut o = Outcome::NONE;
        for d in &seq {
            o = b.apply_delta(d);
        }
        if ok {
            assert_eq!(o, A | Outcome::ASK_TOP, "{row}");
            assert_eq!(b.state(), BookState::Live, "{row}");
            assert_eq!(b.last_id(), 101, "{row}");
        } else {
            assert_eq!(o, Outcome::INVALID | Outcome::WENT_STALE, "{row}");
            assert!(stale(&b, StaleReason::Split), "{row}");
        }
    }
}

#[test]
fn book_035_bad_level() {
    let rows: [(&str, Side, i64, i64, bool); 5] = [
        ("BOOK-035.1", Buy, 10005, 1, false),
        ("BOOK-035.2", Buy, 0, 1, false),
        ("BOOK-035.3", Buy, -10, 1, false),
        ("BOOK-035.4", Sell, 10010, -1, false),
        ("BOOK-035.5", Sell, 10010, 0, true),
    ];
    for (row, s, px, q, ok) in rows {
        let mut b = book_s0(&P);
        assert_eq!(
            b.apply_delta(&delta(100, 101, Buy, 9990, 4, 0, false)),
            A,
            "{row}"
        );
        let o2 = b.apply_delta(&delta(100, 101, s, px, q, 1, false));
        let o3 = b.apply_delta(&delta(100, 101, Sell, 10010, 1, 2, true));
        if ok {
            assert_eq!(o2, A | Outcome::ASK_TOP, "{row}");
            assert_eq!(o3, A | Outcome::ASK_TOP, "{row}");
            assert_eq!(b.state(), BookState::Live, "{row}");
        } else {
            assert_eq!(o2, Outcome::INVALID | Outcome::WENT_STALE, "{row}");
            assert!(stale(&b, StaleReason::BadLevel), "{row}");
            assert_eq!(o3, Outcome::NOT_LIVE, "{row}");
            assert_eq!(b.last_id(), 100, "{row}");
        }
    }
}

#[test]
fn book_036_delete_missing_level() {
    let mut b = book_s0(&P);
    assert_eq!(b.apply_delta(&delta(100, 101, Buy, 9500, 0, 0, true)), A);
    assert_eq!(side(&b, Buy), S0_BIDS);
    assert_eq!(b.last_id(), 101);
}

#[test]
fn book_037_not_live_drops() {
    let rows: [(&str, fn() -> L2Book, u64); 3] = [
        ("BOOK-037.1", || L2Book::new(&P), 0),
        (
            "BOOK-037.2",
            || {
                let mut b = book_s0(&P);
                b.mark_stale(StaleReason::Refdata);
                b
            },
            100,
        ),
        (
            "BOOK-037.3",
            || {
                let mut b = book_s0(&P);
                b.on_conn(ConnStateKind::Resync);
                b
            },
            100,
        ),
    ];
    for (row, mk, last) in rows {
        let mut b = mk();
        let before = (side(&b, Buy), side(&b, Sell));
        assert_eq!(
            b.apply_delta(&delta(100, 101, Buy, 10000, 1, 0, true)),
            Outcome::NOT_LIVE,
            "{row}"
        );
        assert_eq!(
            batch(&mut b, 100, 101, &[(10000, 1)], &[]),
            Outcome::NOT_LIVE,
            "{row}"
        );
        assert_eq!(b.last_id(), last, "{row}");
        assert_eq!((side(&b, Buy), side(&b, Sell)), before, "{row}");
    }
}

#[test]
fn book_038_best_remove_bitmap_scan() {
    let mut b = L2Book::new(&P);
    snap(&mut b, EXCHANGE, 100, &[(10000, 5), (9360, 2)], &[(10010, 2)]);
    assert_eq!(b.meta().base, 936);
    assert_eq!(
        b.apply_delta(&delta(100, 101, Buy, 10000, 0, 0, true)),
        A | Outcome::BID_TOP
    );
    assert_eq!(b.best_bid(), some(9360, 2));
    assert_eq!(b.meta().base, 936);
}

#[test]
fn book_039_crossed_tolerance() {
    let mut b = book_s1(&p_tol(2));
    let ab = A | Outcome::BID_TOP;
    let rows: [(&str, i64, i64, Option<Outcome>, bool, u16); 6] = [
        ("BOOK-039.1", 10010, 1, Some(ab | Outcome::CROSSED), true, 1),
        ("BOOK-039.2", 10010, 2, Some(ab | Outcome::CROSSED), true, 2),
        ("BOOK-039.3", 10010, 0, Some(ab), true, 0),
        ("BOOK-039.4", 10020, 1, None, true, 1),
        ("BOOK-039.5", 10020, 2, None, true, 2),
        (
            "BOOK-039.6",
            10020,
            3,
            Some(ab | Outcome::CROSSED | Outcome::WENT_STALE),
            false,
            3,
        ),
    ];
    for (k, (row, px, q, want, live, run)) in rows.into_iter().enumerate() {
        let id = 100 + k as u64;
        let o = b.apply_delta(&delta(id, id + 1, Buy, px, q, 0, true));
        if let Some(w) = want {
            assert_eq!(o, w, "{row}");
        }
        assert_eq!(b.meta().crossed_run, run, "{row}");
        if live {
            assert_eq!(b.state(), BookState::Live, "{row}");
        } else {
            assert!(stale(&b, StaleReason::Crossed), "{row}");
        }
    }
}

#[test]
fn book_040_zero_tolerance() {
    let mut b = book_s1(&P);
    assert_eq!(
        b.apply_delta(&delta(100, 101, Buy, 10010, 1, 0, true)),
        A | Outcome::BID_TOP | Outcome::CROSSED | Outcome::WENT_STALE
    );
    assert!(stale(&b, StaleReason::Crossed));
}

#[test]
fn book_041_top_bits() {
    let ask_cross = A | Outcome::ASK_TOP | Outcome::CROSSED | Outcome::WENT_STALE;
    let rows: [(&str, Side, i64, i64, Outcome); 5] = [
        ("BOOK-041.1", Buy, 10000, 9, A | Outcome::BID_TOP),
        ("BOOK-041.2", Buy, 9990, 9, A),
        ("BOOK-041.3", Sell, 10010, 2, A),
        ("BOOK-041.4", Buy, 9980, 1, A),
        ("BOOK-041.5", Sell, 10000, 1, ask_cross),
    ];
    for (row, s, px, q, want) in rows {
        let mut b = book_s0(&P);
        assert_eq!(b.apply_delta(&delta(100, 101, s, px, q, 0, true)), want, "{row}");
    }
}
