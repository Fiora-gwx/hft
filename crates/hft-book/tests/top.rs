//! BOOK-070, BOOK-072: `Top::apply`.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)]
use crate::common;

use common::*;
use hft_book::{Top, TopOutcome as T};
use hft_types::{Px, Qty, Seq, Tsc};

#[test]
fn book_070_top_apply() {
    let all = T::BID_PX | T::ASK_PX | T::BID_QTY | T::ASK_QTY;
    // (row, update_id, bid, ask, want)
    let rows: [(&str, u64, (i64, i64), (i64, i64), T); 10] = [
        ("BOOK-070.1", 5, (100, 1), (110, 2), all),
        ("BOOK-070.2", 5, (101, 1), (110, 2), T::DUPLICATE),
        ("BOOK-070.3", 4, (101, 1), (110, 2), T::OLDER),
        ("BOOK-070.4", 6, (100, 3), (110, 2), T::BID_QTY),
        ("BOOK-070.5", 7, (110, 1), (110, 2), T::INVALID),
        ("BOOK-070.6", 8, (120, 1), (110, 2), T::INVALID),
        ("BOOK-070.7", 9, (0, 0), (110, 2), T::BID_PX | T::BID_QTY),
        ("BOOK-070.8", 10, (0, 0), (110, 2), T::NONE),
        ("BOOK-070.9", 0, (100, 1), (110, 2), T::BID_PX | T::BID_QTY),
        ("BOOK-070.10", 0, (100, 1), (111, 2), T::ASK_PX),
    ];
    let mut t = Top::ZERO;
    for (k, (row, uid, bid, ask, want)) in rows.into_iter().enumerate() {
        let n = k as u64 + 1;
        let before = t;
        let o = t.apply(&bbo(uid, bid, ask, 1000 + n), Seq(n));
        assert_eq!(o, want, "{row}");
        let applied = !(o.contains(T::DUPLICATE) || o.contains(T::OLDER) || o.contains(T::INVALID));
        if applied {
            assert_eq!((t.bid_px, t.bid_qty), (Px(bid.0), Qty(bid.1)), "{row}");
            assert_eq!((t.ask_px, t.ask_qty), (Px(ask.0), Qty(ask.1)), "{row}");
            assert_eq!(t.update_id, uid, "{row}");
            assert_eq!(t.seq, n, "{row}");
            assert_eq!(t.rx_tsc, Tsc(1000 + n), "{row}");
        } else {
            assert_eq!(t, before, "{row}");
        }
    }
}

#[test]
fn book_072_rx_tsc_only_on_apply() {
    let mut t = Top::ZERO;
    t.apply(&bbo(5, (100, 1), (110, 2), 1000), Seq(1));
    assert_eq!(t.apply(&bbo(5, (100, 1), (110, 2), 2000), Seq(2)), T::DUPLICATE);
    assert_eq!(t.apply(&bbo(4, (100, 1), (110, 2), 3000), Seq(3)), T::OLDER);
    assert_eq!(t.apply(&bbo(6, (120, 1), (110, 2), 4000), Seq(4)), T::INVALID);
    assert_eq!(t.rx_tsc, Tsc(1000));
    assert_eq!(t.apply(&bbo(7, (100, 1), (110, 2), 5000), Seq(5)), T::NONE);
    assert_eq!(t.rx_tsc, Tsc(5000));
    assert_eq!(
        (t.bid_px, t.bid_qty, t.ask_px, t.ask_qty),
        (Px(100), Qty(1), Px(110), Qty(2))
    );
    // No age or expiry field: the struct is exactly the eight quote/id/time fields (BOOK-071).
    assert_eq!(size_of::<Top>(), 64);
}
