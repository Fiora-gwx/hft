//! Reference implementations for property tests and the fuzz target: decimal digit vectors and
//! sign-magnitude integer arithmetic, written independently of the crate's algorithms.
//! Included with `mod reference;` by the property tests (and by `fuzz/fuzz_targets/parse.rs`).
#![allow(dead_code)]

use hft_types::{AssetExp, FxErr, MAX_RAW, Notional, RoundMode};

pub const MODES: [RoundMode; 4] = [
    RoundMode::Floor,
    RoundMode::Ceil,
    RoundMode::TowardZero,
    RoundMode::HalfEven,
];

/// Where the dropped part lies relative to half a unit.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Rest {
    Zero,
    Below,
    Half,
    Above,
}

/// Round a magnitude `q` with dropped part `rest` for a value of sign `neg`.
pub fn round_mag(q: u128, rest: Rest, neg: bool, m: RoundMode) -> u128 {
    let up = match (rest, m) {
        (Rest::Zero, _) => false,
        (_, RoundMode::TowardZero) => false,
        (_, RoundMode::Floor) => neg,
        (_, RoundMode::Ceil) => !neg,
        (Rest::Below, RoundMode::HalfEven) => false,
        (Rest::Above, RoundMode::HalfEven) => true,
        (Rest::Half, RoundMode::HalfEven) => q % 2 == 1,
    };
    q + up as u128
}

/// `n / d` rounded by `m`, via sign and magnitude.
pub fn ref_div_round(n: i128, d: i128, m: RoundMode) -> i128 {
    assert!(d != 0);
    let neg = (n < 0) != (d < 0);
    let (un, ud) = (n.unsigned_abs(), d.unsigned_abs());
    let q = un / ud;
    let r = un % ud;
    let rest = if r == 0 {
        Rest::Zero
    } else if r * 2 < ud {
        Rest::Below
    } else if r * 2 == ud {
        Rest::Half
    } else {
        Rest::Above
    };
    let mag = round_mag(q, rest, neg, m) as i128;
    if neg { -mag } else { mag }
}

/// Reference `Px::mul_qty` (legal exponents only).
pub fn ref_mul_qty(px: i64, q: i64, e: AssetExp, m: RoundMode) -> (Notional, FxErr) {
    let prod = px as i128 * q as i128;
    let s = e.px_exp as i32 + e.qty_exp as i32 + 6;
    let v = if s >= 0 {
        prod.checked_mul(10i128.pow(s as u32))
    } else {
        Some(ref_div_round(prod, 10i128.pow((-s) as u32), m))
    };
    match v {
        Some(v) if v.abs() <= i64::MAX as i128 => (Notional(v as i64), FxErr::OK),
        Some(v) if v < 0 => (Notional(-i64::MAX), FxErr::OVERFLOW),
        None if prod < 0 => (Notional(-i64::MAX), FxErr::OVERFLOW),
        _ => (Notional(i64::MAX), FxErr::OVERFLOW),
    }
}

/// Classify a digit string (most significant first) against "5000…".
pub fn rest_of(digits: &[u8]) -> Rest {
    match digits.first() {
        None => Rest::Zero,
        Some(&f) => {
            let tail_nonzero = digits[1..].iter().any(|&d| d != 0);
            match f.cmp(&5) {
                std::cmp::Ordering::Less if f == 0 && !tail_nonzero => Rest::Zero,
                std::cmp::Ordering::Less => Rest::Below,
                std::cmp::Ordering::Equal if tail_nonzero => Rest::Above,
                std::cmp::Ordering::Equal => Rest::Half,
                std::cmp::Ordering::Greater => Rest::Above,
            }
        }
    }
}

/// Digit values → u128, `None` when longer than 38 significant digits.
pub fn digits_to_u128(d: &[u8]) -> Option<u128> {
    let d: Vec<u8> = d.iter().copied().skip_while(|&x| x == 0).collect();
    if d.len() > 38 {
        return None;
    }
    Some(d.iter().fold(0u128, |a, &x| a * 10 + x as u128))
}

/// Reference grammar check; returns (neg, digits, frac_len, exponent part).
pub fn ref_syntax(s: &[u8]) -> Option<(bool, Vec<u8>, usize, i64)> {
    let mut i = 0;
    let neg = s.first() == Some(&b'-');
    if neg {
        i += 1;
    }
    let mut digits = Vec::new();
    let start = i;
    while i < s.len() && s[i].is_ascii_digit() {
        digits.push(s[i] - b'0');
        i += 1;
    }
    if i == start {
        return None;
    }
    let mut nf = 0;
    if i < s.len() && s[i] == b'.' {
        i += 1;
        let fs = i;
        while i < s.len() && s[i].is_ascii_digit() {
            digits.push(s[i] - b'0');
            i += 1;
        }
        nf = i - fs;
        if nf == 0 {
            return None;
        }
    }
    let mut x: i64 = 0;
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        i += 1;
        let mut xneg = false;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            xneg = s[i] == b'-';
            i += 1;
        }
        let xs = i;
        while i < s.len() && s[i].is_ascii_digit() {
            x = x * 10 + (s[i] - b'0') as i64;
            i += 1;
        }
        if i == xs || i - xs > 3 {
            return None;
        }
        if xneg {
            x = -x;
        }
    }
    if i != s.len() {
        return None;
    }
    Some((neg, digits, nf, x))
}

