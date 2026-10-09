//! The cap set: records, (source, scope) order, setting with per-source merge, removal,
//! notes and risk events, derived masks and the kill-word projection (README "cap 记录",
//! "kill 字", "Kill").

use hft_event::{CapRecord, CmdScope, NoteLevel};
use hft_strategy_sdk::{CapScopeView, RiskEvent};
use hft_types::{AccountId, CapSource, GROUP_ALL, InstId, KillBits, KillLevel, StrategyId, VenueId};

use super::{CancelScope, CellKey, KillCmd, Mode, ModeCancel, ModeOutcome, StorageKind, kill_params};
use crate::types::{N_CAP_CLEARED, N_CAP_SET, cap_note};

/// Who set a cap (decides the `CapSet` Note level).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum Origin {
    Command,
    Auto,
}

/// `CmdScope` with every field "any" (strategy/venue/account/inst 0, group `GROUP_ALL`).
pub(crate) fn scope_of(_: &[()]) -> CmdScope {
    CmdScope {
        strategy: StrategyId(0),
        venue: VenueId(0),
        account: AccountId(0),
        _pad: 0,
        group: GROUP_ALL,
        inst: InstId(0),
        _pad2: 0,
    }
}

pub(crate) fn record(
    source: CapSource,
    level: u8,
    cflags: u8,
    scope: CmdScope,
    params: [u64; 2],
) -> CapRecord {
    CapRecord {
        source,
        level,
        cflags,
        _pad: [0; 5],
        scope,
        set_seq: 0,
        params,
    }
}

type Key = (u8, u8, u8, u8, u32, u32);

fn key_of(source: CapSource, s: &CmdScope) -> Key {
    (
        source.0,
        s.strategy.0,
        s.venue.0,
        s.account.0,
        s.group.0,
        s.inst.0,
    )
}

/// Venue-level caps (`Kill{Venue}`, `Recon`, `UnknownTimeout`, `AccountCfg`, `Delisting`) act
/// in check #3 by venue/inst and never enter the cell computation.
pub(crate) fn is_venue_level(r: &CapRecord) -> bool {
    match r.source {
        CapSource::KILL => r.scope.venue.0 != 0,
        CapSource::RECON | CapSource::UNKNOWN_TIMEOUT | CapSource::ACCOUNT_CFG | CapSource::DELISTING => true,
        _ => false,
    }
}

/// Caps scoped below the account (a group or a strategy): Broken, Reorg, Halted.
pub(crate) fn is_cell_level(r: &CapRecord) -> bool {
    !is_venue_level(r) && (r.scope.group != GROUP_ALL || r.scope.strategy.0 != 0)
}

fn bit(s: CapSource) -> u32 {
    if s.0 < 32 { 1u32 << s.0 } else { 0 }
}

fn view(s: &CmdScope) -> CapScopeView {
    CapScopeView {
        strategy: s.strategy,
        venue: s.venue,
        account: s.account,
        group: s.group,
        inst: s.inst,
    }
}

/// Merge of `new` into `old` (same source and scope): the more severe of the two.
fn merge(old: &CapRecord, new: &CapRecord) -> CapRecord {
    let mut m = *old;
    m.cflags |= new.cflags;
    match old.source {
        CapSource::KILL | CapSource::ACCOUNT_CFG | CapSource::DRAINING => {
            m.params[0] |= new.params[0];
        }
        CapSource::DAILY_LOSS => {
            let (o, n) = (old.params[0] as i64, new.params[0] as i64);
            m.params[0] = o.max(n) as u64;
        }
        CapSource::MARGIN_STRESS | CapSource::STORAGE => {
            // Level and venue bitmap / current levels follow the latest input.
            m.level = new.level;
            m.params = new.params;
        }
        CapSource::DELISTING => {
            let (o, n) = (old.params[0] as i64, new.params[0] as i64);
            m.params[0] = if o == 0 {
                n as u64
            } else if n == 0 {
                o as u64
            } else {
                o.min(n) as u64
            };
        }
        _ => {
            m.level = m.level.max(new.level);
        }
    }
    m
}

impl Mode {
    pub(crate) fn find_cap(&self, source: CapSource, scope: &CmdScope) -> Option<usize> {
        let k = key_of(source, scope);
        self.caps
            .as_slice()
            .binary_search_by(|r| key_of(r.source, &r.scope).cmp(&k))
            .ok()
    }

