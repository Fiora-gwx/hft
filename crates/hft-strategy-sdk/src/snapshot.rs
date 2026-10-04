//! Fixed-capacity little-endian snapshot writer and reader (README "快照节"), shared by the
//! strategy fast part, Model checkpoints and the engine L1 components.
//!
//! Engine sections are framed by hft-event `SectionHdr` (`begin_section`/`end_section` on the
//! writer, `next_section`/`find_section` on the reader; section bodies zero-padded to 8 B).
//! `section(tag)` is a layout marker for strategy and Model state (`ModelLayoutDesc` tags): the
//! plain writer writes no bytes for it.

use hft_event::SectionHdr;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes};

/// The writer reached its limit; the write that crossed it and every later one were dropped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotFull;

/// Reasons a component rejects a snapshot section.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreError {
    /// A read past the end of the section or snapshot.
    Truncated,
    /// Row or state layout version the reader does not accept.
    VersionUnsupported,
    /// Malformed `SectionHdr` (length not a multiple of 8, longer than the rest, reserved ≠ 0).
    BadSection,
    /// Illegal value in a row (enum discriminant, code, flag, padding).
    BadValue,
    /// More rows than the current capacity.
    Capacity,
}

const HDR: usize = size_of::<SectionHdr>();

/// Byte writer with a fixed upper limit, allocated once at construction (`new`) and reused
/// (`clear`); no allocation afterwards.
#[derive(Debug)]
pub struct SnapshotWriter {
    buf: Vec<u8>,
    limit: usize,
    full: bool,
    /// Open engine section: offset of its `SectionHdr` and rows counted so far.
    open: Option<(usize, u32)>,
    tag: Option<u16>,
}

impl SnapshotWriter {
    /// Writer for at most `limit` bytes (e.g. `strategy_snapshot_max` = 262 144 for a strategy).
    pub fn new(limit: usize) -> SnapshotWriter {
        SnapshotWriter {
            buf: Vec::with_capacity(limit),
            limit,
            full: false,
            open: None,
            tag: None,
        }
    }

    /// Empties the writer (keeps its allocation) for the next snapshot.
    pub fn clear(&mut self) {
        self.buf.clear();
        self.full = false;
        self.open = None;
        self.tag = None;
    }

    /// `Ok(bytes written)`, or `Err(SnapshotFull)` once a write crossed the limit.
    pub fn status(&self) -> Result<usize, SnapshotFull> {
        if self.full {
            Err(SnapshotFull)
        } else {
            Ok(self.buf.len())
        }
    }

    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The written bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Last `section(tag)` marker (for recording writers and diagnostics).
    pub fn current_tag(&self) -> Option<u16> {
        self.tag
    }

    pub fn put_bytes(&mut self, b: &[u8]) {
        if self.full {
            return;
        }
        if self.buf.len().saturating_add(b.len()) > self.limit {
            self.full = true;
            return;
        }
        self.buf.extend_from_slice(b);
    }

    pub fn put_u8(&mut self, v: u8) {
        self.put_bytes(&[v]);
    }
    pub fn put_u16(&mut self, v: u16) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_u32(&mut self, v: u32) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_u64(&mut self, v: u64) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_u128(&mut self, v: u128) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_i8(&mut self, v: i8) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_i16(&mut self, v: i16) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_i32(&mut self, v: i32) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_i64(&mut self, v: i64) {
        self.put_bytes(&v.to_le_bytes());
    }
    pub fn put_i128(&mut self, v: i128) {
        self.put_bytes(&v.to_le_bytes());
    }

    /// Writes one `#[repr(C)]` row (explicit padding, no implicit padding) and counts it in the
    /// open engine section.
    pub fn put_row<T: IntoBytes + Immutable>(&mut self, row: &T) {
        self.put_bytes(row.as_bytes());
        self.add_rows(1);
    }

    /// Counts `n` rows written field by field in the open engine section.
    pub fn add_rows(&mut self, n: u32) {
        if let Some((_, c)) = self.open.as_mut() {
            *c = c.saturating_add(n);
        }
    }

    /// Layout marker of a strategy/Model section (`SectionDesc.tag`); writes no bytes.
    pub fn section(&mut self, tag: u16) {
        self.tag = Some(tag);
    }

    /// Starts an engine snapshot section (`SectionHdr{section, version}`; `count` and `len`
    /// are filled by `end_section`). An open section is ended first.
    pub fn begin_section(&mut self, section: u16, version: u16) {
        self.end_section();
        let at = self.buf.len();
        let hdr = SectionHdr {
            section,
            version,
            count: 0,
            len: 0,
            _rsv: 0,
        };
        self.put_bytes(hdr.as_bytes());
        if !self.full {
            self.open = Some((at, 0));
        }
    }

