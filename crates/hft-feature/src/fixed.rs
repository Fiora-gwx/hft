//! Fixed-point arithmetic shared by the nodes (README "定点约定"): `Rate` = 1e-9, integer square
//! root, logarithms by the atanh series, ratio imbalance and linear-interpolation quantiles. Every
//! formula is exact integer arithmetic (truncation as written), so an offline port reproduces it
//! bit for bit.

/// 1e9 (`Rate` one).
pub const ONE: i64 = 1_000_000_000;
/// ln 2 × 1e9.
pub const LN2_NANO: i128 = 693_147_181;

const ONE128: i128 = ONE as i128;

/// Floor square root.
pub fn isqrt(v: u128) -> u128 {
    if v < 2 {
        return v;
    }
    let mut x = v;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + v / x) / 2;
    }
    x
}

/// 2·atanh(u / 1e9) × 1e9 = ln((1 + u)/(1 − u)) for |u| < 1e9: 2·Σ_{j=0}^{11} u^(2j+1) / (2j+1),
/// with u² = ⌊u·u / 1e9⌋, every power ⌊power·u² / 1e9⌋ and every term ⌊power / (2j+1)⌋
/// (truncation toward zero).
pub fn atanh2(u: i128) -> i128 {
    let u2 = u * u / ONE128;
    let mut p = u;
    let mut sum = 0i128;
    for j in 0..12i128 {
        sum += p / (2 * j + 1);
        p = p * u2 / ONE128;
    }
    2 * sum
}

/// ln(a / b) × 1e9 for a, b > 0: `atanh2(⌊(a − b)·1e9 / (a + b)⌋)`; 0 otherwise.
pub fn ln_ratio(a: i128, b: i128) -> i128 {
    if a <= 0 || b <= 0 {
        return 0;
    }
    atanh2((a - b) * ONE128 / (a + b))
}

/// ln(x / 1e9) × 1e9 for x ≥ 1e9: halve x (⌊x / 2⌋) k times until x < 2e9, then
/// k·`LN2_NANO` + `ln_ratio(x, 1e9)`; 0 for x < 1e9.
pub fn ln_rate(x: i128) -> i128 {
    if x < ONE128 {
        return 0;
    }
    let mut m = x;
    let mut k = 0i128;
    while m >= 2 * ONE128 {
        m /= 2;
        k += 1;
    }
    k * LN2_NANO + ln_ratio(m, ONE128)
}

/// (a − b)·1e9 / (a + b) (`Rate`, truncated), 0 when a + b ≤ 0.
pub fn imbalance(a: i64, b: i64) -> i64 {
    let (a, b) = (i128::from(a), i128::from(b));
    if a + b <= 0 {
        return 0;
    }
    ((a - b) * ONE128 / (a + b)) as i64
}

/// Linear-interpolation quantile `q10 / 10` of the sorted `v` (pandas' default), ×10: with
/// h = q10·(n − 1), lo = ⌊h / 10⌋, fr = h mod 10: 10·v[lo] + fr·(v[lo + 1] − v[lo]).
pub fn quantile_x10(v: &[i64], q10: i64) -> Option<i64> {
    let n = i64::try_from(v.len()).ok().filter(|n| *n > 0)?;
    let h = q10 * (n - 1);
    let lo = usize::try_from(h / 10).ok()?;
    let fr = h % 10;
    let a = *v.get(lo)?;
    let b = if fr > 0 { *v.get(lo + 1)? } else { a };
    Some(
        a.saturating_mul(10)
            .saturating_add(fr.saturating_mul(b.saturating_sub(a))),
    )
}
