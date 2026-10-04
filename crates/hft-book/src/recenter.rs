//! Re-centering (README "重新居中"): after a message, when the mid tick index leaves
//! `[base + W/2, base + 3W/2)` or a side's best is only in the overflow area, the window moves to
//! `base' = m − W`. Overflow levels that land in the new window move in; window levels that fall
//! out go to the overflow area (nearest to the new window first; the area keeps the `cap`
//! levels nearest to the new centre). O(L), rare.

use crate::Outcome;
use crate::bitmap;
use crate::l2::{ASK, BID, Core, mid_floor, rd, sl, sl_mut, wr};

impl Core<'_> {
    pub(crate) fn recenter_check(&mut self) -> Outcome {
        let (bb, ba) = (self.best(BID), self.best(ASK));
        let m = match (bb, ba) {
            (Some(b), Some(a)) => mid_floor(b.0, a.0),
            (Some(b), None) => b.0,
            (None, Some(a)) => a.0,
            (None, None) => return Outcome::NONE,
        };
        let (w, l, base) = (self.g.w, self.g.l as i64, self.h.base);
        let rel = m.wrapping_sub(base);
        let out_band = rel < w / 2 || rel >= 3 * w / 2;
        let in_ovf = |b: Option<(i64, i64)>| {
            b.is_some_and(|(i, _)| {
                let r = i.wrapping_sub(base);
                !(0..l).contains(&r)
            })
        };
        if !(out_band || in_ovf(bb) || in_ovf(ba)) {
            return Outcome::NONE;
        }
        let nb = m.wrapping_sub(w);
        if nb == base {
            return Outcome::NONE;
        }
        self.recenter_to(nb)
    }

    pub(crate) fn recenter_to(&mut self, nb: i64) -> Outcome {
        let g = self.g;
        let ob = self.h.base;
        let l = g.l;
        let li = l as i64;
        let c = g.center(nb);
        let d = nb.wrapping_sub(ob);
        let mut trunc = false;
        for s in [BID, ASK] {
            // 1. Overflow levels inside the new window → stash.
            let n = usize::from(self.h.ovf_len[s]);
            let ovp = sl(self.w, g.ovp(s), n);
            let lo = ovp.partition_point(|x| (*x as i64) < nb);
            let hi = ovp.partition_point(|x| (*x as i64) < nb.wrapping_add(li));
            let k = hi - lo;
            for j in 0..k {
                let p = rd(self.w, g.ovp(s) + lo + j);
                let q = rd(self.w, g.ovq(s) + lo + j);
                wr(self.w, g.stash_p() + j, p);
                wr(self.w, g.stash_q() + j, q);
            }
            if k > 0 {
                sl_mut(self.w, g.ovp(s), g.cap).copy_within(hi..n, lo);
                sl_mut(self.w, g.ovq(s), g.cap).copy_within(hi..n, lo);
                self.h.ovf_len[s] -= k as u16;
            }
            // 2. Old window levels outside the new window → overflow, nearest first.
            if d > 0 {
                let end = if d >= li { l } else { d as usize };
                let mut cur = end;
                while cur > 0 {
                    let Some(r) = bitmap::highest_at_or_below(sl(self.w, g.occ(s), g.nw), cur - 1) else {
                        break;
                    };
                    let q = rd(self.w, g.qty(s) + r);
                    trunc |= self.put_ovf(s, ob.wrapping_add(r as i64), q, c);
                    cur = r;
                }
            } else {
                let start = if -d >= li { 0 } else { (li + d) as usize };
                let mut cur = start;
                while cur < l {
                    let Some(r) = bitmap::lowest_at_or_above(sl(self.w, g.occ(s), g.nw), cur) else {
                        break;
                    };
                    let q = rd(self.w, g.qty(s) + r);
                    trunc |= self.put_ovf(s, ob.wrapping_add(r as i64), q, c);
                    cur = r + 1;
                }
            }
            // 3. Shift the quantity array.
            let qa = sl_mut(self.w, g.qty(s), l);
            let ad = d.unsigned_abs();
            if ad >= l as u64 {
                qa.fill(0);
            } else if d > 0 {
                let dd = ad as usize;
                qa.copy_within(dd.., 0);
                if let Some(t) = qa.get_mut(l - dd..) {
                    t.fill(0);
                }
            } else {
                let dd = ad as usize;
                qa.copy_within(..l - dd, dd);
                if let Some(t) = qa.get_mut(..dd) {
                    t.fill(0);
                }
            }
            // 4. Stash → window.
            for j in 0..k {
                let p = rd(self.w, g.stash_p() + j);
                let q = rd(self.w, g.stash_q() + j);
                wr(self.w, g.qty(s) + p.wrapping_sub(nb) as usize, q);
            }
            // 5. Rebuild the bitmap and the window best.
            for kw in 0..g.nw {
                let mut word = 0u64;
                for (b, q) in sl(self.w, g.qty(s) + kw * 64, 64).iter().enumerate() {
                    word |= u64::from(*q != 0) << b;
                }
                wr(self.w, g.occ(s) + kw, word as i64);
            }
            let occ = sl(self.w, g.occ(s), g.nw);
            let b = if s == BID {
                bitmap::highest_at_or_below(occ, l - 1)
            } else {
                bitmap::lowest_at_or_above(occ, 0)
            };
            self.h.best[s] = b.map_or(-1, |x| x as i32);
            self.h.dirty[s] = false;
        }
        self.h.base = nb;
        if trunc {
            Outcome::RECENTERED | Outcome::TRUNCATED
        } else {
            Outcome::RECENTERED
        }
    }
}
