//! BOOK-001–BOOK-003: parameters, construction and the empty book.
#![allow(clippy::type_complexity)]
mod common;

use common::*;
use hft_book::{BookErr, BookParams, BookState, L2Book};
use hft_refdata::InstHot;

#[test]
fn book_001_from_inst_defaults() {
    let h = InstHot {
        tick: 10,
        book_window: 2048,
        crossed_tolerance: 2,
        ..InstHot::NONE
    };
    assert_eq!(
        BookParams::from_inst(&h),
        BookParams {
            tick: 10,
            window: 2048,
            ovf_cap: 64,
            crossed_tolerance: 2
        }
    );
}

#[test]
fn book_002_new_in_checks() {
    let n = L2Book::bytes_needed(&P);
    let with = |f: fn(&mut BookParams)| {
        let mut p = P;
        f(&mut p);
        L2Book::new_in(&p, aligned_mem(L2Book::bytes_needed(&p))).map(|b| b.state())
    };
    let rows: Vec<(&str, Result<BookState, BookErr>, Result<BookState, BookErr>)> = vec![
        (
            "BOOK-002.1",
            L2Book::new_in(&P, aligned_mem(n)).map(|b| b.state()),
            Ok(BookState::Uninit),
        ),
        (
            "BOOK-002.2",
            L2Book::new_in(&P, aligned_mem(n - 1)).map(|b| b.state()),
            Err(BookErr::MemTooSmall),
        ),
        (
            "BOOK-002.3",
            L2Book::new_in(&P, misaligned_mem(n + 64)).map(|b| b.state()),
            Err(BookErr::MemAlign),
        ),
        ("BOOK-002.4", with(|p| p.tick = 0), Err(BookErr::BadParams)),
        ("BOOK-002.5", with(|p| p.tick = -10), Err(BookErr::BadParams)),
        ("BOOK-002.6", with(|p| p.window = 63), Err(BookErr::BadParams)),
        ("BOOK-002.7", with(|p| p.window = 64), Ok(BookState::Uninit)),
        ("BOOK-002.8", with(|p| p.window = 65536), Ok(BookState::Uninit)),
        ("BOOK-002.9", with(|p| p.window = 65537), Err(BookErr::BadParams)),
        ("BOOK-002.10", with(|p| p.ovf_cap = 0), Err(BookErr::BadParams)),
        ("BOOK-002.11", with(|p| p.ovf_cap = 1), Ok(BookState::Uninit)),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn book_003_new_book_is_empty() {
    let a = L2Book::new(&P);
    let b = match L2Book::new_in(&P, aligned_mem(L2Book::bytes_needed(&P))) {
        Ok(b) => b,
        Err(e) => panic!("{e:?}"),
    };
    for x in [&a, &b] {
        assert_eq!(x.state(), BookState::Uninit);
        assert_eq!(x.last_id(), 0);
        assert_eq!(x.best_bid(), None);
        assert_eq!(x.best_ask(), None);
        let m = x.meta();
        assert_eq!(
            (m.state, m.stale_reason, m.origin, m.truncated, m.crossed_run),
            (0, 0, 0, 0, 0)
        );
    }
    assert_eq!(a.state_hash(), b.state_hash());
}
