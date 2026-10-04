//! `iter`, `export`, `state_hash`: visible levels in price order, best first.

use hft_event::Level;
use hft_types::{Px, Qty, Side};
use xxhash_rust::xxh3::Xxh3;

use crate::bitmap;
use crate::l2::{BID, L2Book, side_ix, sl};

/// Visible levels of one side from the inside out (best first): overflow levels better than the
/// window, the window, then overflow levels behind the window. Levels merged into the far
/// aggregate are not visible and end the iteration.
pub struct LevelIter<'a> {
    qty: &'a [u64],
    occ: &'a [u64],
    ovp: &'a [u64],
    ovq: &'a [u64],
    base: i64,
    top: i64,
    tick: i64,
    bid: bool,
    phase: u8,
    /// Overflow cursor: next entry (bids walk down from the top, asks up from 0).
    oi: isize,
    /// Window scan: current bitmap word index and its not-yet-returned bits.
    k: usize,
    word: u64,
}

impl core::fmt::Debug for LevelIter<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LevelIter")
            .field("bid", &self.bid)
            .field("phase", &self.phase)
            .field("oi", &self.oi)
            .field("k", &self.k)
            .finish()
    }
}

impl LevelIter<'_> {
    #[inline]
    fn level(&self, abs: i64, q: i64) -> Level {
        Level {
            px: Px(abs.wrapping_mul(self.tick)),
            qty: Qty(q),
        }
    }

    #[inline]
    fn ovf_at(&self, i: isize) -> Option<(i64, i64)> {
        let i = usize::try_from(i).ok()?;
        Some((
            *self.ovp.get(i)? as i64,
            self.ovq.get(i).copied().unwrap_or(0) as i64,
        ))
    }

    /// Next set window index in scan order.
    #[inline]
    fn next_win(&mut self) -> Option<usize> {
        if self.bid {
            while self.word == 0 {
                if self.k == 0 {
                    return None;
                }
                self.k -= 1;
                self.word = self.occ.get(self.k).copied().unwrap_or(0);
            }
            let b = 63 - self.word.leading_zeros() as usize;
            self.word &= !(1u64 << b);
            Some(self.k * 64 + b)
        } else {
            while self.word == 0 {
                self.k += 1;
                if self.k >= self.occ.len() {
                    return None;
                }
                self.word = self.occ.get(self.k).copied().unwrap_or(0);
            }
            let b = self.word.trailing_zeros() as usize;
            self.word &= self.word - 1;
            Some(self.k * 64 + b)
        }
    }
}

impl Iterator for LevelIter<'_> {
    type Item = Level;

    #[inline]
    fn next(&mut self) -> Option<Level> {
        let step: isize = if self.bid { -1 } else { 1 };
        loop {
            match self.phase {
                0 => {
                    // Overflow levels better than the window.
                    if let Some((p, q)) = self.ovf_at(self.oi) {
                        let ahead = if self.bid { p >= self.top } else { p < self.base };
                        if ahead {
                            self.oi += step;
                            return Some(self.level(p, q));
                        }
                    }
                    self.phase = 1;
                }
                1 => match self.next_win() {
                    Some(r) => {
                        let q = self.qty.get(r).copied().unwrap_or(0) as i64;
                        return Some(self.level(self.base.wrapping_add(r as i64), q));
                    }
                    None => self.phase = 2,
                },
                2 => {
                    let (p, q) = self.ovf_at(self.oi)?;
                    self.oi += step;
                    return Some(self.level(p, q));
                }
                _ => return None,
            }
        }
    }
}

impl L2Book {
    /// Visible levels of `side`, best first.
    pub fn iter(&self, side: Side) -> LevelIter<'_> {
        let s = side_ix(side);
        self.iter_ix(s)
    }

    pub(crate) fn iter_ix(&self, s: usize) -> LevelIter<'_> {
        let g = self.g;
        let w = self.words();
        let n = usize::from(self.h.ovf_len[s]);
        let bid = s == BID;
        let occ = sl(w, g.occ(s), g.nw);
        // Start at the window best (bits beyond it are clear by invariant).
        let (k, word) = if self.h.best[s] < 0 {
            (if bid { 0 } else { g.nw }, 0)
        } else {
            let r = self.h.best[s] as usize;
            let word = occ.get(r / 64).copied().unwrap_or(0);
            let mask = if bid {
                u64::MAX >> (63 - r % 64)
            } else {
                u64::MAX << (r % 64)
            };
            (r / 64, word & mask)
        };
        LevelIter {
            qty: sl(w, g.qty(s), g.l),
            occ,
            ovp: sl(w, g.ovp(s), n),
            ovq: sl(w, g.ovq(s), n),
            base: self.h.base,
            top: self.h.base.wrapping_add(g.l as i64),
            tick: g.tick,
            bid,
            phase: 0,
            oi: if bid { n as isize - 1 } else { 0 },
            k,
            word,
        }
    }

    /// Number of visible levels of side index `s`.
    pub(crate) fn count_ix(&self, s: usize) -> usize {
        bitmap::count(sl(self.words(), self.g.occ(s), self.g.nw)) + usize::from(self.h.ovf_len[s])
    }

    /// Keyframe: bids (price descending) then asks (ascending), overflow included. When `out`
    /// cannot hold both sides each keeps its innermost `out.len() / 2` levels and `truncated` is
    /// `true`. Returns `(n_bids, n_asks, truncated)`.
    pub fn export(&self, out: &mut [Level]) -> (u16, u16, bool) {
        let (nb, na) = (self.count_ix(BID), self.count_ix(1));
        let (kb, ka, trunc) = if nb + na <= out.len() {
            (nb, na, false)
        } else {
            let k = out.len() / 2;
            (nb.min(k), na.min(k), true)
        };
        let (kb, ka) = (kb.min(usize::from(u16::MAX)), ka.min(usize::from(u16::MAX)));
        let trunc = trunc || kb < nb || ka < na;
        let (ob, oa) = out.split_at_mut(kb.min(out.len()));
        for (o, l) in ob.iter_mut().zip(self.iter_ix(BID)) {
            *o = l;
        }
        for (o, l) in oa.iter_mut().take(ka).zip(self.iter_ix(1)) {
            *o = l;
        }
        (kb as u16, ka as u16, trunc)
    }

    /// xxh3-128 over the state, `last_id` and every visible level (window layout excluded).
    pub fn state_hash(&self) -> u128 {
        let mut x = Xxh3::new();
        x.update(&[self.h.state as u8]);
        x.update(&self.h.last_id.to_le_bytes());
        for s in [BID, 1] {
            let mut n: u64 = 0;
            for l in self.iter_ix(s) {
                x.update(&l.px.0.to_le_bytes());
                x.update(&l.qty.0.to_le_bytes());
                n += 1;
            }
            x.update(&n.to_le_bytes());
        }
        x.digest128()
    }
}
