//! BOOK-071, BOOK-075: layouts.
use core::mem::offset_of;

use hft_book::{BookMetaRow, Top, TopRow};
use hft_types::{InstId, Px, Qty};

#[test]
fn book_071_top_layout() {
    assert_eq!((size_of::<Top>(), align_of::<Top>()), (64, 64));
    let offs = [
        offset_of!(Top, bid_px),
        offset_of!(Top, bid_qty),
        offset_of!(Top, ask_px),
        offset_of!(Top, ask_qty),
        offset_of!(Top, update_id),
        offset_of!(Top, ex_ts_ns),
        offset_of!(Top, rx_tsc),
        offset_of!(Top, seq),
    ];
    assert_eq!(offs, [0, 8, 16, 24, 32, 40, 48, 56]);
}

#[test]
fn book_075_snapshot_row_layout() {
    assert_eq!(size_of::<BookMetaRow>(), 32, "BOOK-075.1");
    let m = [
        offset_of!(BookMetaRow, inst),
        offset_of!(BookMetaRow, state),
        offset_of!(BookMetaRow, stale_reason),
        offset_of!(BookMetaRow, origin),
        offset_of!(BookMetaRow, _pad0),
        offset_of!(BookMetaRow, last_id),
        offset_of!(BookMetaRow, base),
        offset_of!(BookMetaRow, truncated),
        offset_of!(BookMetaRow, crossed_run),
        offset_of!(BookMetaRow, _pad1),
    ];
    assert_eq!(m, [0, 4, 5, 6, 7, 8, 16, 24, 28, 30], "BOOK-075.2");
    assert_eq!(size_of::<TopRow>(), 64, "BOOK-075.3");
    let t = [
        offset_of!(TopRow, inst),
        offset_of!(TopRow, _pad),
        offset_of!(TopRow, bid_px),
        offset_of!(TopRow, bid_qty),
        offset_of!(TopRow, ask_px),
        offset_of!(TopRow, ask_qty),
        offset_of!(TopRow, update_id),
        offset_of!(TopRow, ex_ts_ns),
        offset_of!(TopRow, seq),
    ];
    assert_eq!(t, [0, 4, 8, 16, 24, 32, 40, 48, 56], "BOOK-075.4");
    // BOOK-075.5: the field set has no `Tsc` (exhaustive destructuring fails to compile if a
    // field is added; every field type is listed).
    let TopRow {
        inst,
        _pad,
        bid_px,
        bid_qty,
        ask_px,
        ask_qty,
        update_id,
        ex_ts_ns,
        seq,
    } = TopRow::from_top(InstId(1), &Top::ZERO);
    let _: (InstId, [u8; 4], Px, Qty, Px, Qty, u64, i64, u64) = (
        inst, _pad, bid_px, bid_qty, ask_px, ask_qty, update_id, ex_ts_ns, seq,
    );
}
