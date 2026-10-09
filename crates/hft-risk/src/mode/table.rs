//! `ModeTable`: desired modes per cell and per (strategy, account) `default_mode`; the
//! effective-mode cache (README "有效模式").

use hft_event::CmdScope;
use hft_types::{AccountId, GROUP_ALL, RunMode, StrategyId};

use super::{CancelScope, CellKey, Effective, INITIAL_DEFAULT_MODE, Mode, ModeCancel, ModeOutcome, ModeView};

/// One cell: desired mode, Shadow landing state and the cached effective mode.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct CellRow {
    pub key: CellKey,
    pub desired: RunMode,
    /// Desired Shadow already effective (the cell was flat when it was set or became flat).
    pub landed: bool,
    pub eff: Effective,
    /// Cap-source mask of the cell-level caps (Broken, Reorg, Halted) covering the cell.
    pub mask: u32,
}

/// `default_mode` of a (strategy, account).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) struct DefaultRow {
    pub strategy: StrategyId,
    pub account: AccountId,
    pub mode: RunMode,
}

pub(crate) fn scope_covers_cell(s: &CmdScope, c: CellKey) -> bool {
    (s.strategy.0 == 0 || s.strategy == c.strategy)
        && (s.account.0 == 0 || s.account == c.account)
        && (s.group == GROUP_ALL || s.group == c.group)
}

impl Mode {
    #[inline]
    pub(crate) fn cell_ix(&self, k: CellKey) -> Option<usize> {
        self.cells.as_slice().binary_search_by(|c| c.key.cmp(&k)).ok()
    }

    /// `SetMode`: writes the desired mode of every matching cell and, for `group = *`, the
    /// matching `default_mode`s. Returns whether the table changed.
    pub(crate) fn set_mode(&mut self, scope: &CmdScope, mode: RunMode, v: &impl ModeView) -> bool {
        let mut changed = false;
        let mut to_close = false;
        for c in self.cells.as_mut_slice().iter_mut() {
            if !scope_covers_cell(scope, c.key) || c.desired == mode {
                continue;
            }
            c.desired = mode;
            c.landed = mode == RunMode::Shadow && v.cell_flat(c.key);
            changed = true;
            to_close |= mode != RunMode::Live;
        }
        if scope.group == GROUP_ALL {
            for a in 1..=self.f.n_accounts {
                let acc = AccountId(a as u8);
                let s = self.f.bound.get(a).copied().unwrap_or(StrategyId(0));
                if s.0 == 0
                    || (scope.account.0 != 0 && scope.account != acc)
                    || (scope.strategy.0 != 0 && scope.strategy != s)
                {
                    continue;
                }
                match self
                    .defaults
                    .as_mut_slice()
                    .iter_mut()
                    .find(|d| d.strategy == s && d.account == acc)
                {
                    Some(d) => {
                        if d.mode != mode {
                            d.mode = mode;
                            changed = true;
                        }
                    }
                    None => {
                        let row = DefaultRow {
                            strategy: s,
                            account: acc,
                            mode,
                        };
                        let at = self
                            .defaults
                            .iter()
                            .position(|d| (d.strategy, d.account) > (s, acc))
                            .unwrap_or(self.defaults.len());
                        self.defaults.insert(at, row);
                        changed = true;
                    }
                }
            }
        }
        if to_close {
            self.cancel_candidate(
                ModeCancel {
                    scope: CancelScope::Cell(CellKey {
                        strategy: scope.strategy,
                        account: scope.account,
                        group: scope.group,
                    }),
                    open_only: true,
                },
                true,
            );
        }
        changed
    }

    /// `CellFlat`: lands a deferred Shadow (no `ctl_state_version` change).
    pub(crate) fn cell_flat(&mut self, cell: CellKey, flat: bool) {
        if !flat {
            return;
        }
        if let Some(i) = self.cell_ix(cell)
            && let Some(c) = self.cells.get_mut(i)
            && c.desired == RunMode::Shadow
            && !c.landed
        {
            c.landed = true;
        }
    }

    /// `GroupsChanged`: adds cells with the (strategy, account) `default_mode`, removes cells,
    /// then re-checks the leverage part of `AccountCfg` for the touched accounts.
    pub(crate) fn groups_changed(
        &mut self,
        added: &[CellKey],
        removed: &[CellKey],
        seq: u64,
        v: &impl ModeView,
        out: &mut ModeOutcome,
    ) -> bool {
        let mut changed = false;
        for k in removed {
            if let Some(i) = self.cell_ix(*k) {
                self.cells.remove(i);
                changed = true;
            }
        }
        for k in added {
            if self.cell_ix(*k).is_some() {
                continue;
            }
            let desired = self
                .default_mode(k.strategy, k.account)
                .unwrap_or(INITIAL_DEFAULT_MODE);
            let mut row = CellRow {
                key: *k,
                desired,
                landed: desired == RunMode::Shadow && v.cell_flat(*k),
                eff: Effective::Live,
                mask: 0,
            };
            row.eff = self.effective_of(&row);
            let at = self
                .cells
                .as_slice()
                .binary_search_by(|c| c.key.cmp(k))
                .unwrap_or_else(|e| e);
            if self.cells.insert(at, row) {
                changed = true;
            }
        }
        // Re-align the "before" scratch with the new row set (new cells report no change).
        self.scratch_eff.clear();
        for c in self.cells.iter() {
            let e = c.eff;
            self.scratch_eff.push(e);
        }
        let mut touched = [false; 256];
        for k in added.iter().chain(removed.iter()) {
            if let Some(t) = touched.get_mut(usize::from(k.account.0)) {
                *t = true;
            }
        }
        for (a, t) in touched.iter().enumerate() {
            if *t && a > 0 && a <= self.f.n_accounts {
                changed |= self.recheck_account_cfg(AccountId(a as u8), seq, v, out);
            }
        }
        changed
    }

    /// Effective mode of a row under the current masks (README "有效模式").
    pub(crate) fn effective_of(&self, c: &CellRow) -> Effective {
        match c.desired {
            RunMode::Shadow => {
                if c.landed {
                    Effective::Shadow
                } else {
                    Effective::CloseOnly {
                        deferred_shadow: true,
                    }
                }
            }
            d => {
                let m = c.mask | self.global_mask | self.acct_mask_of(c.key.account);
                if m != 0 || d == RunMode::CloseOnly {
                    Effective::CloseOnly {
                        deferred_shadow: false,
                    }
                } else {
                    Effective::Live
                }
            }
        }
    }

    /// Recomputes cell masks and effective modes of every cell.
    pub(crate) fn recompute_effective(&mut self) {
        let n = self.cells.len();
        for i in 0..n {
            let Some(key) = self.cells.get(i).map(|c| c.key) else {
                continue;
            };
            let mut mask = 0u32;
            for r in self.caps.iter() {
                if super::caps::is_cell_level(r) && scope_covers_cell(&r.scope, key) && r.source.0 < 32 {
                    mask |= 1u32 << r.source.0;
                }
            }
            if let Some(c) = self.cells.get_mut(i) {
                c.mask = mask;
            }
            let eff = match self.cells.get(i) {
                Some(c) => self.effective_of(c),
                None => continue,
            };
            if let Some(c) = self.cells.get_mut(i) {
                c.eff = eff;
            }
        }
    }
}
