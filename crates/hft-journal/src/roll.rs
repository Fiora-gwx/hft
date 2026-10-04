//! `roll`, `BootWriter`, `seal_and_close`: End/Begin/BootDone records and the `[W]` header fields
//! (README "roll 步骤", journal-format "段滚动与引导块").

use hft_event::{BlobRef, Body, EventKind, RecordFlags, SegmentRoll};
use hft_rt::CalibPair;
use hft_types::{Seq, Tsc};
use sha2::{Digest, Sha256};
use zerocopy::IntoBytes;

use crate::config::{AppendErr, JournalKind, RollErr, RollMark, RollReason, SegState};
use crate::fs;
use crate::layout::{self, HEADER_LEN, OFF_COMMITTED, OFF_LAST_SEQ, OFF_STATE, SEGMENT_MAGIC, SegMap};
use crate::oplog::{self, Op};
use crate::prepare::p_header;
use crate::scan::header_sha;
use crate::writer::{BootState, Cursor, JournalWriter, RetiredSegment, Src};

/// Data for the `[W]` header fields of the new segment.
#[derive(Copy, Clone, Debug)]
pub struct RollInfo<'a> {
    pub utc_ns: i64,
    pub calib: CalibPair,
    pub tsc_hz_milli: u64,
    pub configs: &'a [BlobRef],
}

/// `config_digest`: SHA-256 of the `BlobRef`s concatenated in order.
pub(crate) fn config_digest(configs: &[BlobRef]) -> [u8; 32] {
    let mut h = Sha256::new();
    for c in configs {
        h.update(c.as_bytes());
    }
    h.finalize().into()
}

impl JournalWriter {
    /// Writes a `SegmentRoll` record (`EVENT_END` per `flags`), using the reserve.
    pub(crate) fn put_roll(&mut self, map: &SegMap, r: &SegmentRoll, flags: u16, tsc: u64) -> u64 {
        let seq = self.next_seq;
        let pieces: [&[u8]; 1] = [r.as_bytes()];
        let mut cur = Cursor::new(&pieces);
        let crc = self.crc_for(EventKind::SEGMENT_ROLL);
        let flags = flags | if crc { RecordFlags::CRC.0 } else { 0 };
        self.put(
            map,
            EventKind::SEGMENT_ROLL.0,
            flags,
            tsc,
            None,
            Src::Pieces(&mut cur),
            48,
            crc,
        );
        oplog::record(Op::Append(EventKind::SEGMENT_ROLL.0, seq));
        seq
    }

    pub(crate) fn roll_rec(
        &self,
        mark: RollMark,
        no: u64,
        other: u64,
        reason: RollReason,
        utc_ns: i64,
    ) -> SegmentRoll {
        SegmentRoll {
            segment_no: no,
            other_segment_no: other,
            seq_mark: self.next_seq - 1,
            utc_ns,
            mark: mark as u8,
            reason: reason as u8,
            journal_kind: self.cfg.kind as u8,
            _pad: 0,
            boot_records: 0,
            _rsv: 0,
        }
    }

    /// Appends `End`, publishes and seals the current segment.
    pub(crate) fn close_current(&mut self, other: u64, reason: RollReason, utc_ns: i64) -> Option<SegMap> {
        let map = self.cur?;
        let r = self.roll_rec(RollMark::End, self.cur_no, other, reason, utc_ns);
        let tsc = 0;
        self.put_roll(&map, &r, RecordFlags::EVENT_END.0, tsc);
        self.bound_off = self.off;
        self.bound_seq = self.next_seq - 1;
        self.in_event = false;
        self.last = None;
        self.publish();
        map.store32(OFF_STATE, SegState::Sealed as u32);
        Some(map)
    }

