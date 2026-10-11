//! Read-only DST diagnostics. Neither identities nor retired rows enter snapshots.

use hft_strategy_sdk::{HedgePlan, LinkRole};
use hft_types::InstId;

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

/// Raw facts before hedge accounting changes residuals or requested quantities.
#[derive(Copy, Clone, Debug)]
pub struct HedgeFacts {
    pub row: LinkAudit,
    pub first_inst: InstId,
    pub first_before: i128,
    pub first_after: i128,
    pub residual_before: i128,
}

#[derive(Debug)]
pub(crate) struct Audit {
    pub keys: Vec<(u16, u64)>,
    pub capped: Vec<i128>,
    pub retired: Vec<LinkAudit>,
    pub overflow: bool,
    pub enabled: bool,
    seeds: Vec<Option<HedgeFacts>>,
    requests: Vec<(u32, u64, LinkRole, HedgeFacts, bool)>,
}

impl Audit {
    pub fn new(links: usize, capacity: usize) -> Self {
        Self {
            keys: vec![(0, 0); links],
            capped: vec![0; links],
            retired: Vec::with_capacity(capacity),
            overflow: false,
            enabled: false,
            seeds: vec![None; links],
            requests: Vec::with_capacity(capacity),
        }
    }
}

impl Oms {
    /// Starts one diagnostic event. Storage was allocated at construction.
    pub fn audit_begin(&mut self) {
        self.audit.enabled = true;
        self.audit.retired.clear();
        self.audit.requests.clear();
        self.audit.seeds.fill(None);
        self.audit.overflow = false;
    }

    pub fn audit_retired(&self) -> &[LinkAudit] {
        &self.audit.retired
    }

    pub fn audit_overflow(&self) -> bool {
        self.audit.overflow
    }

    /// Consumes a diagnostic copy only; never changes an order, link or request.
    pub fn audit_take_hedge(&mut self, r: &crate::HedgeRequest) -> Option<HedgeFacts> {
        let row = self.audit.requests.iter_mut().find(|(id, seq, role, _, used)| {
            !*used && *id == r.link_id && *seq == r.trigger_seq && *role == r.role
        })?;
        row.4 = true;
        Some(row.3)
    }

    pub(crate) fn audit_seed(&mut self, i: usize, first_before: i128, first_after: i128, residual: i128) {
        let Some(row) = self.audit_link(i as u32 + 1) else {
            return;
        };
        let first_inst = self.first_inst(i);
        if let Some(s) = self.audit.seeds.get_mut(i) {
            *s = Some(HedgeFacts {
                row,
                first_inst,
                first_before,
                first_after,
                residual_before: residual,
            });
        }
    }

    pub(crate) fn audit_request(&mut self, r: &crate::HedgeRequest) {
        if !self.audit.enabled {
            return;
        }
        let Some(i) = r.link_id.checked_sub(1).map(|x| x as usize) else {
            return;
        };
        let facts = self.audit.seeds.get_mut(i).and_then(Option::take).or_else(|| {
            let row = self.audit_link(r.link_id)?;
            Some(HedgeFacts {
                row,
                first_inst: self.first_inst(i),
                first_before: i128::from(row.link.first_net.0),
                first_after: i128::from(row.link.first_net.0),
                residual_before: 0,
            })
        });
        if let Some(f) = facts {
            if self.audit.requests.len() == self.audit.requests.capacity() {
                self.audit.overflow = true;
            } else {
                self.audit
                    .requests
                    .push((r.link_id, r.trigger_seq, r.role, f, false));
            }
        }
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
