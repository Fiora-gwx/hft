//! `ConnPool`: one venue role's connections on its hot thread — `Race` (market data, both
//! active) or `PrimaryStandby` (orders) — with planned rotation (README §8).

use hft_types::ConnId;

use crate::arena::{Handoff, RetireMsg};
use crate::conn::Conn;
use crate::types::{ConnPhase, ErrBits};

/// Pool mode.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PoolMode {
    Race,
    PrimaryStandby,
}

/// Rotation policy: `rotate_stagger_s` converted to TSC ticks.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct RotatePolicy {
    pub stagger_tsc: u64,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PoolErr {
    Full,
    NotOpen,
    NoStandby,
    WrongMode,
    UnknownConn,
}

/// Why a build is wanted.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BuildWhy {
    /// Active connections < target.
    Fill,
    /// Planned rotation of this connection.
    Rotate(ConnId),
    /// This connection carries `SERVER_HINT`.
    Hint(ConnId),
}

#[derive(Copy, Clone, Debug)]
struct PSlot {
    h: Handoff,
    rotate_at: Option<u64>,
    /// `ROTATE_DUE` already reported.
    due: bool,
    /// A replacement was requested for this connection.
    replacing: bool,
}

const NONE: u8 = u8::MAX;

/// The pool (fixed `N` slots, no allocation).
#[derive(Debug)]
pub struct ConnPool<const N: usize = 4> {
    mode: PoolMode,
    target: u8,
    slots: [Option<PSlot>; N],
    primary: u8,
    standby: u8,
    policy: RotatePolicy,
    building: bool,
    building_for: u8,
}

/// Active connections, primary first.
#[derive(Debug)]
pub struct ActiveIter<'a> {
    idx: [u8; 2],
    k: usize,
    slots: &'a mut [Option<PSlot>],
}

impl<'a> Iterator for ActiveIter<'a> {
    type Item = &'a mut Conn;

    fn next(&mut self) -> Option<&'a mut Conn> {
        while self.k < 2 {
            let i = self.idx[self.k] as usize;
            self.k += 1;
            if let Some(Some(s)) = self.slots.get(i) {
                let h = s.h;
                // SAFETY: each index appears once (primary != standby), so the returned borrows
                // are of distinct slots; the pool owns the tokens for the iterator's lifetime.
                let c: Option<&'a mut Conn> = unsafe { h.arena.conn_mut(h.slot) };
                if let Some(c) = c {
                    return Some(c);
                }
            }
        }
        None
    }
}

impl<const N: usize> ConnPool<N> {
    /// `target` = wanted active connections (Race 2, PrimaryStandby 2, single-connection route 1).
    pub fn new(mode: PoolMode, target: u8, policy: RotatePolicy) -> Self {
        ConnPool {
            mode,
            target,
            slots: [None; N],
            primary: NONE,
            standby: NONE,
            policy,
            building: false,
            building_for: NONE,
        }
    }

    fn conn_at(&mut self, i: u8) -> Option<&mut Conn> {
        let s = self.slots.get_mut(i as usize)?.as_mut()?;
        s.h.conn()
    }

    fn id_at(&self, i: u8) -> Option<ConnId> {
        self.slots.get(i as usize)?.as_ref().map(|s| s.h.id)
    }

