//! Read-only DST diagnostics. Neither identities nor retired rows enter snapshots.

use hft_strategy_sdk::HedgePlan;

use crate::{LinkEntry, LinkState, Oms};

/// A link immediately before its table entry can be reused.
#[derive(Copy, Clone, Debug)]
pub struct LinkAudit {
    pub link: LinkEntry,
    pub plan: HedgePlan,
    pub first_epoch: u16,
    pub created_seq: u64,
    pub capped: i128,
}

#[derive(Debug)]
pub(crate) struct Audit {
    pub keys: Vec<(u16, u64)>,
    pub capped: Vec<i128>,
    pub retired: Vec<LinkAudit>,
    pub overflow: bool,
    pub enabled: bool,
}

impl Audit {
    pub fn new(links: usize, capacity: usize) -> Self {
        Self {
            keys: vec![(0, 0); links],
            capped: vec![0; links],
            retired: Vec::with_capacity(capacity),
            overflow: false,
            enabled: false,
        }
    }
}

impl Oms {
    /// Starts one diagnostic event. Storage was allocated at construction.
    pub fn audit_begin(&mut self) {
        self.audit.enabled = true;
        self.audit.retired.clear();
        self.audit.overflow = false;
    }

    pub fn audit_retired(&self) -> &[LinkAudit] {
        &self.audit.retired
    }

    pub fn audit_overflow(&self) -> bool {
        self.audit.overflow
    }

    pub fn audit_links(&self) -> impl Iterator<Item = LinkAudit> + '_ {
        self.links
            .iter()
            .enumerate()
            .filter_map(|(i, _)| self.audit_link(i as u32 + 1))
    }

    pub fn audit_link(&self, id: u32) -> Option<LinkAudit> {
        let i = id.checked_sub(1)? as usize;
        let l = *self.links.get(i)?;
        if l.state == LinkState::Free {
            return None;
        }
        let (epoch, created_seq) = *self.audit.keys.get(i)?;
        let epoch = if epoch != 0 {
            epoch
        } else {
            self.slot(l.first)
                .map(|s| s.epoch)
                .or_else(|| {
                    self.tomb(l.first.venue, l.first.account, l.first.slot, l.first.r#gen)
                        .map(|t| t.epoch)
                })
                .unwrap_or(0)
        };
        Some(LinkAudit {
            link: l,
            plan: self.plans.get(i)?.1,
            first_epoch: epoch,
            created_seq,
            capped: *self.audit.capped.get(i)?,
        })
    }

    pub(crate) fn audit_reuse(&mut self, i: usize, epoch: u16, seq: u64) {
        if self.audit.enabled
            && let Some(row) = self.audit_link(i as u32 + 1)
        {
            if self.audit.retired.len() < self.audit.retired.capacity() {
                self.audit.retired.push(row);
            } else {
                self.audit.overflow = true;
            }
        }
        if let Some(k) = self.audit.keys.get_mut(i) {
            *k = (epoch, seq);
        }
        if let Some(c) = self.audit.capped.get_mut(i) {
            *c = 0;
        }
    }

    pub(crate) fn audit_cap(&mut self, i: usize, delta: i128) {
        if let Some(c) = self.audit.capped.get_mut(i) {
            *c += delta;
        }
    }
}
