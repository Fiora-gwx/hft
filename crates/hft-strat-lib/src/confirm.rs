//! `ConfirmWindow` (README "ConfirmWindow"): a condition must hold over N updates, T ms, slow-side
//! updates and a slow-side sequence span before it is confirmed.

use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};

use crate::units::ms_to_ns;

/// Window state.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum Confirm {
    #[default]
    Idle = 0,
    Pending = 1,
    Confirmed = 2,
}

impl Confirm {
    const fn from_u8(v: u8) -> Option<Confirm> {
        match v {
            0 => Some(Confirm::Idle),
            1 => Some(Confirm::Pending),
            2 => Some(Confirm::Confirmed),
            _ => None,
        }
    }
}

/// Confirmation parameters; 0 = that item is not required (all 0 = confirm on the first true).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ConfirmParams {
    pub min_ms: u32,
    pub min_updates: u16,
    pub min_slow_updates: u16,
    pub min_slow_span: u8,
}

/// [`ConfirmParams`] with `min_ms` converted to ns (recomputed on parameter changes).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ConfirmParamsNs {
    pub min_ns: i64,
    pub min_updates: u16,
    pub min_slow_updates: u16,
    pub min_slow_span: u8,
}

impl ConfirmParams {
    #[inline]
    pub const fn ns(&self) -> ConfirmParamsNs {
        ConfirmParamsNs {
            min_ns: ms_to_ns(self.min_ms),
            min_updates: self.min_updates,
            min_slow_updates: self.min_slow_updates,
            min_slow_span: self.min_slow_span,
        }
    }
}

/// Confirmation window. `Default` = `Idle`, counters 0. Times are engine_ns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ConfirmWindow {
    since_ens: i64,
    n_updates: u16,
    n_slow: u16,
    first_slow_seq: u64,
    last_slow_seq: u64,
    state: Confirm,
}

impl ConfirmWindow {
    /// One evaluation: `cond` = the condition holds now; `from_slow` = triggered by a slow-side
    /// update; `slow_seq` = slow-side sequence (block number, update id; 0 when none).
    pub fn observe(
        &mut self,
        cond: bool,
        from_slow: bool,
        slow_seq: u64,
        now_ens: i64,
        p: &ConfirmParamsNs,
    ) -> Confirm {
        if !cond {
            self.reset();
            return Confirm::Idle;
        }
        if self.state == Confirm::Idle {
            self.since_ens = now_ens;
            self.n_updates = 1;
            self.n_slow = u16::from(from_slow);
            self.first_slow_seq = slow_seq;
            self.last_slow_seq = slow_seq;
            self.state = Confirm::Pending;
        } else {
            self.n_updates = self.n_updates.saturating_add(1);
            self.n_slow = self.n_slow.saturating_add(u16::from(from_slow));
            self.last_slow_seq = self.last_slow_seq.max(slow_seq);
        }
        if self.state == Confirm::Pending {
            let elapsed = i128::from(now_ens) - i128::from(self.since_ens);
            let span = self
                .last_slow_seq
                .saturating_sub(self.first_slow_seq)
                .saturating_add(1);
            if elapsed >= i128::from(p.min_ns)
                && self.n_updates >= p.min_updates
                && self.n_slow >= p.min_slow_updates
                && span >= u64::from(p.min_slow_span)
            {
                self.state = Confirm::Confirmed;
            }
        }
        self.state
    }

    /// Back to `Idle` with zero counters.
    #[inline]
    pub fn reset(&mut self) {
        *self = ConfirmWindow::default();
    }

    /// Current state.
    #[inline]
    pub fn state(&self) -> Confirm {
        self.state
    }

    /// 29 B: `since_ens` i64, `n_updates` u16, `n_slow` u16, `first_slow_seq` u64,
    /// `last_slow_seq` u64, `state` u8.
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_i64(self.since_ens);
        w.put_u16(self.n_updates);
        w.put_u16(self.n_slow);
        w.put_u64(self.first_slow_seq);
        w.put_u64(self.last_slow_seq);
        w.put_u8(self.state as u8);
    }

    /// On error (truncated, illegal state) `self` is unchanged.
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let since_ens = r.get_i64()?;
        let n_updates = r.get_u16()?;
        let n_slow = r.get_u16()?;
        let first_slow_seq = r.get_u64()?;
        let last_slow_seq = r.get_u64()?;
        let state = Confirm::from_u8(r.get_u8()?).ok_or(RestoreError::BadValue)?;
        *self = ConfirmWindow {
            since_ens,
            n_updates,
            n_slow,
            first_slow_seq,
            last_slow_seq,
            state,
        };
        Ok(())
    }
}
