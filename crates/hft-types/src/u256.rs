//! On-chain U256 ↔ `Qty` conversion (optional chain components); U256 is exchanged as
//! little-endian 64-bit limbs (`alloy_primitives::U256::as_limbs()` layout).

use crate::err::FxErr;
use crate::fixed::{POW10_I128, POW10_U64, asset_exp_ok};
use crate::types::{MAX_RAW, Qty, RoundMode};

/// Little-endian 64-bit limbs.
pub type U256Limbs = [u64; 4];

/// `d = token_decimals + qty_exp` with both range checks (`qty_exp ∈ [-18, 6]`, `d ∈ [-18, 36]`).
#[inline]
fn shift(token_decimals: u8, qty_exp: i8) -> Result<i32, FxErr> {
    if !asset_exp_ok(qty_exp) {
        return Err(FxErr::EXP_RANGE);
    }
    let d = i32::from(token_decimals).wrapping_add(i32::from(qty_exp));
    if !(-18..=36).contains(&d) {
        return Err(FxErr::EXP_RANGE);
    }
    Ok(d)
}

/// Long division of a 256-bit value by a non-zero u64.
#[inline]
fn divmod_u64(v: &U256Limbs, d: u64) -> (U256Limbs, u64) {
    let d = u128::from(d);
    let mut q = [0u64; 4];
    let mut rem = 0u128;
    for (qi, vi) in q.iter_mut().zip(v.iter()).rev() {
        let cur = rem.wrapping_shl(64) | u128::from(*vi);
        *qi = cur.checked_div(d).unwrap_or(0) as u64;
        rem = cur.checked_rem(d).unwrap_or(0);
    }
    (q, rem as u64)
}

#[inline]
fn pow10_u64(k: i32) -> u64 {
    usize::try_from(k)
        .ok()
        .and_then(|i| POW10_U64.get(i))
        .copied()
        .unwrap_or(1)
}

/// `v / 10^d` (`d ≥ 0`, remainder rounded by `m`) or `v × 10^(-d)` (`d < 0`, checked).
pub fn qty_from_u256(v: &U256Limbs, token_decimals: u8, qty_exp: i8, m: RoundMode) -> Result<Qty, FxErr> {
    let d = shift(token_decimals, qty_exp)?;
    if d < 0 {
        if v[1] | v[2] | v[3] != 0 {
            return Err(FxErr::OVERFLOW);
        }
        let r = v[0]
            .checked_mul(pow10_u64(d.wrapping_neg()))
            .ok_or(FxErr::OVERFLOW)?;
        if r > MAX_RAW as u64 {
            return Err(FxErr::OVERFLOW);
        }
        return Ok(Qty(r as i64));
    }
    // Quotient and remainder by 10^d; d > 19 divides twice (10^18, then 10^(d−18)).
    let (q, rem) = if d <= 19 {
        let (q, r) = divmod_u64(v, pow10_u64(d));
        (q, u128::from(r))
    } else {
        let (q1, r1) = divmod_u64(v, POW10_U64[18]);
        let (q2, r2) = divmod_u64(&q1, pow10_u64(d.wrapping_sub(18)));
        (
            q2,
            u128::from(r2)
                .wrapping_mul(POW10_U64[18] as u128)
                .wrapping_add(u128::from(r1)),
        )
    };
    if q[1] | q[2] | q[3] != 0 || q[0] > MAX_RAW as u64 {
        return Err(FxErr::OVERFLOW);
    }
    let div = usize::try_from(d)
        .ok()
        .and_then(|i| POW10_I128.get(i))
        .copied()
        .unwrap_or(1) as u128;
    let up = rem != 0
        && match m {
            RoundMode::Floor | RoundMode::TowardZero => false,
            RoundMode::Ceil => true,
            RoundMode::HalfEven => match rem.cmp(&div.wrapping_sub(rem)) {
                core::cmp::Ordering::Less => false,
                core::cmp::Ordering::Greater => true,
                core::cmp::Ordering::Equal => q[0] & 1 == 1,
            },
        };
    let r = if up { q[0].wrapping_add(1) } else { q[0] };
    if r > MAX_RAW as u64 {
        return Err(FxErr::OVERFLOW);
    }
    Ok(Qty(r as i64))
}

/// `q × 10^d` exactly (`d ≥ 0`) or `q / 10^(-d)` when divisible (`d < 0`, else `PRECISION`);
/// `q < 0` → `NEGATIVE`.
pub fn qty_to_u256(q: Qty, token_decimals: u8, qty_exp: i8) -> Result<U256Limbs, FxErr> {
    let d = shift(token_decimals, qty_exp)?;
    if q.0 < 0 {
        return Err(FxErr::NEGATIVE);
    }
    let qv = q.0 as u64;
    if d < 0 {
        let p = pow10_u64(d.wrapping_neg());
        if qv.checked_rem(p) != Some(0) {
            return Err(FxErr::PRECISION);
        }
        return Ok([qv.checked_div(p).unwrap_or(0), 0, 0, 0]);
    }
    let p = usize::try_from(d)
        .ok()
        .and_then(|i| POW10_I128.get(i))
        .copied()
        .unwrap_or(1) as u128;
    let (lo, hi) = (p as u64, p.wrapping_shr(64) as u64);
    // q < 2^63 and 10^36 < 2^120: the product fits in three limbs.
    let a = u128::from(qv).wrapping_mul(u128::from(lo));
    let b = u128::from(qv).wrapping_mul(u128::from(hi));
    let t = b.wrapping_add(a.wrapping_shr(64));
    Ok([a as u64, t as u64, t.wrapping_shr(64) as u64, 0])
}