    pub(crate) fn has_source(&self, s: CapSource) -> bool {
        self.caps.iter().any(|r| r.source == s)
    }

    pub(crate) fn storage_level2(&self) -> bool {
        self.caps
            .iter()
            .any(|r| r.source == CapSource::STORAGE && (r.params[0] >= 2 || r.params[1] >= 2))
    }

    /// Pushes one risk event per affected account (ascending).
    fn cap_events(&self, r: &CapRecord, set: bool, out: &mut ModeOutcome) {
        let ev = if set {
            RiskEvent::CapSet {
                source: r.source,
                scope: view(&r.scope),
            }
        } else {
            RiskEvent::CapCleared {
                source: r.source,
                scope: view(&r.scope),
            }
        };
        let s = &r.scope;
        for a in 1..=self.f.n_accounts {
            let acc = AccountId(a as u8);
            let hit = if s.account.0 != 0 {
                s.account == acc
            } else if s.venue.0 != 0 {
                self.f.venues(acc) & (1u64 << (s.venue.0 & 63)) != 0
            } else if s.strategy.0 != 0 {
                self.f.bound.get(a).copied() == Some(s.strategy)
            } else {
                true
            };
            if hit {
                out.risk_events.push((acc, ev));
            }
        }
    }

    /// Sets or merges one record; `false` when nothing changed (no output at all).
    pub(crate) fn set_cap(&mut self, r: CapRecord, seq: u64, origin: Origin, out: &mut ModeOutcome) -> bool {
        let level = match (origin, r.source) {
            (Origin::Command, _) | (_, CapSource::RECOVERY | CapSource::DRAINING) => NoteLevel::Info,
            _ => NoteLevel::P1,
        };
        let rec = match self.find_cap(r.source, &r.scope) {
            Some(i) => {
                let Some(old) = self.caps.get(i).copied() else {
                    return false;
                };
                let m = merge(&old, &r);
                if m == old {
                    return false;
                }
                if let Some(x) = self.caps.get_mut(i) {
                    *x = m;
                }
                m
            }
            None => {
                let mut m = r;
                m.set_seq = seq;
                let k = key_of(m.source, &m.scope);
                let at = self
                    .caps
                    .as_slice()
                    .binary_search_by(|x| key_of(x.source, &x.scope).cmp(&k))
                    .unwrap_or_else(|e| e);
                if !self.caps.insert(at, m) {
                    return false;
                }
                m
            }
        };
        out.notes.push(cap_note(N_CAP_SET, level, &rec));
        self.cap_events(&rec, true, out);
        self.cancel_for(&rec);
        self.caps_dirty = true;
        true
    }

    /// Removes exactly (source, scope); `false` when absent.
    pub(crate) fn remove_cap(&mut self, source: CapSource, scope: &CmdScope, out: &mut ModeOutcome) -> bool {
        let Some(i) = self.find_cap(source, scope) else {
            return false;
        };
        let Some(r) = self.caps.remove(i) else {
            return false;
        };
        if r.source == CapSource::KILL && r.cflags & CapRecord::HARD != 0 {
            out.resolve_broken = true;
        }
        if r.source == CapSource::STORAGE {
            self.storage_drain_sent = false;
        }
        out.notes.push(cap_note(N_CAP_CLEARED, NoteLevel::Info, &r));
        self.cap_events(&r, false, out);
        self.caps_dirty = true;
        true
    }

