//! Bounded, one-to-one comparison before the advisor's outbox coalescing.

use std::collections::BTreeMap;

use hft_event::AdviceHdr;
use zerocopy::{FromZeros, IntoBytes, TryFromBytes};

use crate::advisor::AdviceItem;
use crate::journal::MemRecord;

use super::recompute::{AdviceDifference, RecomputeError, RecomputeLimits};

type Key = (u8, u8, u32, u16, u64);

fn key(h: &AdviceHdr) -> Key {
    (h.strategy.0, h.account.0, h.group.0, h.local_kind, h.basis_seq)
}

struct Pending {
    bytes: Vec<u8>,
    countable: bool,
}

pub(super) struct Compare {
    pending: BTreeMap<Key, Pending>,
    bytes: usize,
    detail_bytes: usize,
    limits: RecomputeLimits,
    pub compared: u64,
    pub coalesced: u64,
    pub differences: Vec<AdviceDifference>,
}

impl Compare {
    pub fn new(limits: RecomputeLimits) -> Self {
        Self {
            pending: BTreeMap::new(),
            bytes: 0,
            detail_bytes: 0,
            limits,
            compared: 0,
            coalesced: 0,
            differences: Vec::new(),
        }
    }

    pub fn produced(&mut self, item: AdviceItem, countable: bool) -> Result<(), RecomputeError> {
        let mut h = AdviceHdr::new_zeroed();
        h.strategy = item.strategy;
        h.account = item.account;
        h.group = item.group;
        h.local_kind = item.local_kind;
        h.basis_seq = item.basis_seq;
        h.basis_tsc = item.basis_tsc;
        h.len = u16::try_from(item.payload.len()).map_err(|_| RecomputeError::DiagnosticLimit)?;
        let mut bytes = h.as_bytes().to_vec();
        bytes.extend_from_slice(&item.payload);
        let k = key(&h);
        // All calls for the same basis event precede a flush. Matching an older payload
        // would accept an Advice which could not survive the actual outbox.
        if let Some(old) = self.pending.remove(&k) {
            self.bytes -= old.bytes.len();
            self.coalesced += u64::from(old.countable);
        }
        let size = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(RecomputeError::DiagnosticLimit)?;
        if self.pending.len() >= self.limits.records
            || size
                .checked_add(self.detail_bytes)
                .is_none_or(|n| n > self.limits.bytes)
        {
            return Err(RecomputeError::DiagnosticLimit);
        }
        self.bytes = size;
        self.pending.insert(k, Pending { bytes, countable });
        Ok(())
    }

    pub fn logged(&mut self, r: &MemRecord, compare: bool) -> Result<(), RecomputeError> {
        let (h, payload) =
            AdviceHdr::try_read_from_prefix(&r.body).map_err(|_| RecomputeError::Advice { seq: r.seq })?;
        if usize::from(h.len) != payload.len() {
            return Err(RecomputeError::Advice { seq: r.seq });
        }
        let actual = self.pending.remove(&key(&h)).map(|p| {
            self.bytes -= p.bytes.len();
            p.bytes
        });
        if !compare {
            return Ok(());
        }
        self.compared += 1;
        if actual.as_deref() == Some(r.body.as_slice()) {
            return Ok(());
        }
        if self.differences.len() >= self.limits.records {
            return Err(RecomputeError::DiagnosticLimit);
        }
        let detail = r
            .body
            .len()
            .checked_add(actual.as_ref().map_or(0, Vec::len))
            .and_then(|n| n.checked_add(self.detail_bytes))
            .ok_or(RecomputeError::DiagnosticLimit)?;
        if detail
            .checked_add(self.bytes)
            .is_none_or(|n| n > self.limits.bytes)
        {
            return Err(RecomputeError::DiagnosticLimit);
        }
        self.detail_bytes = detail;
        self.differences.push(AdviceDifference {
            seq: r.seq,
            strategy: h.strategy,
            account: h.account,
            group: h.group,
            local_kind: h.local_kind,
            basis_seq: h.basis_seq,
            logged: r.body.clone(),
            regenerated: actual,
        });
        Ok(())
    }

    pub fn finish(&mut self) {
        self.coalesced += self.pending.values().filter(|p| p.countable).count() as u64;
        self.pending.clear();
        self.bytes = 0;
    }
}
