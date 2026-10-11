//! `held_by_instance`: space already held by this instance's own hot segments (README "段大小与热段内存" 5).

#![allow(clippy::disallowed_types)] // allow-list: startup space accounting

use std::io;
use std::path::Path;

use hft_rt::HeldBySelf;

use crate::config::InstanceName;
use crate::fs;
use crate::layout::{FsKind, SEGMENT_MAGIC};

/// Sums `<hot_dir>/<instance>/<journal>/<10 digits>.jrn` files that start with `HFTJRNL1`
/// (Preparing included), plus unpublished `.jrn.preparing` files whose header can be incomplete,
/// rounded up to the page of their mount, per page size.
pub fn held_by_instance(hot_dirs: &[&Path], instance: InstanceName) -> Result<HeldBySelf, io::Error> {
    let mut h = HeldBySelf::default();
    for root in hot_dirs {
        let inst = root.join(instance.as_str());
        let kind = fs::ram_fs(root)?;
        let page: u64 = match kind {
            FsKind::Hugetlbfs { page } => page as u64,
            _ => 4096,
        };
        let journals: Vec<std::path::PathBuf> = if fs::is_mem(&inst) {
            crate::layout::mem_list_all(&inst)
                .into_iter()
                .filter_map(|p| p.parent().map(Path::to_path_buf))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect()
        } else {
            match std::fs::read_dir(&inst) {
                Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
                Err(e) => return Err(e),
            }
        };
        for j in journals {
            let files = fs::hot_list(&j)
                .into_values()
                .map(|p| (p, false))
                .chain(fs::hot_staging_list(&j).into_values().map(|p| (p, true)));
            for (p, staging) in files {
                if !staging && fs::read_prefix(&p, 8)?.as_slice() != SEGMENT_MAGIC.as_slice() {
                    continue;
                }
                let len = fs::file_len(&p).unwrap_or(0);
                let pages = len.div_ceil(page);
                match kind {
                    FsKind::Hugetlbfs { page } if page >= 1 << 30 => h.hugepages_1g += pages as u32,
                    FsKind::Hugetlbfs { .. } => h.hugepages_2m += pages as u32,
                    _ => h.shm_bytes += pages * page,
                }
            }
        }
    }
    Ok(h)
}
