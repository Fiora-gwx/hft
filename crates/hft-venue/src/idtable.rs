//! `IdTable`: exchange order id → `(epoch, slot, gen)` (README §6). Linear probing at load ≤ 0.5,
//! backward-shift deletion (no tombstone markers), terminal entries kept for `gw.id_tomb_ms` in
//! entry order of becoming terminal (the retention ring stores the entry's key hash: entries move
//! on backward shifts, so ring entries are found again by hash).

use xxhash_rust::xxh3::xxh3_64;

/// One entry (64 B, one cache line).
#[repr(C, align(64))]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct IdEntry {
    pub hash: u64,
    /// When the order became terminal (`TERMINAL` set).
    pub term_tsc: u64,
    pub slot: u32,
    pub r#gen: u32,
    pub epoch: u16,
    /// 0 = empty entry.
    pub key_len: u8,
    pub flags: u8,
    /// Exchange id text (≤ 36 B).
    pub key: [u8; 36],
}
const _: () = assert!(core::mem::size_of::<IdEntry>() == 64);
const _: () = assert!(core::mem::align_of::<IdEntry>() == 64);
const _: () = assert!(core::mem::offset_of!(IdEntry, key) == 28);

impl IdEntry {
    pub const TERMINAL: u8 = 1;
    pub const KEY_MAX: usize = 36;

    const EMPTY: IdEntry = IdEntry {
        hash: 0,
        term_tsc: 0,
        slot: 0,
        r#gen: 0,
        epoch: 0,
        key_len: 0,
        flags: 0,
        key: [0; 36],
    };

    fn is_empty(&self) -> bool {
        self.key_len == 0
    }

    fn key(&self) -> &[u8] {
        self.key.get(..self.key_len as usize).unwrap_or(&[])
    }
}

/// `exch_order_id` of an exchange id: decimal digits that fit `u64` are used as is, anything else is
/// the xxh3-64 hash of the text (README §6 last item).
pub fn exch_id_value(key: &[u8]) -> u64 {
    if !key.is_empty() && key.len() <= 20 && key.iter().all(u8::is_ascii_digit) {
        let mut v: u64 = 0;
        for &d in key {
            match v.checked_mul(10).and_then(|v| v.checked_add(u64::from(d - b'0'))) {
                Some(n) => v = n,
                None => return xxh3_64(key),
            }
        }
        return v;
    }
    xxh3_64(key)
}

/// Insert failures (counted by the gw).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum IdErr {
    /// Longer than 36 B (`id_too_long`).
    TooLong,
    /// Load would exceed 0.5 and no retained entry is old enough to evict.
    Full,
}

/// Counters.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct IdCounters {
    pub too_long: u64,
    pub full: u64,
    pub tomb_full: u64,
    pub evicted: u64,
}

#[derive(Debug)]
pub struct IdTable {
    e: Box<[IdEntry]>,
    len: u32,
    mask: u32,
    tomb: Box<[u64]>,
    tomb_head: usize,
    tomb_len: usize,
    keep_ticks: u64,
    pub n: IdCounters,
}

impl IdTable {
    /// `cap` = `gw.id_table_cap` (power of two, checked by `GwRuntime::new`), `tomb_cap` =
    /// `gw.id_tomb_cap`, `keep_ticks` = `gw.id_tomb_ms` in TSC ticks. Allocates once.
    pub fn new(cap: u32, tomb_cap: u32, keep_ticks: u64) -> IdTable {
        let cap = cap.max(2).next_power_of_two();
        IdTable {
            e: vec![IdEntry::EMPTY; cap as usize].into_boxed_slice(),
            len: 0,
            mask: cap - 1,
            tomb: vec![0u64; tomb_cap.max(1) as usize].into_boxed_slice(),
            tomb_head: 0,
            tomb_len: 0,
            keep_ticks,
            n: IdCounters::default(),
        }
    }

    pub fn len(&self) -> u32 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn find(&self, key: &[u8], h: u64) -> Option<usize> {
        let mut i = (h as u32 & self.mask) as usize;
        for _ in 0..=self.mask {
            let e = self.e.get(i)?;
            if e.is_empty() {
                return None;
            }
            if e.hash == h && e.key() == key {
                return Some(i);
            }
            i = (i + 1) & self.mask as usize;
        }
        None
    }

    fn find_by_hash(&self, h: u64) -> Option<usize> {
        let mut i = (h as u32 & self.mask) as usize;
        for _ in 0..=self.mask {
            let e = self.e.get(i)?;
            if e.is_empty() {
                return None;
            }
            if e.hash == h && e.flags & IdEntry::TERMINAL != 0 {
                return Some(i);
            }
            i = (i + 1) & self.mask as usize;
        }
        None
    }

