//! Unit helpers (README "单位约定"): millisecond parameters → engine_ns or TSC intervals, and
//! `Rate` constants (1e-9: 1 bp = 100 000, 1 % = 10 000 000).

use hft_types::Rate;

/// One basis point as a `Rate`.
pub const BP: i64 = 100_000;
/// One percent as a `Rate`.
pub const PCT: i64 = 10_000_000;
/// 100 % as a `Rate` (the 1e9 scale).
pub const ONE: i64 = 1_000_000_000;

/// `n` basis points as a `Rate` (saturating).
#[inline]
pub const fn bp(n: i64) -> Rate {
    Rate(n.saturating_mul(BP))
}

/// `n` percent as a `Rate` (saturating).
#[inline]
pub const fn pct(n: i64) -> Rate {
    Rate(n.saturating_mul(PCT))
}

/// Milliseconds → engine_ns interval (`× 1 000 000`; a `u32` never overflows).
#[inline]
pub const fn ms_to_ns(ms: u32) -> i64 {
    (ms as i64) * 1_000_000
}

/// Milliseconds → TSC ticks of this run (`× tsc_per_ms`, saturating).
#[inline]
pub const fn ms_to_tsc(ms: u32, tsc_per_ms: u64) -> u64 {
    (ms as u64).saturating_mul(tsc_per_ms)
}

/// `t + d` saturating at `i64::MAX`/`i64::MIN` (deadline arithmetic of the stateful components).
#[inline]
pub const fn deadline(t: i64, d: i64) -> i64 {
    t.saturating_add(d)
}
