//! Byte-wise decimal string → fixed-point parsing (`parse_exact`, `parse_round`).
//!
//! Grammar: `["-"] 1*DIGIT ["." 1*DIGIT] [("e" / "E") ["+" / "-"] 1*3DIGIT]`, at most 40 bytes.
//! Digit runs are validated and accumulated 8 bytes at a time (SWAR).

use crate::err::FxErr;
use crate::fixed::POW10_U64;
use crate::types::{MAX_RAW, RoundMode};

/// Wire exponent range accepted by `parse_*` and `write_fixed`.
pub(crate) const WIRE_EXP_MIN: i8 = -30;
pub(crate) const WIRE_EXP_MAX: i8 = 18;
/// Longest accepted input.
const MAX_LEN: usize = 40;
/// Most significant digits that fit the u64 mantissa.
const MAX_SIG: usize = 19;

const ZEROS: u64 = 0x3030_3030_3030_3030;
const LOW7: u64 = 0x7F7F_7F7F_7F7F_7F7F;
const ADD10: u64 = 0x7676_7676_7676_7676;
const HIGH: u64 = 0x8080_8080_8080_8080;

/// Length of the ASCII-digit run at the start of `s`.
#[inline]
fn digit_run(s: &[u8]) -> usize {
    let mut n = 0usize;
    let mut rest = s;
    while let Some((head, tail)) = rest.split_first_chunk::<8>() {
        let x = u64::from_le_bytes(*head) ^ ZEROS;
        // A byte is a digit iff x < 10: (x & 0x7F) + 0x76 sets bit 7 for 10..=0x7F, x itself for ≥ 0x80.
        let mask = ((x & LOW7).wrapping_add(ADD10) | x) & HIGH;
        if mask != 0 {
            return n.wrapping_add((mask.trailing_zeros() / 8) as usize);
        }
        n = n.wrapping_add(8);
        rest = tail;
    }
    n.wrapping_add(rest.iter().take_while(|b| b.is_ascii_digit()).count())
}

/// Eight ASCII digits (first byte most significant) loaded little-endian → value.
#[inline]
fn swar8(v: u64) -> u64 {
    let v = (v & 0x0F0F_0F0F_0F0F_0F0F).wrapping_mul(2561).wrapping_shr(8);
    let v = (v & 0x00FF_00FF_00FF_00FF)
        .wrapping_mul(6_553_601)
        .wrapping_shr(16);
    (v & 0x0000_FFFF_0000_FFFF)
        .wrapping_mul(42_949_672_960_001)
        .wrapping_shr(32)
}

/// `m × 10^len(bytes) + digits(bytes)`; callers keep the total ≤ 19 digits.
#[inline]
fn accumulate(mut m: u64, bytes: &[u8]) -> u64 {
    let mut rest = bytes;
    while let Some((head, tail)) = rest.split_first_chunk::<8>() {
        m = m
            .wrapping_mul(100_000_000)
            .wrapping_add(swar8(u64::from_le_bytes(*head)));
        rest = tail;
    }
    for b in rest {
        m = m.wrapping_mul(10).wrapping_add(u64::from(b.wrapping_sub(b'0')));
    }
    m
}

/// A syntactically valid number: digits `D = int ++ frac`, value `D × 10^(x − len(frac))`.
struct Scanned<'a> {
    neg: bool,
    int: &'a [u8],
    frac: &'a [u8],
    x: i32,
}

impl Scanned<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.int.len().wrapping_add(self.frac.len())
    }

    /// Digit `D[i]` as a byte (`b'0'` when out of range).
    #[inline]
    fn digit(&self, i: usize) -> u8 {
        match self.int.get(i) {
            Some(b) => *b,
            None => self
                .frac
                .get(i.wrapping_sub(self.int.len()))
                .copied()
                .unwrap_or(b'0'),
        }
    }

    /// Value of `D[from .. from + count]` (count ≤ 19).
    #[inline]
    fn value(&self, from: usize, count: usize) -> u64 {
        let end = from.wrapping_add(count);
        let ni = self.int.len();
        let mut m = 0u64;
        if from < ni {
            m = accumulate(m, self.int.get(from..end.min(ni)).unwrap_or(&[]));
        }
        if end > ni {
            let f0 = from.max(ni).wrapping_sub(ni);
            m = accumulate(m, self.frac.get(f0..end.wrapping_sub(ni)).unwrap_or(&[]));
        }
        m
    }
}

/// Syntax scan of a non-empty input of at most 40 bytes.
#[inline]
fn scan(s: &[u8]) -> Result<Scanned<'_>, FxErr> {
    let (neg, body) = match s.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, s),
    };
    let ni = digit_run(body);
    if ni == 0 {
        return Err(FxErr::SYNTAX);
    }
    let (int, mut rest) = body.split_at(ni);
    let mut frac: &[u8] = &[];
    if let Some((b'.', r)) = rest.split_first() {
        let nf = digit_run(r);
        if nf == 0 {
            return Err(FxErr::SYNTAX);
        }
        (frac, rest) = r.split_at(nf);
    }
    let mut x = 0i32;
    if let Some((b'e' | b'E', r)) = rest.split_first() {
        let (xneg, r) = match r.split_first() {
            Some((b'-', r2)) => (true, r2),
            Some((b'+', r2)) => (false, r2),
            _ => (false, r),
        };
        let nx = digit_run(r);
        if nx == 0 || nx > 3 {
            return Err(FxErr::SYNTAX);
        }
        let (xd, r2) = r.split_at(nx);
        for b in xd {
            x = x.wrapping_mul(10).wrapping_add(i32::from(b.wrapping_sub(b'0')));
        }
        if xneg {
            x = x.wrapping_neg();
        }
        rest = r2;
    }
    if !rest.is_empty() {
        return Err(FxErr::SYNTAX);
    }
    Ok(Scanned { neg, int, frac, x })
}