    /// `(epoch, slot, gen)` of `key`; terminal entries resolve too.
    pub fn get(&self, key: &[u8]) -> Option<(u16, u32, u32)> {
        let i = self.find(key, xxh3_64(key))?;
        self.e.get(i).map(|e| (e.epoch, e.slot, e.r#gen))
    }

    /// Records `key` → owner. A retained entry of the same key is overwritten (the key is the
    /// exchange id; a new generation of the same slot never touches other keys).
    pub fn insert(&mut self, key: &[u8], epoch: u16, slot: u32, r#gen: u32, now: u64) -> Result<(), IdErr> {
        if key.is_empty() || key.len() > IdEntry::KEY_MAX {
            self.n.too_long += 1;
            return Err(IdErr::TooLong);
        }
        let h = xxh3_64(key);
        if let Some(i) = self.find(key, h) {
            if let Some(e) = self.e.get_mut(i) {
                e.epoch = epoch;
                e.slot = slot;
                e.r#gen = r#gen;
            }
            return Ok(());
        }
        if (self.len + 1) * 2 > self.mask + 1 && !self.evict_one(now) {
            self.n.full += 1;
            return Err(IdErr::Full);
        }
        let mut i = (h as u32 & self.mask) as usize;
        loop {
            let Some(e) = self.e.get_mut(i) else {
                return Err(IdErr::Full);
            };
            if e.is_empty() {
                let mut k = [0u8; 36];
                if let Some(d) = k.get_mut(..key.len()) {
                    d.copy_from_slice(key);
                }
                *e = IdEntry {
                    hash: h,
                    term_tsc: 0,
                    slot,
                    r#gen,
                    epoch,
                    key_len: key.len() as u8,
                    flags: 0,
                    key: k,
                };
                self.len += 1;
                return Ok(());
            }
            i = (i + 1) & self.mask as usize;
        }
    }

    /// The order of `key` reached a terminal state: keep the entry for `keep_ticks` (README §6).
    pub fn terminal(&mut self, key: &[u8], now: u64) {
        let h = xxh3_64(key);
        let Some(i) = self.find(key, h) else { return };
        if self.e.get(i).is_some_and(|e| e.flags & IdEntry::TERMINAL != 0) {
            return;
        }
        if self.tomb_len == self.tomb.len() && !self.evict_one(now) {
            // Retention ring full and its head not expired: delete right away.
            self.n.tomb_full += 1;
            self.remove_at(i);
            return;
        }
        // Eviction performs backward-shift deletion: the live entry can move.
        let Some(i) = self.find(key, h) else { return };
        if let Some(e) = self.e.get_mut(i) {
            e.flags |= IdEntry::TERMINAL;
            e.term_tsc = now;
        }
        let t = (self.tomb_head + self.tomb_len) % self.tomb.len();
        if let Some(s) = self.tomb.get_mut(t) {
            *s = h;
        }
        self.tomb_len += 1;
    }

    /// Evicts the oldest retained entry if it is at least `keep_ticks` old.
    fn evict_one(&mut self, now: u64) -> bool {
        while self.tomb_len > 0 {
            let h = self.tomb.get(self.tomb_head).copied().unwrap_or(0);
            let Some(i) = self.find_by_hash(h) else {
                // Already removed (overwritten key); drop the ring entry.
                self.tomb_head = (self.tomb_head + 1) % self.tomb.len();
                self.tomb_len -= 1;
                continue;
            };
            let t = self.e.get(i).map_or(0, |e| e.term_tsc);
            if now.saturating_sub(t) < self.keep_ticks {
                return false;
            }
            self.tomb_head = (self.tomb_head + 1) % self.tomb.len();
            self.tomb_len -= 1;
            self.remove_at(i);
            self.n.evicted += 1;
            return true;
        }
        false
    }

    /// Backward-shift deletion at `i`.
    fn remove_at(&mut self, mut i: usize) {
        let m = self.mask as usize;
        let mut j = i;
        loop {
            j = (j + 1) & m;
            let Some(ej) = self.e.get(j).copied() else { break };
            if ej.is_empty() {
                break;
            }
            let home = (ej.hash as u32 & self.mask) as usize;
            // ej may move to i if its home is not cyclically in (i, j].
            let in_range = if i <= j {
                home > i && home <= j
            } else {
                home > i || home <= j
            };
            if !in_range {
                if let Some(ei) = self.e.get_mut(i) {
                    *ei = ej;
                }
                i = j;
            }
        }
        if let Some(ei) = self.e.get_mut(i) {
            *ei = IdEntry::EMPTY;
        }
        self.len = self.len.saturating_sub(1);
    }
}