    /// Cancel candidate of a newly set or changed record.
    fn cancel_for(&mut self, r: &CapRecord) {
        let s = &r.scope;
        match r.source {
            CapSource::KILL => {
                if r.params[0] & kill_params::CANCEL_RESTING == 0 {
                    return;
                }
                let open_only = r.cflags & CapRecord::HARD == 0;
                let scope = if s.venue.0 != 0 {
                    if s.account.0 == 0 {
                        CancelScope::Venue(s.venue)
                    } else {
                        CancelScope::VenueAccount(s.venue, s.account)
                    }
                } else if s.account.0 != 0 {
                    CancelScope::Account(s.account)
                } else {
                    CancelScope::Global
                };
                self.cancel_candidate(ModeCancel { scope, open_only }, false);
            }
            CapSource::DRAINING => {
                if r.params[0] & 1 != 0 {
                    self.cancel_candidate(
                        ModeCancel {
                            scope: CancelScope::Global,
                            open_only: true,
                        },
                        false,
                    );
                }
            }
            CapSource::DELISTING => {
                let scope = if s.inst.0 == 0 {
                    CancelScope::Venue(s.venue)
                } else {
                    CancelScope::Inst(s.venue, s.inst)
                };
                self.cancel_candidate(
                    ModeCancel {
                        scope,
                        open_only: true,
                    },
                    false,
                );
            }
            CapSource::RECON | CapSource::UNKNOWN_TIMEOUT | CapSource::ACCOUNT_CFG => {}
            _ => {
                let scope = if s.account.0 == 0 && s.strategy.0 == 0 && s.group == GROUP_ALL {
                    CancelScope::Global
                } else if s.strategy.0 == 0 && s.group == GROUP_ALL {
                    CancelScope::Account(s.account)
                } else {
                    CancelScope::Cell(CellKey {
                        strategy: s.strategy,
                        account: s.account,
                        group: s.group,
                    })
                };
                self.cancel_candidate(
                    ModeCancel {
                        scope,
                        open_only: true,
                    },
                    true,
                );
            }
        }
    }

    /// Queues a cancel; `conditional` ones survive only when a covered cell turned CloseOnly.
    pub(crate) fn cancel_candidate(&mut self, c: ModeCancel, conditional: bool) {
        self.pending_cancels.push((c, conditional));
    }

    // ------------------------------------------------------------------------- inputs

    pub(crate) fn kill(&mut self, k: &KillCmd, seq: u64, out: &mut ModeOutcome) -> bool {
        let mut s = scope_of(&[]);
        match k.level {
            KillLevel::Global => {}
            KillLevel::Account => s.account = k.scope.account,
            KillLevel::Venue => {
                s.venue = k.scope.venue;
                s.account = k.scope.account;
            }
        }
        let p = u64::from(k.cancel_resting) * kill_params::CANCEL_RESTING
            + u64::from(k.flatten) * kill_params::FLATTEN;
        let cf = if k.hard { CapRecord::HARD } else { 0 };
        let r = record(CapSource::KILL, k.level as u8, cf, s, [p, 0]);
        self.set_cap(r, seq, Origin::Command, out)
    }

    pub(crate) fn auto(&mut self, r: CapRecord, seq: u64, out: &mut ModeOutcome) -> bool {
        if r.source == CapSource::BROKEN {
            let existed = self.find_cap(r.source, &r.scope).is_some();
            if !existed {
                out.risk_events
                    .push((r.scope.account, RiskEvent::HedgeBroken { group: r.scope.group }));
            }
            let mut changed = self.set_cap(r, seq, Origin::Auto, out);
            let mut hf = scope_of(&[]);
            hf.account = r.scope.account;
            changed |= self.set_cap(
                record(CapSource::HEDGE_FAILURE, 0, 0, hf, [0, 0]),
                seq,
                Origin::Auto,
                out,
            );
            return changed;
        }
        self.set_cap(r, seq, Origin::Auto, out)
    }

    pub(crate) fn resume(&mut self, source: CapSource, scope: &CmdScope, out: &mut ModeOutcome) -> bool {
        let removed = self.remove_cap(source, scope, out);
        if removed && source == CapSource::DAILY_LOSS {
            out.daily_loss_resumed.push(scope.account);
        }
        removed
    }

    pub(crate) fn storage(&mut self, kind: StorageKind, level: u8, seq: u64, out: &mut ModeOutcome) -> bool {
        let scope = scope_of(&[]);
        let cur = self
            .find_cap(CapSource::STORAGE, &scope)
            .and_then(|i| self.caps.get(i))
            .map_or([0, 0], |r| r.params);
        let mut p = cur;
        match kind {
            StorageKind::PersistLag => p[0] = u64::from(level),
            StorageKind::DiskLow => p[1] = u64::from(level),
            StorageKind::Both => p = [0, 0],
        }
        if p == [0, 0] {
            return self.remove_cap(CapSource::STORAGE, &scope, out);
        }
        let r = record(CapSource::STORAGE, 0, CapRecord::CONDITIONAL, scope, p);
        self.set_cap(r, seq, Origin::Auto, out)
    }

    // ------------------------------------------------------------------------- derived state

