//! Startup journal reservation; the same journal list feeds storage preparation.
use crate::{Error, config::Config};

pub fn hot_resident(segment_bytes: u64, hot_keep: u8) -> Result<u64, Error> {
    segment_bytes
        .checked_mul(2 + u64::from(hot_keep))
        .ok_or_else(|| Error::runtime("MemBudget", "journal reservation overflow", 3))
}

pub fn journals(c: &Config, dry: bool) -> Vec<String> {
    let mut names = vec!["seq".into()];
    if !dry {
        for (name, venue) in &c.venues {
            names.push(format!("raw-feed-{name}"));
            if !venue.accounts.is_empty() {
                names.push(format!("raw-gw-{name}"));
            }
        }
    }
    names
}

pub fn check_space(available: u64, held_by_self: u64, required: u64) -> Result<(), Error> {
    if u128::from(available) + u128::from(held_by_self) < u128::from(required) {
        return Err(Error::runtime(
            "ShmSpace",
            format!("need={required} available={available} held={held_by_self}"),
            3,
        ));
    }
    Ok(())
}

pub fn required(c: &Config, dry: bool) -> Result<u64, Error> {
    journals(c, dry).into_iter().try_fold(4096u64, |total, name| {
        let layout = c.journal.layout(&name);
        total
            .checked_add(hot_resident(layout.segment_bytes, layout.hot_keep)?)
            .ok_or_else(|| Error::runtime("MemBudget", "journal total overflow", 3))
    })
}

#[cfg(target_os = "linux")]
pub fn preflight(c: &Config, dry: bool) -> Result<(), Error> {
    use std::os::unix::ffi::OsStrExt;
    let hot_mount = c.paths.journal_ram.join(crate::storage::instance(c).as_str());
    std::fs::create_dir_all(&hot_mount)
        .map_err(|_| Error::runtime("ShmSpace", "cannot create hot root", 3))?;
    let path = std::ffi::CString::new(hot_mount.as_os_str().as_bytes())
        .map_err(|_| Error::runtime("ShmSpace", "invalid hot root", 3))?;
    // SAFETY: statvfs writes its defined structure into a valid initialized object.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return Err(Error::runtime("ShmSpace", "cannot inspect hot filesystem", 3));
    }
    let required = required(c, dry)?;
    let held = hft_journal::held_by_instance(&[&c.paths.journal_ram], crate::storage::instance(c))
        .map_err(|_| Error::runtime("ShmSpace", "cannot inspect held segments", 3))?;
    let available = stat.f_bavail.saturating_mul(stat.f_frsize);
    check_space(available, held.shm_bytes, required)
}
