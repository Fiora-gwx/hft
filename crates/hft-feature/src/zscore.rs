//! `zsum` (README "节点库"): composite z-score of `N` features normalized by caller-given means and
//! standard deviations, z = ⌊Σ_i ⌊sign_i·(x_i − mu_i)·1e9 / sd_i⌋ · 1e9 / `sqrt_n_nano`⌋
//! (`Rate`). A missing feature (or one with sd ≤ 0) adds 0.

use crate::fixed::ONE;

/// `sqrt_n_nano` = √N × 1e9 of the caller (exact constant, so ports reproduce it).
pub fn zsum<const N: usize>(
    f: &[Option<i64>; N],
    mu: &[i64; N],
    sd: &[i64; N],
    sign: &[i128; N],
    sqrt_n_nano: i128,
) -> i64 {
    let one = i128::from(ONE);
    let mut sum = 0i128;
    for k in 0..N {
        let (Some(Some(x)), Some(m), Some(d), Some(sg)) = (f.get(k), mu.get(k), sd.get(k), sign.get(k))
        else {
            continue;
        };
        if *d <= 0 {
            continue;
        }
        sum += sg * (i128::from(*x) - i128::from(*m)) * one / i128::from(*d);
    }
    if sqrt_n_nano <= 0 {
        return 0;
    }
    (sum * one / sqrt_n_nano).clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}
