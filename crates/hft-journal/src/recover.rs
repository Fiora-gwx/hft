//! `JournalWriter::open` (recovery steps 1–6) and `open_offline` (README "恢复").

use std::cell::Cell;
use std::fs::File;

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use hft_event::{RecordFlags, SegmentRoll, encode_fixed};
use sha2::Sha256;
use zerocopy::FromBytes;

use crate::config::{JournalConfig, JournalKind, OpenErr, Position, RollMark, RollReason, SegState};
use crate::fs::{self, HotOpen};
use crate::layout::{
    FsKind, HEADER_LEN, OFF_COMMITTED, OFF_LAST_SEQ, OFF_PERSISTED, OFF_STATE, OFF_SYNCED, RECOVERY_GAP,
    SHARED_ROWS, SegMap, SegmentHeaderFixed,
};
use crate::persist::{copy_range, mark_synced, open_cold, write_seal};
use crate::scan;
use crate::scan::header_sha;
use crate::scan::{end_reason, hash_masked, rebuild_point};
use crate::writer::{BootState, JournalWriter, RetiredSegment, WriterEnv};

/// What `open` found and repaired.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct RecoveryReport {
    pub last_seq: hft_types::Seq,
    pub repaired_segments: u32,
    pub bytes_copied: u64,
    pub bytes_discarded: u64,
    /// Power loss: the cold copy's tail may be lost.
    pub gap_possible: bool,
    /// Latest complete boot block or `Keyframe` snapshot (record start).
    pub rebuild_from: Position,
    /// Reason for the first `roll`.
    pub boot_reason: RollReason,
}

struct HotSeg {
    no: u64,
    path: PathBuf,
    h: HotOpen,
    fixed: SegmentHeaderFixed,
}

fn release_all(v: Vec<HotSeg>) {
    for s in v {
        fs::release(Some(&s.path), s.h.map);
    }
}

impl JournalWriter {
    pub(crate) fn blank(cfg: &JournalConfig, epoch: u32, git: [u8; 20], host: [u8; 32]) -> JournalWriter {
        JournalWriter {
            cfg: cfg.clone(),
            cold_jdir: cfg.cold_journal_dir(),
            hot_jdir: cfg.hot_journal_dir(),
            epoch,
            build_git: git,
            host_sha: host,
            prepared: None,
            retired: None,
            offline: None,
            cur: None,
            cur_no: 0,
            cap: 0,
            off: 0,
            next_seq: 1,
            bound_off: 0,
            bound_seq: 0,
            published_off: 0,
            in_event: false,
            last: None,
            boot: BootState::None,
            boot_records: 0,
            boot_utc: 0,
            reserve: 0,
            deadline: None,
            next_flags: 0,
            skip_on_roll: 0,
            scratch: Vec::new(),
            _not_sync: PhantomData::<Cell<()>>,
        }
    }

    /// Startup: checks the hot directory, recovers (README "恢复" steps 1–6) and returns a writer
    /// without a writable segment; the first call must be `roll(report.boot_reason, …)`.
    pub fn open(cfg: &JournalConfig, env: WriterEnv) -> Result<(JournalWriter, RecoveryReport), OpenErr> {
        if !cfg.validate() {
            return Err(OpenErr::BadConfig);
        }
        match fs::ram_fs(&cfg.hot_dir) {
            Err(_) => return Err(OpenErr::Io),
            Ok(FsKind::Other) => return Err(OpenErr::DiskBacked),
            Ok(_) => {}
        }
        let mut w = JournalWriter::blank(cfg, env.writer_epoch, env.build_git, env.host_profile_sha256);
        w.prepared = Some(env.prepared);
        w.retired = Some(env.retired);
        fs::ensure_hot_dir(&w.hot_jdir).map_err(|_| OpenErr::Io)?;
        fs::mkdir_0770(&cfg.cold_root, &w.cold_jdir).map_err(|_| OpenErr::Io)?;
        let rep = recover(&mut w)?;
        Ok((w, rep))
    }

