//! `JournalWriter`: RAM hot-segment append, publish, event boundaries and the size valves
//! (README "写者", "实现说明").

use std::cell::Cell;
use std::marker::PhantomData;
use std::path::PathBuf;

use hft_event::{
    Body, EventKind, Layout, PART_MAX_CHUNK, PartHdr, RecordFlags, Role, classify, kind_info, record_len,
};
use hft_rt::{Consumer, Producer};
use hft_types::{Seq, Tsc};
use zerocopy::IntoBytes;

use crate::config::{AppendErr, JournalConfig, JournalKind, RollAt, RollReason};
use crate::layout::{HEADER_LEN, OFF_COMMITTED, OFF_LAST_SEQ, SegMap};

/// Largest record.
const MAX_RECORD: usize = 65_536;

/// A prepared next segment handed from `SegmentPreparer` to the writer (README "预创建").
#[derive(Copy, Clone, Debug)]
pub struct PreparedSegment {
    pub(crate) map: SegMap,
    pub segment_no: u64,
}

/// An old segment handed from the writer to `HotRetirer` (the writer never unmaps).
#[derive(Copy, Clone, Debug)]
pub struct RetiredSegment {
    pub(crate) map: SegMap,
    pub segment_no: u64,
}

impl RetiredSegment {
    /// Base address of the mapping.
    pub fn base(&self) -> usize {
        self.map.base_addr()
    }
    /// Mapped length.
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.len() == 0
    }
}

/// Assembly-time environment of a writer.
#[derive(Debug)]
pub struct WriterEnv {
    pub writer_epoch: u32,
    pub build_git: [u8; 20],
    pub host_profile_sha256: [u8; 32],
    pub prepared: Consumer<PreparedSegment>,
    pub retired: Producer<RetiredSegment>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum BootState {
    None,
    Active,
    Dropped,
}

/// The last non-Meta record of the open event (gets `EVENT_END` at `end_event`).
#[derive(Copy, Clone, Debug)]
pub(crate) struct LastRec {
    off: usize,
    len: usize,
    /// The record carries a CRC (recomputed when `end_event` changes its flags).
    crc: bool,
}

/// Single writer of one journal (`Send`, `!Sync`).
#[derive(Debug)]
pub struct JournalWriter {
    pub(crate) cfg: JournalConfig,
    pub(crate) cold_jdir: PathBuf,
    pub(crate) hot_jdir: PathBuf,
    pub(crate) epoch: u32,
    pub(crate) build_git: [u8; 20],
    pub(crate) host_sha: [u8; 32],
    pub(crate) prepared: Option<Consumer<PreparedSegment>>,
    pub(crate) retired: Option<Producer<RetiredSegment>>,
    /// `open_offline`: `<out_dir>/<instance>/<journal>/`.
    pub(crate) offline: Option<PathBuf>,
    pub(crate) cur: Option<SegMap>,
    pub(crate) cur_no: u64,
    pub(crate) cap: usize,
    pub(crate) off: usize,
    pub(crate) next_seq: u64,
    /// Last event boundary (`publish` target).
    pub(crate) bound_off: usize,
    pub(crate) bound_seq: u64,
    pub(crate) published_off: usize,
    pub(crate) in_event: bool,
    pub(crate) last: Option<LastRec>,
    pub(crate) boot: BootState,
    pub(crate) boot_records: u32,
    pub(crate) boot_utc: i64,
    pub(crate) reserve: usize,
    pub(crate) deadline: Option<u64>,
    /// Header `flags` for the next segment (RECOVERY_GAP after a lossy recovery).
    pub(crate) next_flags: u8,
    /// Test hook: seqs skipped between the next `End` and `Begin` (JNL-081).
    pub(crate) skip_on_roll: u64,
    pub(crate) scratch: Vec<u8>,
    pub(crate) _not_sync: PhantomData<Cell<()>>,
}

/// Where the bytes of a record body come from.
pub(crate) enum Src<'a, 'p> {
    /// Already in place at `off + 24` (+ 16 for Multi).
    InPlace,
    /// Copied from the cursor.
    Pieces(&'a mut Cursor<'p>),
}

/// Reads consecutive bytes out of scattered pieces.
pub(crate) struct Cursor<'p> {
    pieces: &'p [&'p [u8]],
    i: usize,
    pos: usize,
}

