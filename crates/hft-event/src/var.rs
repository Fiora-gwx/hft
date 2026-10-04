//! Variable-length records: `PartHdr` and Multi prefixes (`L2SnapHdr`, `BlobHdr`, `RawHdr`,
//! `SnapshotHdr`/`SectionHdr`), `multi_view`, splitting (`PartIter`) and reassembly
//! (`Reassembler`, caller-provided buffer).
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_types::{ConnId, Px, Qty, VenueId};
use zerocopy::FromBytes;

use crate::bytes::{u8_at, u16_at, u32_at};
use crate::cmd::{BlobRef, CmdScope};
use crate::codes;
use crate::header::RecordFlags;
use crate::macros::{layout, wire_enum};
use crate::market::{L2BatchHdr, MarketHdr};
use crate::schema::FieldTy;
use crate::{Body, DecodeErr, EncodeErr, EventKind, RecordView};

crate::event_body! {
    /// Prefix of every segment of a Multi record (16 B).
    pub struct PartHdr : plain, FromBytes {
        /// Logical body length (sum of all chunks).
        pub total_len: u32,
        /// Logical bytes carried by this segment.
        pub chunk_len: u32,
        pub part: u16,
        pub parts: u16,
        pub _rsv: u32,
    }
}
layout!(PartHdr, 16, 4; total_len @ 0, chunk_len @ 4, part @ 8, parts @ 10, _rsv @ 12);

crate::event_body! {
    /// `L2Snapshot` (0x0103, Multi) prefix (72 B); followed by `[Level; n_bids + n_asks]`.
    pub struct L2SnapHdr : multi(EventKind::L2_SNAPSHOT, 1), FromBytes {
        pub h: MarketHdr,
        pub snap_id: u64,
        pub origin: u8 = FieldTy::Enum(&codes::L2_ORIGIN),
        pub _pad: [u8; 3],
        pub n_bids: u16,
        pub n_asks: u16,
    }
}
layout!(L2SnapHdr, 72, 8; h @ 0, snap_id @ 56, origin @ 64, _pad @ 65, n_bids @ 68, n_asks @ 70);

impl L2SnapHdr {
    pub const EXCHANGE: u8 = 1;
    pub const KEYFRAME: u8 = 2;
    pub const TOP_N: u8 = 3;
}

crate::event_body! {
    /// One book level (16 B): bids price-descending first, then asks ascending.
    pub struct Level : plain, FromBytes {
        pub px: Px,
        pub qty: Qty,
    }
}
layout!(Level, 16, 8; px @ 0, qty @ 8);

/// Borrowed view written by venue decoders to the runtime sink (not a journal format).
#[derive(Copy, Clone, Debug)]
pub struct L2SnapshotPart<'a> {
    pub hdr: L2SnapHdr,
    pub bids: &'a [Level],
    pub asks: &'a [Level],
    pub part: u16,
    pub parts: u16,
}

/// Borrowed view of one incremental depth message.
#[derive(Copy, Clone, Debug)]
pub struct L2BatchView<'a> {
    pub hdr: L2BatchHdr,
    pub levels: &'a [Level],
}

wire_enum!(
    /// `BlobRef.kind` (append-only).
    BlobKind: u16 = "BlobKind" { Refdata = 1, StrategyParams = 2, RiskParams = 3, HostManifest = 4, Engine = 5, SimConfig = 6 }
);

impl BlobRef {
    /// `encoding` of `Refdata` blobs: RDB1 (hft-refdata).
    pub const ENC_RDB1: u16 = 1;
    /// `encoding` of `StrategyParams` blobs: SPB1 (this crate).
    pub const ENC_SPB1: u16 = 1;
    /// `encoding` of `Engine` blobs: ECF1 (hft-refdata).
    pub const ENC_ECF1: u16 = 1;
}

crate::event_body! {
    /// `ConfigBlob` (0x0403, Multi) prefix (72 B); followed by `blob.len` content bytes.
    pub struct BlobHdr : multi(EventKind::CONFIG_BLOB, 1), TryFromBytes {
        pub blob: BlobRef,
        pub scope: CmdScope,
        /// 1 = segment boot block restating the active config (Output).
        pub boot: u8 = FieldTy::Enum(&codes::BOOT),
        pub _pad: [u8; 7],
    }
}
layout!(BlobHdr, 72, 8; blob @ 0, scope @ 48, boot @ 64, _pad @ 65);

