//! Fixed-capacity containers: `FixedVec<T, N>` (inline, compile-time capacity) and `PreVec<T>`
//! (heap, capacity given once at start-up). Neither ever reallocates; a push beyond capacity is
//! refused (`false`) and is a bug of the caller's capacity bound (hot-path-rules M1).

/// Inline vector of at most `N` `Copy` elements.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct FixedVec<T: Copy, const N: usize> {
    buf: [Option<T>; N],
    len: usize,
}

impl<T: Copy, const N: usize> Default for FixedVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const N: usize> FixedVec<T, N> {
    pub const fn new() -> Self {
        FixedVec {
            buf: [None; N],
            len: 0,
        }
    }
    /// Appends `v`; `false` (nothing stored) when full.
    #[inline]
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
    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn capacity(&self) -> usize {
        N
    }
    #[inline]
    pub fn get(&self, i: usize) -> Option<&T> {
        if i < self.len {
            self.buf.get(i).and_then(Option::as_ref)
        } else {
            None
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.buf.iter().take(self.len).filter_map(Option::as_ref)
    }
    /// Copies the elements into a `Vec` (tests and cold callers).
    pub fn to_vec(&self) -> Vec<T> {
        self.iter().copied().collect()
    }
}

/// Heap vector whose capacity is fixed at construction; never reallocates.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct PreVec<T> {
    v: Vec<T>,
    cap: usize,
}

impl<T> PreVec<T> {
    /// One allocation of `cap` elements.
    pub fn with_capacity(cap: usize) -> Self {
        PreVec {
            v: Vec::with_capacity(cap),
            cap,
        }
    }
    /// Takes `v` as is; its capacity bound is `max(v.len(), v.capacity())`.
    pub fn from_vec(v: Vec<T>) -> Self {
        let cap = v.capacity().max(v.len());
        PreVec { v, cap }
    }
    /// Appends `x`; `false` (nothing stored) when full.
    #[inline]
    pub fn push(&mut self, x: T) -> bool {
        if self.v.len() >= self.cap {
            return false;
        }
        self.v.push(x);
        true
    }
    #[inline]
    pub fn clear(&mut self) {
        self.v.clear();
    }
    #[inline]
    pub fn len(&self) -> usize {
        self.v.len()
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.v.is_empty()
    }
    #[inline]
    pub fn capacity(&self) -> usize {
        self.cap
    }
    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.v
    }
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.v
    }
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.v.iter()
    }
    #[inline]
    pub fn get(&self, i: usize) -> Option<&T> {
        self.v.get(i)
    }
    #[inline]
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.v.get_mut(i)
    }
    /// Inserts at `i` (≤ len) shifting the tail; `false` when full or `i` out of range.
    pub fn insert(&mut self, i: usize, x: T) -> bool {
        if self.v.len() >= self.cap || i > self.v.len() {
            return false;
        }
        self.v.insert(i, x);
        true
    }
    pub fn remove(&mut self, i: usize) -> Option<T> {
        (i < self.v.len()).then(|| self.v.remove(i))
    }
    pub fn retain(&mut self, f: impl FnMut(&T) -> bool) {
        self.v.retain(f);
    }
    pub fn truncate(&mut self, n: usize) {
        self.v.truncate(n);
    }
}

impl<T: Clone> PreVec<T> {
    /// Replaces the contents by `src` without reallocating; `false` (unchanged) when `src` is
    /// longer than the capacity.
    pub fn assign(&mut self, src: &[T]) -> bool {
        if src.len() > self.cap {
            return false;
        }
        self.v.clear();
        self.v.extend_from_slice(src);
        true
    }
}

impl<'a, T> IntoIterator for &'a PreVec<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.v.iter()
    }
}
