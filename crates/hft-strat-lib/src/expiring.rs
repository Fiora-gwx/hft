//! `Expiring<T>` (README "快速部分可用的构件"): a value with a validity deadline, e.g. a cached
//! Advice decode. Across runs it expires by `basis_seq < boot_seq`; within a run its validity is
//! counted from the basis time (engine_ns), not from the arrival time.

use hft_strategy_sdk::{Now, RestoreError, SnapshotReader, SnapshotWriter};
use hft_types::Tsc;

use crate::units::{deadline, ms_to_ns};

/// Freshness of an [`Expiring`] value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Fresh<'a, T> {
    Fresh(&'a T),
    Stale(&'a T),
    Missing,
}

/// A value with an engine_ns deadline and the journal seq it is based on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Expiring<T: Copy> {
    v: T,
    valid_until_ens: i64,
    basis_seq: u64,
    set: bool,
}

impl<T: Copy> Expiring<T> {
    /// Not set (`get` → `Missing`); `init` only fills the slot.
    #[inline]
    pub const fn new(init: T) -> Self {
        Expiring {
            v: init,
            valid_until_ens: 0,
            basis_seq: 0,
            set: false,
        }
    }

    /// Caches an Advice (SDK Advice payload expiry rules):
    /// `valid_until_ens = now.ens_of(basis_tsc) + valid_ms` (basis time, not arrival time).
    #[inline]
    pub fn put_advice(&mut self, v: T, basis_seq: u64, basis_tsc: Tsc, valid_ms: u32, now: &Now) {
        self.put(v, deadline(now.ens_of(basis_tsc), ms_to_ns(valid_ms)), basis_seq);
    }

    /// Other sources: the engine_ns deadline directly (`basis_seq` = `now.seq` for a value valid
    /// within this run).
    #[inline]
    pub fn put(&mut self, v: T, valid_until_ens: i64, basis_seq: u64) {
        self.v = v;
        self.valid_until_ens = valid_until_ens;
        self.basis_seq = basis_seq;
        self.set = true;
    }

    /// Never put → `Missing`; basis from an earlier run (`basis_seq < now.boot_seq`) or
    /// `now.engine_ns > valid_until_ens` → `Stale`; otherwise `Fresh`.
    #[inline]
    pub fn get(&self, now: &Now) -> Fresh<'_, T> {
        if !self.set {
            Fresh::Missing
        } else if self.basis_seq < now.boot_seq || now.engine_ns > self.valid_until_ens {
            Fresh::Stale(&self.v)
        } else {
            Fresh::Fresh(&self.v)
        }
    }

    /// 17 B header (`set` u8, `valid_until_ens` i64, `basis_seq` u64), then `put` writes the value
    /// when set.
    pub fn snapshot(&self, w: &mut SnapshotWriter, put: fn(&T, &mut SnapshotWriter)) {
        w.put_u8(u8::from(self.set));
        w.put_i64(self.valid_until_ens);
        w.put_u64(self.basis_seq);
        if self.set {
            put(&self.v, w);
        }
    }

    /// Reads the header and, when set, the value with `get`; on any error `self` is unchanged.
    pub fn restore(
        &mut self,
        r: &mut SnapshotReader<'_>,
        get: fn(&mut SnapshotReader<'_>) -> Result<T, RestoreError>,
    ) -> Result<(), RestoreError> {
        let set = match r.get_u8()? {
            0 => false,
            1 => true,
            _ => return Err(RestoreError::BadValue),
        };
        let valid_until_ens = r.get_i64()?;
        let basis_seq = r.get_u64()?;
        let v = if set { get(r)? } else { self.v };
        *self = Expiring {
            v,
            valid_until_ens,
            basis_seq,
            set,
        };
        Ok(())
    }
}
