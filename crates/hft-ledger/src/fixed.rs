//! `FixedVec` (inline fixed-capacity vector) and the i128 fixed-point helpers of the ledger
//! (README "数据结构": conversions truncate towards zero, fees round expense up / income down).

use hft_types::RoundMode;

/// Inline vector of at most `N` elements; `push` beyond `N` is refused (returns `false`), never
/// allocates. Used for caller-provided output buffers and the per-venue tables.
#[derive(Clone, Copy)]
pub struct FixedVec<T: Copy, const N: usize> {
    len: usize,
    buf: [Option<T>; N],
}

impl<T: Copy + core::fmt::Debug, const N: usize> core::fmt::Debug for FixedVec<T, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T: Copy + PartialEq, const N: usize> PartialEq for FixedVec<T, N> {
    fn eq(&self, o: &Self) -> bool {
        self.len == o.len && self.iter().zip(o.iter()).all(|(a, b)| a == b)
    }
}

impl<T: Copy + Eq, const N: usize> Eq for FixedVec<T, N> {}

impl<T: Copy, const N: usize> Default for FixedVec<T, N> {
    fn default() -> Self {
        FixedVec {
            len: 0,
            buf: [None; N],
        }
    }
}

impl<T: Copy, const N: usize> FixedVec<T, N> {
    /// Empty vector.
    pub const fn new() -> Self {
        FixedVec {
            len: 0,
            buf: [None; N],
        }
    }

    pub const CAPACITY: usize = N;

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len >= N
    }

    pub fn clear(&mut self) {
        for x in self.buf.iter_mut().take(self.len) {
            *x = None;
        }
        self.len = 0;
    }

    /// Appends `v`; `false` (and nothing stored) when full.
    pub fn push(&mut self, v: T) -> bool {
        match self.buf.get_mut(self.len) {
            Some(s) => {
                *s = Some(v);
                self.len += 1;
                true
            }
            None => false,
        }
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        if i < self.len {
            self.buf.get(i).and_then(|x| x.as_ref())
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        if i < self.len {
            self.buf.get_mut(i).and_then(|x| x.as_mut())
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buf.iter().take(self.len).filter_map(|x| x.as_ref())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        let n = self.len;
        self.buf.iter_mut().take(n).filter_map(|x| x.as_mut())
    }

    /// Removes element `i`, shifting the tail left (order kept).
    pub fn remove(&mut self, i: usize) -> Option<T> {
        if i >= self.len {
            return None;
        }
        let out = self.buf.get(i).copied().flatten();
        let mut k = i;
        while k + 1 < self.len {
            let next = self.buf.get(k + 1).copied().flatten();
            if let Some(s) = self.buf.get_mut(k) {
                *s = next;
            }
            k += 1;
        }
        if let Some(s) = self.buf.get_mut(self.len - 1) {
            *s = None;
        }
        self.len -= 1;
        out
    }

    /// Inserts `v` at `i` (`i ≤ len`), shifting the tail right; `false` when full.
    pub fn insert(&mut self, i: usize, v: T) -> bool {
        if self.len >= N || i > self.len {
            return false;
        }
        let mut k = self.len;
        while k > i {
            let prev = self.buf.get(k - 1).copied().flatten();
            if let Some(s) = self.buf.get_mut(k) {
                *s = prev;
            }
            k -= 1;
        }
        if let Some(s) = self.buf.get_mut(i) {
            *s = Some(v);
        }
        self.len += 1;
        true
    }

    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }

    pub fn position(&self, f: impl Fn(&T) -> bool) -> Option<usize> {
        self.iter().position(f)
    }
}

/// `10^k` for `k ∈ 0..=38`.
pub(crate) const fn pow10(k: u32) -> i128 {
    let mut v: i128 = 1;
    let mut i = 0;
    while i < k && i < 38 {
        v *= 10;
        i += 1;
    }
    v
}

/// `n / d` rounded by `m`; `d = 0` → 0.
pub(crate) fn div_round(n: i128, d: i128, m: RoundMode) -> i128 {
    if d == 0 {
        return 0;
    }
    let (n, d) = if d < 0 {
        (n.saturating_neg(), d.saturating_neg())
    } else {
        (n, d)
    };
    let q = n.div_euclid(d);
    let r = n.rem_euclid(d);
    if r == 0 {
        return q;
    }
    let up = match m {
        RoundMode::Floor => false,
        RoundMode::Ceil => true,
        RoundMode::TowardZero => n < 0,
        RoundMode::HalfEven => match r.cmp(&(d - r)) {
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Equal => q & 1 != 0,
        },
    };
    if up { q.saturating_add(1) } else { q }
}

/// Rescales `v` from exponent `from` to exponent `to`, rounding by `m`; returns the value and
/// whether digits were dropped.
pub(crate) fn rescale(v: i128, from: i32, to: i32, m: RoundMode) -> (i128, bool) {
    if from >= to {
        let k = (from - to) as u32;
        (v.saturating_mul(pow10(k)), false)
    } else {
        let k = (to - from) as u32;
        let p = pow10(k);
        let q = div_round(v, p, m);
        (q, q.saturating_mul(p) != v)
    }
}

/// i128 → i64 saturating.
pub(crate) fn sat64(v: i128) -> i64 {
    if v > i64::MAX as i128 {
        i64::MAX
    } else if v < i64::MIN as i128 {
        i64::MIN
    } else {
        v as i64
    }
}

/// Little-endian `[u64; 2]` of an i128 (snapshot rows).
pub(crate) fn i128_words(v: i128) -> [u64; 2] {
    let u = v as u128;
    [u as u64, (u >> 64) as u64]
}

pub(crate) fn words_i128(w: [u64; 2]) -> i128 {
    ((w[1] as u128) << 64 | w[0] as u128) as i128
}

/// Deterministic 64-bit mixer (splitmix64 finaliser) for the open-addressing indexes.
pub(crate) fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
