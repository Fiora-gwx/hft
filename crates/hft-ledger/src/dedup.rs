//! External-fill de-duplication ring: FIFO of the last `ext_dedup_cap` keys per (account, venue)
//! with an open-addressing index (O(1) lookup, deterministic hashing, no allocation after `new`).

use crate::fixed::mix64;

/// `ExtSeen.kind`.
pub const KEY_TRADE_ID: u8 = 1;
pub const KEY_ORDER_FILL: u8 = 2;

/// One de-duplication key: `(inst, trade_id)` or `(inst, exch_order_id, fill_seq)`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExtKey {
    pub inst: u32,
    pub kind: u8,
    pub a: u64,
    pub b: u64,
}

impl ExtKey {
    fn hash(&self) -> u64 {
        mix64(
            mix64(u64::from(self.inst) << 8 | u64::from(self.kind))
                ^ mix64(self.a.rotate_left(17))
                ^ self.b.rotate_left(41),
        )
    }
}

const EMPTY: u32 = 0;
const TOMB: u32 = u32::MAX;

#[derive(Clone, Debug)]
pub(crate) struct ExtRing {
    cap: usize,
    /// FIFO storage (`head` = oldest).
    keys: Vec<ExtKey>,
    head: usize,
    len: usize,
    /// Slot → ring position + 1; `EMPTY`, `TOMB`.
    index: Vec<u32>,
    tombs: usize,
}

impl ExtRing {
    pub(crate) fn new(cap: usize) -> ExtRing {
        let size = (cap.max(1) * 2).next_power_of_two();
        ExtRing {
            cap: cap.max(1),
            keys: Vec::with_capacity(cap.max(1)),
            head: 0,
            len: 0,
            index: vec![EMPTY; size],
            tombs: 0,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn clear(&mut self) {
        self.keys.clear();
        self.head = 0;
        self.len = 0;
        for x in self.index.iter_mut() {
            *x = EMPTY;
        }
        self.tombs = 0;
    }

    fn mask(&self) -> usize {
        self.index.len() - 1
    }

    /// Index slot holding `k`, if present.
    fn find(&self, k: &ExtKey) -> Option<usize> {
        let mask = self.mask();
        let mut i = (k.hash() as usize) & mask;
        for _ in 0..self.index.len() {
            match self.index.get(i).copied().unwrap_or(EMPTY) {
                EMPTY => return None,
                TOMB => {}
                v => {
                    if self.keys.get(v as usize - 1) == Some(k) {
                        return Some(i);
                    }
                }
            }
            i = (i + 1) & mask;
        }
        None
    }

    pub(crate) fn contains(&self, k: &ExtKey) -> bool {
        self.find(k).is_some()
    }

    fn put_index(&mut self, k: &ExtKey, pos: usize) {
        let mask = self.mask();
        let mut i = (k.hash() as usize) & mask;
        loop {
            match self.index.get(i).copied().unwrap_or(EMPTY) {
                EMPTY => break,
                TOMB => {
                    self.tombs = self.tombs.saturating_sub(1);
                    break;
                }
                _ => i = (i + 1) & mask,
            }
        }
        if let Some(s) = self.index.get_mut(i) {
            *s = pos as u32 + 1;
        }
    }

    fn rebuild(&mut self) {
        for x in self.index.iter_mut() {
            *x = EMPTY;
        }
        self.tombs = 0;
        for j in 0..self.len {
            let pos = (self.head + j) % self.cap;
            if let Some(k) = self.keys.get(pos).copied() {
                self.put_index(&k, pos);
            }
        }
    }

    /// Records `k` (caller checked it is absent); a full ring evicts the oldest key.
    pub(crate) fn insert(&mut self, k: ExtKey) {
        let pos = if self.len < self.cap {
            let pos = (self.head + self.len) % self.cap;
            if pos == self.keys.len() {
                self.keys.push(k);
            } else if let Some(x) = self.keys.get_mut(pos) {
                *x = k;
            }
            self.len += 1;
            pos
        } else {
            let pos = self.head;
            if let Some(old) = self.keys.get(pos).copied()
                && let Some(i) = self.find(&old)
                && let Some(s) = self.index.get_mut(i)
            {
                *s = TOMB;
                self.tombs += 1;
            }
            if let Some(x) = self.keys.get_mut(pos) {
                *x = k;
            }
            self.head = (self.head + 1) % self.cap;
            pos
        };
        self.put_index(&k, pos);
        if self.tombs * 4 > self.index.len() {
            self.rebuild();
        }
    }

    /// Keys in FIFO order (oldest first).
    pub(crate) fn iter(&self) -> impl Iterator<Item = &ExtKey> {
        (0..self.len).filter_map(move |j| self.keys.get((self.head + j) % self.cap))
    }
}
