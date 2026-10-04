//! `HotRetirer` (hot segment retirement: `retired = 1`, grace, `munmap` + `unlink`), its hugetlbfs
//! audit, and `cold_retire` (local retention of cold copies).

use std::io;
use std::path::Path;

use zerocopy::FromBytes;

use crate::config::{ColdRetention, RetireReport};
use crate::fs;
use crate::layout::{SEAL_MAGIC, SealRecord};

#[cfg(feature = "writer")]
pub use hot::HotRetirer;

#[cfg(feature = "writer")]
mod hot {
    #[cfg(target_os = "linux")]
    use std::path::Path;
    use std::path::PathBuf;

    use crate::config::{HugeAudit, JournalConfig, RETIRE_GRACE_MS, SegState};
    use crate::fs;
    use crate::layout::{OFF_COMMITTED, OFF_RETIRED, OFF_STATE, OFF_SYNCED};
    use crate::oplog::{self, Op};
    use crate::writer::RetiredSegment;

    #[derive(Debug)]
    struct Pending {
        seg: RetiredSegment,
        path: PathBuf,
        retired_at: Option<i64>,
    }

    /// Retires old hot segments of one journal on the persist thread.
    #[derive(Debug)]
    pub struct HotRetirer {
        from: hft_rt::Consumer<RetiredSegment>,
        hot_jdir: PathBuf,
        cold_jdir: PathBuf,
        hot_keep: u64,
        pend: Vec<Pending>,
        /// Current active segment number (newest retired + 1).
        active: u64,
    }

    impl HotRetirer {
        pub fn new(cfg: &JournalConfig, from_writer: hft_rt::Consumer<RetiredSegment>) -> HotRetirer {
            HotRetirer {
                from: from_writer,
                hot_jdir: cfg.hot_journal_dir(),
                cold_jdir: cfg.cold_journal_dir(),
                hot_keep: u64::from(cfg.hot_keep),
                pend: Vec::new(),
                active: 0,
            }
        }

        /// Sets `retired` on segments that are sealed, synced, have `.seal` and are `hot_keep` behind
        /// the active one; after `RETIRE_GRACE_MS` unmaps and unlinks them. Returns how many were removed.
        pub fn poll(&mut self, now_utc_ns: i64) -> usize {
            while let Some(seg) = self.from.try_pop() {
                self.active = self.active.max(seg.segment_no + 1);
                let path = self.hot_jdir.join(fs::hot_seg_name(seg.segment_no));
                self.pend.push(Pending {
                    seg,
                    path,
                    retired_at: None,
                });
            }
            let grace = (RETIRE_GRACE_MS * 1_000_000) as i64;
            let mut removed = 0;
            let mut i = 0;
            while i < self.pend.len() {
                let Some(p) = self.pend.get_mut(i) else { break };
                match p.retired_at {
                    None => {
                        if self.retirable(&self.pend[i]) {
                            let p = &mut self.pend[i];
                            p.seg.map.store32(OFF_RETIRED, 1);
                            p.retired_at = Some(now_utc_ns);
                            oplog::record(Op::Retire(p.seg.segment_no));
                        }
                        i += 1;
                    }
                    Some(t) if now_utc_ns >= t.saturating_add(grace) => {
                        let p = self.pend.swap_remove(i);
                        fs::release(Some(&p.path), p.seg.map);
                        let _ = fs::hot_unlink(&p.path);
                        removed += 1;
                    }
                    Some(_) => i += 1,
                }
            }
            self.pend.sort_by_key(|p| p.seg.segment_no);
            removed
        }

        fn retirable(&self, p: &Pending) -> bool {
            let m = &p.seg.map;
            let n = p.seg.segment_no;
            if m.load32(OFF_STATE) != SegState::Sealed as u32 {
                return false;
            }
            let committed = m.load64(OFF_COMMITTED);
            if m.load64(OFF_SYNCED) != committed {
                return false;
            }
            if n + self.hot_keep >= self.active {
                return false;
            }
            let h = m.fixed();
            let stem = fs::cold_stem(&self.cold_jdir, h.created_utc_ns, h.segment_no, h.first_seq);
            fs::with_ext(&stem, "seal").exists()
        }