    /// Replayer `--emit-journal`: a writer on an ordinary directory (no preparation, persist or
    /// retirement); segments are written with `pwrite` and synced when sealed.
    pub fn open_offline(cfg: &JournalConfig, out_dir: &Path) -> Result<JournalWriter, OpenErr> {
        if !cfg.validate() {
            return Err(OpenErr::BadConfig);
        }
        if fs::is_mem(out_dir) {
            return Err(OpenErr::OfflineTarget);
        }
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let roots = [
            cfg.hot_dir.clone(),
            cfg.cold_root.clone(),
            PathBuf::from("/dev/hugepages"),
            PathBuf::from("/dev/hugepages2M"),
            PathBuf::from("/dev/shm/hft"),
        ];
        if roots
            .iter()
            .any(|r| canon(out_dir).starts_with(canon(r)) || out_dir.starts_with(r))
        {
            return Err(OpenErr::OfflineTarget);
        }
        std::fs::create_dir_all(out_dir).map_err(|_| OpenErr::Io)?;
        if !matches!(crate::layout::fs_kind(out_dir), Ok(FsKind::Other)) {
            return Err(OpenErr::OfflineTarget);
        }
        let out = canon(out_dir);
        if roots.iter().any(|r| out.starts_with(canon(r))) {
            return Err(OpenErr::OfflineTarget);
        }
        let jdir = out.join(cfg.instance.as_str()).join(cfg.name.as_str());
        std::fs::create_dir_all(&jdir).map_err(|_| OpenErr::Io)?;
        let mut w = JournalWriter::blank(cfg, 0, [0; 20], [0; 32]);
        w.offline = Some(jdir);
        Ok(w)
    }
}

/// Compares hot `[0, n)` with the cold copy, ignoring `3072..3200`.
fn same_as_cold(map: &SegMap, cold: &Path, n: usize) -> bool {
    let Ok(f) = File::open(cold) else { return false };
    if f.metadata().map(|m| (m.len() as usize) < n).unwrap_or(true) {
        return false;
    }
    let mut off = 0;
    let mut buf = vec![0u8; 1 << 20];
    while off < n {
        let k = (n - off).min(buf.len());
        let Some(b) = buf.get_mut(..k) else { return false };
        if fs::pread(&f, off as u64, b).is_err() {
            return false;
        }
        let hot = map.bytes(off, k);
        for (i, (x, y)) in hot.iter().zip(b.iter()).enumerate() {
            if x != y && !SHARED_ROWS.contains(&(off + i)) {
                return false;
            }
        }
        off += k;
    }
    true
}

fn delete_cold(cold_jdir: &Path, h: &SegmentHeaderFixed) {
    let stem = fs::cold_stem(cold_jdir, h.created_utc_ns, h.segment_no, h.first_seq);
    for e in ["jrn", "seal", "uploaded"] {
        let _ = std::fs::remove_file(fs::with_ext(&stem, e));
    }
}

/// `(other_segment_no, reason)` of the final `End` record.
fn last_end(bytes: &[u8], kind: JournalKind) -> Option<(u64, u8)> {
    let reason = end_reason(bytes, kind)?;
    let rl = if kind == JournalKind::Raw { 80 } else { 72 };
    let rec = bytes.get(bytes.len() - rl..)?;
    let other = u64::from_le_bytes(rec.get(24 + 8..24 + 16)?.try_into().ok()?);
    Some((other, reason))
}

