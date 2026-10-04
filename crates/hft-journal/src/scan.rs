//! Record framing checks shared by recovery, readers and `verify` (journal-format "提交游标",
//! "记录对齐与 CRC", "变长与多段记录").

use std::path::Path;

use hft_event::{DecodeErr, EventKind, Layout, PartHdr, RecordFlags, RecordView, decode_record, kind_info};
use sha2::{Digest, Sha256};
use zerocopy::FromBytes;

use crate::config::{JournalKind, Position, ReadErr, RollMark};
use crate::fs;
use crate::layout::{
    HEADER_FIXED_LEN, HEADER_LEN, OFF_STATE, SEGMENT_MAGIC, SHARED_ROWS, SegmentHeaderFixed,
};

/// Whether this binary registers the dev kinds `0x7Fxx` (hft-event feature `dev-kinds`).
pub(crate) fn dev_kinds_known() -> bool {
    kind_info(EventKind::T_IN).is_some()
}

/// How strictly kinds are checked.
#[derive(Copy, Clone, Debug)]
pub(crate) struct KindRules {
    pub journal: JournalKind,
    /// Unregistered kinds are invalid (recovery, verify) instead of delivered (reader).
    pub require_registered: bool,
    /// `0x7Fxx` → `DevKind`.
    pub production: bool,
}

/// Checks the record at the start of `buf` (bounded by the readable end): framing, CRC, seq and
/// kind. `buf` shorter than 24 B or `len == 0` → `Ok(None)` (end of data).
pub(crate) fn check<'a>(
    buf: &'a [u8],
    expected_seq: u64,
    r: KindRules,
) -> Result<Option<RecordView<'a>>, ReadErr> {
    let len = buf
        .get(..4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if len == 0 {
        return Ok(None);
    }
    let v = match decode_record(buf) {
        Ok(v) => v,
        Err(DecodeErr::BadCrc) => return Err(ReadErr::BadCrc),
        Err(_) => return Err(ReadErr::Corrupt),
    };
    if v.hdr.seq != expected_seq {
        return Err(ReadErr::Corrupt);
    }
    let k = EventKind(v.hdr.kind);
    if k.0 >> 8 == 0x7F && r.production {
        return Err(ReadErr::DevKind);
    }
    match kind_info(k) {
        Some(i) => {
            if !i.journals.contains(r.journal.mask()) {
                return Err(ReadErr::Corrupt);
            }
        }
        None => {
            if r.require_registered {
                return Err(ReadErr::Corrupt);
            }
        }
    }
    Ok(Some(v))
}

/// Whether records of `kind` carry a `PartHdr`.
pub(crate) fn is_multi(kind: u16) -> bool {
    matches!(
        kind_info(EventKind(kind)).map(|i| i.layout),
        Some(Layout::Multi { .. })
    )
}

pub(crate) fn part_hdr(body: &[u8]) -> Option<PartHdr> {
    PartHdr::read_from_prefix(body).ok().map(|(p, _)| p)
}

/// Result of scanning forward from a committed point (recovery step 2).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tail {
    /// Truncation point: after the last `EVENT_END` record in the scanned range (or `from`).
    pub end: usize,
    /// Seq of the last record before `end`.
    pub end_seq: u64,
    /// End of the last structurally valid record.
    pub valid_end: usize,
}

/// Scans `seg[from..]` (whole segment memory) for complete records; `seq_before` = seq of the
/// record before `from`.
pub(crate) fn scan_tail(seg: &[u8], from: usize, seq_before: u64, journal: JournalKind) -> Tail {
    let rules = KindRules {
        journal,
        require_registered: true,
        production: false,
    };
    let mut t = Tail {
        end: from,
        end_seq: seq_before,
        valid_end: from,
    };
    let mut off = from;
    let mut seq = seq_before;
    while off < seg.len() {
        let Some(buf) = seg.get(off..) else { break };
        let v = match check(buf, seq + 1, rules) {
            Ok(Some(v)) => v,
            _ => break,
        };
        off += v.hdr.len as usize;
        seq += 1;
        t.valid_end = off;
        let f = RecordFlags(v.hdr.flags);
        if f.contains(RecordFlags::EVENT_END) && !f.contains(RecordFlags::PART) {
            t.end = off;
            t.end_seq = seq;
        }
    }
    t
}

