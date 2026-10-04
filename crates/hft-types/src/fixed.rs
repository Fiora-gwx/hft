//! Fixed-point arithmetic: products, ratios, rounding to tick/lot, `pair_lot`,
//! `cap_cross_px` and `funding_settle_norm_ns`.
//!
//! Every intermediate is i128 with explicit `checked_*`/`wrapping_*` operations; no
//! function panics on any input (README invariant 4).

use crate::err::FxErr;
use crate::types::{AssetExp, HedgePxLimits, MAX_RAW, Notional, Px, Qty, Rate, RoundMode, Side};

/// `10^k` for `k ∈ 0..=19` (u64).
pub(crate) const POW10_U64: [u64; 20] = {
    let mut t = [1u64; 20];
    let mut i = 1;
    while i < 20 {
        t[i] = t[i - 1] * 10;
        i += 1;
    }
    t
};

/// `10^k` for `k ∈ 0..=38` (i128).
pub(crate) const POW10_I128: [i128; 39] = {
    let mut t = [1i128; 39];
    let mut i = 1;
    while i < 39 {
        t[i] = t[i - 1] * 10;
        i += 1;
    }
    t
};

const RATE_ONE: i128 = 1_000_000_000;
const MAX_RAW_I128: i128 = MAX_RAW as i128;
const NEG_MAX_RAW_I128: i128 = (MAX_RAW as i128).wrapping_neg();
const I64_MAX_I128: i128 = i64::MAX as i128;
const NEG_I64_MAX_I128: i128 = (i64::MAX as i128).wrapping_neg();

/// `10^k` as i128, `None` outside `0..=38`.
#[inline]
pub(crate) fn pow10_i128(k: i32) -> Option<i128> {
    usize::try_from(k).ok().and_then(|i| POW10_I128.get(i).copied())
}

/// Asset exponent range `[-18, 6]`.
#[inline]
pub(crate) const fn asset_exp_ok(e: i8) -> bool {
    e >= -18 && e <= 6
}

/// `n / d` rounded by `m` (Floor → −∞, Ceil → +∞, TowardZero truncates, HalfEven ties to even).
/// `None` when `d == 0` or the result does not fit i128.
#[inline]
pub(crate) fn div_round(n: i128, d: i128, m: RoundMode) -> Option<i128> {
    let (n, d) = if d < 0 {
        (n.checked_neg()?, d.checked_neg()?)
    } else {
        (n, d)
    };
    let q = n.checked_div_euclid(d)?;
    let r = n.checked_rem_euclid(d)?; // 0 ≤ r < d
    if r == 0 {
        return Some(q);
    }
    let up = match m {
        RoundMode::Floor => false,
        RoundMode::Ceil => true,
        RoundMode::TowardZero => n < 0,
        RoundMode::HalfEven => match r.cmp(&d.wrapping_sub(r)) {
            core::cmp::Ordering::Less => false,
            core::cmp::Ordering::Greater => true,
            core::cmp::Ordering::Equal => q & 1 != 0,
        },
    };
    if up { q.checked_add(1) } else { Some(q) }
}

/// Clamp to `[-MAX_RAW, MAX_RAW]`.
#[inline]
fn clamp_raw(v: i128) -> i64 {
    if v > MAX_RAW_I128 {
        MAX_RAW
    } else if v < NEG_MAX_RAW_I128 {
        -MAX_RAW
    } else {
        v as i64
    }
}

/// Clamp to `[-i64::MAX, i64::MAX]`, reporting saturation.
#[inline]
fn sat_i64(v: i128) -> (i64, FxErr) {
    if v > I64_MAX_I128 {
        (i64::MAX, FxErr::OVERFLOW)
    } else if v < NEG_I64_MAX_I128 {
        (-i64::MAX, FxErr::OVERFLOW)
    } else {
        (v as i64, FxErr::OK)
    }
}

/// Round `x` to a multiple of `step` (> 0) by `m`; results beyond `±MAX_RAW` saturate to the
/// largest same-sign multiple of `step` whose magnitude is ≤ `MAX_RAW`.
#[inline]
fn round_step(x: i64, step: i64, m: RoundMode) -> i64 {
    if step <= 0 {
        return 0;
    }
    let s = step as i128;
    let lim = MAX_RAW_I128.checked_div(s).unwrap_or(0).wrapping_mul(s);
    let r = div_round(x as i128, s, m).and_then(|q| q.checked_mul(s));
    match r {
        Some(v) if v > lim => lim as i64,
        Some(v) if v < lim.wrapping_neg() => lim.wrapping_neg() as i64,
        Some(v) => v as i64,
        None => 0,
    }
}

