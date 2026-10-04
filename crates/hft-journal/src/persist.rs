//! `Persister`: copies hot segments to cold copies, `fdatasync`s by size/time thresholds and seals
//! (`.seal` with the masked SHA-256) — README "persist", journal-format "冷副本与热段".

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zerocopy::IntoBytes;

use crate::config::{JournalConfig, JournalKind, JournalLag, PersistConfig, PersistProgress, SegState};
use crate::fs;
use crate::layout::{
    OFF_COMMITTED, OFF_LAST_SEQ, OFF_PERSISTED, OFF_STATE, OFF_SYNCED, SEAL_MAGIC, SealRecord, SegMap,
    SegmentHeaderFixed,
};
use crate::oplog::{self, Op};
use crate::scan::{end_reason, hash_masked, read_header};

/// The cold copy file of a hot segment (created with `fallocate(segment_bytes)` on first use).
/// Returns the file, its path stem and whether it already existed.
pub(crate) fn open_cold(
    cfg_cold_root_inst: &Path,
    cold_jdir: &Path,
    h: &SegmentHeaderFixed,
) -> io::Result<(File, PathBuf, bool)> {
    let stem = fs::cold_stem(cold_jdir, h.created_utc_ns, h.segment_no, h.first_seq);
    let path = fs::with_ext(&stem, "jrn");
    if let Some(day) = stem.parent() {
        fs::mkdir_0770(cfg_cold_root_inst, day)?;
    }
    let existed = path.exists();
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    if !existed && let Err(e) = crate::layout::fallocate(&f, h.segment_bytes) {
        drop(f);
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok((f, stem, existed))
}

/// Copies hot `[from, to)` to the cold file at the same offsets (no sync).
pub(crate) fn copy_range(map: &SegMap, cold: &File, from: usize, to: usize) -> io::Result<()> {
    fs::pwrite(cold, from as u64, map.bytes(from, to - from))
}

/// After `fdatasync`: publish `synced_len` in the hot persist row and the cold header.
pub(crate) fn mark_synced(map: &SegMap, cold: &File, synced: u64) -> io::Result<()> {
    map.store64(OFF_SYNCED, synced);
    fs::pwrite(cold, OFF_SYNCED as u64, &synced.to_le_bytes())
}

/// Truncates, syncs and writes `.seal` for a fully synced sealed segment.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_seal(
    cold: &File,
    stem: &Path,
    h: &SegmentHeaderFixed,
    last_seq: u64,
    len: u64,
    sha: [u8; 32],
    reason: u8,
    utc_ns: i64,
) -> io::Result<()> {
    cold.set_len(len)?;
    cold.sync_all()?;
    if let Some(d) = stem.parent() {
        fs::sync_dir(d)?;
    }
    let mut s = SealRecord {
        magic: SEAL_MAGIC,
        segment_no: h.segment_no,
        first_seq: h.first_seq,
        last_seq,
        len,
        sealed_utc_ns: utc_ns,
        sha256: sha,
        reason,
        flags: h.flags,
        _pad: [0; 2],
        _rsv: [0; 40],
        crc32c: 0,
    };
    s.crc32c = s.compute_crc();
    let p = fs::with_ext(stem, "seal");
    let f = File::create(&p)?;
    fs::pwrite(&f, 0, s.as_bytes())?;
    f.sync_all()?;
    if let Some(d) = stem.parent() {
        fs::sync_dir(d)?;
    }
    Ok(())
}

#[derive(Debug)]
struct PSeg {
    map: SegMap,
    hdr: SegmentHeaderFixed,
    cold: Option<(File, PathBuf)>,
    sha: Sha256,
    hashed: usize,
    unsynced: u64,
}

#[derive(Debug)]
struct PJournal {
    cfg: JournalConfig,
    hot_jdir: PathBuf,
    cold_jdir: PathBuf,
    cold_inst: PathBuf,
    segs: BTreeMap<u64, PSeg>,
    done: BTreeSet<u64>,
    last_sync: Option<u64>,
    lag_since: Option<u64>,
}

/// Copies and syncs the hot segments of several journals (persist thread).
#[derive(Debug)]
pub struct Persister {
    cfg: PersistConfig,
    js: Vec<PJournal>,
    mono: u64,
}