/// Zeroes `[end, next all-zero page)` in `seg` through `zero` (recovery step 2).
pub(crate) fn zero_to_clean_page(seg: &[u8], end: usize, mut zero: impl FnMut(usize, usize)) {
    const PG: usize = 4096;
    let first_pg = end.div_ceil(PG) * PG;
    let upto = first_pg.min(seg.len());
    if seg.get(end..upto).is_some_and(|b| b.iter().any(|x| *x != 0)) {
        zero(end, upto - end);
    }
    let mut p = first_pg;
    while p < seg.len() {
        let q = (p + PG).min(seg.len());
        match seg.get(p..q) {
            Some(b) if b.iter().any(|x| *x != 0) => zero(p, q - p),
            _ => break,
        }
        p = q;
    }
}

/// SHA-256 of a header's immutable region (`prev_header_sha256` of the next segment).
pub(crate) fn header_sha(fixed: &[u8]) -> [u8; 32] {
    Sha256::digest(fixed.get(..HEADER_FIXED_LEN).unwrap_or(fixed)).into()
}

/// A segment header read from a file prefix: `(fixed, state)`; `None` when not ours.
pub(crate) fn read_header(path: &Path) -> Option<(SegmentHeaderFixed, u32)> {
    let b = fs::read_prefix(path, HEADER_LEN).ok()?;
    let f = SegmentHeaderFixed::read_from_bytes(b.get(..HEADER_FIXED_LEN)?).ok()?;
    if f.magic != SEGMENT_MAGIC {
        return None;
    }
    let st = b
        .get(OFF_STATE..OFF_STATE + 4)
        .map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    Some((f, st))
}

/// Feeds `bytes` (located at file offset `at`) to `h` with `3072..3200` replaced by zeros.
pub(crate) fn hash_masked(h: &mut Sha256, at: usize, bytes: &[u8]) {
    let end = at + bytes.len();
    let (ms, me) = (SHARED_ROWS.start.max(at), SHARED_ROWS.end.min(end));
    if ms >= me {
        h.update(bytes);
        return;
    }
    h.update(bytes.get(..ms - at).unwrap_or(&[]));
    h.update(&[0u8; 128][..me - ms]);
    h.update(bytes.get(me - at..).unwrap_or(&[]));
}

/// Reason byte of the segment's final `End` record (`None` when the last record is not `End`).
pub(crate) fn end_reason(bytes: &[u8], kind: JournalKind) -> Option<u8> {
    let n = bytes.len();
    let rl = if kind == JournalKind::Raw { 80 } else { 72 };
    let rec = bytes.get(n.checked_sub(rl)?..)?;
    let k = u16::from_le_bytes([*rec.get(4)?, *rec.get(5)?]);
    (k == EventKind::SEGMENT_ROLL.0 && *rec.get(24 + 32)? == 1).then(|| rec.get(24 + 33).copied())?
}

/// Latest complete boot block (`Begin`) or `Keyframe` snapshot in `[4096, bound)`.
pub(crate) fn rebuild_point(bytes: &[u8], no: u64, first_seq: u64, kind: JournalKind) -> Option<Position> {
    let rules = KindRules {
        journal: kind,
        require_registered: false,
        production: false,
    };
    let mut off = HEADER_LEN;
    let mut seq = first_seq;
    let mut begin: Option<Position> = None;
    let mut best: Option<Position> = None;
    while let Some(buf) = bytes.get(off..) {
        let Ok(Some(v)) = check(buf, seq, rules) else {
            break;
        };
        let k = EventKind(v.hdr.kind);
        if k == EventKind::SEGMENT_ROLL {
            let mark = v.body.get(32).copied().unwrap_or(0);
            if mark == RollMark::Begin as u8 {
                begin = Some(Position {
                    segment_no: no,
                    offset: off as u64,
                    seq,
                });
                if !kind.has_boot() {
                    best = begin;
                }
            } else if mark == RollMark::BootDone as u8 && best.is_none() {
                best = begin;
            }
        } else if k == EventKind::STATE_SNAPSHOT
            && let Some(ph) = part_hdr(v.body)
            && ph.part == 0
            && v.body.get(16).copied() == Some(2)
        {
            best = Some(Position {
                segment_no: no,
                offset: off as u64,
                seq,
            });
        }
        off += v.hdr.len as usize;
        seq += 1;
    }
    best
}
