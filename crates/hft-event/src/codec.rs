//! Record codec: `encode_fixed`, `encode_var`, `encode_part`, `decode_record`, `RecordView`,
//! `classify`; CRC32C via the `crc32c` crate (ARMv8 CRC instructions).

use zerocopy::{FromBytes, IntoBytes};

use crate::Body;
use crate::bytes::{put, u8_at};
use crate::header::{RecordFlags, RecordHeader, record_len};
use crate::kinds::{EventKind, Layout, Role, kind_info, var_tail_len};
use crate::var::PartHdr;

/// Largest record (`len`).
pub const MAX_RECORD: usize = 65_536;
/// Largest fixed body / Var record body: `65 536 − 24 − 8`.
pub const MAX_BODY: usize = MAX_RECORD - 24 - 8;

/// Encoding errors.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum EncodeErr {
    TooLong = 1,
    BufTooSmall = 2,
    Count = 3,
}

/// Decoding errors.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum DecodeErr {
    Short = 1,
    TooLong = 2,
    Misaligned = 3,
    BadCrc = 4,
    KindMismatch = 5,
    InvalidEnum = 6,
    UnknownKind = 7,
    /// Inconsistent multi-part group (see `Reassembler`).
    BadPart = 8,
    /// Nonzero bytes in the transport alignment padding.
    BadPadding = 9,
}

/// Writes header + `parts` + zero padding (+ CRC) into `rec` (exactly `record_len` bytes).
fn write_record(rec: &mut [u8], kind: u16, flags: RecordFlags, seq: u64, tsc: u64, parts: [&[u8]; 2]) {
    let n = rec.len();
    let h = RecordHeader {
        len: n as u32,
        kind,
        flags: flags.0,
        seq,
        tsc,
    };
    put(rec, 0, h.as_bytes());
    let mut o = 24;
    for p in parts {
        put(rec, o, p);
        o += p.len();
    }
    let crc = flags.contains(RecordFlags::CRC);
    let end = if crc { n - 4 } else { n };
    if let Some(pad) = rec.get_mut(o..end) {
        pad.fill(0);
    }
    if crc {
        let c = rec.get(..end).map_or(0, crc32c::crc32c);
        put(rec, end, &c.to_le_bytes());
    }
}

/// Encodes a fixed body (`kind = B::KIND`); returns `record_len(size_of::<B>(), CRC ∈ flags)`.
/// `out` shorter than that → debug_assert (release: nothing written, returns 0).
/// `OrderDecision.oflags & !WIRE_MASK ≠ 0` and `LatencySample.action > 2` → debug_assert.
#[inline]
pub fn encode_fixed<B: Body>(out: &mut [u8], seq: u64, tsc: u64, flags: RecordFlags, b: &B) -> u32 {
    let bytes = b.as_bytes();
    if B::KIND == EventKind::ORDER_DECISION {
        debug_assert!(
            u8_at(bytes, 63) == 0,
            "OrderDecision.oflags uses reserved bits 8..15"
        );
    }
    if B::KIND == EventKind::LATENCY_SAMPLE {
        debug_assert!(u8_at(bytes, 121) <= 2, "LatencySample.action > 2");
    }
    let n = record_len(bytes.len(), flags.contains(RecordFlags::CRC));
    debug_assert!(out.len() >= n as usize, "encode_fixed: output buffer too small");
    let Some(rec) = out.get_mut(..n as usize) else {
        return 0;
    };
    write_record(rec, B::KIND.0, flags, seq, tsc, [bytes, &[]]);
    n
}

/// Encodes a Var record: prefix `P` + `tail`. `tail.len()` ≠ L (from the prefix count) → `Count`;
/// L > the kind's `max` → `TooLong`; `out` too short → `BufTooSmall`.
pub fn encode_var<P: Body>(
    out: &mut [u8],
    seq: u64,
    tsc: u64,
    flags: RecordFlags,
    prefix: &P,
    tail: &[u8],
) -> Result<u32, EncodeErr> {
    let pb = prefix.as_bytes();
    let Some(Layout::Var { max, .. }) = kind_info(P::KIND).map(|i| i.layout) else {
        return Err(EncodeErr::Count);
    };
    let l = var_tail_len(P::KIND, pb).ok_or(EncodeErr::Count)?;
    if tail.len() != l {
        return Err(EncodeErr::Count);
    }
    if l > max as usize {
        return Err(EncodeErr::TooLong);
    }
    let n = record_len(pb.len() + l, flags.contains(RecordFlags::CRC));
    let rec = out.get_mut(..n as usize).ok_or(EncodeErr::BufTooSmall)?;
    write_record(rec, P::KIND.0, flags, seq, tsc, [pb, tail]);
    Ok(n)
}

