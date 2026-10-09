//! BOOK-004, BOOK-073: `BookSet` construction and the Top/L2 independence.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
use crate::common;

use common::*;
use hft_book::{BookErr, BookSet, Top};
use hft_types::{InstId, Px, Seq};

#[test]
fn book_004_bookset_new_in() {
    let rd = refdata(5, 10, 64, 0);
    let ids = |v: &[u32]| v.iter().map(|i| InstId(*i)).collect::<Vec<_>>();
    let big = || aligned_mem(1 << 20);
    // BOOK-004.1
    let s = match BookSet::new_in(rd, &ids(&[2, 4]), big()) {
        Ok(s) => s,
        Err(e) => panic!("BOOK-004.1: {e:?}"),
    };
    for i in 1..=5 {
        assert_eq!(s.l2(InstId(i)).is_some(), i == 2 || i == 4, "BOOK-004.1 inst {i}");
        assert_eq!(*s.top(InstId(i)), Top::ZERO, "BOOK-004.1 inst {i}");
    }
    // BOOK-004.2
    let s = match BookSet::new_in(rd, &[], big()) {
        Ok(s) => s,
        Err(e) => panic!("BOOK-004.2: {e:?}"),
    };
    assert!((1..=5).all(|i| s.l2(InstId(i)).is_none()), "BOOK-004.2");
    // BOOK-004.3–BOOK-004.5
    let err = |v: &[u32], m: &'static mut [u8]| BookSet::new_in(rd, &ids(v), m).err();
    assert_eq!(err(&[2, 9], big()), Some(BookErr::UnknownInst), "BOOK-004.3");
    assert_eq!(err(&[2, 2], big()), Some(BookErr::DupInst), "BOOK-004.4");
    assert_eq!(
        err(&[2, 4], aligned_mem(16)),
        Some(BookErr::MemTooSmall),
        "BOOK-004.5"
    );
}

#[test]
fn book_073_top_and_l2_independent() {
    let mut s = bookset(1, &[1]);
    let o = s.top_mut(InstId(1)).apply(&bbo(5, (200, 1), (210, 1), 1), Seq(1));
    assert!(!o.contains(hft_book::TopOutcome::INVALID));
    let Some(b) = s.l2_mut(InstId(1)) else {
        panic!("no L2 book")
    };
    let o = s0(b);
    assert_eq!(o, hft_book::Outcome::APPLIED | hft_book::Outcome::WENT_LIVE);
    assert_eq!(s.top(InstId(1)).bid_px, Px(200));
    assert_eq!(s.l2(InstId(1)).and_then(|b| b.best_bid()), some(10000, 5));
}