crate::event_body! {
    /// `RawRx`/`RawTx` (0x0601/0x0602, Multi) prefix (24 B); followed by `payload_len` bytes.
    pub struct RawHdr : multi(EventKind::RAW_RX, 1), FromBytes {
        pub venue: VenueId,
        pub conn: ConnId,
        pub what: u8 = FieldTy::Enum(&codes::RAW_WHAT),
        pub rflags: u8 = FieldTy::Bits(&codes::RAW_RFLAGS),
        pub stream: u32,
        /// RawTx: 0.
        pub hw_rx_ns: i64,
        pub payload_len: u32,
        pub _pad: u32,
    }
}
layout!(RawHdr, 24, 8; venue @ 0, conn @ 1, what @ 2, rflags @ 3, stream @ 4, hw_rx_ns @ 8, payload_len @ 16,
    _pad @ 20);

impl RawHdr {
    pub const REDACTED: u8 = 1;
    pub const SAMPLED: u8 = 2;
}

crate::event_body! {
    /// `StateSnapshot` (0x0504, Multi) logical-body header (96 B; journal-format "快照记录结构").
    pub struct SnapshotHdr : multi(EventKind::STATE_SNAPSHOT, 1), FromBytes {
        pub purpose: u8 = FieldTy::Enum(&codes::SNAPSHOT_PURPOSE),
        pub _pad: [u8; 3],
        pub n_sections: u32,
        pub at_seq: u64,
        pub state_hash: [u8; 16],
        pub n_books: u32,
        pub _pad2: u32,
        pub out_hash: [u8; 16],
        pub snap_tsc: u64 => measure,
        pub boot_id: [u8; 16] => measure,
        pub snap_utc_ns: i64 => measure,
        pub engine_ns: i64,
    }
}
layout!(SnapshotHdr, 96, 8; purpose @ 0, n_sections @ 4, at_seq @ 8, state_hash @ 16, n_books @ 32,
    out_hash @ 40, snap_tsc @ 56, boot_id @ 64, snap_utc_ns @ 80, engine_ns @ 88);

crate::event_body! {
    /// Snapshot section prefix (16 B); section bodies 8 B aligned.
    pub struct SectionHdr : plain, FromBytes {
        pub section: u16,
        pub version: u16,
        pub count: u32,
        pub len: u32,
        pub _rsv: u32,
    }
}
layout!(SectionHdr, 16, 4; section @ 0, version @ 2, count @ 4, len @ 8, _rsv @ 12);

/// Largest chunk per segment: `65 536 − 24 − 16 − 8` (journal-format multi-part rule 1).
pub const PART_MAX_CHUNK: usize = 65_488;

/// Reads a Multi logical body: prefix `P` by value plus its tail, checking the discriminating
/// fields and the tail length implied by the prefix (README "变长记录前缀").
pub fn multi_view<P: Body>(logical: &[u8]) -> Result<(P, &[u8]), DecodeErr> {
    let size = size_of::<P>();
    let (pb, tail) = logical.split_at_checked(size).ok_or(DecodeErr::Short)?;
    let p = P::try_read_from_bytes(pb).map_err(|_| DecodeErr::InvalidEnum)?;
    let want = match P::KIND {
        EventKind::L2_SNAPSHOT => {
            if !(1..=3).contains(&u8_at(pb, 64)) {
                return Err(DecodeErr::InvalidEnum);
            }
            16 * (usize::from(u16_at(pb, 68)) + usize::from(u16_at(pb, 70)))
        }
        EventKind::CONFIG_BLOB => {
            if u8_at(pb, 64) > 1 {
                return Err(DecodeErr::InvalidEnum);
            }
            u32_at(pb, 4) as usize
        }
        EventKind::RAW_RX | EventKind::RAW_TX => u32_at(pb, 16) as usize,
        EventKind::CONTROL_CMD => u32_at(pb, 76) as usize,
        _ => tail.len(),
    };
    if tail.len() < want {
        return Err(DecodeErr::Short);
    }
    if tail.len() > want {
        return Err(DecodeErr::TooLong);
    }
    Ok((p, tail))
}

/// Splits a logical body into segments of ≤ `PART_MAX_CHUNK` bytes.
#[derive(Clone, Debug)]
pub struct PartIter<'a> {
    logical: &'a [u8],
    parts: u16,
    next: u16,
}

