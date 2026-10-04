//! BOOK-045–BOOK-049: whole-message `apply_batch`.
mod common;

use common::*;
use hft_book::{BookState, Outcome, StaleReason};
use hft_event::ConnStateKind;

const A: Outcome = Outcome::APPLIED;

#[test]
fn book_045_batch_applies_whole_message() {
    let mut b = book_s0(&P);
    let o = batch(
        &mut b,
        100,
        101,
        &[(10000, 0), (9990, 3), (9980, 1)],
        &[(10010, 1)],
    );
    assert_eq!(o, A | Outcome::BID_TOP | Outcome::ASK_TOP);
    assert_eq!(b.best_bid(), some(9990, 3));
    assert_eq!(b.best_ask(), some(10010, 1));
    assert_eq!(b.last_id(), 101);
}

#[test]
fn book_046_batch_atomic_reject() {
    let mut b = book_s0(&P);
    let o = batch(&mut b, 100, 101, &[(10000, 9), (9985, 1)], &[(10010, 1)]);
    assert_eq!(o, Outcome::INVALID | Outcome::WENT_STALE);
    assert_eq!(b.state(), BookState::Stale);
    assert_eq!(b.meta().stale_reason, StaleReason::BadLevel as u8);
    assert_eq!(b.last_id(), 100);
    assert_eq!(side(&b, Buy), S0_BIDS);
    assert_eq!(side(&b, Sell), S0_ASKS);
}

#[test]
fn book_047_batch_failures() {
    // BOOK-047.1
    let mut b = book_s0(&P);
    assert_eq!(
        batch(&mut b, 99, 100, &[(10000, 9)], &[]),
        Outcome::DUPLICATE,
        "BOOK-047.1"
    );
    assert_eq!(side(&b, Buy), S0_BIDS);
    // BOOK-047.2
    let mut b = book_s0(&P);
    assert_eq!(
        batch(&mut b, 100, 100, &[(10000, 9)], &[]),
        Outcome::DUPLICATE,
        "BOOK-047.2"
    );
    // BOOK-047.3
    let mut b = book_s0(&P);
    assert_eq!(
        batch(&mut b, 101, 102, &[(10000, 9)], &[]),
        Outcome::GAP | Outcome::WENT_STALE,
        "BOOK-047.3"
    );
    assert_eq!(b.state(), BookState::Stale);
    assert_eq!(b.meta().stale_reason, StaleReason::Gap as u8);
    assert_eq!(b.best_bid(), some(10000, 5));
    // BOOK-047.4: n_bids = 2 with one level.
    let mut b = book_s0(&P);
    let h = batch_hdr(100, 101, 2, 0);
    assert_eq!(
        b.apply_batch(&h, &[lv(10000, 9)]),
        Outcome::INVALID | Outcome::WENT_STALE,
        "BOOK-047.4"
    );
    assert_eq!(b.meta().stale_reason, StaleReason::BadLevel as u8);
    assert_eq!(side(&b, Buy), S0_BIDS);
    // BOOK-047.5
    let mut b = book_s0(&P);
    b.on_conn(ConnStateKind::Resync);
    assert_eq!(
        batch(&mut b, 100, 101, &[(10000, 9)], &[]),
        Outcome::NOT_LIVE,
        "BOOK-047.5"
    );
}

#[test]
fn book_049_batch_top_bits_net() {
    let mut b = book_s0(&P);
    assert_eq!(batch(&mut b, 100, 101, &[(10000, 0), (10000, 5)], &[]), A);
    assert_eq!(b.best_bid(), some(10000, 5));
    assert_eq!(b.last_id(), 101);
}
