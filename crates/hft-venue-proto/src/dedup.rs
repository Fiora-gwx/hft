//! Contract 22: domain-separated keys and a bounded FIFO of distinct observed keys.
use hft_types::InstId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeKey {
    Id(u64),
    OrderSeq { exch_order_id: u64, fill_seq: u64 },
    Funding { settle_ts_ns: i64, disc: u64 },
}

pub fn dedup_key(inst: InstId, key: TradeKey) -> u64 {
    let (tag, a, b) = match key {
        TradeKey::Id(id) => (1, id, 0),
        TradeKey::OrderSeq {
            exch_order_id,
            fill_seq,
        } => (2, exch_order_id, fill_seq),
        TradeKey::Funding { settle_ts_ns, disc } => (3, settle_ts_ns as u64, disc),
    };
    let mut bytes = [0u8; 21];
    bytes[0] = tag;
    bytes[1..5].copy_from_slice(&inst.0.to_le_bytes());
    bytes[5..13].copy_from_slice(&a.to_le_bytes());
    bytes[13..21].copy_from_slice(&b.to_le_bytes());
    xxhash_rust::xxh3::xxh3_64(&bytes).max(1)
}

#[derive(Clone, Debug)]
pub struct TradeIdRing<const N: usize> {
    keys: [u64; N],
    next: usize,
}
impl<const N: usize> Default for TradeIdRing<N> {
    fn default() -> Self {
        Self {
            keys: [0; N],
            next: 0,
        }
    }
}
impl<const N: usize> TradeIdRing<N> {
    pub fn new() -> Self {
        Self::default()
    }
    /// Duplicate observations do not extend retention. Zero is reserved and never inserted.
    pub fn seen_or_insert(&mut self, key: u64) -> bool {
        debug_assert_ne!(key, 0);
        if key == 0 || N == 0 {
            return false;
        }
        if self.keys.contains(&key) {
            return true;
        }
        if let Some(slot) = self.keys.get_mut(self.next) {
            *slot = key;
        }
        self.next = if self.next + 1 == N { 0 } else { self.next + 1 };
        false
    }
}