    /// Recomputes masks, HARD/flatten flags, Delisting blocks and kill bits; returns whether
    /// any gw's kill bits changed.
    pub(crate) fn recompute_caps(&mut self) -> bool {
        self.global_mask = 0;
        self.acct_mask.iter_mut().for_each(|m| *m = 0);
        self.av_mask.iter_mut().for_each(|m| *m = 0);
        self.hard_global = false;
        self.hard_acct.iter_mut().for_each(|m| *m = false);
        self.flatten_acct.iter_mut().for_each(|m| *m = false);
        self.inst_blocked.iter_mut().for_each(|m| *m = false);
        self.venue_blocked = 0;
        let n = self.f.n_accounts;
        for i in 0..self.caps.len() {
            let Some(r) = self.caps.get(i).copied() else {
                continue;
            };
            let b = bit(r.source);
            let s = r.scope;
            if r.source == CapSource::DELISTING {
                if s.inst.0 == 0 {
                    self.venue_blocked |= 1u64 << (s.venue.0 & 63);
                } else if let Some(x) = self.inst_blocked.get_mut(s.inst.0 as usize) {
                    *x = true;
                }
                continue;
            }
            if is_venue_level(&r) {
                for a in 1..=n {
                    if s.account.0 != 0 && usize::from(s.account.0) != a {
                        continue;
                    }
                    if self.f.venues.get(a).copied().unwrap_or(0) & (1u64 << (s.venue.0 & 63)) == 0
                        && s.account.0 == 0
                    {
                        continue;
                    }
                    if let Some(m) = self.av_mask.get_mut(a * 64 + usize::from(s.venue.0 & 63)) {
                        *m |= b;
                    }
                }
                continue;
            }
            if is_cell_level(&r) {
                continue;
            }
            let hard = r.source == CapSource::KILL && r.cflags & CapRecord::HARD != 0;
            let flat = r.source == CapSource::KILL && r.params[0] & kill_params::FLATTEN != 0;
            if s.account.0 == 0 {
                self.global_mask |= b;
                self.hard_global |= hard;
            } else {
                let a = usize::from(s.account.0);
                if let Some(m) = self.acct_mask.get_mut(a) {
                    *m |= b;
                }
                if let Some(m) = self.hard_acct.get_mut(a) {
                    *m |= hard;
                }
                if let Some(m) = self.flatten_acct.get_mut(a) {
                    *m |= flat;
                }
            }
        }
        let mut changed = false;
        for g in 0..self.kill_bits.len() {
            let v = self.f.gw_venue.get(g).copied().unwrap_or(VenueId(0));
            let mut bits = 0u32;
            for r in self.caps.iter().filter(|r| r.source == CapSource::KILL) {
                let hard = r.cflags & CapRecord::HARD != 0;
                let s = &r.scope;
                if s.venue.0 != 0 {
                    if s.venue != v {
                        continue;
                    }
                    if s.account.0 == 0 {
                        bits |= KillBits::VENUE.0;
                    } else if let Some(ix) = self.kix(s.account, v) {
                        bits |= KillBits::account(ix).0;
                    }
                } else if s.account.0 != 0 {
                    if let Some(ix) = self.kix(s.account, v) {
                        bits |= KillBits::account(ix).0;
                        if hard {
                            bits |= KillBits::account_hard(ix).0;
                        }
                    }
                } else {
                    bits |= KillBits::GLOBAL.0;
                    if hard {
                        bits |= KillBits::HARD.0;
                    }
                }
            }
            if let Some(k) = self.kill_bits.get_mut(g)
                && k.0 != bits
            {
                *k = KillBits(bits);
                changed = true;
            }
        }
        changed
    }

    fn kix(&self, a: AccountId, v: VenueId) -> Option<u8> {
        let x = self
            .f
            .kix
            .get(usize::from(a.0) * 64 + usize::from(v.0 & 63))
            .copied()
            .unwrap_or(u8::MAX);
        (x != u8::MAX).then_some(x)
    }
}

impl Mode {
    /// `StorageCritical`: produced once, by the first apply with a level-2 Storage cap, no
    /// `Draining` cap and `engine_reduce_pending() = false`.
    pub(crate) fn storage_drain(&mut self, v: &impl super::ModeView, out: &mut ModeOutcome) {
        if out.drain.is_some()
            || self.storage_drain_sent
            || !self.storage_level2()
            || self.has_source(CapSource::DRAINING)
            || v.engine_reduce_pending()
        {
            return;
        }
        out.drain = Some(super::DrainCause::StorageCritical);
        self.storage_drain_sent = true;
    }
}