/// Normalized number: significant digits `D[lz .. lz + nd]`, value `sig × 10^(k + exp)`.
struct Decomposed<'a> {
    sc: Scanned<'a>,
    /// Leading zeros skipped in `D`.
    lz: usize,
    /// Significant digit count (0 = the value is zero).
    nd: usize,
    /// Target shift: result = `sig × 10^k`.
    k: i32,
}

/// Steps shared by both parsers: exponent range, emptiness, length, syntax, normalization.
#[inline]
fn decompose(s: &[u8], exp: i8) -> Result<Decomposed<'_>, FxErr> {
    if !(WIRE_EXP_MIN..=WIRE_EXP_MAX).contains(&exp) {
        return Err(FxErr::EXP_RANGE);
    }
    if s.is_empty() {
        return Err(FxErr::EMPTY);
    }
    if s.len() > MAX_LEN {
        return Err(FxErr::SYNTAX);
    }
    let sc = scan(s)?;
    let total = sc.len();
    let lz = sc
        .int
        .iter()
        .chain(sc.frac.iter())
        .take_while(|b| **b == b'0')
        .count();
    if lz == total {
        return Ok(Decomposed { sc, lz, nd: 0, k: 0 });
    }
    let tz = sc
        .frac
        .iter()
        .rev()
        .chain(sc.int.iter().rev())
        .take_while(|b| **b == b'0')
        .count();
    let nd = total.wrapping_sub(lz).wrapping_sub(tz);
    // E = x + tz − nf; k = E − exp. All terms are small (|x| ≤ 999, tz, nf ≤ 40).
    let k =
        sc.x.wrapping_add(tz as i32)
            .wrapping_sub(sc.frac.len() as i32)
            .wrapping_sub(i32::from(exp));
    Ok(Decomposed { sc, lz, nd, k })
}

#[inline]
fn signed(neg: bool, mag: u64) -> Result<i64, FxErr> {
    if mag > MAX_RAW as u64 {
        return Err(FxErr::OVERFLOW);
    }
    let v = mag as i64;
    Ok(if neg { v.wrapping_neg() } else { v })
}

/// Parse a price/quantity string exactly at wire exponent `exp`: non-zero digits finer than
/// `10^exp` → `PRECISION` (never truncates); more than 19 significant digits or
/// `|raw| > MAX_RAW` → `OVERFLOW`.
pub fn parse_exact(s: &[u8], exp: i8) -> Result<i64, FxErr> {
    let d = decompose(s, exp)?;
    if d.nd == 0 {
        return Ok(0);
    }
    if d.nd > MAX_SIG {
        return Err(FxErr::OVERFLOW);
    }
    // The significant digits end in a non-zero digit, so any k < 0 drops a non-zero digit.
    if d.k < 0 {
        return Err(FxErr::PRECISION);
    }
    let p = usize::try_from(d.k)
        .ok()
        .and_then(|k| POW10_U64.get(k))
        .ok_or(FxErr::OVERFLOW)?;
    let mag = d.sc.value(d.lz, d.nd).checked_mul(*p).ok_or(FxErr::OVERFLOW)?;
    signed(d.sc.neg, mag)
}

/// Position of the dropped digits relative to one half unit of the last kept digit.
#[derive(Copy, Clone, Eq, PartialEq)]
enum Frac {
    Below,
    Half,
    Above,
}

/// Parse and round the exact decimal value to a multiple of `10^exp` by `m`.
pub fn parse_round(s: &[u8], exp: i8, m: RoundMode) -> Result<i64, FxErr> {
    let d = decompose(s, exp)?;
    if d.nd == 0 {
        return Ok(0);
    }
    let neg = d.sc.neg;
    if d.k >= 0 {
        let k = d.k as usize;
        if d.nd.saturating_add(k) > MAX_SIG {
            return Err(FxErr::OVERFLOW);
        }
        let p = POW10_U64.get(k).ok_or(FxErr::OVERFLOW)?;
        let mag = d.sc.value(d.lz, d.nd).checked_mul(*p).ok_or(FxErr::OVERFLOW)?;
        return signed(neg, mag);
    }
    // Drop j ≥ 1 digits; the last significant digit is non-zero, so the dropped part is never 0.
    let j = d.k.unsigned_abs() as usize;
    let (q, frac) = if j > d.nd {
        (0u64, Frac::Below)
    } else {
        let keep = d.nd.wrapping_sub(j);
        if keep > MAX_SIG {
            return Err(FxErr::OVERFLOW);
        }
        let q = d.sc.value(d.lz, keep);
        let first = d.sc.digit(d.lz.wrapping_add(keep));
        let frac = match first.cmp(&b'5') {
            core::cmp::Ordering::Less => Frac::Below,
            core::cmp::Ordering::Greater => Frac::Above,
            core::cmp::Ordering::Equal if j > 1 => Frac::Above,
            core::cmp::Ordering::Equal => Frac::Half,
        };
        (q, frac)
    };
    let up = match m {
        RoundMode::Floor => neg,
        RoundMode::Ceil => !neg,
        RoundMode::TowardZero => false,
        RoundMode::HalfEven => match frac {
            Frac::Below => false,
            Frac::Above => true,
            Frac::Half => q & 1 == 1,
        },
    };
    let mag = if up {
        q.checked_add(1).ok_or(FxErr::OVERFLOW)?
    } else {
        q
    };
    signed(neg, mag)
}