impl Persister {
    pub fn new(cfg: PersistConfig) -> Persister {
        Persister {
            cfg,
            js: Vec::new(),
            mono: 0,
        }
    }

    /// Adds a journal to persist.
    pub fn add(&mut self, cfg: &JournalConfig) {
        self.js.push(PJournal {
            cfg: cfg.clone(),
            hot_jdir: cfg.hot_journal_dir(),
            cold_jdir: cfg.cold_journal_dir(),
            cold_inst: cfg.cold_root.join(cfg.instance.as_str()),
            segs: BTreeMap::new(),
            done: BTreeSet::new(),
            last_sync: None,
            lag_since: None,
        });
    }

    /// One round: per journal at most `chunk_bytes` copied, `fdatasync` on thresholds, sealed
    /// segments get `.seal` (`sealed_utc_ns = utc_ns`).
    pub fn poll(&mut self, mono_ns: u64, utc_ns: i64) -> PersistProgress {
        self.mono = mono_ns;
        let mut pr = PersistProgress::default();
        for j in &mut self.js {
            j.poll(&self.cfg, mono_ns, utc_ns, &mut pr);
        }
        pr
    }

    /// Unsynced bytes per journal and their estimated age (watchtower `journal_persist_lag_*`).
    pub fn lag(&self) -> Vec<JournalLag> {
        self.js
            .iter()
            .map(|j| {
                let bytes: u64 = j
                    .segs
                    .values()
                    .map(|s| {
                        s.map
                            .load64(OFF_COMMITTED)
                            .saturating_sub(s.map.load64(OFF_SYNCED))
                    })
                    .sum();
                let ms = match (bytes, j.lag_since) {
                    (0, _) | (_, None) => 0,
                    (_, Some(t)) => self.mono.saturating_sub(t) / 1_000_000,
                };
                JournalLag {
                    name: j.cfg.name.clone(),
                    bytes,
                    ms,
                }
            })
            .collect()
    }
}

impl PJournal {
    fn refresh(&mut self) {
        let files = fs::hot_list(&self.hot_jdir);
        // Forget segments whose file is gone (retired).
        let gone: Vec<u64> = self
            .segs
            .keys()
            .filter(|n| !files.contains_key(n))
            .copied()
            .collect();
        for n in gone {
            if let Some(s) = self.segs.remove(&n) {
                fs::release(None, s.map);
            }
        }
        self.done.retain(|n| files.contains_key(n));
        for (no, p) in files {
            if self.segs.contains_key(&no) || self.done.contains(&no) {
                continue;
            }
            let Some((h, st)) = read_header(&p) else { continue };
            if st == SegState::Preparing as u32 || h.segment_no != no {
                continue;
            }
            let stem = fs::cold_stem(&self.cold_jdir, h.created_utc_ns, h.segment_no, h.first_seq);
            if st == SegState::Sealed as u32 && fs::with_ext(&stem, "seal").exists() {
                self.done.insert(no);
                continue;
            }
            let Ok(o) = fs::hot_open(&p, true) else { continue };
            self.segs.insert(
                no,
                PSeg {
                    map: o.map,
                    hdr: h,
                    cold: None,
                    sha: Sha256::new(),
                    hashed: 0,
                    unsynced: 0,
                },
            );
        }
    }

    fn poll(&mut self, cfg: &PersistConfig, mono: u64, utc: i64, pr: &mut PersistProgress) {
        self.refresh();
        let mut budget = cfg.chunk_bytes as u64;
        let nos: Vec<u64> = self.segs.keys().copied().collect();
        for no in nos {
            let Some(s) = self.segs.get_mut(&no) else { continue };
            if step(
                s,
                &self.cold_inst,
                &self.cold_jdir,
                cfg,
                mono,
                &mut self.last_sync,
                &mut budget,
                pr,
            )
            .is_err()
            {
                // Reopen (or recreate) the cold copy on the next round.
                s.cold = None;
                pr.errors += 1;
                break;
            }
            let committed = s.map.load64(OFF_COMMITTED);
            let ready = s.map.load32(OFF_STATE) == SegState::Sealed as u32
                && s.map.load64(OFF_PERSISTED) == committed
                && s.map.load64(OFF_SYNCED) == committed;
            if !ready {
                continue;
            }
            match seal(s, self.cfg.kind, utc) {
                Ok(()) => {
                    if let Some(s) = self.segs.remove(&no) {
                        fs::release(None, s.map);
                    }
                    self.done.insert(no);
                    pr.sealed += 1;
                }
                Err(_) => {
                    pr.errors += 1;
                    break;
                }
            }
        }
        let lag: u64 = self
            .segs
            .values()
            .map(|s| {
                s.map
                    .load64(OFF_COMMITTED)
                    .saturating_sub(s.map.load64(OFF_SYNCED))
            })
            .sum();
        if lag == 0 {
            self.lag_since = None;
        } else if self.lag_since.is_none() {
            self.lag_since = Some(mono);
        }
    }
}