    /// Hands a sealed segment to the retirer (or, offline, writes it to its file).
    fn retire(&mut self, map: SegMap, no: u64) -> Result<(), RollErr> {
        if let Some(dir) = &self.offline {
            let n = map.load64(OFF_COMMITTED) as usize;
            let path = dir.join(fs::hot_seg_name(no));
            let r = std::fs::File::create(&path)
                .and_then(|f| fs::pwrite(&f, 0, map.bytes(0, n)).and_then(|()| f.sync_all()));
            layout::release(map);
            return r.map_err(|_| RollErr::Io);
        }
        if let Some(p) = self.retired.as_mut() {
            let _ = p.try_push(RetiredSegment { map, segment_no: no });
        }
        Ok(())
    }

    /// The next segment: from the preparer, or (offline) a fresh heap block.
    fn take_next(&mut self) -> Result<SegMap, RollErr> {
        let want = self.cur_no + 1;
        if self.offline.is_some() {
            let len = usize::try_from(self.cfg.segment_bytes).map_err(|_| RollErr::Io)?;
            let m = layout::heap_alloc(len).map_err(|_| RollErr::Io)?;
            let prev = match self.cur {
                Some(c) => header_sha(c.bytes(0, layout::HEADER_FIXED_LEN)),
                None => [0u8; 32],
            };
            m.write_fixed(&p_header(
                &self.cfg,
                self.epoch,
                self.build_git,
                self.host_sha,
                want,
                prev,
            ));
            return Ok(m);
        }
        let p = self
            .prepared
            .as_mut()
            .and_then(|c| c.try_pop())
            .ok_or(RollErr::NextNotReady)?;
        let h = p.map.fixed();
        let ok = h.magic == SEGMENT_MAGIC
            && h.format_version == layout::FORMAT_VERSION
            && h.journal_kind == self.cfg.kind as u8
            && h.journal_name == self.cfg.name.to_header()
            && h.instance == self.cfg.instance.to_header()
            && h.segment_no == want
            && p.segment_no == want
            && h.segment_bytes == self.cfg.segment_bytes
            && p.map.len() as u64 >= self.cfg.segment_bytes
            && p.map.load32(OFF_STATE) == SegState::Preparing as u32;
        if !ok {
            return Err(RollErr::Io);
        }
        Ok(p.map)
    }

    /// Rolls to the next segment between two events: `End` + seal on the current segment, `[W]`
    /// header and `Begin` on the new one; returns the boot-block writer.
    pub fn roll(&mut self, reason: RollReason, info: &RollInfo<'_>) -> Result<BootWriter<'_>, RollErr> {
        if self.boot == BootState::Dropped {
            return Err(RollErr::InBoot);
        }
        let next = self.take_next()?;
        let new_no = self.cur_no + 1;
        let old_no = self.cur_no;
        if let Some(old) = self.close_current(new_no, reason, info.utc_ns) {
            self.retire(old, old_no)?;
        }
        let prev_last = self.next_seq - 1;
        self.next_seq += std::mem::take(&mut self.skip_on_roll);
        // [W] fields.
        let mut h = next.fixed();
        h.first_seq = self.next_seq;
        h.prev_last_seq = prev_last;
        h.created_utc_ns = info.utc_ns;
        h.created_tsc = info.calib.tsc.0;
        h.roll_reason = reason as u8;
        h.flags = self.next_flags;
        h.tsc_hz_milli = info.tsc_hz_milli;
        h.calib_tsc = info.calib.tsc.0;
        h.calib_realtime_ns = info.calib.realtime_ns;
        h.calib_phc_ns = info.calib.phc_ns;
        h.calib_err_ns = info.calib.err_ns;
        h.clock_max_error_ns = 0;
        h.n_config = u16::try_from(info.configs.len()).unwrap_or(u16::MAX);
        h.config_digest = config_digest(info.configs);
        h.header_crc32c = h.compute_crc();
        next.write_fixed(&h);
        next.store64(OFF_LAST_SEQ, self.next_seq - 1);
        next.store64(OFF_COMMITTED, HEADER_LEN as u64);
        next.store32(OFF_STATE, SegState::Open as u32);
        self.next_flags = 0;
        self.cur = Some(next);
        self.cur_no = new_no;
        self.cap = next.len().min(self.cfg.segment_bytes as usize);
        self.off = HEADER_LEN;
        self.bound_off = HEADER_LEN;
        self.bound_seq = self.next_seq - 1;
        self.published_off = HEADER_LEN;
        self.in_event = false;
        self.last = None;
        self.deadline = None;
        // Begin.
        let begin = self.roll_rec(RollMark::Begin, new_no, old_no, reason, info.utc_ns);
        let flags = self.boot_flags();
        self.put_roll(&next, &begin, flags, info.calib.tsc.0);
        if self.cfg.kind == JournalKind::Raw {
            self.bound_off = self.off;
            self.bound_seq = self.next_seq - 1;
        }
        self.boot = BootState::Active;
        self.boot_records = 1;
        self.boot_utc = info.utc_ns;
        Ok(BootWriter { w: self, done: false })
    }

    /// Clean shutdown: `End{other_segment_no = 0, reason = Manual}`, publish, `Sealed`.
    pub fn seal_and_close(mut self) -> Result<(), RollErr> {
        if self.boot == BootState::Dropped {
            return Err(RollErr::InBoot);
        }
        let no = self.cur_no;
        if let Some(map) = self.close_current(0, RollReason::Manual, 0) {
            self.cur = None;
            self.retire(map, no)?;
        }
        Ok(())
    }
}

/// Writes the boot block of a freshly rolled segment; exists only between `roll` and `finish`.
#[derive(Debug)]
pub struct BootWriter<'a> {
    w: &'a mut JournalWriter,
    done: bool,
}