impl<'p> Cursor<'p> {
    pub(crate) fn new(pieces: &'p [&'p [u8]]) -> Cursor<'p> {
        Cursor { pieces, i: 0, pos: 0 }
    }

    /// Copies the next `n` bytes into `map` at `off`.
    fn copy_to(&mut self, map: &SegMap, mut off: usize, mut n: usize) {
        while n > 0 {
            let Some(p) = self.pieces.get(self.i) else { return };
            let rest = p.get(self.pos..).unwrap_or(&[]);
            if rest.is_empty() {
                self.i += 1;
                self.pos = 0;
                continue;
            }
            let k = rest.len().min(n);
            map.write(off, rest.get(..k).unwrap_or(&[]));
            off += k;
            n -= k;
            self.pos += k;
        }
    }
}

/// Whether a record of `kind` with logical body prefix `prefix` is a Meta record.
pub(crate) fn is_meta(kind: EventKind, prefix: &[u8]) -> bool {
    match kind_info(kind).map(|i| i.role) {
        Some(Role::Meta) => true,
        Some(Role::ByBody) => classify(kind, prefix) == Ok(Role::Meta),
        _ => false,
    }
}

/// Segment shape of a variable-length append.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Shape {
    /// `None` = single record without `PartHdr`.
    pub parts: Option<u16>,
    /// Total bytes of all records.
    pub total: usize,
}

impl JournalWriter {
    pub(crate) fn crc_for(&self, kind: EventKind) -> bool {
        self.cfg.kind == JournalKind::Raw
            || kind_info(kind).is_some_and(|i| i.crc == hft_event::CrcPolicy::Always)
    }

    fn writable(&self) -> Result<SegMap, AppendErr> {
        if self.boot == BootState::Dropped {
            return Err(AppendErr::InBoot);
        }
        self.cur.ok_or(AppendErr::Sealed)
    }

    pub(crate) fn limit(&self, boot: bool) -> usize {
        if boot {
            self.cap
        } else {
            self.cap.saturating_sub(self.reserve)
        }
    }

