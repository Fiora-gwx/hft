//! Per-venue symbol table: a sorted `[SymEntry]` (binary search) plus a deterministic FxHash
//! open-addressing front table, and a sorted numeric-id table.

use core::hash::Hasher;

use hft_types::InstId;
use rustc_hash::FxHasher;

use crate::types::MAX_SYMBOL_LEN;

/// One symbol entry (key padded with zeros).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SymEntry {
    pub key: [u8; MAX_SYMBOL_LEN],
    pub len: u8,
    pub inst: u32,
}
const _: () = assert!(size_of::<SymEntry>() == 32);
const _: () = assert!(core::mem::offset_of!(SymEntry, key) == 0);
const _: () = assert!(core::mem::offset_of!(SymEntry, len) == 27);
const _: () = assert!(core::mem::offset_of!(SymEntry, inst) == 28);

impl SymEntry {
    #[inline]
    fn key(&self) -> &[u8] {
        self.key.get(..usize::from(self.len)).unwrap_or(&[])
    }
}

/// Symbol → `InstId` table of one venue. Built once at load; lookups never allocate.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct SymbolIndex {
    entries: Box<[SymEntry]>,
    /// Entry index + 1 per slot; 0 = empty. Capacity `next_pow2(2n)` (0 when empty).
    front: Box<[u32]>,
    /// `(venue_num_id, inst)` ascending; 0 never present.
    nums: Box<[(u64, InstId)]>,
    /// Front table of `nums` (index + 1 per slot), capacity `next_pow2(2n)`.
    nums_front: Box<[u32]>,
}

#[inline]
fn fx(key: &[u8]) -> u64 {
    let mut h = FxHasher::default();
    h.write(key);
    h.finish()
}

#[inline]
fn fx_u64(v: u64) -> u64 {
    let mut h = FxHasher::default();
    h.write_u64(v);
    h.finish()
}

/// Deterministic open-addressing table over `n` items (inserted in order), slot = index + 1.
fn front_table(n: usize, hash: impl Fn(usize) -> u64) -> Vec<u32> {
    let cap = if n == 0 { 0 } else { (2 * n).next_power_of_two() };
    let mut front = vec![0u32; cap];
    if cap > 0 {
        let mask = cap - 1;
        for i in 0..n {
            let mut slot = (hash(i) as usize) & mask;
            for _ in 0..cap {
                match front.get_mut(slot) {
                    Some(s) if *s == 0 => {
                        *s = i as u32 + 1;
                        break;
                    }
                    _ => slot = (slot + 1) & mask,
                }
            }
        }
    }
    front
}

