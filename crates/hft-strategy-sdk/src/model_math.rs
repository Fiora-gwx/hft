//! The only floating-point module of the SDK (README "确定性规则"): transcendental functions from
//! the pure-Rust `libm` (bit-identical across platforms) and the single f64 → fixed-point
//! boundary (`to_*`: HalfEven, saturating, NaN/±∞ → `None`). Only a strategy's `src/model/` may
//! use it (`cargo xtask hot-check` N3); `Advice` payloads never contain floats.
#![allow(clippy::float_arithmetic)]

/// Natural logarithm.
#[inline]
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// e^x.
#[inline]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

/// x^y.
#[inline]
pub fn powf(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

/// Base-10 logarithm.
#[inline]
pub fn log10(x: f64) -> f64 {
    libm::log10(x)
}

/// Square root (IEEE correctly rounded).
#[inline]
pub fn sqrt(x: f64) -> f64 {
    libm::sqrt(x)
}

/// Exact powers of ten representable in f64 (10^0 ..= 10^22).
const POW10: [f64; 23] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18,
    1e19, 1e20, 1e21, 1e22,
];

/// HalfEven(x) as i64, saturating to `i64::MIN`/`i64::MAX`; NaN and ±∞ → `None`.
#[inline]
pub fn round_even(x: f64) -> Option<i64> {
    if !x.is_finite() {
        return None;
    }
    let r = x.round_ties_even();
    // 2^63 is exactly representable; every finite f64 ≥ 2^63 saturates.
    Some(if r >= 9_223_372_036_854_775_808.0 {
        i64::MAX
    } else if r < -9_223_372_036_854_775_808.0 {
        i64::MIN
    } else {
        r as i64
    })
}

/// HalfEven(x × 10^k) (product in f64), `k` in `-22..=22`; outside → `None`.
#[inline]
pub fn to_scaled(x: f64, k: i32) -> Option<i64> {
    let p = *POW10.get(usize::try_from(k.unsigned_abs()).ok()?)?;
    round_even(if k >= 0 { x * p } else { x / p })
}

/// `Rate` raw value: HalfEven(x × 10^9).
#[inline]
pub fn to_rate(x: f64) -> Option<i64> {
    to_scaled(x, 9)
}

/// `Notional` raw value: HalfEven(x × 10^6).
#[inline]
pub fn to_notional(x: f64) -> Option<i64> {
    to_scaled(x, 6)
}

/// `Px` raw value at the asset's `px_exp`: HalfEven(x × 10^(−px_exp)).
#[inline]
pub fn to_px(x: f64, px_exp: i8) -> Option<i64> {
    to_scaled(x, -i32::from(px_exp))
}

/// `Qty` raw value at the asset's `qty_exp`: HalfEven(x × 10^(−qty_exp)).
#[inline]
pub fn to_qty(x: f64, qty_exp: i8) -> Option<i64> {
    to_scaled(x, -i32::from(qty_exp))
}
