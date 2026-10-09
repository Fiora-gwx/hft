//! Estimated liquidation distance of one perp short with equity ratio e = 0.30 and maintenance
//! rate m = 0.5% after a one-sided rise y (RSK-171): `(0.295 − 1.005·y) / (1.005·(1 + y))`,
//! as a Rate rounded toward zero.

/// `y` as Rate raw (1e-9) → distance as Rate raw.
pub fn est_liq_dist(y: i64) -> i64 {
    let y = i128::from(y);
    let num = (295_000_000_000i128 - 1_005 * y) * 1_000_000_000;
    let den = 1_005 * (1_000_000_000 + y);
    (num / den) as i64
}