    /// Writes one record at `off` (body from `src`; header fields; CRC; Release `len`) and advances.
    /// The caller has checked the space. Returns the record's offset, length and CRC rest.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn put(
        &mut self,
        map: &SegMap,
        kind: u16,
        flags: u16,
        tsc: u64,
        part: Option<PartHdr>,
        src: Src<'_, '_>,
        body_len: usize,
        crc: bool,
    ) -> LastRec {
        let off = self.off;
        let ph_len = if part.is_some() { 16 } else { 0 };
        let len = record_len(ph_len + body_len, crc) as usize;
        if let Some(ph) = part {
            map.write(off + 24, ph.as_bytes());
        }
        if let Src::Pieces(c) = src {
            c.copy_to(map, off + 24 + ph_len, body_len);
        }
        let seq = self.next_seq;
        let mut h = [0u8; 20];
        h[0..2].copy_from_slice(&kind.to_le_bytes());
        h[2..4].copy_from_slice(&flags.to_le_bytes());
        h[4..12].copy_from_slice(&seq.to_le_bytes());
        h[12..20].copy_from_slice(&tsc.to_le_bytes());
        map.write(off + 4, &h);
        if crc {
            // `len` takes part with its final value although it is stored last (Release).
            let mut head = [0u8; 8];
            head[0..4].copy_from_slice(&(len as u32).to_le_bytes());
            head[4..8].copy_from_slice(&h[0..4]);
            let c = crc32c::crc32c_append(crc32c::crc32c(&head), map.bytes(off + 8, len - 12));
            map.write(off + len - 4, &c.to_le_bytes());
        }
        map.store32(off, len as u32);
        self.off = off + len;
        self.next_seq = seq + 1;
        LastRec { off, len, crc }
    }

    /// Flags for the last (or only) part of a record outside the boot block.
    fn event_flags(&self, meta: bool) -> u16 {
        if self.cfg.kind == JournalKind::Raw || (meta && !self.in_event) {
            RecordFlags::EVENT_END.0
        } else {
            0
        }
    }

    /// Event bookkeeping after the last part of a record outside the boot block.
    fn after(&mut self, meta: bool, rec: LastRec) {
        if self.cfg.kind == JournalKind::Raw || (meta && !self.in_event) {
            self.bound_off = self.off;
            self.bound_seq = self.next_seq - 1;
        } else if !meta {
            self.in_event = true;
            self.last = Some(rec);
        }
    }

    /// Appends a fixed body. The caller's `CRC`, `PART` and `EVENT_END` bits are ignored.
    #[inline]
    pub fn append<B: Body>(
        &mut self,
        flags: hft_event::RecordFlags,
        tsc: Tsc,
        body: &B,
    ) -> Result<Seq, AppendErr> {
        let map = self.writable()?;
        let kind = B::KIND;
        let bytes = body.as_bytes();
        let crc = self.crc_for(kind);
        let len = record_len(bytes.len(), crc) as usize;
        if len > MAX_RECORD {
            return Err(AppendErr::TooLarge);
        }
        if self.off + len > self.limit(false) {
            return Err(AppendErr::Full);
        }
        let meta = is_meta(kind, bytes);
        let f = (flags.0 & !(RecordFlags::CRC.0 | RecordFlags::PART.0 | RecordFlags::EVENT_END.0))
            | self.event_flags(meta)
            | if crc { RecordFlags::CRC.0 } else { 0 };
        let seq = self.next_seq;
        let pieces: [&[u8]; 1] = [bytes];
        let mut cur = Cursor::new(&pieces);
        let rec = self.put(
            &map,
            kind.0,
            f,
            tsc.0,
            None,
            Src::Pieces(&mut cur),
            bytes.len(),
            crc,
        );
        self.after(meta, rec);
        Ok(Seq(seq))
    }

    /// Appends a Meta record: with `EVENT_END` outside an event, without inside one.
    pub fn append_meta<B: Body>(&mut self, tsc: Tsc, body: &B) -> Result<Seq, AppendErr> {
        let map = self.writable()?;
        let kind = B::KIND;
        let bytes = body.as_bytes();
        let crc = self.crc_for(kind);
        let len = record_len(bytes.len(), crc) as usize;
        if self.off + len > self.limit(false) {
            return Err(AppendErr::Full);
        }
        let f = self.event_flags(true) | if crc { RecordFlags::CRC.0 } else { 0 };
        let seq = self.next_seq;
        let pieces: [&[u8]; 1] = [bytes];
        let mut cur = Cursor::new(&pieces);
        let rec = self.put(
            &map,
            kind.0,
            f,
            tsc.0,
            None,
            Src::Pieces(&mut cur),
            bytes.len(),
            crc,
        );
        self.after(true, rec);
        Ok(Seq(seq))
    }

    /// Segment shape of a variable-length logical body of `l` bytes (`TooLarge` checks).
    pub(crate) fn shape(&self, kind: EventKind, l: u64, crc: bool) -> Result<Shape, AppendErr> {
        let single = |body: u64| -> Result<Shape, AppendErr> {
            let n = record_len(usize::try_from(body).map_err(|_| AppendErr::TooLarge)?, crc) as usize;
            if n > MAX_RECORD {
                return Err(AppendErr::TooLarge);
            }
            Ok(Shape {
                parts: None,
                total: n,
            })
        };
        let s = match kind_info(kind).map(|i| i.layout) {
            Some(Layout::Var { prefix, max }) => {
                if l < u64::from(prefix) || l - u64::from(prefix) > u64::from(max) {
                    return Err(AppendErr::TooLarge);
                }
                single(l)?
            }
            Some(Layout::Fixed { .. }) => single(l)?,
            _ => {
                if l >= 1 << 32 {
                    return Err(AppendErr::TooLarge);
                }
                let chunk = PART_MAX_CHUNK as u64;
                let parts = l.div_ceil(chunk).max(1);
                if parts > 65_535 {
                    return Err(AppendErr::TooLarge);
                }
                let full = record_len(16 + PART_MAX_CHUNK, crc) as u64;
                let last = l - (parts - 1) * chunk;
                let total = (parts - 1) * full + u64::from(record_len(16 + last as usize, crc));
                Shape {
                    parts: Some(parts as u16),
                    total: usize::try_from(total).map_err(|_| AppendErr::TooLarge)?,
                }
            }
        };
        let room = self.cap.saturating_sub(HEADER_LEN + self.reserve);
        if s.total > room {
            return Err(AppendErr::TooLarge);
        }
        Ok(s)
    }

    /// Writes a variable-length body (all parts). `boot` = inside the boot block.
    pub(crate) fn put_var(
        &mut self,
        kind: EventKind,
        flags: u16,
        tsc: u64,
        pieces: &[&[u8]],
        boot: bool,
    ) -> Result<Seq, AppendErr> {
        let map = self.writable()?;
        let l: u64 = pieces.iter().map(|p| p.len() as u64).sum();
        let crc = self.crc_for(kind);
        let s = self.shape(kind, l, crc)?;
        if self.off + s.total > self.limit(boot) {
            return Err(AppendErr::Full);
        }
        let mut head = [0u8; 96];
        let mut c = Cursor::new(pieces);
        c.copy_head(&mut head);
        let meta = is_meta(kind, &head);
        let user = flags & !(RecordFlags::CRC.0 | RecordFlags::PART.0 | RecordFlags::EVENT_END.0);
        let crc_f = if crc { RecordFlags::CRC.0 } else { 0 };
        let last_f = if boot {
            self.boot_flags()
        } else {
            self.event_flags(meta)
        };
        let seq = self.next_seq;
        let mut cur = Cursor::new(pieces);
        let rec = match s.parts {
            None => self.put(
                &map,
                kind.0,
                user | crc_f | last_f,
                tsc,
                None,
                Src::Pieces(&mut cur),
                l as usize,
                crc,
            ),
            Some(parts) => {
                let mut rec = None;
                for p in 0..parts {
                    let from = u64::from(p) * PART_MAX_CHUNK as u64;
                    let n = (l - from).min(PART_MAX_CHUNK as u64) as usize;
                    let lastp = p + 1 == parts;
                    let ph = PartHdr {
                        total_len: l as u32,
                        chunk_len: n as u32,
                        part: p,
                        parts,
                        _rsv: 0,
                    };
                    let f = user | crc_f | if lastp { last_f } else { RecordFlags::PART.0 };
                    rec = Some(self.put(&map, kind.0, f, tsc, Some(ph), Src::Pieces(&mut cur), n, crc));
                }
                rec.unwrap_or(LastRec {
                    off: self.off,
                    len: 0,
                    crc: false,
                })
            }
        };
        if boot {
            self.boot_records += s.parts.map_or(1, u32::from);
        } else {
            self.after(meta, rec);
        }
        Ok(Seq(seq))
    }

    /// Flags of a boot-block record's last part.
    pub(crate) fn boot_flags(&self) -> u16 {
        if self.cfg.kind == JournalKind::Raw {
            RecordFlags::EVENT_END.0
        } else {
            0
        }
    }

    /// Appends a variable-length record (Var: one record; Multi: split into parts) from scattered
    /// pieces. `TooLarge`/`Full` write nothing.
    pub fn append_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        pieces: &[&[u8]],
    ) -> Result<Seq, AppendErr> {
        self.put_var(kind, flags.0, tsc.0, pieces, false)
    }

    /// Reserves a variable-length record of logical length `len` for in-place writing.
    pub fn reserve_var(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        len: u32,
    ) -> Result<VarSlot<'_>, AppendErr> {
        let map = self.writable()?;
        let crc = self.crc_for(kind);
        let s = self.shape(kind, u64::from(len), crc)?;
        if self.off + s.total > self.limit(false) {
            return Err(AppendErr::Full);
        }
        let in_place = s.parts.is_none_or(|p| p == 1);
        if !in_place {
            self.scratch.clear();
            self.scratch.resize(len as usize, 0);
        }
        Ok(VarSlot {
            w: self,
            map,
            kind,
            flags: flags.0,
            tsc: tsc.0,
            len: len as usize,
            multi: s.parts.is_some(),
            in_place,
            done: false,
        })
    }

    /// Release-stores `committed_len` and `last_seq` up to the last event boundary.
    #[inline]
    pub fn publish(&mut self) {
        if let Some(m) = self.cur {
            m.store64(OFF_LAST_SEQ, self.bound_seq);
            m.store64(OFF_COMMITTED, self.bound_off as u64);
            self.published_off = self.bound_off;
        }
    }

    /// Sets `EVENT_END` on the last non-Meta record since the previous `end_event` (no-op for raw
    /// journals and when there is none).
    #[inline]
    pub fn end_event(&mut self) {
        if !self.in_event {
            return;
        }
        self.in_event = false;
        let (Some(m), Some(r)) = (self.cur, self.last.take()) else {
            return;
        };
        let old = m.bytes(r.off + 6, 2);
        let f = u16::from_le_bytes([
            old.first().copied().unwrap_or(0),
            old.get(1).copied().unwrap_or(0),
        ]) | RecordFlags::EVENT_END.0;
        m.write(r.off + 6, &f.to_le_bytes());
        if r.crc {
            let c = crc32c::crc32c(m.bytes(r.off, r.len - 4));
            m.write(r.off + r.len - 4, &c.to_le_bytes());
        }
        self.bound_off = self.off;
        self.bound_seq = self.next_seq - 1;
    }

    /// The seq the next record will get.
    #[inline]
    pub fn next_seq(&self) -> Seq {
        Seq(self.next_seq)
    }

    /// Two-level size valve and the raw hourly deadline (README 调用约束第 5 条).
    pub fn needs_roll(&self, now: Tsc) -> Option<(RollReason, RollAt)> {
        self.cur?;
        let used = (self.published_off + self.reserve) as u128 * 100;
        let seg = u128::from(self.cfg.segment_bytes);
        if used >= u128::from(self.cfg.size_valve_pct) * seg {
            return Some((RollReason::Size, RollAt::EventBoundary));
        }
        if self.deadline.is_some_and(|d| now.0 >= d) {
            return Some((RollReason::Hourly, RollAt::EventBoundary));
        }
        if self.cfg.kind != JournalKind::Raw && used >= u128::from(self.cfg.size_idle_pct) * seg {
            return Some((RollReason::Size, RollAt::IdleRound));
        }
        None
    }

    /// Raw writers: the TSC of the next UTC hour (cleared by the next roll).
    pub fn set_roll_deadline(&mut self, at: Tsc) {
        self.deadline = Some(at.0);
    }

    /// Byte offset of the next record in the current segment (tests and tools).
    #[doc(hidden)]
    pub fn offset(&self) -> u64 {
        self.off as u64
    }

    /// Current segment number (0 before the first roll).
    #[doc(hidden)]
    pub fn segment_no(&self) -> u64 {
        if self.cur.is_some() { self.cur_no } else { 0 }
    }

    /// Bytes reserved for the next roll (`2 × boot block`).
    #[doc(hidden)]
    pub fn reserve_bytes(&self) -> u64 {
        self.reserve as u64
    }

    /// Test hook: at the next roll, skip `n` seqs after `End` and (optionally) flag the new segment
    /// `RECOVERY_GAP` — fabricates a lossy segment boundary for reader tests.
    #[doc(hidden)]
    pub fn debug_gap_on_roll(&mut self, n: u64, flag: bool) {
        self.skip_on_roll = n;
        if flag {
            self.next_flags = crate::layout::RECOVERY_GAP;
        }
    }

    /// Test hook: writes exactly one record with the given part header and flags, bypassing the
    /// writer's grouping (JNL-079); no event bookkeeping.
    #[doc(hidden)]
    pub fn append_part_raw(
        &mut self,
        kind: EventKind,
        flags: RecordFlags,
        tsc: Tsc,
        ph: PartHdr,
        chunk: &[u8],
    ) -> Result<Seq, AppendErr> {
        let map = self.writable()?;
        let crc = self.crc_for(kind);
        let len = record_len(16 + chunk.len(), crc) as usize;
        if self.off + len > self.cap {
            return Err(AppendErr::Full);
        }
        let seq = self.next_seq;
        let f = flags.0 | if crc { RecordFlags::CRC.0 } else { 0 };
        let pieces: [&[u8]; 1] = [chunk];
        let mut cur = Cursor::new(&pieces);
        self.put(
            &map,
            kind.0,
            f,
            tsc.0,
            Some(ph),
            Src::Pieces(&mut cur),
            chunk.len(),
            crc,
        );
        Ok(Seq(seq))
    }
}