fn recover(w: &mut JournalWriter) -> Result<RecoveryReport, OpenErr> {
    let kind = w.cfg.kind;
    let mut rep = RecoveryReport {
        last_seq: hft_types::Seq(0),
        repaired_segments: 0,
        bytes_copied: 0,
        bytes_discarded: 0,
        gap_possible: false,
        rebuild_from: Position::default(),
        boot_reason: RollReason::Startup,
    };
    // Step 1: enumerate hot segments, drop Preparing ones, check headers.
    let mut segs: Vec<HotSeg> = Vec::new();
    for (no, path) in fs::hot_list(&w.hot_jdir) {
        let Ok(h) = fs::hot_open(&path, true) else {
            release_all(segs);
            return Err(OpenErr::Io);
        };
        let fixed = h.map.fixed();
        if fixed.magic != crate::layout::SEGMENT_MAGIC || h.map.len() < HEADER_LEN {
            fs::release(Some(&path), h.map);
            continue;
        }
        if h.map.load32(OFF_STATE) == SegState::Preparing as u32 {
            fs::release(Some(&path), h.map);
            let _ = fs::hot_unlink(&path);
            continue;
        }
        if fixed.header_crc32c != fixed.compute_crc() || fixed.segment_no != no {
            fs::release(Some(&path), h.map);
            release_all(segs);
            return Err(OpenErr::ChainBroken);
        }
        segs.push(HotSeg { no, path, h, fixed });
    }
    // Segments whose boot block was never committed are deleted (hot, cold, seal).
    let mut dropped_boot = false;
    while kind.has_boot()
        && let Some(last) = segs.last()
        && last.h.map.load64(OFF_COMMITTED) <= HEADER_LEN as u64
    {
        let Some(s) = segs.pop() else { break };
        delete_cold(&w.cold_jdir, &s.fixed);
        fs::release(Some(&s.path), s.h.map);
        let _ = fs::hot_unlink(&s.path);
        rep.repaired_segments += 1;
        dropped_boot = true;
    }
    // Chain checks.
    let cold = fs::cold_list(&w.cold_jdir);
    for pair in segs.windows(2) {
        let [a, b] = pair else { continue };
        if b.no != a.no + 1 {
            if (a.no + 1..b.no).any(|n| !cold.contains_key(&n)) {
                release_all(segs);
                return Err(OpenErr::ChainBroken);
            }
            continue;
        }
        let gap = b.fixed.flags & RECOVERY_GAP != 0;
        let sha_ok =
            b.fixed.prev_header_sha256 == header_sha(a.h.map.bytes(0, crate::layout::HEADER_FIXED_LEN));
        let seq_ok = b.fixed.first_seq == a.h.map.load64(OFF_LAST_SEQ) + 1
            && b.fixed.prev_last_seq + 1 == b.fixed.first_seq;
        if !gap && !(sha_ok && seq_ok) {
            release_all(segs);
            return Err(OpenErr::ChainBroken);
        }
    }
    // Cold/hot conflicts.
    for s in &segs {
        let synced = s.h.map.load64(OFF_SYNCED) as usize;
        if synced == 0 {
            continue;
        }
        let stem = fs::cold_stem(&w.cold_jdir, s.fixed.created_utc_ns, s.no, s.fixed.first_seq);
        if !same_as_cold(&s.h.map, &fs::with_ext(&stem, "jrn"), synced) {
            release_all(segs);
            return Err(OpenErr::ColdHotConflict);
        }
    }
    if segs.is_empty() {
        return recover_cold(w, rep, dropped_boot);
    }
    // Step 2: truncate the last Open segment to its last complete event.
    let cold_inst = w.cfg.cold_root.join(w.cfg.instance.as_str());
    let mut last_open = false;
    if let Some(last) = segs.last() {
        let m = last.h.map;
        if m.load32(OFF_STATE) == SegState::Open as u32 {
            last_open = true;
            let committed = m.load64(OFF_COMMITTED) as usize;
            let all = m.bytes(0, m.len());
            let t = scan::scan_tail(all, committed, m.load64(OFF_LAST_SEQ), kind);
            let mut ranges = Vec::new();
            scan::zero_to_clean_page(all, t.end, |a, n| ranges.push((a, n)));
            for (a, n) in ranges {
                m.zero(a, n);
            }
            rep.bytes_discarded = (t.valid_end - t.end) as u64;
            m.store64(OFF_LAST_SEQ, t.end_seq);
            m.store64(OFF_COMMITTED, t.end as u64);
        }
    }
    // Step 3: copy and sync whatever the cold copies lack, before any new write.
    for s in &segs {
        let m = s.h.map;
        let committed = m.load64(OFF_COMMITTED);
        if m.load64(OFF_SYNCED) >= committed {
            continue;
        }
        let (f, _, existed) = open_cold(&cold_inst, &w.cold_jdir, &s.fixed).map_err(|_| OpenErr::Io)?;
        let from = if existed {
            m.load64(OFF_PERSISTED).min(committed)
        } else {
            0
        };
        copy_range(&m, &f, from as usize, committed as usize).map_err(|_| OpenErr::Io)?;
        m.store64(OFF_PERSISTED, committed);
        crate::fs::datasync(&f).map_err(|_| OpenErr::Io)?;
        mark_synced(&m, &f, committed).map_err(|_| OpenErr::Io)?;
        rep.bytes_copied += committed - from;
    }
    // Step 4: End{Recovery} on the last Open segment.
    let Some(last) = segs.last() else {
        return Err(OpenErr::Io);
    };
    let (last_no, last_map, last_first) = (last.no, last.h.map, last.fixed.first_seq);
    if last_open {
        w.cur = Some(last_map);
        w.cur_no = last_no;
        w.cap = last_map.len();
        w.off = last_map.load64(OFF_COMMITTED) as usize;
        w.next_seq = last_map.load64(OFF_LAST_SEQ) + 1;
        w.close_current(last_no + 1, RollReason::Recovery, 0);
        w.cur = None;
        rep.repaired_segments += 1;
        rep.boot_reason = RollReason::Recovery;
    } else {
        let committed = last_map.load64(OFF_COMMITTED) as usize;
        let clean = matches!(
            last_end(last_map.bytes(0, committed), kind),
            Some((0, r)) if r == RollReason::Manual as u8
        );
        rep.boot_reason = if clean && !dropped_boot {
            RollReason::Startup
        } else {
            RollReason::Recovery
        };
    }
    let last_seq = last_map.load64(OFF_LAST_SEQ);
    rep.last_seq = hft_types::Seq(last_seq);
    // Step 6.
    let committed = last_map.load64(OFF_COMMITTED) as usize;
    rep.rebuild_from =
        rebuild_point(last_map.bytes(0, committed), last_no, last_first, kind).unwrap_or_default();
    w.cur_no = last_no;
    w.next_seq = last_seq + 1;
    // The retirer adopts every recovered hot segment.
    for s in segs {
        let r = RetiredSegment {
            map: s.h.map,
            segment_no: s.no,
        };
        let pushed = w.retired.as_mut().is_some_and(|p| p.try_push(r).is_ok());
        if !pushed {
            fs::release(Some(&s.path), s.h.map);
        }
    }
    Ok(rep)
}