impl Px {
    /// `px × q` converted to `Notional` (exponent −6), rounded by `m`; saturates to `±i64::MAX`
    /// with `OVERFLOW`; exponent outside `[-18, 6]` → `(Notional(0), EXP_RANGE)`.
    #[inline]
    pub fn mul_qty(self, q: Qty, e: AssetExp, m: RoundMode) -> (Notional, FxErr) {
        if !asset_exp_ok(e.px_exp) || !asset_exp_ok(e.qty_exp) {
            return (Notional(0), FxErr::EXP_RANGE);
        }
        // |px × q| ≤ 2^126: never overflows i128.
        let prod = (self.0 as i128).wrapping_mul(q.0 as i128);
        let s = (e.px_exp as i32).wrapping_add(e.qty_exp as i32).wrapping_add(6);
        let r = if s >= 0 {
            pow10_i128(s).and_then(|p| prod.checked_mul(p))
        } else {
            pow10_i128(s.wrapping_neg()).and_then(|p| div_round(prod, p, m))
        };
        let (v, err) = match r {
            Some(v) => sat_i64(v),
            None if prod < 0 => (-i64::MAX, FxErr::OVERFLOW),
            None => (i64::MAX, FxErr::OVERFLOW),
        };
        (Notional(v), err)
    }

    /// Average price `n / q`; `q = 0` → `DIV_ZERO`; `|result| > MAX_RAW` → `OVERFLOW`.
    #[inline]
    pub fn from_notional(n: Notional, q: Qty, e: AssetExp, m: RoundMode) -> Result<Px, FxErr> {
        if !asset_exp_ok(e.px_exp) || !asset_exp_ok(e.qty_exp) {
            return Err(FxErr::EXP_RANGE);
        }
        if q.0 == 0 {
            return Err(FxErr::DIV_ZERO);
        }
        // px = n × 10^t / q with t = −6 − px_exp − qty_exp ∈ [−18, 30].
        let t = (-6i32)
            .wrapping_sub(e.px_exp as i32)
            .wrapping_sub(e.qty_exp as i32);
        let (num, den) = if t >= 0 {
            let p = pow10_i128(t).ok_or(FxErr::OVERFLOW)?;
            ((n.0 as i128).checked_mul(p).ok_or(FxErr::OVERFLOW)?, q.0 as i128)
        } else {
            let p = pow10_i128(t.wrapping_neg()).ok_or(FxErr::OVERFLOW)?;
            (n.0 as i128, (q.0 as i128).checked_mul(p).ok_or(FxErr::OVERFLOW)?)
        };
        let r = div_round(num, den, m).ok_or(FxErr::OVERFLOW)?;
        if !(NEG_MAX_RAW_I128..=MAX_RAW_I128).contains(&r) {
            return Err(FxErr::OVERFLOW);
        }
        Ok(Px(r as i64))
    }

    /// `px × (1e9 + r) / 1e9`, rounded once by `m`, saturated to `±MAX_RAW`.
    #[inline]
    pub fn mul_rate(self, r: Rate, m: RoundMode) -> Px {
        let factor = RATE_ONE.wrapping_add(r.0 as i128);
        let v = (self.0 as i128)
            .checked_mul(factor)
            .and_then(|p| div_round(p, RATE_ONE, m));
        match v {
            Some(v) => Px(clamp_raw(v)),
            None if (self.0 < 0) != (factor < 0) => Px(-MAX_RAW),
            None => Px(MAX_RAW),
        }
    }

    /// Round to a multiple of `tick` (asset-exponent integer step); `tick ≤ 0` → `Px(0)`.
    #[inline]
    pub fn round_to_tick(self, tick: i64, m: RoundMode) -> Px {
        Px(round_step(self.0, tick, m))
    }

    /// `(a + b) / 2` rounded towards −∞.
    #[inline]
    pub fn mid_floor(a: Px, b: Px) -> Px {
        let s = (a.0 as i128).wrapping_add(b.0 as i128);
        Px(s.checked_div_euclid(2).map_or(0, |v| v as i64))
    }

    /// `(self − base) / base` as a `Rate`, HalfEven; `base ≤ 0` → `Rate(0)`; saturates to `±i64::MAX`.
    #[inline]
    pub fn ratio(self, base: Px) -> Rate {
        if base.0 <= 0 {
            return Rate(0);
        }
        let diff = (self.0 as i128).wrapping_sub(base.0 as i128);
        let v = diff
            .checked_mul(RATE_ONE)
            .and_then(|n| div_round(n, base.0 as i128, RoundMode::HalfEven));
        match v {
            Some(v) => Rate(sat_i64(v).0),
            None if diff < 0 => Rate(-i64::MAX),
            None => Rate(i64::MAX),
        }
    }
}

impl Qty {
    /// Round to a multiple of `lot`; `lot ≤ 0` → `Qty(0)`.
    #[inline]
    pub fn round_to_lot(self, lot: i64, m: RoundMode) -> Qty {
        Qty(round_step(self.0, lot, m))
    }

    /// `(TowardZero part, remainder)`, summing to `self`; `lot ≤ 0` → `(Qty(0), self)`.
    #[inline]
    pub fn split_lot(self, lot: i64) -> (Qty, Qty) {
        if lot <= 0 {
            return (Qty(0), self);
        }
        // |a| ≤ |self| and a, self share sign: none of these can overflow.
        let a = self.0.checked_div(lot).unwrap_or(0).wrapping_mul(lot);
        (Qty(a), Qty(self.0.wrapping_sub(a)))
    }
}

