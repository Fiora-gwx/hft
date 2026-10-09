//! BOOK-076–BOOK-078, BOOK-083: snapshot sections 6 and 7.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
use crate::common;

use common::*;
use hft_book::{BookMetaRow, BookSet, BookState, ROW_VERSION, SECTION_BOOK_META, SECTION_TOPS, TopRow};
use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};
use hft_types::{InstId, Seq, Tsc};
use zerocopy::FromBytes;

fn rows<T: FromBytes + zerocopy::KnownLayout + zerocopy::Immutable>(body: &[u8], n: u32) -> Vec<T> {
    let mut r = SnapshotReader::new(body);
    (0..n)
        .map(|_| r.get_row::<T>().unwrap_or_else(|e| panic!("{e:?}")))
        .collect()
}

fn snapshot_of(s: &BookSet) -> SnapshotWriter {
    let mut w = SnapshotWriter::new(1 << 16);
    s.snapshot(&mut w);
    w
}

#[test]
fn book_076_sections_written_once() {
    let mut s = bookset(5, &[4, 2]);
    s.top_mut(InstId(3)).apply(&bbo(5, (100, 1), (110, 2), 9), Seq(7));
    s.top_mut(InstId(1)).apply(&bbo(6, (200, 3), (210, 4), 9), Seq(8));
    let w = snapshot_of(&s);
    let mut r = SnapshotReader::new(w.as_bytes());
    let mut secs = Vec::new();
    while let Some(v) = r.next_section().unwrap_or_else(|e| panic!("{e:?}")) {
        secs.push(v);
    }
    assert_eq!(secs.len(), 2);
    let (s6, s7) = (secs[0], secs[1]);
    assert_eq!((s6.section, s6.version, s6.count), (SECTION_BOOK_META, 1, 2));
    assert_eq!((s7.section, s7.version, s7.count), (SECTION_TOPS, 1, 2));
    let m: Vec<BookMetaRow> = rows(s6.body, 2);
    assert_eq!(m.iter().map(|r| r.inst.0).collect::<Vec<_>>(), [2, 4]);
    let t: Vec<TopRow> = rows(s7.body, 2);
    assert_eq!(t.iter().map(|r| r.inst.0).collect::<Vec<_>>(), [1, 3]);
    for row in &t {
        assert_eq!(*row, TopRow::from_top(row.inst, s.top(row.inst)));
    }
}

#[test]
fn book_077_restore_round_trip() {
    let rd = refdata(5, 10, 64, 1);
    let ids = [InstId(2), InstId(4)];
    let mk = || match BookSet::new_in(rd, &ids, aligned_mem(BookSet::bytes_needed(rd, &ids))) {
        Ok(s) => s,
        Err(e) => panic!("{e:?}"),
    };
    let mut a = mk();
    let b2 = a.l2_mut(InstId(2)).unwrap_or_else(|| panic!());
    s0(b2);
    b2.apply_delta(&delta(101, 102, Buy, 10000, 1, 0, true));
    let b4 = a.l2_mut(InstId(4)).unwrap_or_else(|| panic!());
    s1(b4);
    // 66 bids below the window: overflow cap 64 → 2 truncations.
    let far: Vec<(i64, i64)> = (0..66).map(|k| (9000 - 10 * k, 1)).collect();
    batch(b4, 100, 101, &far, &[]);
    batch(b4, 101, 102, &[(10010, 1)], &[]);
    assert_eq!(b4.state(), BookState::Live);
    assert_eq!((b4.meta().crossed_run, b4.meta().truncated), (1, 2));
    let t3 = a.top_mut(InstId(3));
    t3.apply(&bbo(5, (100, 1), (110, 2), 777), Seq(9));
    t3.ex_ts_ns = 123;
    let w = snapshot_of(&a);
    let mut b = mk();
    assert_eq!(b.restore(&mut SnapshotReader::new(w.as_bytes())), Ok(()));
    let m2 = b.l2(InstId(2)).map(|x| x.meta()).unwrap_or_else(|| panic!());
    assert_eq!((m2.state, m2.stale_reason, m2.last_id, m2.base), (2, 1, 100, 936));
    let ma = a.l2(InstId(4)).map(|x| x.meta());
    let mb = b.l2(InstId(4)).map(|x| x.meta());
    assert_eq!(ma, mb);
    let (ta, tb) = (a.top(InstId(3)), b.top(InstId(3)));
    assert_eq!(
        (
            ta.bid_px,
            ta.bid_qty,
            ta.ask_px,
            ta.ask_qty,
            ta.update_id,
            ta.ex_ts_ns,
            ta.seq
        ),
        (
            tb.bid_px,
            tb.bid_qty,
            tb.ask_px,
            tb.ask_qty,
            tb.update_id,
            tb.ex_ts_ns,
            tb.seq
        )
    );
    assert_eq!(tb.rx_tsc, Tsc(0));
}