/// Step 5: the hot directory holds nothing usable; continue from the last cold copy.
fn recover_cold(
    w: &mut JournalWriter,
    mut rep: RecoveryReport,
    dropped_boot: bool,
) -> Result<RecoveryReport, OpenErr> {
    let kind = w.cfg.kind;
    let cold = fs::cold_list(&w.cold_jdir);
    let Some((no, c)) = cold.into_iter().next_back() else {
        rep.boot_reason = if dropped_boot {
            RollReason::Recovery
        } else {
            RollReason::Startup
        };
        return Ok(rep);
    };
    let jrn = c.jrn();
    let bytes = std::fs::read(&jrn).map_err(|_| OpenErr::Io)?;
    let fixed = bytes
        .get(..crate::layout::HEADER_FIXED_LEN)
        .and_then(|b| SegmentHeaderFixed::read_from_bytes(b).ok())
        .ok_or(OpenErr::Io)?;
    if c.seal().exists() {
        let seal = std::fs::read(c.seal()).map_err(|_| OpenErr::Io)?;
        let s = crate::layout::SealRecord::read_from_bytes(&seal).map_err(|_| OpenErr::Io)?;
        let clean = matches!(last_end(&bytes, kind), Some((0, r)) if r == RollReason::Manual as u8);
        rep.boot_reason = if clean && !dropped_boot {
            RollReason::Startup
        } else {
            RollReason::Recovery
        };
        rep.last_seq = hft_types::Seq(s.last_seq);
        rep.rebuild_from = rebuild_point(&bytes, no, fixed.first_seq, kind).unwrap_or_default();
        w.cur_no = no;
        w.next_seq = s.last_seq + 1;
        return Ok(rep);
    }
    // Unsealed: scan from the start, truncate after the last complete event, append End{Recovery}.
    let t = scan::scan_tail(&bytes, HEADER_LEN, fixed.first_seq - 1, kind);
    rep.bytes_discarded = (t.valid_end - t.end) as u64;
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&jrn)
        .map_err(|_| OpenErr::Io)?;
    f.set_len(t.end as u64).map_err(|_| OpenErr::Io)?;
    let end = SegmentRoll {
        segment_no: no,
        other_segment_no: no + 1,
        seq_mark: t.end_seq,
        utc_ns: 0,
        mark: RollMark::End as u8,
        reason: RollReason::Recovery as u8,
        journal_kind: kind as u8,
        _pad: 0,
        boot_records: 0,
        _rsv: 0,
    };
    let crc = kind == JournalKind::Raw;
    let mut rec = [0u8; 80];
    let mut flags = RecordFlags::EVENT_END;
    if crc {
        flags |= RecordFlags::CRC;
    }
    let n = encode_fixed(&mut rec, t.end_seq + 1, 0, flags, &end) as usize;
    let new_len = t.end + n;
    let last_seq = t.end_seq + 1;
    fs::pwrite(&f, t.end as u64, rec.get(..n).unwrap_or(&[])).map_err(|_| OpenErr::Io)?;
    let mut rows = [0u8; 24];
    rows[0..8].copy_from_slice(&(new_len as u64).to_le_bytes());
    rows[8..16].copy_from_slice(&last_seq.to_le_bytes());
    rows[16..20].copy_from_slice(&(SegState::Sealed as u32).to_le_bytes());
    fs::pwrite(&f, OFF_COMMITTED as u64, &rows).map_err(|_| OpenErr::Io)?;
    let mut prow = [0u8; 16];
    prow[0..8].copy_from_slice(&(new_len as u64).to_le_bytes());
    prow[8..16].copy_from_slice(&(new_len as u64).to_le_bytes());
    fs::pwrite(&f, OFF_PERSISTED as u64, &prow).map_err(|_| OpenErr::Io)?;
    let mut content = vec![0u8; new_len];
    fs::pread(&f, 0, &mut content).map_err(|_| OpenErr::Io)?;
    let mut h = Sha256::new();
    hash_masked(&mut h, 0, &content);
    use sha2::Digest;
    let sha: [u8; 32] = h.finalize().into();
    write_seal(
        &f,
        &c.stem,
        &fixed,
        last_seq,
        new_len as u64,
        sha,
        RollReason::Recovery as u8,
        0,
    )
    .map_err(|_| OpenErr::Io)?;
    rep.repaired_segments += 1;
    rep.gap_possible = true;
    rep.boot_reason = RollReason::Recovery;
    rep.last_seq = hft_types::Seq(last_seq);
    rep.rebuild_from = rebuild_point(&content, no, fixed.first_seq, kind).unwrap_or_default();
    w.next_flags = RECOVERY_GAP;
    w.cur_no = no;
    w.next_seq = last_seq + 1;
    Ok(rep)
}