/// Encodes one segment of a Multi record (body = `PartHdr` + `chunk`), as produced by
/// `PartIter`. `chunk.len() ≠ ph.chunk_len` → `Count`; record > 65 536 → `TooLong`;
/// `out` too short → `BufTooSmall`.
pub fn encode_part(
    out: &mut [u8],
    seq: u64,
    tsc: u64,
    flags: RecordFlags,
    kind: EventKind,
    ph: &PartHdr,
    chunk: &[u8],
) -> Result<u32, EncodeErr> {
    if chunk.len() != ph.chunk_len as usize {
        return Err(EncodeErr::Count);
    }
    let n = record_len(16 + chunk.len(), flags.contains(RecordFlags::CRC));
    if n as usize > MAX_RECORD {
        return Err(EncodeErr::TooLong);
    }
    let rec = out.get_mut(..n as usize).ok_or(EncodeErr::BufTooSmall)?;
    write_record(rec, kind.0, flags, seq, tsc, [ph.as_bytes(), chunk]);
    Ok(n)
}

/// A decoded record: header (by value; the buffer need not be aligned) and body
/// `[24, len)` (`[24, len − 4)` with CRC), trailing zero padding included.
#[derive(Copy, Clone, Debug)]
pub struct RecordView<'a> {
    pub hdr: RecordHeader,
    pub body: &'a [u8],
}

/// Frames and checks one record at the start of `buf`; does not check that the kind is registered.
pub fn decode_record(buf: &[u8]) -> Result<RecordView<'_>, DecodeErr> {
    let (hdr, _) = RecordHeader::read_from_prefix(buf).map_err(|_| DecodeErr::Short)?;
    let len = hdr.len as usize;
    let crc = RecordFlags(hdr.flags).contains(RecordFlags::CRC);
    if len < 24 || len > buf.len() || (crc && len < 32) {
        return Err(DecodeErr::Short);
    }
    if len > MAX_RECORD {
        return Err(DecodeErr::TooLong);
    }
    if !len.is_multiple_of(8) {
        return Err(DecodeErr::Misaligned);
    }
    let rec = buf.get(..len).ok_or(DecodeErr::Short)?;
    let body_end = if crc {
        let (data, tail) = rec.split_at_checked(len - 4).ok_or(DecodeErr::Short)?;
        let want = tail
            .try_into()
            .map(u32::from_le_bytes)
            .map_err(|_| DecodeErr::Short)?;
        if crc32c::crc32c(data) != want {
            return Err(DecodeErr::BadCrc);
        }
        len - 4
    } else {
        len
    };
    let body = rec.get(24..body_end).ok_or(DecodeErr::Short)?;
    Ok(RecordView { hdr, body })
}

