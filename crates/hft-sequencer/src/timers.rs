//! `TimerHeap` (README "定时器"): preallocated binary heap keyed `(deadline_tsc, owner, timer_id,
//! group)`, the section-8 row and the cross-restart shift.

use hft_types::{GroupId, Tsc};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Section 8 row (24 B, row version 2).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct TimerRow {
    /// This snapshot's run TSC (section 25 gives the base).
    pub deadline_tsc: u64,
    pub timer_id: u32,
    pub owner: u16,
    pub _pad: u16,
    pub group: GroupId,
    pub _pad2: u32,
}
const _: () = assert!(core::mem::size_of::<TimerRow>() == 24);
const _: () = assert!(core::mem::offset_of!(TimerRow, timer_id) == 8);
const _: () = assert!(core::mem::offset_of!(TimerRow, owner) == 12);
const _: () = assert!(core::mem::offset_of!(TimerRow, group) == 16);

pub const SECTION_TIMERS: u16 = 8;
pub const TIMERS_VERSION: u16 = 2;

/// One heap entry; ordering = the heap key.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct TimerEntry {
    pub deadline: u64,
    pub owner: u16,
    pub timer_id: u32,
    pub group: u32,
}

impl TimerEntry {
    #[inline]
    fn key(&self) -> (u64, u16, u32, u32) {
        (self.deadline, self.owner, self.timer_id, self.group)
    }
    pub fn row(&self) -> TimerRow {
        TimerRow {
            deadline_tsc: self.deadline,
            timer_id: self.timer_id,
            owner: self.owner,
            _pad: 0,
            group: GroupId(self.group),
            _pad2: 0,
        }
    }
    pub fn from_row(r: &TimerRow) -> TimerEntry {
        TimerEntry {
            deadline: r.deadline_tsc,
            owner: r.owner,
            timer_id: r.timer_id,
            group: r.group.0,
        }
    }
}

/// The heap is full (engine defect: `Fatal(TimerOverflow)`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct HeapFull;

/// Preallocated min-heap of timers with a total order.
#[derive(Clone, Debug)]
pub struct TimerHeap {
    v: Vec<TimerEntry>,
    cap: usize,
}