impl SymbolIndex {
    /// Builds a table. Keys outside 1..=27 bytes and numeric id 0 are skipped; for duplicate
    /// keys the first occurrence wins (the loader rejects duplicates before building).
    pub fn from_entries(syms: &[(&[u8], InstId)], nums: &[(u64, InstId)]) -> SymbolIndex {
        let mut entries: Vec<SymEntry> = syms
            .iter()
            .filter(|(k, _)| (1..=MAX_SYMBOL_LEN).contains(&k.len()))
            .map(|(k, id)| {
                let mut key = [0u8; MAX_SYMBOL_LEN];
                if let Some(dst) = key.get_mut(..k.len()) {
                    dst.copy_from_slice(k);
                }
                SymEntry {
                    key,
                    len: k.len() as u8,
                    inst: id.0,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.key().cmp(b.key()));
        entries.dedup_by(|b, a| a.key() == b.key());
        let front = front_table(entries.len(), |i| entries.get(i).map_or(0, |e| fx(e.key())));
        let mut nums: Vec<(u64, InstId)> = nums.iter().copied().filter(|(n, _)| *n != 0).collect();
        nums.sort_by_key(|(n, _)| *n);
        nums.dedup_by_key(|(n, _)| *n);
        let nums_front = front_table(nums.len(), |i| nums.get(i).map_or(0, |(n, _)| fx_u64(*n)));
        SymbolIndex {
            entries: entries.into(),
            front: front.into(),
            nums: nums.into(),
            nums_front: nums_front.into(),
        }
    }

    /// Exact wire bytes (case sensitive) → inst. Uses the front table.
    #[inline]
    pub fn get(&self, wire_symbol: &[u8]) -> Option<InstId> {
        self.get_hashed(wire_symbol)
    }

    /// Front-table (FxHash, linear probing) path.
    #[inline]
    pub fn get_hashed(&self, s: &[u8]) -> Option<InstId> {
        let cap = self.front.len();
        if s.is_empty() || s.len() > MAX_SYMBOL_LEN || cap == 0 {
            return None;
        }
        let mask = cap - 1;
        let mut slot = (fx(s) as usize) & mask;
        for _ in 0..cap {
            let ix = *self.front.get(slot)?;
            let e = self.entries.get((ix as usize).checked_sub(1)?)?;
            if e.key() == s {
                return Some(InstId(e.inst));
            }
            slot = (slot + 1) & mask;
        }
        None
    }

    /// Sorted-array (binary search) path.
    #[inline]
    pub fn get_sorted(&self, s: &[u8]) -> Option<InstId> {
        self.bsearch_counted(s).0
    }

    /// Binary search returning the number of ordered comparisons made (≤ ⌈log₂ n⌉).
    pub fn bsearch_counted(&self, s: &[u8]) -> (Option<InstId>, u32) {
        let n = self.entries.len();
        if n == 0 || s.is_empty() || s.len() > MAX_SYMBOL_LEN {
            return (None, 0);
        }
        let mut base = 0usize;
        let mut size = n;
        let mut cmps = 0u32;
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            cmps += 1;
            if self.entries.get(mid).is_some_and(|e| e.key() <= s) {
                base = mid;
            }
            size -= half;
        }
        let hit = self
            .entries
            .get(base)
            .filter(|e| e.key() == s)
            .map(|e| InstId(e.inst));
        (hit, cmps)
    }

    /// Numeric venue id (OKX instIdCode, SBE symbolId …) → inst; 0 never matches.
    #[inline]
    pub fn get_num(&self, venue_num_id: u64) -> Option<InstId> {
        let cap = self.nums_front.len();
        if venue_num_id == 0 || cap == 0 {
            return None;
        }
        let mask = cap - 1;
        let mut slot = (fx_u64(venue_num_id) as usize) & mask;
        for _ in 0..cap {
            let ix = *self.nums_front.get(slot)?;
            let (n, id) = self.nums.get((ix as usize).checked_sub(1)?)?;
            if *n == venue_num_id {
                return Some(*id);
            }
            slot = (slot + 1) & mask;
        }
        None
    }

    /// Reads every entry in order and returns a checksum (keeps the reads alive). Call it on
    /// the thread that will use the table.
    pub fn prewarm(&self) -> u64 {
        let mut acc: u64 = 0xcbf2_9ce4_8422_2325;
        for e in self.entries.iter() {
            for &b in &e.key {
                acc = (acc ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
            acc = (acc ^ u64::from(e.len) ^ (u64::from(e.inst) << 8)).wrapping_mul(0x0100_0000_01b3);
        }
        for &f in self.front.iter() {
            acc = (acc ^ u64::from(f)).wrapping_mul(0x0100_0000_01b3);
        }
        for &(n, id) in self.nums.iter() {
            acc = (acc ^ n ^ u64::from(id.0)).wrapping_mul(0x0100_0000_01b3);
        }
        for &f in self.nums_front.iter() {
            acc = (acc ^ u64::from(f)).wrapping_mul(0x0100_0000_01b3);
        }
        acc
    }

    /// Number of symbol entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Sorted entries.
    pub fn entries(&self) -> &[SymEntry] {
        &self.entries
    }
    /// Front table (entry index + 1 per slot, 0 = empty).
    pub fn front_table(&self) -> &[u32] {
        &self.front
    }
}
