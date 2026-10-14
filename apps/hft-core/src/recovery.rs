//! Cold startup ownership and durable epoch reservation. No order session may precede this.
use crate::Error;
use hft_journal::{DurableMeta, InstanceName};
use std::{fs::File, path::Path};

/// The lock file stays in place: unlinking a locked inode would permit two owners.
/// The kernel releases the lock on exit, including an unclean process death.
#[derive(Debug)]
pub struct InstanceLease(File);

impl InstanceLease {
    pub fn acquire(cold_root: &Path, instance: InstanceName) -> Result<Self, Error> {
        let dir = cold_root.join(instance.as_str());
        std::fs::create_dir_all(&dir)
            .map_err(|_| Error::runtime("InstanceBusy", "cannot create instance directory", 3))?;
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("core.lock"))
            .map_err(|_| Error::runtime("InstanceBusy", "cannot open instance lock", 3))?;
        file.try_lock()
            .map_err(|_| Error::runtime("InstanceBusy", "instance lock is held or unavailable", 3))?;
        Ok(Self(file))
    }

    /// Reserve while holding exclusive instance ownership; failure never returns a usable epoch.
    pub fn reserve_epoch(&self, meta: &DurableMeta, snapshot: Option<u32>) -> Result<u32, Error> {
        let _keep_open = &self.0;
        let durable = meta
            .read("engine_epoch")
            .map_err(|_| Error::runtime("Epoch", "cannot read durable epoch", 4))?;
        let epoch = next_epoch(snapshot, durable)?;
        meta.write_sync("engine_epoch", u64::from(epoch))
            .map_err(|_| Error::runtime("Epoch", "cannot sync durable epoch", 4))?;
        Ok(epoch)
    }
}

pub fn next_epoch(snapshot: Option<u32>, durable: Option<u64>) -> Result<u32, Error> {
    u64::from(snapshot.unwrap_or(0))
        .max(durable.unwrap_or(0))
        .checked_add(1)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| Error::runtime("Epoch", "epoch exhausted", 4))
}
