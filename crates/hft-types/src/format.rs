//! Fixed-point → shortest decimal string (`write_fixed`), allocation-free.

use crate::parse::{WIRE_EXP_MAX, WIRE_EXP_MIN};

/// Two-digit lookup table `"00".."99"`.
const DIGITS2: [u8; 200] = {
    let mut t = [0u8; 200];
    let mut i = 0;
    while i < 100 {
        t[2 * i] = b'0' + (i / 10) as u8;
        t[2 * i + 1] = b'0' + (i % 10) as u8;
        i += 1;
    }
    t
};

/// Bounded writer over the 40-byte output; writes past the end are dropped (never happens
/// for `exp ∈ [-30, 18]`, longest output is 38 bytes).
struct Out<'a> {
    buf: &'a mut [u8; 40],
    len: usize,
}

impl Out<'_> {
    #[inline]
    fn push(&mut self, b: u8) {
        if let Some(slot) = self.buf.get_mut(self.len) {
            *slot = b;
            self.len = self.len.wrapping_add(1);
        }
    }

    #[inline]
    fn push_slice(&mut self, s: &[u8]) {
        let end = self.len.wrapping_add(s.len());
        if let Some(dst) = self.buf.get_mut(self.len..end) {
            dst.copy_from_slice(s);
            self.len = end;
        }
    }

    #[inline]
    fn push_zeros(&mut self, n: usize) {
        let end = self.len.wrapping_add(n);
        if let Some(dst) = self.buf.get_mut(self.len..end) {
            dst.fill(b'0');
            self.len = end;
        }
    }
}

/// Write the two digits of `r < 100` at `buf[i..i + 2]`.
#[inline]
fn put2(buf: &mut [u8; 20], i: usize, r: usize) {
    let src = r.wrapping_mul(2);
    if let (Some(dst), Some(src)) = (
        buf.get_mut(i..i.wrapping_add(2)),
        DIGITS2.get(src..src.wrapping_add(2)),
    ) {
        dst.copy_from_slice(src);
    }
}

/// Decimal digits of `n` written right-aligned into `buf`; returns the first digit index.
#[inline]
fn utoa(mut n: u64, buf: &mut [u8; 20]) -> usize {
    let mut i = buf.len();
    while n >= 100 {
        let r = (n % 100) as usize;
        n /= 100;
        i = i.wrapping_sub(2);
        put2(buf, i, r);
    }
    if n >= 10 {
        i = i.wrapping_sub(2);
        put2(buf, i, n as usize);
    } else {
        i = i.wrapping_sub(1);
        if let Some(d) = buf.get_mut(i) {
            *d = b'0'.wrapping_add(n as u8);
        }
    }
    i
}

/// Strip trailing `'0'` bytes.
#[inline]
fn trim_zeros(s: &[u8]) -> &[u8] {
    let n = s.iter().rev().take_while(|b| **b == b'0').count();
    s.get(..s.len().wrapping_sub(n)).unwrap_or(s)
}

/// Write `raw × 10^exp` as the shortest decimal (no exponent, no trailing zeros, `"0."` for
/// `|x| < 1`, leading `"-"` for negatives) and return the byte count; `exp` outside
/// `[-30, 18]` → 0 and `out` is untouched.
pub fn write_fixed(raw: i64, exp: i8, out: &mut [u8; 40]) -> usize {
    if !(WIRE_EXP_MIN..=WIRE_EXP_MAX).contains(&exp) {
        return 0;
    }
    let mut w = Out { buf: out, len: 0 };
    if raw == 0 {
        w.push(b'0');
        return w.len;
    }
    let mut dbuf = [0u8; 20];
    let start = utoa(raw.unsigned_abs(), &mut dbuf);
    let digits = dbuf.get(start..).unwrap_or(&[]);
    let nd = digits.len();
    if raw < 0 {
        w.push(b'-');
    }
    if exp >= 0 {
        w.push_slice(digits);
        w.push_zeros(exp as usize);
        return w.len;
    }
    let f = exp.unsigned_abs() as usize;
    if nd > f {
        let (int, frac) = digits.split_at(nd.wrapping_sub(f));
        w.push_slice(int);
        let frac = trim_zeros(frac);
        if !frac.is_empty() {
            w.push(b'.');
            w.push_slice(frac);
        }
    } else {
        w.push_slice(b"0.");
        w.push_zeros(f.wrapping_sub(nd));
        w.push_slice(trim_zeros(digits));
    }
    w.len
}