fn meta_row(inst: u32) -> BookMetaRow {
    BookMetaRow {
        inst: InstId(inst),
        state: 1,
        stale_reason: 0,
        origin: 1,
        _pad0: 0,
        last_id: 55,
        base: 900,
        truncated: 0,
        crossed_run: 0,
        _pad1: [0; 2],
    }
}

#[test]
fn book_078_restore_rejects() {
    fn run(f: impl FnOnce(&mut SnapshotWriter)) -> (Result<(), RestoreError>, BookSet) {
        let mut s = bookset(5, &[2]);
        let mut w = SnapshotWriter::new(4096);
        f(&mut w);
        w.end_section();
        let r = s.restore(&mut SnapshotReader::new(w.as_bytes()));
        (r, s)
    }
    let bad = |m: fn(&mut BookMetaRow)| {
        run(|w| {
            w.begin_section(SECTION_BOOK_META, ROW_VERSION);
            let mut r = meta_row(2);
            m(&mut r);
            w.put_row(&r);
        })
        .0
    };
    let (r, _) = run(|w| w.begin_section(SECTION_BOOK_META, 2));
    assert_eq!(r, Err(RestoreError::VersionUnsupported), "BOOK-078.1");
    let (r, _) = run(|w| w.begin_section(SECTION_TOPS, 2));
    assert_eq!(r, Err(RestoreError::VersionUnsupported), "BOOK-078.2");
    assert!(bad(|r| r.state = 7).is_err(), "BOOK-078.3");
    assert!(bad(|r| r.stale_reason = 9).is_err(), "BOOK-078.4");
    assert!(bad(|r| r.origin = 4).is_err(), "BOOK-078.5");
    let (r, s) = run(|w| {
        w.begin_section(SECTION_BOOK_META, ROW_VERSION);
        w.put_row(&meta_row(2));
        w.put_row(&meta_row(4));
    });
    assert_eq!(r, Ok(()), "BOOK-078.6");
    let m = s.l2(InstId(2)).map(|b| b.meta()).unwrap_or_else(|| panic!());
    assert_eq!((m.state, m.last_id, m.base), (1, 55, 900), "BOOK-078.6");
    assert!(s.l2(InstId(4)).is_none(), "BOOK-078.6");
    let (r, s) = run(|w| {
        w.begin_section(SECTION_TOPS, ROW_VERSION);
        let mut t = TopRow::from_top(InstId(9), &hft_book::Top::ZERO);
        t.seq = 3;
        w.put_row(&t);
    });
    assert_eq!(r, Ok(()), "BOOK-078.7");
    assert!((1..=5).all(|i| s.top(InstId(i)).seq == 0), "BOOK-078.7");
}

#[test]
fn book_083_meta_fields() {
    let mut b = book_s0(&P);
    b.apply_delta(&delta(101, 102, Buy, 10000, 1, 0, true));
    let want = BookMetaRow {
        inst: InstId(0),
        state: 2,
        stale_reason: 1,
        origin: 1,
        _pad0: 0,
        last_id: 100,
        base: 936,
        truncated: 0,
        crossed_run: 0,
        _pad1: [0; 2],
    };
    assert_eq!(b.meta(), want);
    let mut s = bookset(7, &[7]);
    let x = s.l2_mut(InstId(7)).unwrap_or_else(|| panic!());
    s0(x);
    x.apply_delta(&delta(101, 102, Buy, 10000, 1, 0, true));
    let w = snapshot_of(&s);
    let r = SnapshotReader::new(w.as_bytes());
    let s6 = r
        .find_section(SECTION_BOOK_META)
        .ok()
        .flatten()
        .unwrap_or_else(|| panic!());
    let got: Vec<BookMetaRow> = rows(s6.body, s6.count);
    assert_eq!(
        got,
        [BookMetaRow {
            inst: InstId(7),
            ..want
        }]
    );
}