/// Reference parser: `m = None` is `parse_exact`, `Some(m)` is `parse_round`.
pub fn ref_parse(s: &[u8], exp: i8, m: Option<RoundMode>) -> Result<i64, FxErr> {
    if !(-30..=18).contains(&exp) {
        return Err(FxErr::EXP_RANGE);
    }
    if s.is_empty() {
        return Err(FxErr::EMPTY);
    }
    if s.len() > 40 {
        return Err(FxErr::SYNTAX);
    }
    let Some((neg, digits, nf, x)) = ref_syntax(s) else {
        return Err(FxErr::SYNTAX);
    };
    if digits.iter().all(|&d| d == 0) {
        return Ok(0);
    }
    let first = digits.iter().position(|&d| d != 0).unwrap();
    let last = digits.iter().rposition(|&d| d != 0).unwrap();
    if m.is_none() && last - first + 1 > 19 {
        return Err(FxErr::OVERFLOW);
    }
    // value = digits × 10^(x − nf); result = value × 10^(−exp)
    let shift = x - nf as i64 - exp as i64;
    let (int, frac): (Vec<u8>, Vec<u8>) = if shift >= 0 {
        if digits.len() as i64 - first as i64 + shift > 40 {
            return Err(FxErr::OVERFLOW);
        }
        let mut v = digits.clone();
        v.extend(std::iter::repeat_n(0, shift as usize));
        (v, Vec::new())
    } else {
        let j = (-shift) as usize;
        if j >= digits.len() {
            let mut f = vec![0u8; j - digits.len()];
            f.extend(&digits);
            (Vec::new(), f)
        } else {
            (
                digits[..digits.len() - j].to_vec(),
                digits[digits.len() - j..].to_vec(),
            )
        }
    };
    let rest = rest_of(&frac);
    let q = match digits_to_u128(&int) {
        Some(q) => q,
        None => return Err(FxErr::OVERFLOW),
    };
    let mag = match m {
        None => {
            if rest != Rest::Zero {
                return Err(FxErr::PRECISION);
            }
            q
        }
        Some(m) => round_mag(q, rest, neg, m),
    };
    if mag > MAX_RAW as u128 {
        return Err(FxErr::OVERFLOW);
    }
    Ok(if neg { -(mag as i64) } else { mag as i64 })
}

/// Little-endian limbs → decimal digits (most significant first), by bitwise doubling.
pub fn limbs_to_digits(v: &[u64; 4]) -> Vec<u8> {
    let mut d: Vec<u8> = vec![0];
    for bit in (0..256).rev() {
        let b = (v[bit / 64] >> (bit % 64)) & 1;
        let mut carry = b as u8;
        for x in d.iter_mut().rev() {
            let y = *x * 2 + carry;
            *x = y % 10;
            carry = y / 10;
        }
        if carry > 0 {
            d.insert(0, carry);
        }
    }
    d
}

/// Decimal digits → little-endian limbs (`None` above 2^256 − 1).
pub fn digits_to_limbs(d: &[u8]) -> Option<[u64; 4]> {
    let mut l = [0u64; 4];
    for &x in d {
        let mut carry = x as u128;
        for limb in l.iter_mut() {
            let y = *limb as u128 * 10 + carry;
            *limb = y as u64;
            carry = y >> 64;
        }
        if carry != 0 {
            return None;
        }
    }
    Some(l)
}

/// Reference `qty_from_u256`.
pub fn ref_qty_from_u256(v: &[u64; 4], td: u8, qe: i8, m: RoundMode) -> Result<i64, FxErr> {
    let d = td as i32 + qe as i32;
    if !(-18..=6).contains(&qe) || !(-18..=36).contains(&d) {
        return Err(FxErr::EXP_RANGE);
    }
    let digits = limbs_to_digits(v);
    let (int, frac) = if d < 0 {
        let mut x = digits.clone();
        x.extend(std::iter::repeat_n(0, (-d) as usize));
        (x, Vec::new())
    } else {
        let j = d as usize;
        if j >= digits.len() {
            let mut f = vec![0u8; j - digits.len()];
            f.extend(&digits);
            (Vec::new(), f)
        } else {
            (
                digits[..digits.len() - j].to_vec(),
                digits[digits.len() - j..].to_vec(),
            )
        }
    };
    let q = digits_to_u128(&int).ok_or(FxErr::OVERFLOW)?;
    let mag = round_mag(q, rest_of(&frac), false, m);
    if mag > MAX_RAW as u128 {
        return Err(FxErr::OVERFLOW);
    }
    Ok(mag as i64)
}

/// Reference `qty_to_u256`.
pub fn ref_qty_to_u256(q: i64, td: u8, qe: i8) -> Result<[u64; 4], FxErr> {
    let d = td as i32 + qe as i32;
    if !(-18..=6).contains(&qe) || !(-18..=36).contains(&d) {
        return Err(FxErr::EXP_RANGE);
    }
    if q < 0 {
        return Err(FxErr::NEGATIVE);
    }
    let mut digits: Vec<u8> = q.to_string().bytes().map(|b| b - b'0').collect();
    if d >= 0 {
        digits.extend(std::iter::repeat_n(0, d as usize));
    } else {
        let j = (-d) as usize;
        if j >= digits.len() {
            if q != 0 {
                return Err(FxErr::PRECISION);
            }
            digits = vec![0];
        } else {
            if digits[digits.len() - j..].iter().any(|&x| x != 0) {
                return Err(FxErr::PRECISION);
            }
            digits.truncate(digits.len() - j);
        }
    }
    Ok(digits_to_limbs(&digits).unwrap())
}
