//! BOOK-068: code values.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
use hft_book::{BookErr, BookState, Outcome, StaleReason, TopOutcome};

#[test]
fn book_068_codes() {
    let s = [
        BookState::Uninit,
        BookState::Live,
        BookState::Stale,
        BookState::Resyncing,
    ];
    assert_eq!(s.map(|x| x as u8), [0, 1, 2, 3], "BOOK-068.1");
    let r = [
        StaleReason::Gap,
        StaleReason::Crossed,
        StaleReason::ConnDown,
        StaleReason::Split,
        StaleReason::BadLevel,
        StaleReason::Refdata,
    ];
    assert_eq!(r.map(|x| x as u8), [1, 2, 3, 4, 5, 6], "BOOK-068.2");
    let o1 = [
        Outcome::APPLIED,
        Outcome::BID_TOP,
        Outcome::ASK_TOP,
        Outcome::DUPLICATE,
        Outcome::NOT_LIVE,
        Outcome::GAP,
    ];
    assert_eq!(o1.map(|x| x.0), [1, 2, 4, 8, 16, 32], "BOOK-068.3");
    let o2 = [
        Outcome::CROSSED,
        Outcome::TRUNCATED,
        Outcome::RECENTERED,
        Outcome::INVALID,
        Outcome::WENT_STALE,
        Outcome::WENT_LIVE,
    ];
    assert_eq!(o2.map(|x| x.0), [64, 128, 256, 512, 1024, 2048], "BOOK-068.4");
    let t = [
        TopOutcome::BID_PX,
        TopOutcome::ASK_PX,
        TopOutcome::BID_QTY,
        TopOutcome::ASK_QTY,
        TopOutcome::DUPLICATE,
        TopOutcome::OLDER,
        TopOutcome::INVALID,
    ];
    assert_eq!(t.map(|x| x.0), [1, 2, 4, 8, 16, 32, 64], "BOOK-068.5");
    let e = [
        BookErr::MemTooSmall,
        BookErr::MemAlign,
        BookErr::BadParams,
        BookErr::UnknownInst,
        BookErr::DupInst,
    ];
    assert_eq!(e.map(|x| x as u8), [1, 2, 3, 4, 5], "BOOK-068.6");
}
