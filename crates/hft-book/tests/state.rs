//! BOOK-065–BOOK-067: state machine.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
use crate::common;

use common::*;
use hft_book::{BookState, L2Book, Outcome, StaleReason};
use hft_event::ConnStateKind;

#[derive(Clone, Copy, Debug)]
enum Start {
    Live,
    StaleGap,
    Resyncing,
    Uninit,
}

fn start(s: Start) -> L2Book {
    match s {
        Start::Uninit => L2Book::new(&P),
        Start::Live => book_s0(&P),
        Start::StaleGap => {
            let mut b = book_s0(&P);
            b.apply_delta(&delta(99, 101, Buy, 10000, 1, 0, true));
            b
        }
        Start::Resyncing => {
            let mut b = book_s0(&P);
            b.on_conn(ConnStateKind::Resync);
            b
        }
    }
}

fn st(b: &L2Book) -> (BookState, u8) {
    (b.state(), b.meta().stale_reason)
}

#[test]
fn book_065_on_conn_transitions() {
    use ConnStateKind as K;
    let stale = |r: StaleReason| (BookState::Stale, r as u8);
    let rows = [
        (
            "BOOK-065.1",
            Start::Live,
            K::Down,
            Outcome::WENT_STALE,
            Some(stale(StaleReason::ConnDown)),
        ),
        ("BOOK-065.2", Start::Live, K::Resync, Outcome::WENT_STALE, None),
        (
            "BOOK-065.3",
            Start::Live,
            K::Up,
            Outcome::NONE,
            Some((BookState::Live, 0)),
        ),
        (
            "BOOK-065.4",
            Start::Live,
            K::Rotated,
            Outcome::NONE,
            Some((BookState::Live, 0)),
        ),
        (
            "BOOK-065.5",
            Start::Live,
            K::Degraded,
            Outcome::NONE,
            Some((BookState::Live, 0)),
        ),
        (
            "BOOK-065.6",
            Start::Live,
            K::Alive,
            Outcome::NONE,
            Some((BookState::Live, 0)),
        ),
        (
            "BOOK-065.7",
            Start::StaleGap,
            K::Down,
            Outcome::NONE,
            Some(stale(StaleReason::Gap)),
        ),
        ("BOOK-065.8", Start::StaleGap, K::Resync, Outcome::NONE, None),
        (
            "BOOK-065.9",
            Start::Resyncing,
            K::Down,
            Outcome::NONE,
            Some(stale(StaleReason::ConnDown)),
        ),
        ("BOOK-065.10", Start::Resyncing, K::Resync, Outcome::NONE, None),
        (
            "BOOK-065.11",
            Start::Uninit,
            K::Down,
            Outcome::NONE,
            Some((BookState::Uninit, 0)),
        ),
        (
            "BOOK-065.12",
            Start::Uninit,
            K::Resync,
            Outcome::NONE,
            Some((BookState::Uninit, 0)),
        ),
    ];
    for (row, s, k, want, after) in rows {
        let mut b = start(s);
        assert_eq!(b.on_conn(k), want, "{row}");
        match after {
            Some(x) => assert_eq!(st(&b), x, "{row}"),
            None => assert_eq!(b.state(), BookState::Resyncing, "{row}"),
        }
    }
}

#[test]
fn book_066_mark_stale() {
    let rows = [
        ("BOOK-066.1", Start::Live, BookState::Stale),
        ("BOOK-066.2", Start::StaleGap, BookState::Stale),
        ("BOOK-066.3", Start::Resyncing, BookState::Stale),
        ("BOOK-066.4", Start::Uninit, BookState::Uninit),
    ];
    for (row, s, want) in rows {
        let mut b = start(s);
        b.mark_stale(StaleReason::Refdata);
        assert_eq!(b.state(), want, "{row}");
        if want == BookState::Stale {
            assert_eq!(b.meta().stale_reason, 6, "{row}");
        }
    }
}

#[test]
fn book_067_gap_resync_cycle() {
    let mut b = book_s0(&P);
    assert_eq!(
        b.apply_delta(&delta(101, 102, Buy, 10000, 1, 0, true)),
        Outcome::GAP | Outcome::WENT_STALE
    );
    for k in 102..105 {
        assert_eq!(
            b.apply_delta(&delta(k, k + 1, Buy, 10000, 1, 0, true)),
            Outcome::NOT_LIVE
        );
    }
    assert_eq!(b.on_conn(ConnStateKind::Resync), Outcome::NONE);
    assert_eq!(b.state(), BookState::Resyncing);
    assert_eq!(
        b.apply_delta(&delta(105, 106, Buy, 10000, 1, 0, true)),
        Outcome::NOT_LIVE
    );
    assert_eq!(
        snap(&mut b, EXCHANGE, 300, S0_BIDS, S0_ASKS),
        Outcome::APPLIED | Outcome::WENT_LIVE
    );
    assert_eq!(
        b.apply_delta(&delta(300, 301, Sell, 10010, 1, 0, true)),
        Outcome::APPLIED | Outcome::ASK_TOP
    );
    assert_eq!(b.last_id(), 301);
}