    /// Active connections in fixed order: primary first.
    pub fn active(&mut self) -> ActiveIter<'_> {
        ActiveIter {
            idx: [self.primary, self.standby],
            k: 0,
            slots: &mut self.slots,
        }
    }

    pub fn active_count(&self) -> usize {
        [self.primary, self.standby]
            .iter()
            .filter(|&&i| i != NONE)
            .count()
    }

    pub fn primary(&mut self) -> Option<&mut Conn> {
        let i = self.primary;
        self.conn_at(i)
    }

    pub fn standby(&mut self) -> Option<&mut Conn> {
        let i = self.standby;
        self.conn_at(i)
    }

    /// Swaps the two indices (O(1), no system call). The caller guarantees no request in flight.
    pub fn swap_primary(&mut self) -> Result<(ConnId, ConnId), PoolErr> {
        if self.mode == PoolMode::Race {
            return Err(PoolErr::WrongMode);
        }
        let s = self.standby;
        if s == NONE || self.conn_at(s).map(|c| c.phase()) != Some(ConnPhase::Open) {
            return Err(PoolErr::NoStandby);
        }
        std::mem::swap(&mut self.primary, &mut self.standby);
        match (self.id_at(self.primary), self.id_at(self.standby)) {
            (Some(p), Some(s)) => Ok((p, s)),
            _ => Err(PoolErr::NoStandby),
        }
    }

    /// Installs a connection from the `Connector`; `rotate_at_tsc` = planned rotation (None = never).
    /// Ends the "building" state. The hot thread's queue-empty confirmations start afresh.
    pub fn install(&mut self, mut h: Handoff, rotate_at_tsc: Option<u64>) -> Result<ConnId, PoolErr> {
        let Some(free) = self.slots.iter().position(|s| s.is_none()) else {
            return Err(PoolErr::Full);
        };
        let Some(c) = h.conn() else {
            return Err(PoolErr::NotOpen);
        };
        if c.phase() != ConnPhase::Open {
            return Err(PoolErr::NotOpen);
        }
        c.on_install();
        let mut at = rotate_at_tsc;
        let stagger = self.policy.stagger_tsc;
        for s in self.slots.iter_mut().flatten() {
            if let (Some(mine), Some(theirs)) = (at, s.rotate_at)
                && mine.abs_diff(theirs) < stagger
            {
                if mine >= theirs {
                    at = Some(theirs.saturating_sub(stagger));
                } else {
                    s.rotate_at = Some(mine.saturating_sub(stagger));
                }
            }
        }
        let id = h.id;
        if let Some(slot) = self.slots.get_mut(free) {
            *slot = Some(PSlot {
                h,
                rotate_at: at,
                due: false,
                replacing: false,
            });
        }
        let fi = free as u8;
        if self.primary == NONE {
            self.primary = fi;
        } else if self.standby == NONE {
            self.standby = fi;
        }
        self.building = false;
        self.building_for = NONE;
        Ok(id)
    }

    /// Removes a connection and hands it back for `shutdown` + `close`.
    pub fn retire(&mut self, id: ConnId, why: ErrBits) -> Result<RetireMsg, PoolErr> {
        let Some(i) = self.slots.iter().position(|s| s.is_some_and(|s| s.h.id == id)) else {
            return Err(PoolErr::UnknownConn);
        };
        let Some(s) = self.slots.get_mut(i).and_then(Option::take) else {
            return Err(PoolErr::UnknownConn);
        };
        let i = i as u8;
        if self.primary == i {
            self.primary = self.standby;
            self.standby = NONE;
        } else if self.standby == i {
            self.standby = NONE;
        }
        if self.primary == NONE || self.standby == NONE {
            // Promote an installed spare (a replacement waiting for its swap).
            let used = [self.primary, self.standby];
            if let Some(spare) = (0..N as u8)
                .find(|k| !used.contains(k) && self.slots.get(*k as usize).is_some_and(|s| s.is_some()))
            {
                if self.primary == NONE {
                    self.primary = spare;
                } else {
                    self.standby = spare;
                }
            }
        }
        if self.building_for == i {
            self.building_for = NONE;
        }
        Ok(RetireMsg { h: s.h, why })
    }

    /// What to build next, if anything (at most one build in flight): `Hint` > `Rotate` > `Fill`.
    pub fn next_build(&mut self, now_tsc: u64) -> Option<BuildWhy> {
        if self.building || self.slots.iter().all(|s| s.is_some()) {
            return None;
        }
        let mut hint = None;
        let mut rot = None;
        for i in 0..N as u8 {
            let Some(Some(s)) = self.slots.get(i as usize).copied() else {
                continue;
            };
            if s.replacing {
                continue;
            }
            let mut h = s.h;
            let hinted = h.conn().is_some_and(|c| c.err().contains(ErrBits::SERVER_HINT));
            if hinted && hint.is_none() {
                hint = Some(i);
            }
            if s.rotate_at.is_some_and(|t| t <= now_tsc) && rot.is_none() {
                rot = Some(i);
            }
        }
        let pick = |i: u8, this: &mut Self| {
            if let Some(Some(s)) = this.slots.get_mut(i as usize) {
                s.replacing = true;
            }
            this.building = true;
            this.building_for = i;
        };
        if let Some(i) = hint {
            let id = self.id_at(i)?;
            pick(i, self);
            return Some(BuildWhy::Hint(id));
        }
        if let Some(i) = rot {
            let id = self.id_at(i)?;
            pick(i, self);
            return Some(BuildWhy::Rotate(id));
        }
        if self.active_count() < self.target as usize {
            self.building = true;
            self.building_for = NONE;
            return Some(BuildWhy::Fill);
        }
        None
    }

    /// The upper layer reports the build failed: clears "building".
    pub fn build_failed(&mut self) {
        if let Some(Some(s)) = self.slots.get_mut(self.building_for as usize) {
            s.replacing = false;
        }
        self.building = false;
        self.building_for = NONE;
    }

    /// The first connection whose planned rotation is due; it also gets `ROTATE_DUE`.
    pub fn rotate_due(&mut self, now_tsc: u64) -> Option<ConnId> {
        for i in 0..N {
            let Some(Some(s)) = self.slots.get_mut(i) else {
                continue;
            };
            if s.due || !s.rotate_at.is_some_and(|t| t <= now_tsc) {
                continue;
            }
            s.due = true;
            let id = s.h.id;
            if let Some(c) = s.h.conn() {
                c.set_rotate_due();
            }
            return Some(id);
        }
        None
    }

    /// The installed connection `id`, active or not.
    pub fn get(&mut self, id: ConnId) -> Option<&mut Conn> {
        let s = self.slots.iter_mut().flatten().find(|s| s.h.id == id)?;
        s.h.conn()
    }

    /// Planned rotation time of `id` (test and telemetry view).
    pub fn rotate_at(&self, id: ConnId) -> Option<u64> {
        self.slots
            .iter()
            .flatten()
            .find(|s| s.h.id == id)
            .and_then(|s| s.rotate_at)
    }
}
