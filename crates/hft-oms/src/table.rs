//! Per-(venue, account) tables, the fixed-capacity FIFO containers and the refdata copies the hot
//! path reads.

use hft_refdata::StaleGuardKind;
use hft_types::{AccountId, AssetExp, AssetId, InstId, Notional, Qty, VenueId};

use crate::types::{OrderSlot, OutItem, SlotExt, SlotStamps, Tomb};

/// Fixed-capacity FIFO ring (allocated once).
#[derive(Clone, Debug)]
pub(crate) struct Fifo<T: Copy> {
    buf: Box<[T]>,
    head: usize,
    len: usize,
}

impl<T: Copy> Fifo<T> {
    pub(crate) fn new(cap: usize, fill: T) -> Fifo<T> {
        Fifo {
            buf: vec![fill; cap.max(1)].into_boxed_slice(),
            head: 0,
            len: 0,
        }
    }
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }
    #[inline]
    pub(crate) fn cap(&self) -> usize {
        self.buf.len()
    }
    /// Pushes at the tail; `false` when full (the caller treats it as a capacity bug).
    #[inline]
    pub(crate) fn push(&mut self, x: T) -> bool {
        let cap = self.buf.len();
        if self.len >= cap {
            debug_assert!(self.len < cap, "Fifo full");
            return false;
        }
        let i = (self.head + self.len) % cap;
        if let Some(s) = self.buf.get_mut(i) {
            *s = x;
        }
        self.len += 1;
        true
    }
    #[inline]
    pub(crate) fn front(&self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.buf.get(self.head).copied()
    }
    #[inline]
    pub(crate) fn pop(&mut self) -> Option<T> {
        let x = self.front()?;
        self.head = (self.head + 1) % self.buf.len();
        self.len -= 1;
        Some(x)
    }
    #[inline]
    pub(crate) fn get(&self, i: usize) -> Option<T> {
        if i >= self.len {
            return None;
        }
        self.buf.get((self.head + i) % self.buf.len()).copied()
    }
    pub(crate) fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = T> + '_ {
        (0..self.len).filter_map(move |i| self.get(i))
    }
    /// Removes the element at position `i` (shifts the tail; O(len)).
    pub(crate) fn remove_at(&mut self, i: usize) {
        if i >= self.len {
            return;
        }
        let cap = self.buf.len();
        for k in i..self.len - 1 {
            let next = self.buf.get((self.head + k + 1) % cap).copied();
            if let (Some(n), Some(s)) = (next, self.buf.get_mut((self.head + k) % cap)) {
                *s = n;
            }
        }
        self.len -= 1;
    }
}

/// `exch_order_id → Qty` with FIFO eviction (the `foreign` and `ext_held` tables).
#[derive(Clone, Debug)]
pub(crate) struct FifoMap {
    pub(crate) q: Fifo<(u64, Qty)>,
}

impl FifoMap {
    pub(crate) fn new(cap: usize) -> FifoMap {
        FifoMap {
            q: Fifo::new(cap, (0, Qty(0))),
        }
    }
    pub(crate) fn find(&self, k: u64) -> Option<usize> {
        (0..self.q.len()).find(|&i| self.q.get(i).is_some_and(|(x, _)| x == k))
    }
    pub(crate) fn get(&self, k: u64) -> Option<Qty> {
        self.find(k).and_then(|i| self.q.get(i)).map(|(_, v)| v)
    }
    /// Sets `k` to `v` (`v ≤ 0` removes it). Returns `true` when an old entry was evicted.
    pub(crate) fn set(&mut self, k: u64, v: Qty) -> bool {
        if let Some(i) = self.find(k) {
            if v.0 <= 0 {
                self.q.remove_at(i);
            } else {
                let cap = self.q.cap();
                let pos = (self.q.head + i) % cap;
                if let Some(s) = self.q.buf.get_mut(pos) {
                    *s = (k, v);
                }
            }
            return false;
        }
        if v.0 <= 0 {
            return false;
        }
        let mut evicted = false;
        if self.q.len() >= self.q.cap() {
            self.q.pop();
            evicted = true;
        }
        self.q.push((k, v));
        evicted
    }
    pub(crate) fn len(&self) -> usize {
        self.q.len()
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = (u64, Qty)> + '_ {
        self.q.iter()
    }
    pub(crate) fn clear(&mut self) {
        self.q.clear();
    }
}