impl BootWriter<'_> {
    /// Appends a fixed body inside the boot block (may use the roll reserve).
    pub fn append<B: Body>(&mut self, tsc: Tsc, body: &B) -> Result<Seq, AppendErr> {
        let bytes = body.as_bytes();
        let kind = B::KIND;
        let crc = self.w.crc_for(kind);
        let len = hft_event::record_len(bytes.len(), crc) as usize;
        let Some(map) = self.w.cur else {
            return Err(AppendErr::Sealed);
        };
        if self.w.off + len > self.w.limit(true) {
            return Err(AppendErr::Full);
        }
        let seq = self.w.next_seq;
        let f = self.w.boot_flags() | if crc { RecordFlags::CRC.0 } else { 0 };
        let pieces: [&[u8]; 1] = [bytes];
        let mut cur = Cursor::new(&pieces);
        self.w.put(
            &map,
            kind.0,
            f,
            tsc.0,
            None,
            Src::Pieces(&mut cur),
            bytes.len(),
            crc,
        );
        self.w.boot_records += 1;
        Ok(Seq(seq))
    }

    /// Appends a variable-length record inside the boot block.
    pub fn append_var(&mut self, kind: EventKind, tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr> {
        self.w.put_var(kind, 0, tsc.0, pieces, true)
    }

    /// seq/chain: writes `BootDone{boot_records}` (with `EVENT_END`) and publishes; raw: writes
    /// nothing. Returns the seq of the boot block's last record.
    pub fn finish(mut self) -> Result<Seq, AppendErr> {
        self.done = true;
        let w = &mut *self.w;
        let Some(map) = w.cur else {
            return Err(AppendErr::Sealed);
        };
        if w.cfg.kind.has_boot() {
            if w.off + 72 > w.cap {
                w.boot = BootState::Dropped;
                return Err(AppendErr::Full);
            }
            let reason = RollReason::from_u8(map.fixed().roll_reason).unwrap_or(RollReason::Manual);
            let mut r = w.roll_rec(
                RollMark::BootDone,
                w.cur_no,
                w.cur_no.saturating_sub(1),
                reason,
                w.boot_utc,
            );
            r.boot_records = w.boot_records + 1;
            w.put_roll(&map, &r, RecordFlags::EVENT_END.0, 0);
            w.bound_off = w.off;
            w.bound_seq = w.next_seq - 1;
        }
        w.reserve = 2 * (w.off - HEADER_LEN);
        w.boot = BootState::None;
        w.publish();
        Ok(Seq(w.bound_seq))
    }
}

impl Drop for BootWriter<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.w.boot = BootState::Dropped;
        }
    }
}