    /// Ends the open engine section: zero-pads its body to 8 B and fills `count` (rows from
    /// `put_row`/`add_rows`) and `len`. No-op when no section is open.
    pub fn end_section(&mut self) {
        let Some((at, count)) = self.open.take() else {
            return;
        };
        let body = self.buf.len().saturating_sub(at.saturating_add(HDR));
        let pad = body.next_multiple_of(8) - body;
        self.put_bytes([0u8; 8].get(..pad).unwrap_or(&[]));
        if self.full {
            return;
        }
        let len = u32::try_from(body + pad).unwrap_or(u32::MAX);
        if let Some(h) = self.buf.get_mut(at..at + HDR) {
            if let Some(c) = h.get_mut(4..8) {
                c.copy_from_slice(&count.to_le_bytes());
            }
            if let Some(l) = h.get_mut(8..12) {
                l.copy_from_slice(&len.to_le_bytes());
            }
        }
    }
}

/// One engine section located by the reader.
#[derive(Clone, Copy, Debug)]
pub struct SectionView<'a> {
    pub section: u16,
    pub version: u16,
    pub count: u32,
    /// Section body including its trailing zero padding.
    pub body: &'a [u8],
}

impl<'a> SectionView<'a> {
    /// Reader over the body (`desc_version = 0`).
    pub fn reader(&self) -> SnapshotReader<'a> {
        SnapshotReader::new(self.body)
    }
}

/// Little-endian reader over a borrowed snapshot (or section body); every read past the end
/// returns `Err(RestoreError::Truncated)`.
#[derive(Clone, Debug)]
pub struct SnapshotReader<'a> {
    buf: &'a [u8],
    pos: usize,
    desc_version: u16,
}

macro_rules! get_le {
    ($($name:ident -> $t:ty),* $(,)?) => {$(
        pub fn $name(&mut self) -> Result<$t, RestoreError> {
            let b = self.get_bytes(size_of::<$t>())?;
            let mut a = [0u8; size_of::<$t>()];
            a.copy_from_slice(b);
            Ok(<$t>::from_le_bytes(a))
        }
    )*};
}

impl<'a> SnapshotReader<'a> {
    /// Reader for uses other than a strategy section (`desc_version() = 0`).
    pub fn new(buf: &'a [u8]) -> SnapshotReader<'a> {
        SnapshotReader {
            buf,
            pos: 0,
            desc_version: 0,
        }
    }

    /// Reader of a strategy section: `desc_version` = the row header's `desc_version`.
    pub fn with_desc_version(buf: &'a [u8], desc_version: u16) -> SnapshotReader<'a> {
        SnapshotReader {
            buf,
            pos: 0,
            desc_version,
        }
    }

    /// Section version given by the constructor (strategy section = `StrategyRowHdr.desc_version`,
    /// other uses 0).
    pub fn desc_version(&self) -> u16 {
        self.desc_version
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn get_bytes(&mut self, n: usize) -> Result<&'a [u8], RestoreError> {
        let end = self.pos.checked_add(n).ok_or(RestoreError::Truncated)?;
        let b = self.buf.get(self.pos..end).ok_or(RestoreError::Truncated)?;
        self.pos = end;
        Ok(b)
    }

    get_le!(
        get_u8 -> u8, get_u16 -> u16, get_u32 -> u32, get_u64 -> u64, get_u128 -> u128,
        get_i8 -> i8, get_i16 -> i16, get_i32 -> i32, get_i64 -> i64, get_i128 -> i128,
    );

    /// Reads one all-integer row (any bytes are a valid value).
    pub fn get_row<T: FromBytes + KnownLayout + Immutable>(&mut self) -> Result<T, RestoreError> {
        let b = self.get_bytes(size_of::<T>())?;
        T::read_from_bytes(b).map_err(|_| RestoreError::Truncated)
    }

    /// Reads one row with enum fields; an illegal discriminant → `BadValue`.
    pub fn try_row<T: TryFromBytes + KnownLayout + Immutable>(&mut self) -> Result<T, RestoreError> {
        let b = self.get_bytes(size_of::<T>())?;
        T::try_read_from_bytes(b).map_err(|_| RestoreError::BadValue)
    }

    /// Next engine section at the current position (`None` at the end).
    pub fn next_section(&mut self) -> Result<Option<SectionView<'a>>, RestoreError> {
        if self.is_empty() {
            return Ok(None);
        }
        let hb = self.get_bytes(HDR).map_err(|_| RestoreError::BadSection)?;
        let h = SectionHdr::read_from_bytes(hb).map_err(|_| RestoreError::BadSection)?;
        let len = h.len as usize;
        if h._rsv != 0 || !len.is_multiple_of(8) {
            return Err(RestoreError::BadSection);
        }
        let body = self.get_bytes(len).map_err(|_| RestoreError::BadSection)?;
        Ok(Some(SectionView {
            section: h.section,
            version: h.version,
            count: h.count,
            body,
        }))
    }

    /// First section numbered `section`, scanning the whole buffer from its start (the
    /// position is not moved); malformed framing before it → `BadSection`.
    pub fn find_section(&self, section: u16) -> Result<Option<SectionView<'a>>, RestoreError> {
        let mut r = SnapshotReader::new(self.buf);
        while let Some(s) = r.next_section()? {
            if s.section == section {
                return Ok(Some(s));
            }
        }
        Ok(None)
    }
}
