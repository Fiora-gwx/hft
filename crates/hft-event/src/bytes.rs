//! Little-endian field access on byte slices that never panics (out of range reads 0 / writes
//! nothing); the crate denies `clippy::indexing_slicing`.

#[inline]
pub(crate) fn u8_at(b: &[u8], o: usize) -> u8 {
    b.get(o).copied().unwrap_or(0)
}

#[inline]
pub(crate) fn u16_at(b: &[u8], o: usize) -> u16 {
    b.get(o..o.saturating_add(2))
        .and_then(|s| s.try_into().ok())
        .map_or(0, u16::from_le_bytes)
}

#[inline]
pub(crate) fn u32_at(b: &[u8], o: usize) -> u32 {
    b.get(o..o.saturating_add(4))
        .and_then(|s| s.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

#[inline]
pub(crate) fn u64_at(b: &[u8], o: usize) -> u64 {
    b.get(o..o.saturating_add(8))
        .and_then(|s| s.try_into().ok())
        .map_or(0, u64::from_le_bytes)
}

#[inline]
pub(crate) fn put(b: &mut [u8], o: usize, v: &[u8]) {
    if let Some(d) = b.get_mut(o..o.saturating_add(v.len())) {
        d.copy_from_slice(v);
    }
}

/// `x` rounded up to a multiple of 8.
#[inline]
pub(crate) const fn round8(x: usize) -> usize {
    x.saturating_add(7) & !7
}
