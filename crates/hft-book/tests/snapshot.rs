//! BOOK-010–BOOK-017: Exchange and Keyframe snapshots.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)]
use crate::common;

use common::*;
use hft_book::{BookState, L2Book, Outcome, StaleReason};
use hft_event::ConnStateKind;
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use hft_types::InstId;

const A: Outcome = Outcome::APPLIED;

#[test]
fn book_010_exchange_snapshot_goes_live() {
    let mut b = L2Book::new(&P);
    assert_eq!(s0(&mut b), A | Outcome::WENT_LIVE);
    assert_eq!(b.state(), BookState::Live);
    assert_eq!(b.last_id(), 100);
    assert_eq!(b.best_bid(), some(10000, 5));
    assert_eq!(b.best_ask(), some(10010, 2));
    let m = b.meta();
    assert_eq!((m.base, m.origin, m.stale_reason), (936, 1, 0));
}

#[test]
fn book_011_one_sided_snapshot() {
    let rows: [(&str, &[(i64, i64)], &[(i64, i64)], i64); 3] = [
        ("BOOK-011.1", &[], &[(10010, 2)], 937),
        ("BOOK-011.2", &[(10000, 5)], &[], 936),
        ("BOOK-011.3", &[], &[], 0),
    ];
    for (row, bids, asks, base) in rows {
        let mut b = L2Book::new(&P);
        assert_eq!(
            snap(&mut b, EXCHANGE, 100, bids, asks),
            A | Outcome::WENT_LIVE,
            "{row}"
        );
        assert_eq!(b.state(), BookState::Live, "{row}");
        assert_eq!(b.best_bid(), bids.first().map(|&(p, q)| lv(p, q)), "{row}");
        assert_eq!(b.best_ask(), asks.first().map(|&(p, q)| lv(p, q)), "{row}");
        assert_eq!(b.meta().base, base, "{row}");
    }
}

#[test]
fn book_012_rejected_snapshot() {
    type Lv = &'static [(i64, i64)];
    let rows: [(&str, Lv, Lv, StaleReason); 5] = [
        ("BOOK-012.1", &[(10010, 1)], &[(10010, 1)], StaleReason::Crossed),
        ("BOOK-012.2", &[(10020, 1)], &[(10010, 1)], StaleReason::Crossed),
        ("BOOK-012.3", &[(10005, 1)], &[], StaleReason::BadLevel),
        ("BOOK-012.4", &[(0, 1)], &[], StaleReason::BadLevel),
        ("BOOK-012.5", &[], &[(10010, -1)], StaleReason::BadLevel),
    ];
    for (row, bids, asks, why) in rows {
        // Uninit book.
        let mut b = L2Book::new(&P);
        assert_eq!(
            snap(&mut b, EXCHANGE, 200, bids, asks),
            Outcome::INVALID,
            "{row} uninit"
        );
        assert_eq!(b.state(), BookState::Stale, "{row}");
        assert_eq!(b.meta().stale_reason, why as u8, "{row}");
        assert_eq!(b.last_id(), 0, "{row}");
        assert_eq!(b.iter(Buy).count() + b.iter(Sell).count(), 0, "{row}");
        // Live book.
        let mut b = book_s0(&P);
        assert_eq!(
            snap(&mut b, EXCHANGE, 200, bids, asks),
            Outcome::INVALID | Outcome::WENT_STALE,
            "{row} live"
        );
        assert_eq!(b.state(), BookState::Stale, "{row}");
        assert_eq!(b.meta().stale_reason, why as u8, "{row}");
        assert_eq!(b.last_id(), 100, "{row}");
        assert_eq!(side(&b, Buy), S0_BIDS, "{row}");
        assert_eq!(side(&b, Sell), S0_ASKS, "{row}");
    }
}

