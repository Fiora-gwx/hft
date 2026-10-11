//! Bounded DST copies of force-target inputs and results; excluded from persistent state.

use hft_types::{AccountId, GroupId, InstId, Qty, Rate};

#[derive(Copy, Clone, Debug)]
pub struct ForceFacts {
    pub account: AccountId,
    pub group: GroupId,
    pub inst: InstId,
    pub position: Qty,
    pub previous_target: Option<Qty>,
    pub previous_engine: bool,
    pub fraction: Rate,
    pub target: Qty,
}

/// A timing trigger, without reading the deadline calculated by the execution layer.
#[derive(Copy, Clone, Debug)]
pub struct ArmFacts {
    pub account: AccountId,
    pub group: GroupId,
    pub engine_ns: i64,
    pub window_ms: u32,
}

#[derive(Copy, Clone, Debug)]
pub struct BrokenPoll {
    pub account: AccountId,
    pub group: GroupId,
    pub phase: crate::AutoFixPhase,
    pub engine_target: bool,
    /// Read-only gate diagnostic, using the documented `decision::wait` code table.
    pub wait: u64,
}

#[derive(Debug)]
pub(crate) struct Audit {
    force: Vec<ForceFacts>,
    arms: Vec<ArmFacts>,
    polls: Vec<BrokenPoll>,
    overflow: bool,
    enabled: bool,
}

impl Audit {
    pub fn fail(&mut self) {
        self.overflow = true;
    }
    pub fn new(cells: usize) -> Self {
        Self {
            force: Vec::with_capacity(cells.saturating_mul(8).max(8)),
            arms: Vec::with_capacity(cells.saturating_mul(8).max(8)),
            polls: Vec::with_capacity(cells.saturating_mul(8).max(8)),
            overflow: false,
            enabled: false,
        }
    }
    pub fn push(&mut self, f: ForceFacts) {
        if !self.enabled {
            return;
        }
        if self.force.len() == self.force.capacity() {
            self.overflow = true;
        } else {
            self.force.push(f);
        }
    }
    pub fn arm(&mut self, c: &crate::TargetCell, now: hft_strategy_sdk::Now, window_ms: u32) {
        if !self.enabled {
            return;
        }
        if self.arms.len() == self.arms.capacity() {
            self.overflow = true;
        } else {
            self.arms.push(ArmFacts {
                account: c.account,
                group: c.group,
                engine_ns: now.engine_ns,
                window_ms,
            });
        }
    }
    pub fn poll(&mut self, p: BrokenPoll) {
        if !self.enabled {
            return;
        }
        if self.polls.len() == self.polls.capacity() {
            self.overflow = true;
        } else {
            self.polls.push(p);
        }
    }
}

impl crate::Exec {
    pub fn audit_begin(&mut self) {
        self.audit.force.clear();
        self.audit.arms.clear();
        self.audit.polls.clear();
        self.audit.overflow = false;
        self.audit.enabled = true;
    }
    pub fn audit_force(&self) -> &[ForceFacts] {
        &self.audit.force
    }
    pub fn audit_arms(&self) -> &[ArmFacts] {
        &self.audit.arms
    }
    pub fn audit_broken_polls(&self) -> &[BrokenPoll] {
        &self.audit.polls
    }
    pub fn audit_overflow(&self) -> bool {
        self.audit.overflow
    }
}
