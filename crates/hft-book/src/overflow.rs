//! Overflow area of one side: levels outside the window in a sorted fixed-capacity array
//! (`ovp` prices as absolute tick indices, ascending; `ovq` quantities). When full, the level
//! farthest from the window centre (`base + W`; equal distance → the worse price) among the
//! stored ones and the new one is merged into `far_qty` and no longer visible.

use crate::l2::{BID, Core, rd, sl, sl_mut, wr};

/// `a` is evicted before `b` (farther from centre `c`; tie → worse price for side `s`).
#[inline]
pub(crate) fn worse(s: usize, c: i64, a: i64, b: i64) -> bool {
    let (da, db) = (a.abs_diff(c), b.abs_diff(c));
    da > db || (da == db && if s == BID { a < b } else { a > b })
}

#[inline]
fn ins(arr: &mut [u64], n: usize, p: usize, v: i64) {
    if p < n {
        arr.copy_within(p..n, p + 1);
    }
    wr(arr, p, v);
}

#[inline]
fn del(arr: &mut [u64], n: usize, p: usize) {
    if p + 1 < n {
        arr.copy_within(p + 1..n, p);
    }
}

impl Core<'_> {
    /// Position of `abs` in the sorted overflow prices of side `s`.
    #[inline]
    pub(crate) fn ovf_find(&self, s: usize, abs: i64) -> Result<usize, usize> {
        let n = usize::from(self.h.ovf_len[s]);
        sl(self.w, self.g.ovp(s), n).binary_search_by(|x| (*x as i64).cmp(&abs))
    }

    #[inline]
    pub(crate) fn ovf_insert(&mut self, s: usize, p: usize, abs: i64, q: i64) {
        let (g, n) = (self.g, usize::from(self.h.ovf_len[s]));
        ins(sl_mut(self.w, g.ovp(s), g.cap), n, p, abs);
        ins(sl_mut(self.w, g.ovq(s), g.cap), n, p, q);
        self.h.ovf_len[s] += 1;
    }

    #[inline]
    pub(crate) fn ovf_remove(&mut self, s: usize, p: usize) {
        let (g, n) = (self.g, usize::from(self.h.ovf_len[s]));
        del(sl_mut(self.w, g.ovp(s), g.cap), n, p);
        del(sl_mut(self.w, g.ovq(s), g.cap), n, p);
        self.h.ovf_len[s] -= 1;
    }

    /// Insert/update/delete an out-of-window level; `true` when a level was truncated.
    pub(crate) fn put_ovf(&mut self, s: usize, abs: i64, q: i64, c: i64) -> bool {
        let g = self.g;
        match self.ovf_find(s, abs) {
            Ok(p) => {
                if q != 0 {
                    wr(self.w, g.ovq(s) + p, q);
                } else {
                    self.ovf_remove(s, p);
                }
                false
            }
            Err(p) => {
                if q == 0 {
                    return false;
                }
                let n = usize::from(self.h.ovf_len[s]);
                if n < g.cap {
                    self.ovf_insert(s, p, abs, q);
                    return false;
                }
                let lo = rd(self.w, g.ovp(s));
                let hi = rd(self.w, g.ovp(s) + n - 1);
                let (r, far) = if worse(s, c, lo, hi) { (0, lo) } else { (n - 1, hi) };
                self.h.truncated = self.h.truncated.saturating_add(1);
                if worse(s, c, abs, far) {
                    self.h.far_qty[s] = self.h.far_qty[s].saturating_add(q);
                    return true;
                }
                let fq = rd(self.w, g.ovq(s) + r);
                self.h.far_qty[s] = self.h.far_qty[s].saturating_add(fq);
                self.ovf_remove(s, r);
                let p = if r < p { p - 1 } else { p };
                self.ovf_insert(s, p, abs, q);
                true
            }
        }
    }
}