        /// hugetlbfs pages still in use whose files are gone (unlinked while mapped), host-wide.
        pub fn audit(&self) -> HugeAudit {
            audit_impl()
        }
    }

    #[cfg(target_os = "linux")]
    fn audit_impl() -> HugeAudit {
        let read = |p: &str| -> u64 {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0)
        };
        let used = |kb: &str| -> u64 {
            let d = format!("/sys/kernel/mm/hugepages/hugepages-{kb}kB/");
            (read(&(d.clone() + "nr_hugepages")) + read(&(d.clone() + "resv_hugepages")))
                .saturating_sub(read(&(d + "free_hugepages")))
        };
        // Pages held by files on hugetlbfs mounts, per page size.
        let mut files_2m = 0u64;
        let mut files_1g = 0u64;
        if let Ok(m) = std::fs::read_to_string("/proc/mounts") {
            for line in m.lines() {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.get(2) != Some(&"hugetlbfs") {
                    continue;
                }
                let Some(dir) = f.get(1) else { continue };
                let page: u64 = if f
                    .get(3)
                    .is_some_and(|o| o.contains("pagesize=1G") || o.contains("pagesize=1024M"))
                {
                    1 << 30
                } else {
                    2 << 20
                };
                let n = file_pages(Path::new(dir), page);
                if page == 1 << 30 {
                    files_1g += n;
                } else {
                    files_2m += n;
                }
            }
        }
        HugeAudit {
            pages_2m: used("2048").saturating_sub(files_2m),
            pages_1g: used("1048576").saturating_sub(files_1g),
        }
    }

    #[cfg(target_os = "linux")]
    fn file_pages(dir: &Path, page: u64) -> u64 {
        let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
        let mut n = 0;
        for e in rd.flatten() {
            let p = e.path();
            match std::fs::symlink_metadata(&p) {
                Ok(m) if m.is_dir() => n += file_pages(&p, page),
                Ok(m) if m.is_file() => n += m.len().div_ceil(page),
                _ => {}
            }
        }
        n
    }

    #[cfg(not(target_os = "linux"))]
    fn audit_impl() -> HugeAudit {
        HugeAudit::default()
    }
}

/// Deletes sealed cold copies older than `keep_days` (and uploaded when required) below `root`
/// (`<cold_root>/<instance>/<journal>`); empty day directories go too. Unsealed copies stay.
pub fn cold_retire(root: &Path, r: &ColdRetention, now_utc_ns: i64) -> Result<RetireReport, io::Error> {
    let mut rep = RetireReport::default();
    let keep = i128::from(r.keep_days) * 86_400 * 1_000_000_000;
    for (_, c) in fs::cold_list(root) {
        let seal = c.seal();
        let Ok(b) = std::fs::read(&seal) else { continue };
        let Ok(s) = SealRecord::read_from_bytes(&b) else {
            continue;
        };
        if s.magic != SEAL_MAGIC || s.crc32c != s.compute_crc() {
            continue;
        }
        if i128::from(now_utc_ns) - i128::from(s.sealed_utc_ns) <= keep {
            continue;
        }
        let up = fs::with_ext(&c.stem, "uploaded");
        if r.require_uploaded && !up.exists() {
            continue;
        }
        let jrn = c.jrn();
        rep.bytes += std::fs::metadata(&jrn).map(|m| m.len()).unwrap_or(0);
        for p in [&jrn, &seal, &up] {
            match std::fs::remove_file(p) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        rep.segments += 1;
        if let Some(day) = c.stem.parent()
            && std::fs::read_dir(day).is_ok_and(|mut d| d.next().is_none())
        {
            std::fs::remove_dir(day)?;
        }
    }
    Ok(rep)
}