impl Notional {
    /// `n × r / 1e9` rounded by `m`, saturated to `±i64::MAX`.
    #[inline]
    pub fn mul_rate(self, r: Rate, m: RoundMode) -> Notional {
        // |n × r| < 2^126: never overflows i128.
        let p = (self.0 as i128).wrapping_mul(r.0 as i128);
        Notional(div_round(p, RATE_ONE, m).map_or(0, |v| sat_i64(v).0))
    }
}

/// Binary GCD on u128 (`gcd(0, b) = b`).
fn gcd(mut a: u128, mut b: u128) -> u128 {
    if a == 0 {
        return b;
    }
    if b == 0 {
        return a;
    }
    let shift = (a | b).trailing_zeros();
    a >>= a.trailing_zeros();
    loop {
        b >>= b.trailing_zeros();
        if a > b {
            core::mem::swap(&mut a, &mut b);
        }
        b = b.wrapping_sub(a);
        if b == 0 {
            return a.wrapping_shl(shift);
        }
    }
}

/// Smallest first-leg unit `q` such that `q` is a multiple of `lot_f` and `q × num / den` is a
/// multiple of `lot_h`; any argument ≤ 0 or a result above `MAX_RAW` → `None`.
pub fn pair_lot(lot_f: i64, lot_h: i64, num: u32, den: u32) -> Option<Qty> {
    if lot_f <= 0 || lot_h <= 0 || num == 0 || den == 0 {
        return None;
    }
    let (lot_f, lot_h) = (lot_f as u128, lot_h as u128);
    let m = (den as u128).checked_mul(lot_h)?;
    let step_h = m.checked_div(gcd(m, num as u128))?;
    let r = lot_f.checked_div(gcd(lot_f, step_h))?.checked_mul(step_h)?;
    if r > MAX_RAW as u128 {
        return None;
    }
    Some(Qty(r as i64))
}

/// Engine/hedge order limit price: the tighter of the touch cap and the mid cap, rounded once
/// to `tick` by `side.price_round()`; invalid inputs or a result ≤ 0 → `Px(0)`.
pub fn cap_cross_px(side: Side, touch: Px, mid: Px, max_cross: Rate, lim: &HedgePxLimits, tick: i64) -> Px {
    let rr = lim.rr_max_cross;
    if touch.0 <= 0 || mid.0 <= 0 || max_cross.0 < 0 || rr.0 < 0 || tick <= 0 {
        return Px(0);
    }
    let (t, mi, mc, rr) = (touch.0 as i128, mid.0 as i128, max_cross.0 as i128, rr.0 as i128);
    // Both limits share the denominator 1e9; every product is < 2^126.
    let (a, b) = match side {
        Side::Buy => (
            t.wrapping_mul(RATE_ONE.wrapping_add(mc)),
            mi.wrapping_mul(RATE_ONE.wrapping_add(rr)),
        ),
        Side::Sell => (
            t.wrapping_mul(RATE_ONE.wrapping_sub(mc)),
            mi.wrapping_mul(RATE_ONE.wrapping_sub(rr)),
        ),
    };
    let bound = match side {
        Side::Buy => a.min(b),
        Side::Sell => a.max(b),
    };
    let step = tick as i128;
    let v = RATE_ONE
        .checked_mul(step)
        .and_then(|den| div_round(bound, den, side.price_round()))
        .and_then(|k| k.checked_mul(step));
    let Some(v) = v else {
        return Px(0);
    };
    if v <= 0 {
        return Px(0);
    }
    let cap = MAX_RAW_I128.checked_div(step).unwrap_or(0).wrapping_mul(step);
    let v = v.min(cap);
    if v <= 0 { Px(0) } else { Px(v as i64) }
}

/// Nearest funding boundary `anchor_s + k × interval_s` (UTC seconds) to `ts_ns`; ties go to
/// the earlier boundary; `interval_s = 0` returns `ts_ns`; a boundary outside i64 is replaced
/// by the adjacent in-range one.
pub fn funding_settle_norm_ns(ts_ns: i64, interval_s: u32, anchor_s: u32) -> i64 {
    if interval_s == 0 {
        return ts_ns;
    }
    const NS: i128 = 1_000_000_000;
    let iv = (interval_s as i128).wrapping_mul(NS);
    let an = (anchor_s as i128).wrapping_mul(NS);
    let ts = ts_ns as i128;
    let k = ts.wrapping_sub(an).checked_div_euclid(iv).unwrap_or(0);
    let lo = an.wrapping_add(k.wrapping_mul(iv));
    let hi = lo.wrapping_add(iv);
    let in_range = |v: i128| v >= i64::MIN as i128 && v <= i64::MAX as i128;
    let (first, second) = if ts.wrapping_sub(lo) <= hi.wrapping_sub(ts) {
        (lo, hi)
    } else {
        (hi, lo)
    };
    if in_range(first) {
        first as i64
    } else if in_range(second) {
        second as i64
    } else {
        ts_ns
    }
}