/// Copy of the refdata fields the OMS reads per inst.
#[derive(Copy, Clone, Debug)]
pub(crate) struct InstInfo {
    pub(crate) valid: bool,
    pub(crate) venue: VenueId,
    pub(crate) spot: bool,
    pub(crate) asset: AssetId,
    pub(crate) exp: AssetExp,
    pub(crate) lot: i64,
    pub(crate) min_qty: Qty,
    pub(crate) min_notional: Notional,
    pub(crate) no_amend: bool,
    pub(crate) perp_cash: bool,
}

impl InstInfo {
    pub(crate) const NONE: InstInfo = InstInfo {
        valid: false,
        venue: VenueId(0),
        spot: false,
        asset: AssetId(0),
        exp: AssetExp {
            px_exp: 0,
            qty_exp: 0,
        },
        lot: 0,
        min_qty: Qty(0),
        min_notional: Notional(0),
        no_amend: false,
        perp_cash: false,
    };
}

/// One (venue, account) table.
#[derive(Debug)]
pub(crate) struct Table {
    pub(crate) venue: VenueId,
    pub(crate) account: AccountId,
    pub(crate) tix: u16,
    pub(crate) aix: usize,
    /// Number of slots N (ids 1..=N; index 0 unused).
    pub(crate) n: u32,
    pub(crate) slots: Box<[OrderSlot]>,
    pub(crate) ext: Box<[SlotExt]>,
    pub(crate) stamps: Box<[SlotStamps]>,
    pub(crate) tombs: Box<[Tomb]>,
    pub(crate) free: Fifo<u32>,
    pub(crate) foreign: FifoMap,
    pub(crate) foreign_cash: Box<[crate::types::ForeignCashRow]>,
    /// Same index/capacity as foreign_cash. Empty or invalidated proof has eid 0.
    pub(crate) foreign_receipts: Box<[crate::types::ForeignCashReceipt]>,
    pub(crate) ext_held: FifoMap,
    pub(crate) unknown_count: u32,
    pub(crate) conn_up: bool,
    pub(crate) decode_fails: u8,
    pub(crate) hedge_gate: bool,
    pub(crate) sweep_cursor: u32,
    pub(crate) smoke_slot: u32,
    /// Queued decisions: risk-reducing (cancels, queries, hedges) before Open.
    pub(crate) q_rr: Fifo<OutItem>,
    pub(crate) q_open: Fifo<OutItem>,
    // ---- resolved parameters ----
    pub(crate) ack_to: u64,
    pub(crate) cancel_to: u64,
    pub(crate) ioc_to: u64,
    pub(crate) grace: u64,
    pub(crate) stale_guard: StaleGuardKind,
    pub(crate) amend: bool,
    pub(crate) amend_down_only: bool,
    pub(crate) batch_cancel: bool,
    pub(crate) has_dms: bool,
    pub(crate) out_quota: u16,
}

impl Table {
    #[inline]
    pub(crate) fn slot(&self, s: u32) -> Option<&OrderSlot> {
        if s == 0 || s > self.n {
            return None;
        }
        self.slots.get(s as usize)
    }
    #[inline]
    pub(crate) fn tomb_ix(&self, s: u32, r#gen: u32, t: u8) -> usize {
        let t = u32::from(t.max(1));
        (s as usize) * (t as usize) + (r#gen % t) as usize
    }
    /// Whether the table is ready (`venue_ready`).
    #[inline]
    pub(crate) fn ready(&self) -> bool {
        self.conn_up && self.unknown_count == 0
    }
}

/// One LegTracker entry: an (account, group) with its leg definitions.
#[derive(Clone, Debug)]
pub(crate) struct GroupEntry {
    pub(crate) account: AccountId,
    pub(crate) group: hft_types::GroupId,
    pub(crate) n_legs: u8,
    pub(crate) insts: [InstId; 4],
    /// Present in the current `GroupTable` of its instance.
    pub(crate) in_spec: bool,
    pub(crate) st: crate::types::LegState,
}

/// Counters of one (venue, account) in the in-flight book.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct VenueBook {
    pub open_notional: i64,
    pub buy_notional: i64,
    pub open_margin: i64,
}
