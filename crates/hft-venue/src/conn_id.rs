//! Connection numbers (README §1 "连接号"): BBO slots (slot = connection id, smallest free slot
//! first, reusable only after the old connection's `Down{inst = 0}` was pushed) and the cyclic
//! ranges of the other tasks.

use hft_net::{ConnId, ConnSet};
use hft_types::MAX_BBO_CONN_SLOTS;

/// Feed-md connections (TopN, trades, mark/index/funding).
pub const MD_FIRST: u8 = MAX_BBO_CONN_SLOTS;
pub const MD_LAST: u8 = 63;
/// snap-fetch REST connections.
pub const SNAP_FIRST: u8 = 64;
pub const SNAP_LAST: u8 = 79;
/// acct-fetch REST connections.
pub const ACCT_FIRST: u8 = 80;
pub const ACCT_LAST: u8 = 111;
/// gw order, user-stream and REST connections.
pub const GW_FIRST: u8 = 112;
pub const GW_LAST: u8 = 255;

/// BBO slot allocator: a bit per slot `0..MAX_BBO_CONN_SLOTS`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct BboSlots {
    used: u8,
}

impl BboSlots {
    /// Smallest free slot (taken); `None` when all are in use.
    pub fn alloc(&mut self) -> Option<ConnId> {
        let free = !self.used;
        if free == 0 {
            return None;
        }
        let s = free.trailing_zeros() as u8;
        if s >= MAX_BBO_CONN_SLOTS {
            return None;
        }
        self.used |= 1 << s;
        Some(ConnId(s))
    }

    /// Frees a slot (after its `Down{inst = 0}` was pushed, or a build that never produced records failed).
    pub fn free(&mut self, id: ConnId) {
        if id.0 < MAX_BBO_CONN_SLOTS {
            self.used &= !(1u8 << id.0);
        }
    }

    pub fn in_use(&self) -> u32 {
        self.used.count_ones()
    }

    pub fn is_used(&self, id: ConnId) -> bool {
        id.0 < MAX_BBO_CONN_SLOTS && self.used & (1 << id.0) != 0
    }
}

/// Cyclic allocator of one id range `[lo, hi]`, skipping ids still in use.
#[derive(Copy, Clone, Debug)]
pub struct IdRange {
    lo: u8,
    hi: u8,
    next: u8,
    used: ConnSet,
}

impl IdRange {
    pub fn new(lo: u8, hi: u8) -> IdRange {
        IdRange {
            lo,
            hi,
            next: lo,
            used: ConnSet::default(),
        }
    }

    pub fn alloc(&mut self) -> Option<ConnId> {
        let n = u16::from(self.hi) - u16::from(self.lo) + 1;
        for _ in 0..n {
            let id = ConnId(self.next);
            self.next = if self.next >= self.hi {
                self.lo
            } else {
                self.next + 1
            };
            if !self.used.contains(id) {
                self.used.insert(id);
                return Some(id);
            }
        }
        None
    }

    pub fn free(&mut self, id: ConnId) {
        let i = (id.0 >> 6) as usize & 3;
        if let Some(w) = self.used.bits.get_mut(i) {
            *w &= !(1u64 << (id.0 & 63));
        }
    }
}