/// Copies (within the budget) and syncs one segment.
#[allow(clippy::too_many_arguments)]
fn step(
    s: &mut PSeg,
    cold_inst: &Path,
    cold_jdir: &Path,
    cfg: &PersistConfig,
    mono: u64,
    last_sync: &mut Option<u64>,
    budget: &mut u64,
    pr: &mut PersistProgress,
) -> io::Result<()> {
    let committed = s.map.load64(OFF_COMMITTED);
    let mut persisted = s.map.load64(OFF_PERSISTED);
    if persisted < committed && *budget > 0 {
        if s.cold.is_none() {
            let (f, stem, existed) = open_cold(cold_inst, cold_jdir, &s.hdr)?;
            if !existed && persisted > 0 {
                // The cold copy vanished: start over from byte 0.
                s.map.store64(OFF_SYNCED, 0);
                s.map.store64(OFF_PERSISTED, 0);
                s.hashed = 0;
                s.sha = Sha256::new();
                persisted = 0;
            }
            s.cold = Some((f, stem));
        }
        let n = (committed - persisted).min(*budget);
        let (from, to) = (persisted as usize, (persisted + n) as usize);
        if let Some((f, _)) = &s.cold {
            copy_range(&s.map, f, from, to)?;
        }
        if s.hashed == from {
            hash_masked(&mut s.sha, from, s.map.bytes(from, to - from));
            s.hashed = to;
        }
        s.map.store64(OFF_PERSISTED, to as u64);
        oplog::record(Op::Copy(s.hdr.segment_no, to as u64));
        s.unsynced += n;
        *budget -= n;
        pr.bytes_copied += n;
        if last_sync.is_none() {
            *last_sync = Some(mono);
        }
    }
    let persisted = s.map.load64(OFF_PERSISTED);
    let synced = s.map.load64(OFF_SYNCED);
    if persisted > synced {
        let sealed_done = s.map.load32(OFF_STATE) == SegState::Sealed as u32 && persisted == committed;
        let due = s.unsynced >= u64::from(cfg.sync_bytes)
            || last_sync
                .is_some_and(|t| mono.saturating_sub(t) >= u64::from(cfg.sync_interval_ms) * 1_000_000)
            || sealed_done;
        if due && let Some((f, _)) = &s.cold {
            fs::datasync(f)?;
            mark_synced(&s.map, f, persisted)?;
            *last_sync = Some(mono);
            s.unsynced = 0;
            pr.syncs += 1;
        }
    }
    Ok(())
}

fn seal(s: &mut PSeg, kind: JournalKind, utc: i64) -> io::Result<()> {
    let committed = s.map.load64(OFF_COMMITTED) as usize;
    if s.cold.is_none() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    if s.hashed != committed {
        s.sha = Sha256::new();
        hash_masked(&mut s.sha, 0, s.map.bytes(0, committed));
        s.hashed = committed;
    }
    let sha: [u8; 32] = std::mem::take(&mut s.sha).finalize().into();
    let bytes = s.map.bytes(0, committed);
    let reason = end_reason(bytes, kind).unwrap_or(s.hdr.roll_reason);
    let last_seq = s.map.load64(OFF_LAST_SEQ);
    let Some((f, stem)) = &s.cold else {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    };
    write_seal(f, stem, &s.hdr, last_seq, committed as u64, sha, reason, utc)
}
