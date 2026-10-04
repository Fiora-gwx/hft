//! Snapshot rows of sections 6 (`BookMeta`) and 7 (`Tops`), `BookSet::snapshot` / `restore`.
//! Row versions are registered here only; rows only grow at the tail, zero = not provided.

use hft_strategy_sdk::{RestoreError, SectionView, SnapshotReader, SnapshotWriter};
use hft_types::{InstId, Px, Qty, Tsc};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::set::BookSet;
use crate::top::Top;
use crate::{BookState, StaleReason};

/// Section number of `BookMeta`.
pub const SECTION_BOOK_META: u16 = 6;
/// Section number of `Tops`.
pub const SECTION_TOPS: u16 = 7;
/// Row version of both sections.
pub const ROW_VERSION: u16 = 1;

/// Section 6 `BookMeta`, row version 1, 32 B, key `inst`. The levels follow in an
/// `L2Snapshot{origin = Keyframe}`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct BookMetaRow {
    pub inst: InstId,
    /// `BookState`.
    pub state: u8,
    /// `StaleReason`; 0 = none.
    pub stale_reason: u8,
    /// `L2Snapshot.origin` of the last snapshot; 0 = none yet.
    pub origin: u8,
    pub _pad0: u8,
    pub last_id: u64,
    /// Tick index of window slot 0.
    pub base: i64,
    /// Overflow truncations so far.
    pub truncated: u32,
    pub crossed_run: u16,
    pub _pad1: [u8; 2],
}
const _: () = assert!(size_of::<BookMetaRow>() == 32 && align_of::<BookMetaRow>() == 8);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, inst) == 0);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, state) == 4);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, stale_reason) == 5);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, origin) == 6);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, _pad0) == 7);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, last_id) == 8);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, base) == 16);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, truncated) == 24);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, crossed_run) == 28);
const _: () = assert!(core::mem::offset_of!(BookMetaRow, _pad1) == 30);

/// Section 7 `Tops`, row version 1, 64 B, key `inst`: `Top` without `rx_tsc` (no TSC in
/// snapshots).
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct TopRow {
    pub inst: InstId,
    pub _pad: [u8; 4],
    pub bid_px: Px,
    pub bid_qty: Qty,
    pub ask_px: Px,
    pub ask_qty: Qty,
    pub update_id: u64,
    pub ex_ts_ns: i64,
    pub seq: u64,
}
const _: () = assert!(size_of::<TopRow>() == 64 && align_of::<TopRow>() == 8);
const _: () = assert!(core::mem::offset_of!(TopRow, inst) == 0);
const _: () = assert!(core::mem::offset_of!(TopRow, _pad) == 4);
const _: () = assert!(core::mem::offset_of!(TopRow, bid_px) == 8);
const _: () = assert!(core::mem::offset_of!(TopRow, bid_qty) == 16);
const _: () = assert!(core::mem::offset_of!(TopRow, ask_px) == 24);
const _: () = assert!(core::mem::offset_of!(TopRow, ask_qty) == 32);
const _: () = assert!(core::mem::offset_of!(TopRow, update_id) == 40);
const _: () = assert!(core::mem::offset_of!(TopRow, ex_ts_ns) == 48);
const _: () = assert!(core::mem::offset_of!(TopRow, seq) == 56);

impl TopRow {
    pub fn from_top(inst: InstId, t: &Top) -> TopRow {
        TopRow {
            inst,
            _pad: [0; 4],
            bid_px: t.bid_px,
            bid_qty: t.bid_qty,
            ask_px: t.ask_px,
            ask_qty: t.ask_qty,
            update_id: t.update_id,
            ex_ts_ns: t.ex_ts_ns,
            seq: t.seq,
        }
    }

    /// The `Top` this row restores (`rx_tsc` = 0).
    pub fn to_top(&self) -> Top {
        Top {
            bid_px: self.bid_px,
            bid_qty: self.bid_qty,
            ask_px: self.ask_px,
            ask_qty: self.ask_qty,
            update_id: self.update_id,
            ex_ts_ns: self.ex_ts_ns,
            rx_tsc: Tsc(0),
            seq: self.seq,
        }
    }
}

/// Rows of one section with the declared count; `Truncated` when the body is shorter.
fn rows<'a, T: FromBytes + KnownLayout + Immutable>(
    s: &SectionView<'a>,
) -> Result<impl Iterator<Item = Result<T, RestoreError>> + 'a, RestoreError> {
    let need = (s.count as usize)
        .checked_mul(size_of::<T>())
        .ok_or(RestoreError::Truncated)?;
    if s.body.len() < need {
        return Err(RestoreError::Truncated);
    }
    let mut r = SnapshotReader::new(s.body);
    Ok((0..s.count).map(move |_| r.get_row::<T>()))
}

fn check_meta(r: &BookMetaRow) -> Result<BookState, RestoreError> {
    let st = BookState::from_u8(r.state).ok_or(RestoreError::BadValue)?;
    if r.stale_reason != 0 && StaleReason::from_u8(r.stale_reason).is_none() {
        return Err(RestoreError::BadValue);
    }
    if r.origin > 3 {
        return Err(RestoreError::BadValue);
    }
    Ok(st)
}

fn section<'a>(r: &SnapshotReader<'a>, n: u16) -> Result<Option<SectionView<'a>>, RestoreError> {
    let s = r.find_section(n)?;
    if let Some(v) = &s
        && v.version != ROW_VERSION
    {
        return Err(RestoreError::VersionUnsupported);
    }
    Ok(s)
}

impl BookSet {
    /// Writes section 6 (one `BookMetaRow` per allocated L2 book) and section 7 (one `TopRow`
    /// per inst whose `Top.seq ≠ 0`), both in ascending `inst` order.
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.begin_section(SECTION_BOOK_META, ROW_VERSION);
        for i in 1..self.tops_len() {
            let id = InstId(i as u32);
            if let Some(b) = self.l2(id) {
                let mut row = b.meta();
                row.inst = id;
                w.put_row(&row);
            }
        }
        w.begin_section(SECTION_TOPS, ROW_VERSION);
        for i in 1..self.tops_len() {
            let id = InstId(i as u32);
            let t = self.top(id);
            if t.seq != 0 {
                w.put_row(&TopRow::from_top(id, t));
            }
        }
        w.end_section();
    }

    /// Reads sections 6 and 7 back (whole snapshot is scanned; a missing section restores
    /// nothing). Every row is checked before anything is written. Rows of insts without an L2
    /// book here (section 6) or outside refdata (section 7) are skipped; `Top.rx_tsc` = 0.
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError> {
        let s6 = section(r, SECTION_BOOK_META)?;
        let s7 = section(r, SECTION_TOPS)?;
        if let Some(s) = &s6 {
            for row in rows::<BookMetaRow>(s)? {
                check_meta(&row?)?;
            }
        }
        if let Some(s) = &s7 {
            for row in rows::<TopRow>(s)? {
                row?;
            }
        }
        if let Some(s) = &s6 {
            for row in rows::<BookMetaRow>(s)? {
                let row = row?;
                let st = check_meta(&row)?;
                if let Some(b) = self.l2_mut(row.inst) {
                    b.restore_meta(&row, st);
                }
            }
        }
        if let Some(s) = &s7 {
            for row in rows::<TopRow>(s)? {
                let row = row?;
                if row.inst.0 != 0 && (row.inst.0 as usize) < self.tops_len() {
                    *self.top_mut(row.inst) = row.to_top();
                }
            }
        }
        Ok(())
    }
}