impl Cursor<'_> {
    /// Copies the first bytes of the logical body (classification prefix).
    fn copy_head(&mut self, out: &mut [u8]) {
        let mut o = 0;
        for p in self.pieces {
            if o >= out.len() {
                break;
            }
            let k = p.len().min(out.len() - o);
            if let (Some(d), Some(s)) = (out.get_mut(o..o + k), p.get(..k)) {
                d.copy_from_slice(s);
            }
            o += k;
        }
    }
}

/// An in-place variable-length record (`reserve_var`). Dropping it without `commit` appends
/// nothing, leaves `next_seq` unchanged and zeroes the bytes written.
#[derive(Debug)]
pub struct VarSlot<'a> {
    w: &'a mut JournalWriter,
    map: SegMap,
    kind: EventKind,
    flags: u16,
    tsc: u64,
    len: usize,
    multi: bool,
    in_place: bool,
    done: bool,
}

impl VarSlot<'_> {
    fn body_off(&self) -> usize {
        self.w.off + 24 + if self.multi { 16 } else { 0 }
    }

    /// The logical body to fill (`len` bytes).
    pub fn buf(&mut self) -> &mut [u8] {
        if self.in_place {
            let o = self.body_off();
            self.map.bytes_mut(o, self.len)
        } else {
            &mut self.w.scratch[..]
        }
    }

    /// Appends the record; returns its seq (first part).
    pub fn commit(mut self) -> Seq {
        self.done = true;
        let seq = self.w.next_seq;
        if self.in_place {
            let kind = self.kind;
            let crc = self.w.crc_for(kind);
            let o = self.body_off();
            let head = self.map.bytes(o, self.len.min(96)).to_vec();
            let meta = is_meta(kind, &head);
            let user = self.flags & !(RecordFlags::CRC.0 | RecordFlags::PART.0 | RecordFlags::EVENT_END.0);
            let f = user | if crc { RecordFlags::CRC.0 } else { 0 } | self.w.event_flags(meta);
            let ph = self.multi.then_some(PartHdr {
                total_len: self.len as u32,
                chunk_len: self.len as u32,
                part: 0,
                parts: 1,
                _rsv: 0,
            });
            let map = self.map;
            let rec = self
                .w
                .put(&map, kind.0, f, self.tsc, ph, Src::InPlace, self.len, crc);
            self.w.after(meta, rec);
            Seq(seq)
        } else {
            let body = std::mem::take(&mut self.w.scratch);
            let r = self.w.put_var(self.kind, self.flags, self.tsc, &[&body], false);
            self.w.scratch = body;
            r.unwrap_or(Seq(seq))
        }
    }
}

impl Drop for VarSlot<'_> {
    fn drop(&mut self) {
        if !self.done && self.in_place {
            let n = 24 + if self.multi { 16 } else { 0 } + self.len + 8;
            self.map.zero(self.w.off, n);
        }
    }
}
