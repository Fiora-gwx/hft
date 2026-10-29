//! `OwnExcludedSide` (README "节点库"): one side of a book from the touch outwards (at most `L`
//! levels) less the caller's own resting order on that side, as an outside observer sees the
//! queue: best price with size left, size at the touch, depth over whole price buckets.

use hft_strategy_sdk::prelude::{BookRead, Level, Px, Qty, Side};

/// Our resting order on the side: price and remaining raw quantity.
pub type Own = Option<(i64, i64)>;

#[derive(Debug)]
pub struct OwnExcludedSide<const L: usize> {
    side: Side,
    lv: [Level; L],
    n: usize,
    own: Own,
}

impl<const L: usize> OwnExcludedSide<L> {
    /// Eval input: copies at most `L` levels of side `side` of `b` (O(L), no allocation).
    pub fn read<B: BookRead>(b: &B, side: Side, own: Own) -> OwnExcludedSide<L> {
        let mut lv = [Level {
            px: Px(0),
            qty: Qty(0),
        }; L];
        let n = b.levels(side, &mut lv);
        OwnExcludedSide { side, lv, n, own }
    }
    /// Size at `px` less our own order there, at least 0 (raw quantity).
    pub fn size_at(&self, px: i64) -> i64 {
        let q: i64 = self
            .lv
            .iter()
            .take(self.n)
            .filter(|l| l.px.0 == px)
            .map(|l| l.qty.0)
            .sum();
        let mine = self.own.filter(|(p, _)| *p == px).map_or(0, |(_, r)| r);
        q.saturating_sub(mine).max(0)
    }
    /// The price nearest the touch with size left after our own order is taken out.
    pub fn best(&self) -> Option<i64> {
        self.lv
            .iter()
            .take(self.n)
            .map(|l| l.px.0)
            .find(|px| self.size_at(*px) > 0)
    }
    /// Size left at `best` (0 without one).
    pub fn top_size(&self) -> i64 {
        self.best().map_or(0, |px| self.size_at(px))
    }
    /// Size left at the `n` bucket prices (multiples of `bucket`) from the bucket `best` is in,
    /// away from the touch (bids downwards, asks upwards); 0 without a best price.
    pub fn depth(&self, bucket: i64, n: i64) -> i64 {
        let Some(b) = self.best() else { return 0 };
        let top = b.div_euclid(bucket) * bucket;
        let dir = if self.side == Side::Buy { -1 } else { 1 };
        (0..n).map(|k| self.size_at(top + dir * k * bucket)).sum()
    }
}
