//! Occupancy bitmap of one window side: bit `i` set ⟺ `qty[i] != 0`. Searches walk 64 levels
//! per step and are bounded by the bitmap length (L/64 words).

#[inline]
pub(crate) fn set(occ: &mut [u64], i: usize, on: bool) {
    if let Some(w) = occ.get_mut(i / 64) {
        let m = 1u64 << (i % 64);
        if on {
            *w |= m;
        } else {
            *w &= !m;
        }
    }
}

/// Highest set bit at or below `i`.
#[inline]
pub(crate) fn highest_at_or_below(occ: &[u64], i: usize) -> Option<usize> {
    let mut k = i / 64;
    let bit = i % 64;
    let mut word = occ.get(k).copied().unwrap_or(0) & (u64::MAX >> (63 - bit));
    loop {
        if word != 0 {
            return Some(k * 64 + 63 - word.leading_zeros() as usize);
        }
        if k == 0 {
            return None;
        }
        k -= 1;
        word = occ.get(k).copied().unwrap_or(0);
    }
}

/// Lowest set bit at or above `i`.
#[inline]
pub(crate) fn lowest_at_or_above(occ: &[u64], i: usize) -> Option<usize> {
    let mut k = i / 64;
    let bit = i % 64;
    let n = occ.len();
    let mut word = occ.get(k).copied().unwrap_or(0) & (u64::MAX << bit);
    loop {
        if word != 0 {
            return Some(k * 64 + word.trailing_zeros() as usize);
        }
        k += 1;
        if k >= n {
            return None;
        }
        word = occ.get(k).copied().unwrap_or(0);
    }
}

/// Number of set bits.
#[inline]
pub(crate) fn count(occ: &[u64]) -> usize {
    occ.iter().map(|w| w.count_ones() as usize).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searches_cross_words() {
        let mut occ = [0u64; 4];
        set(&mut occ, 3, true);
        set(&mut occ, 130, true);
        assert_eq!(highest_at_or_below(&occ, 255), Some(130));
        assert_eq!(highest_at_or_below(&occ, 129), Some(3));
        assert_eq!(highest_at_or_below(&occ, 2), None);
        assert_eq!(lowest_at_or_above(&occ, 0), Some(3));
        assert_eq!(lowest_at_or_above(&occ, 4), Some(130));
        assert_eq!(lowest_at_or_above(&occ, 131), None);
        assert_eq!(count(&occ), 2);
        set(&mut occ, 130, false);
        assert_eq!(lowest_at_or_above(&occ, 4), None);
    }
}
