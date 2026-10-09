//! i128 fixed-point helpers: powers of ten, rounding division and exponent changes. Every
//! function saturates instead of overflowing.

use hft_types::RoundMode;

/// Common exponent of the margin arithmetic (quote units × 10^MARGIN_EXP).
pub(crate) const MARGIN_EXP: i32 = -16;
/// `Rate` exponent.
pub(crate) const RATE_ONE: i128 = 1_000_000_000;
/// `Notional` exponent.
pub(crate) const NOTIONAL_EXP: i32 = -6;

/// 10^n for n ∈ [0, 38]; larger n saturates at `i128::MAX`.
pub(crate) const fn pow10(n: u32) -> i128 {
    if n > 38 {
        return i128::MAX;
    }
    let mut r: i128 = 1;
    let mut i = 0;
    while i < n {
        r *= 10;
        i += 1;
    }
    r
}

/// `n / d` rounded by `m`; `d ≤ 0` → 0.
pub(crate) fn div_round(n: i128, d: i128, m: RoundMode) -> i128 {
    if d <= 0 {
        return 0;
    }
    let q = n.div_euclid(d);
    let r = n.rem_euclid(d);
    if r == 0 {
        return q;
    }
    match m {
        RoundMode::Floor => q,
        RoundMode::Ceil => q.saturating_add(1),
        RoundMode::TowardZero => {
            if n < 0 {
                q.saturating_add(1)
            } else {
                q
            }
        }
        RoundMode::HalfEven => {
            let twice = r.saturating_mul(2);
            if twice > d || (twice == d && q.rem_euclid(2) == 1) {
                q.saturating_add(1)
            } else {
                q
            }
        }
    }
}

/// `v` at exponent `from` expressed at exponent `to` (rounded by `m` when digits are dropped).
pub(crate) fn rescale(v: i128, from: i32, to: i32, m: RoundMode) -> i128 {
    if to <= from {
        v.saturating_mul(pow10(from.abs_diff(to)))
    } else {
        div_round(v, pow10(to.abs_diff(from)), m)
    }
}

/// Clamp into i64.
pub(crate) fn sat64(v: i128) -> i64 {
    i64::try_from(v).unwrap_or(if v < 0 { i64::MIN } else { i64::MAX })
}

/// Floor of `v` to a multiple of `step` (`step ≤ 0` → `v`).
pub(crate) fn floor_to(v: i128, step: i64) -> i128 {
    if step <= 0 {
        return v;
    }
    v.div_euclid(i128::from(step)).saturating_mul(i128::from(step))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding() {
        assert_eq!(div_round(7, 2, RoundMode::Floor), 3);
        assert_eq!(div_round(7, 2, RoundMode::Ceil), 4);
        assert_eq!(div_round(-7, 2, RoundMode::Floor), -4);
        assert_eq!(div_round(-7, 2, RoundMode::Ceil), -3);
        assert_eq!(div_round(5, 2, RoundMode::HalfEven), 2);
        assert_eq!(div_round(7, 2, RoundMode::HalfEven), 4);
        assert_eq!(rescale(15, -2, -1, RoundMode::Floor), 1);
        assert_eq!(rescale(15, -1, -3, RoundMode::Floor), 1500);
    }
}
