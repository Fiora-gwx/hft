//! `ExecOut` (README "输出 `ExecOut`"): fixed-capacity output vectors, allocated once by the
//! sequencer and drained after every call into this crate.

use hft_event::{ExecDecision, Note};
use hft_strategy_sdk::OrderRef;

use crate::types::{BrokenReq, CancelResting, ExecNotify, ExecOrder, ExecTimerReq};

/// Vector with a capacity fixed at construction; `push` never reallocates.
#[derive(Clone, Debug)]
pub struct FixedVec<T> {
    v: Vec<T>,
    cap: usize,
}

impl<T: Copy> FixedVec<T> {
    /// Allocates room for `cap` elements once.
    pub fn with_capacity(cap: usize) -> FixedVec<T> {
        FixedVec {
            v: Vec::with_capacity(cap),
            cap,
        }
    }
    /// Appends `x`; `false` (nothing written) when full. Callers check `remaining()` first, so a
    /// full vector is a bug (`debug_assert`).
    #[inline]
    pub fn push(&mut self, x: T) -> bool {
        if self.v.len() >= self.cap {
            debug_assert!(false, "FixedVec full");
            return false;
        }
        self.v.push(x);
        true
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.v.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }
    #[inline]
    pub fn remaining(&self) -> usize {
        self.cap.saturating_sub(self.v.len())
    }
    #[inline]
    pub fn clear(&mut self) {
        self.v.clear();
    }
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.v
    }
    #[inline]
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.v.iter()
    }
    #[inline]
    pub fn last(&self) -> Option<&T> {
        self.v.last()
    }
}

impl<'a, T> IntoIterator for &'a FixedVec<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.v.iter()
    }
}

/// Capacities of the `ExecOut` vectors.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct OutCaps {
    pub orders: usize,
    pub cancels: usize,
    pub cancel_resting: usize,
    pub decisions: usize,
    pub notify: usize,
    pub broken: usize,
    pub timers: usize,
    pub notes: usize,
}

impl OutCaps {
    /// The README capacities.
    pub const README: OutCaps = OutCaps {
        orders: 8,
        cancels: 8,
        cancel_resting: 8,
        decisions: 16,
        notify: 8,
        broken: 4,
        timers: 8,
        notes: 8,
    };
    /// Per-cell maximum of one `advance` step (README "输出容量与续推").
    pub const PER_CELL: OutCaps = OutCaps {
        orders: 4,
        cancels: 4,
        cancel_resting: 4,
        decisions: 6,
        notify: 2,
        broken: 1,
        timers: 3,
        notes: 3,
    };
    /// The same capacity `n` for every vector (tests: "unlimited").
    pub const fn all(n: usize) -> OutCaps {
        OutCaps {
            orders: n,
            cancels: n,
            cancel_resting: n,
            decisions: n,
            notify: n,
            broken: n,
            timers: n,
            notes: n,
        }
    }
}

/// Output of one call into this crate.
#[derive(Clone, Debug)]
pub struct ExecOut {
    pub orders: FixedVec<ExecOrder>,
    pub cancels: FixedVec<OrderRef>,
    pub cancel_resting: FixedVec<CancelResting>,
    pub decisions: FixedVec<ExecDecision>,
    pub notify: FixedVec<ExecNotify>,
    pub broken: FixedVec<BrokenReq>,
    pub timers: FixedVec<ExecTimerReq>,
    pub notes: FixedVec<Note>,
}

impl Default for ExecOut {
    fn default() -> Self {
        ExecOut::new()
    }
}

impl ExecOut {
    /// README capacities.
    pub fn new() -> ExecOut {
        ExecOut::with_caps(OutCaps::README)
    }
    pub fn with_caps(c: OutCaps) -> ExecOut {
        ExecOut {
            orders: FixedVec::with_capacity(c.orders),
            cancels: FixedVec::with_capacity(c.cancels),
            cancel_resting: FixedVec::with_capacity(c.cancel_resting),
            decisions: FixedVec::with_capacity(c.decisions),
            notify: FixedVec::with_capacity(c.notify),
            broken: FixedVec::with_capacity(c.broken),
            timers: FixedVec::with_capacity(c.timers),
            notes: FixedVec::with_capacity(c.notes),
        }
    }
    /// Drains every vector (keeps the allocations).
    pub fn clear(&mut self) {
        self.orders.clear();
        self.cancels.clear();
        self.cancel_resting.clear();
        self.decisions.clear();
        self.notify.clear();
        self.broken.clear();
        self.timers.clear();
        self.notes.clear();
    }
    /// All vectors empty.
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
            && self.cancels.is_empty()
            && self.cancel_resting.is_empty()
            && self.decisions.is_empty()
            && self.notify.is_empty()
            && self.broken.is_empty()
            && self.timers.is_empty()
            && self.notes.is_empty()
    }
    /// Every vector has room for one more `advance` cell step (`PER_CELL`), plus the `Continue`
    /// timer.
    pub(crate) fn room_for_cell(&self) -> bool {
        let p = OutCaps::PER_CELL;
        self.orders.remaining() >= p.orders
            && self.cancels.remaining() >= p.cancels
            && self.cancel_resting.remaining() >= p.cancel_resting
            && self.decisions.remaining() >= p.decisions
            && self.notify.remaining() >= p.notify
            && self.broken.remaining() >= p.broken
            && self.timers.remaining() > p.timers
            && self.notes.remaining() >= p.notes
    }
}