#[test]
fn book_013_snapshot_zero_and_dup_levels() {
    let mut b = L2Book::new(&P);
    let o = snap(
        &mut b,
        EXCHANGE,
        100,
        &[(10000, 0), (9990, 3), (9990, 7)],
        &[(10010, 2)],
    );
    assert_eq!(o, A | Outcome::WENT_LIVE);
    assert_eq!(b.best_bid(), some(9990, 7));
    assert_eq!(side(&b, Buy), [(9990, 7)]);
}

#[test]
fn book_014_snapshot_replaces_levels() {
    let mut b = book_s0(&P);
    assert_eq!(snap(&mut b, EXCHANGE, 200, &[(10100, 1)], &[(10110, 1)]), A);
    assert_eq!(b.last_id(), 200);
    assert_eq!(side(&b, Buy), [(10100, 1)]);
    assert_eq!(side(&b, Sell), [(10110, 1)]);
}

#[test]
fn book_015_snapshot_from_non_live() {
    let rows: [(&str, fn(&mut L2Book), StaleReason); 3] = [
        (
            "BOOK-015.1",
            |b| {
                b.apply_delta(&delta(99, 101, Buy, 10000, 1, 0, true));
            },
            StaleReason::Gap,
        ),
        (
            "BOOK-015.2",
            |b| {
                b.on_conn(ConnStateKind::Resync);
            },
            StaleReason::Gap,
        ),
        (
            "BOOK-015.3",
            |b| {
                b.on_conn(ConnStateKind::Down);
            },
            StaleReason::ConnDown,
        ),
    ];
    for (row, prep, _) in rows {
        let mut b = book_s0(&P);
        prep(&mut b);
        assert_ne!(b.state(), BookState::Live, "{row}");
        assert_eq!(
            snap(&mut b, EXCHANGE, 200, S0_BIDS, S0_ASKS),
            A | Outcome::WENT_LIVE,
            "{row}"
        );
        assert_eq!(b.state(), BookState::Live, "{row}");
        assert_eq!(b.meta().stale_reason, 0, "{row}");
        assert_eq!(b.last_id(), 200, "{row}");
    }
}

#[test]
fn book_016_keyframe_keeps_restored_meta() {
    let mut set = bookset(7, &[7]);
    let mut w = SnapshotWriter::new(4096);
    w.begin_section(hft_book::SECTION_BOOK_META, hft_book::ROW_VERSION);
    w.put_row(&hft_book::BookMetaRow {
        inst: InstId(7),
        state: BookState::Live as u8,
        stale_reason: 0,
        origin: TOP_N,
        _pad0: 0,
        last_id: 100,
        base: 930,
        truncated: 5,
        crossed_run: 1,
        _pad1: [0; 2],
    });
    w.end_section();
    assert_eq!(set.restore(&mut SnapshotReader::new(w.as_bytes())), Ok(()));
    let Some(b) = set.l2_mut(InstId(7)) else {
        panic!("no book")
    };
    snap(b, KEYFRAME, 100, &[(10000, 5)], &[(10010, 2)]);
    assert_eq!(b.state(), BookState::Live);
    let m = b.meta();
    assert_eq!(
        (m.origin, m.base, m.truncated, m.crossed_run, m.last_id),
        (3, 930, 5, 1, 100)
    );
    assert_eq!(b.best_bid(), some(10000, 5));
    assert_eq!(batch(b, 100, 101, &[(10000, 6)], &[]), Outcome::INVALID);
    assert_eq!(b.state(), BookState::Live);
    assert_eq!(b.best_bid(), some(10000, 5));
    assert_eq!(b.last_id(), 100);
}

#[test]
fn book_017_keyframe_bootstrap() {
    let mut b = L2Book::new(&P);
    assert_eq!(
        snap(&mut b, KEYFRAME, 100, S0_BIDS, S0_ASKS),
        A | Outcome::WENT_LIVE
    );
    assert_eq!((b.meta().origin, b.meta().base), (2, 936));
    assert_eq!(batch(&mut b, 100, 101, &[(10000, 6)], &[]), A | Outcome::BID_TOP);
    assert_eq!(b.last_id(), 101);
}