impl TimerHeap {
    pub fn new(cap: usize) -> TimerHeap {
        TimerHeap {
            v: Vec::with_capacity(cap),
            cap,
        }
    }
    pub fn len(&self) -> usize {
        self.v.len()
    }
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.cap
    }
    /// Test hook: lowers the effective capacity.
    pub fn set_capacity(&mut self, cap: usize) {
        self.cap = cap.min(self.v.capacity().max(cap));
    }
    pub fn peek(&self) -> Option<&TimerEntry> {
        self.v.first()
    }
    /// Entries in heap order (not sorted).
    pub fn iter(&self) -> impl Iterator<Item = &TimerEntry> + '_ {
        self.v.iter()
    }
    pub fn push(&mut self, e: TimerEntry) -> Result<(), HeapFull> {
        if self.v.len() >= self.cap {
            return Err(HeapFull);
        }
        self.v.push(e);
        let n = self.v.len() - 1;
        self.up(n);
        Ok(())
    }
    pub fn pop(&mut self) -> Option<TimerEntry> {
        let n = self.v.len();
        if n == 0 {
            return None;
        }
        self.v.swap(0, n - 1);
        let top = self.v.pop();
        if !self.v.is_empty() {
            self.down(0);
        }
        top
    }
    /// Removes the entry at index `i`.
    fn remove_at(&mut self, i: usize) -> Option<TimerEntry> {
        let n = self.v.len();
        if i >= n {
            return None;
        }
        self.v.swap(i, n - 1);
        let e = self.v.pop();
        if i < self.v.len() {
            self.down(i);
            self.up(i);
        }
        e
    }
    /// Removes every entry with this `(owner, timer_id, group)`; returns how many.
    pub fn remove_key(&mut self, owner: u16, timer_id: u32, group: u32) -> usize {
        let mut n = 0;
        let mut i = 0;
        while i < self.v.len() {
            let e = self.v.get(i).copied().unwrap_or_default();
            if e.owner == owner && e.timer_id == timer_id && e.group == group {
                self.remove_at(i);
                n += 1;
            } else {
                i += 1;
            }
        }
        n
    }
    /// Removes the exact entry; true when found.
    pub fn remove_exact(&mut self, e: &TimerEntry) -> bool {
        match self.v.iter().position(|x| x == e) {
            Some(i) => self.remove_at(i).is_some(),
            None => false,
        }
    }
    /// Removes entries matching `f`; returns how many.
    pub fn remove_if(&mut self, mut f: impl FnMut(&TimerEntry) -> bool) -> usize {
        let mut n = 0;
        let mut i = 0;
        while i < self.v.len() {
            let hit = self.v.get(i).is_some_and(&mut f);
            if hit {
                self.remove_at(i);
                n += 1;
            } else {
                i += 1;
            }
        }
        n
    }
    /// Finds an entry by key.
    pub fn find_key(&self, owner: u16, timer_id: u32, group: u32) -> Option<&TimerEntry> {
        self.v
            .iter()
            .find(|e| e.owner == owner && e.timer_id == timer_id && e.group == group)
    }
    /// Count of entries matching `f`.
    pub fn count_if(&self, f: impl FnMut(&&TimerEntry) -> bool) -> usize {
        self.v.iter().filter(f).count()
    }
    pub fn clear(&mut self) {
        self.v.clear();
    }
    /// Applies `f` to every deadline, then restores the heap order.
    pub fn map_deadlines(&mut self, mut f: impl FnMut(u64) -> u64) {
        for e in self.v.iter_mut() {
            e.deadline = f(e.deadline);
        }
        let n = self.v.len();
        for i in (0..n / 2).rev() {
            self.down(i);
        }
    }
    /// Entries sorted by key (snapshot order; cold path).
    pub fn sorted(&self) -> Vec<TimerEntry> {
        let mut v = self.v.clone();
        v.sort_by_key(|e| e.key());
        v
    }
    fn up(&mut self, mut i: usize) {
        while i > 0 {
            let p = (i - 1) / 2;
            let (a, b) = (self.key_at(i), self.key_at(p));
            if a < b {
                self.v.swap(i, p);
                i = p;
            } else {
                break;
            }
        }
    }
    fn down(&mut self, mut i: usize) {
        let n = self.v.len();
        loop {
            let l = 2 * i + 1;
            let r = l + 1;
            let mut m = i;
            if l < n && self.key_at(l) < self.key_at(m) {
                m = l;
            }
            if r < n && self.key_at(r) < self.key_at(m) {
                m = r;
            }
            if m == i {
                break;
            }
            self.v.swap(i, m);
            i = m;
        }
    }
    #[inline]
    fn key_at(&self, i: usize) -> (u64, u16, u32, u32) {
        self.v.get(i).map_or((u64::MAX, u16::MAX, u32::MAX, u32::MAX), TimerEntry::key)
    }
}

/// Shift of a registered TSC across a restart (formula owned by hft-exec; shared with hft-oms).
/// `t = 0` stays 0; past times land before `now` (at least 1); future deadlines keep their
/// remaining duration converted by `hz_cur / hz_prev`.
pub fn shift_tsc(t: u64, now: Tsc, stop: Tsc, hz_prev: u64, hz_cur: u64) -> u64 {
    if t == 0 {
        return 0;
    }
    let conv = |d: u64| -> u64 {
        let v = i128::from(d) * i128::from(hz_cur) / i128::from(hz_prev.max(1));
        v.clamp(0, i128::from(u64::MAX)) as u64
    };
    if t <= stop.0 {
        now.0.saturating_sub(conv(stop.0 - t)).max(1)
    } else {
        now.0.saturating_add(conv(t - stop.0))
    }
}

impl Default for TimerRow {
    fn default() -> TimerRow {
        <TimerRow as zerocopy::FromZeros>::new_zeroed()
    }
}