impl<'a> PartIter<'a> {
    /// More than 65 535 segments → `TooLong`. An empty body is one empty segment.
    pub fn new(logical: &'a [u8]) -> Result<Self, EncodeErr> {
        let n = logical.len().div_ceil(PART_MAX_CHUNK).max(1);
        let parts = u16::try_from(n).map_err(|_| EncodeErr::TooLong)?;
        Ok(PartIter {
            logical,
            parts,
            next: 0,
        })
    }

    /// Number of segments.
    pub fn parts(&self) -> u16 {
        self.parts
    }
}

impl<'a> Iterator for PartIter<'a> {
    /// `(PartHdr, chunk, flags)`; flags contain `PART` on every segment but the last.
    type Item = (PartHdr, &'a [u8], RecordFlags);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.parts {
            return None;
        }
        let i = usize::from(self.next);
        let from = i * PART_MAX_CHUNK;
        let to = (from + PART_MAX_CHUNK).min(self.logical.len());
        let chunk = self.logical.get(from..to).unwrap_or(&[]);
        let last = self.next + 1 == self.parts;
        let ph = PartHdr {
            total_len: self.logical.len() as u32,
            chunk_len: chunk.len() as u32,
            part: self.next,
            parts: self.parts,
            _rsv: 0,
        };
        self.next += 1;
        let flags = if last { RecordFlags(0) } else { RecordFlags::PART };
        Some((ph, chunk, flags))
    }
}

/// Reassembles Multi segments into a caller-provided buffer (no allocation).
#[derive(Debug)]
pub struct Reassembler<'b> {
    buf: &'b mut [u8],
    active: bool,
    kind: u16,
    tsc: u64,
    parts: u16,
    total: usize,
    next: u16,
    last_seq: u64,
    filled: usize,
}

impl<'b> Reassembler<'b> {
    /// The group size limit is `buf.len()`.
    pub fn new(buf: &'b mut [u8]) -> Self {
        Reassembler {
            buf,
            active: false,
            kind: 0,
            tsc: 0,
            parts: 0,
            total: 0,
            next: 0,
            last_seq: 0,
            filled: 0,
        }
    }

    /// Drops an unfinished group.
    pub fn reset(&mut self) {
        self.active = false;
        self.filled = 0;
        self.next = 0;
    }

    /// Feeds one segment (`rec.body` = `PartHdr` + chunk); the last segment returns the whole
    /// logical body. Any inconsistency drops the group (`BadPart`; `TooLong` when the group
    /// exceeds the buffer).
    pub fn push(&mut self, rec: &RecordView<'_>) -> Result<Option<&[u8]>, DecodeErr> {
        match self.step(rec) {
            Ok(true) => {
                self.active = false;
                Ok(self.buf.get(..self.total))
            }
            Ok(false) => Ok(None),
            Err(e) => {
                self.reset();
                Err(e)
            }
        }
    }

    fn step(&mut self, rec: &RecordView<'_>) -> Result<bool, DecodeErr> {
        let (ph, rest) = PartHdr::read_from_prefix(rec.body).map_err(|_| DecodeErr::BadPart)?;
        let chunk = rest.get(..ph.chunk_len as usize).ok_or(DecodeErr::BadPart)?;
        let h = &rec.hdr;
        if !self.active {
            if ph.part != 0 || ph.parts == 0 {
                return Err(DecodeErr::BadPart);
            }
            if ph.total_len as usize > self.buf.len() {
                return Err(DecodeErr::TooLong);
            }
            self.active = true;
            self.kind = h.kind;
            self.tsc = h.tsc;
            self.parts = ph.parts;
            self.total = ph.total_len as usize;
            self.next = 0;
            self.filled = 0;
        } else if ph.part != self.next
            || h.kind != self.kind
            || h.tsc != self.tsc
            || ph.parts != self.parts
            || ph.total_len as usize != self.total
            || h.seq != self.last_seq.wrapping_add(1)
        {
            return Err(DecodeErr::BadPart);
        }
        let last = ph.part + 1 == ph.parts;
        if RecordFlags(h.flags).contains(RecordFlags::PART) == last {
            return Err(DecodeErr::BadPart);
        }
        let end = self.filled + chunk.len();
        let dst = self.buf.get_mut(self.filled..end).filter(|_| end <= self.total);
        let Some(dst) = dst else {
            return Err(DecodeErr::BadPart);
        };
        dst.copy_from_slice(chunk);
        self.filled = end;
        self.next = ph.part.wrapping_add(1);
        self.last_seq = h.seq;
        if last {
            if self.filled != self.total {
                return Err(DecodeErr::BadPart);
            }
            return Ok(true);
        }
        Ok(false)
    }
}