impl<'a> RecordView<'a> {
    /// Exact current-schema body of a Fixed or Var record, excluding only transport padding.
    /// Multi records must first be reassembled. Unlike `read`, this never extends or truncates
    /// an older/newer layout, and unlike `var`, it rejects nonzero alignment padding.
    pub fn exact_body(&self) -> Result<&'a [u8], DecodeErr> {
        let kind = EventKind(self.hdr.kind);
        let layout = kind_info(kind).ok_or(DecodeErr::UnknownKind)?.layout;
        if RecordFlags(self.hdr.flags).contains(RecordFlags::PART) {
            return Err(DecodeErr::BadPart);
        }
        let n = match layout {
            Layout::Fixed { size } => usize::from(size),
            Layout::Var { prefix, max } => {
                let p = self.body.get(..usize::from(prefix)).ok_or(DecodeErr::Short)?;
                let tail = var_tail_len(kind, p).ok_or(DecodeErr::Short)?;
                if tail > max as usize {
                    return Err(DecodeErr::TooLong);
                }
                usize::from(prefix) + tail
            }
            Layout::Multi { .. } => return Err(DecodeErr::BadPart),
        };
        let want = record_len(n, self.has_crc());
        if self.hdr.len < want {
            return Err(DecodeErr::Short);
        }
        if self.hdr.len > want {
            return Err(DecodeErr::TooLong);
        }
        let wire_body = self.hdr.len as usize - 24 - if self.has_crc() { 4 } else { 0 };
        if self.body.len() < wire_body {
            return Err(DecodeErr::Short);
        }
        if self.body.len() > wire_body {
            return Err(DecodeErr::TooLong);
        }
        let (body, padding) = self.body.split_at_checked(n).ok_or(DecodeErr::Short)?;
        if padding.iter().any(|b| *b != 0) {
            return Err(DecodeErr::BadPadding);
        }
        Ok(body)
    }

    fn check_kind<B: Body>(&self) -> Result<(), DecodeErr> {
        if self.hdr.kind == B::KIND.0 {
            Ok(())
        } else {
            Err(DecodeErr::KindMismatch)
        }
    }

    fn has_crc(&self) -> bool {
        RecordFlags(self.hdr.flags).contains(RecordFlags::CRC)
    }

    /// By-value read: copies `min(body.len(), size_of::<B>())` bytes; a shorter body (old writer)
    /// is zero-filled, a longer one (new writer) truncated; illegal enums → `InvalidEnum`.
    #[inline]
    pub fn read<B: Body>(&self) -> Result<B, DecodeErr> {
        self.check_kind::<B>()?;
        read_zero_extended(self.body)
    }

    /// Zero-copy view: `len` must equal `record_len(size_of::<B>(), CRC)` (smaller → `Short`,
    /// larger → `TooLong`), body 8-byte aligned (`Misaligned`), enums legal (`InvalidEnum`).
    #[inline]
    pub fn view<B: Body>(&self) -> Result<&'a B, DecodeErr> {
        self.check_kind::<B>()?;
        let size = size_of::<B>();
        let want = record_len(size, self.has_crc());
        if self.hdr.len < want {
            return Err(DecodeErr::Short);
        }
        if self.hdr.len > want {
            return Err(DecodeErr::TooLong);
        }
        let body: &'a [u8] = self.body;
        if body.as_ptr().align_offset(8) != 0 {
            return Err(DecodeErr::Misaligned);
        }
        let b = body.get(..size).ok_or(DecodeErr::Short)?;
        B::try_ref_from_bytes(b).map_err(|_| DecodeErr::InvalidEnum)
    }

    /// Var layout: prefix and the tail of exactly L bytes; `len` smaller/larger than
    /// `record_len(size_of::<P>() + L, CRC)` → `Short`/`TooLong` (padding not checked).
    #[inline]
    pub fn var<P: Body>(&self) -> Result<(&'a P, &'a [u8]), DecodeErr> {
        self.check_kind::<P>()?;
        let size = size_of::<P>();
        let body: &'a [u8] = self.body;
        let (pb, rest) = body.split_at_checked(size).ok_or(DecodeErr::Short)?;
        let l = var_tail_len(P::KIND, pb).ok_or(DecodeErr::KindMismatch)?;
        let want = record_len(size + l, self.has_crc());
        if self.hdr.len < want {
            return Err(DecodeErr::Short);
        }
        if self.hdr.len > want {
            return Err(DecodeErr::TooLong);
        }
        if pb.as_ptr().align_offset(8) != 0 {
            return Err(DecodeErr::Misaligned);
        }
        let p = P::try_ref_from_bytes(pb).map_err(|_| DecodeErr::InvalidEnum)?;
        let tail = rest.get(..l).ok_or(DecodeErr::Short)?;
        Ok((p, tail))
    }
}

/// `B` from the first `size_of::<B>()` bytes of `body`, zero-extended when shorter.
fn read_zero_extended<B: Body>(body: &[u8]) -> Result<B, DecodeErr> {
    let size = size_of::<B>();
    if let Some(b) = body.get(..size) {
        return B::try_read_from_bytes(b).map_err(|_| DecodeErr::InvalidEnum);
    }
    let mut buf = [0u8; 512];
    let dst = buf.get_mut(..size).ok_or(DecodeErr::TooLong)?;
    put(dst, 0, body);
    B::try_read_from_bytes(dst).map_err(|_| DecodeErr::InvalidEnum)
}

/// Actual class of a record (`Input`/`Output`/`Meta`, never `ByBody`); unregistered kind →
/// `UnknownKind`; illegal discriminating field → `InvalidEnum`. Missing bytes read as 0.
pub fn classify(kind: EventKind, logical_body: &[u8]) -> Result<Role, DecodeErr> {
    let info = kind_info(kind).ok_or(DecodeErr::UnknownKind)?;
    if info.role != Role::ByBody {
        return Ok(info.role);
    }
    let b = |o: usize| u8_at(logical_body, o);
    match kind {
        EventKind::L2_SNAPSHOT => match b(64) {
            1 | 3 => Ok(Role::Input),
            2 => Ok(Role::Output),
            _ => Err(DecodeErr::InvalidEnum),
        },
        EventKind::CONFIG_BLOB | EventKind::FUNDING_SETTLE => {
            let o = if kind == EventKind::CONFIG_BLOB { 64 } else { 48 };
            match b(o) {
                0 => Ok(Role::Input),
                1 => Ok(Role::Output),
                _ => Err(DecodeErr::InvalidEnum),
            }
        }
        EventKind::NOTE => match b(5) {
            1..=4 | 9 | 10 => Ok(Role::Output),
            5..=8 | 11 | 12 => Ok(Role::Meta),
            _ => Err(DecodeErr::InvalidEnum),
        },
        _ => Err(DecodeErr::UnknownKind),
    }
}
