//! sonic-rs per-message structs over borrowed strings (README "实现说明" 2, 3) and the error
//! mapping of contracts 4 and 5: numbers that may come as JSON numbers or strings (`UInt`, `Code`),
//! fixed-capacity level and element lists (no allocation).

use core::fmt;
use core::marker::PhantomData;

use hft_types::{FxErr, parse_exact};
use hft_venue_proto::DecodeErr;
use serde::Deserialize;
use serde::de::{Deserializer, IgnoredAny, SeqAccess, Visitor};

/// Parses `p` into `T`; the input ending early → `Truncated`, anything else → `Shape`.
pub(crate) fn from<'a, T: Deserialize<'a>>(p: &'a [u8]) -> Result<T, DecodeErr> {
    sonic_rs::from_slice::<T>(p).map_err(|e| {
        if e.is_eof() {
            DecodeErr::Truncated
        } else {
            DecodeErr::Shape
        }
    })
}

/// Decimal string → fixed point at `exp` (contract 4).
pub(crate) fn fixed(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    parse_exact(s.as_bytes(), exp).map_err(fx)
}

/// Decimal string → fixed point; empty → 0 (Bitget sends `""` for absent prices).
pub(crate) fn fixed_or_zero(s: &str, exp: i8) -> Result<i64, DecodeErr> {
    if s.is_empty() { Ok(0) } else { fixed(s, exp) }
}

pub(crate) fn fx(e: FxErr) -> DecodeErr {
    if e.0 & FxErr::OVERFLOW.0 != 0 {
        DecodeErr::Overflow
    } else {
        DecodeErr::Number
    }
}

/// Milliseconds → ns; overflow → `Overflow` (contract 3: never re-guessed).
pub(crate) fn ms_ns(ms: u64) -> Result<i64, DecodeErr> {
    i64::try_from(ms)
        .ok()
        .and_then(|v| v.checked_mul(1_000_000))
        .ok_or(DecodeErr::Overflow)
}

/// Decimal digits → u64 (`None` = not digits; `Some(Err)` = overflow).
fn digits_u64(s: &[u8]) -> UInt {
    if s.is_empty() {
        return UInt::Bad;
    }
    let mut v: u64 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            return UInt::Bad;
        }
        v = match v.checked_mul(10).and_then(|x| x.checked_add(u64::from(c - b'0'))) {
            Some(x) => x,
            None => return UInt::Ovf,
        };
    }
    UInt::Val(v)
}

/// An unsigned integer sent as a JSON number or a decimal string (`seq`, `ts`, ids).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) enum UInt {
    #[default]
    Absent,
    Val(u64),
    /// Not a non-negative integer (→ `DecodeErr::Number`).
    Bad,
    /// Beyond u64 (→ `DecodeErr::Overflow`).
    Ovf,
}

impl UInt {
    /// Required value.
    pub fn get(self) -> Result<u64, DecodeErr> {
        match self {
            UInt::Val(v) => Ok(v),
            UInt::Bad => Err(DecodeErr::Number),
            UInt::Ovf => Err(DecodeErr::Overflow),
            UInt::Absent => Err(DecodeErr::Shape),
        }
    }

    /// Optional value (`""`, `null` and an absent key are `None`).
    pub fn opt(self) -> Result<Option<u64>, DecodeErr> {
        match self {
            UInt::Absent => Ok(None),
            v => v.get().map(Some),
        }
    }
}

impl<'de> Deserialize<'de> for UInt {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = UInt;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("integer or decimal string")
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<UInt, E> {
                Ok(UInt::Val(v))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<UInt, E> {
                Ok(u64::try_from(v).map_or(UInt::Bad, UInt::Val))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<UInt, E> {
                if v.is_empty() {
                    return Ok(UInt::Absent);
                }
                Ok(digits_u64(v.as_bytes()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<UInt, E> {
                Ok(UInt::Absent)
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<UInt, E> {
                Ok(UInt::Absent)
            }
        }
        d.deserialize_any(V)
    }
}

/// HTTP status bit of a raw code without a business code (contract 8).
pub(crate) const HTTP_BIT: u32 = 0x2000_0000;
/// Non-numeric string code segment (contract 8).
const TEXT_BIT: u32 = 0x4000_0000;

/// A response code sent as a JSON number or a string (`"0"`, `"00000"`, `30001`).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) enum Code {
    #[default]
    Absent,
    /// Numeric (number or all-digit string within i64).
    Num(i64),
    /// Non-numeric text: its contract-8 raw code.
    Text(u32),
}

impl Code {
    /// `0` / `"0"` / `"00000"`.
    pub fn is_zero(self) -> bool {
        self == Code::Num(0)
    }

    /// Business code (numeric only).
    pub fn num(self) -> Option<i64> {
        match self {
            Code::Num(v) => Some(v),
            _ => None,
        }
    }

    /// Raw code of contract 8; `status` is used when there is no business code.
    pub fn raw(self, status: u16) -> u32 {
        match self {
            Code::Num(v) => match i32::try_from(v) {
                Ok(x) => x as u32,
                Err(_) => text_code(&v.to_ne_bytes()),
            },
            Code::Text(h) => h,
            Code::Absent => HTTP_BIT | u32::from(status),
        }
    }
}

/// `0x4000_0000 | (hash(text) & 0x00FF_FFFF)` (contract 8). The hash is FNV-1a 64: `xxhash-rust`
/// is not on the venue codec whitelist (`tools/xtask/deps.toml`); the hash segment only has to be
/// stable, the text itself is in the raw journal.
pub(crate) fn text_code(text: &[u8]) -> u32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in text {
        h = (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    TEXT_BIT | ((h & 0x00FF_FFFF) as u32)
}

fn code_of_str(v: &str) -> Code {
    let b = v.as_bytes();
    let (neg, digits) = match b.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, b),
    };
    match digits_u64(digits) {
        UInt::Val(x) if x <= i64::MAX as u64 => {
            let x = x as i64;
            Code::Num(if neg { -x } else { x })
        }
        _ => Code::Text(text_code(b)),
    }
}

impl<'de> Deserialize<'de> for Code {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Code;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("code")
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Code, E> {
                Ok(i64::try_from(v).map_or(Code::Text(text_code(&v.to_ne_bytes())), Code::Num))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Code, E> {
                Ok(Code::Num(v))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Code, E> {
                if v.is_empty() {
                    return Ok(Code::Absent);
                }
                Ok(code_of_str(v))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Code, E> {
                Ok(Code::Absent)
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Code, E> {
                Ok(Code::Absent)
            }
        }
        d.deserialize_any(V)
    }
}

/// The raw JSON text of a value (borrowed; no allocation for text without escapes).
pub(crate) struct Raw<'a>(sonic_rs::LazyValue<'a>);

impl<'a> Raw<'a> {
    pub fn text(&self) -> &[u8] {
        self.0.as_raw_str().as_bytes()
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Raw<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        sonic_rs::LazyValue::deserialize(d).map(Raw)
    }
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
        i += 1;
    }
    i
}

fn expect(b: &[u8], i: usize, c: u8) -> Result<usize, DecodeErr> {
    let i = skip_ws(b, i);
    if b.get(i) == Some(&c) {
        Ok(i + 1)
    } else {
        Err(DecodeErr::Shape)
    }
}

/// One scalar: a string without escapes (its content) or a number token.
fn scalar(b: &[u8], i: usize) -> Result<(&[u8], usize), DecodeErr> {
    let i = skip_ws(b, i);
    match b.get(i) {
        Some(b'"') => {
            let rest = b.get(i + 1..).ok_or(DecodeErr::Shape)?;
            let end = rest
                .iter()
                .position(|&c| c == b'"' || c == b'\\')
                .ok_or(DecodeErr::Shape)?;
            if rest.get(end) != Some(&b'"') {
                return Err(DecodeErr::Shape);
            }
            Ok((rest.get(..end).unwrap_or(&[]), i + 1 + end + 1))
        }
        Some(c) if c.is_ascii_digit() || *c == b'-' => {
            let rest = b.get(i..).unwrap_or(&[]);
            let end = rest
                .iter()
                .position(|c| !(c.is_ascii_digit() || matches!(c, b'.' | b'-' | b'+' | b'e' | b'E')))
                .unwrap_or(rest.len());
            Ok((rest.get(..end).unwrap_or(&[]), i + end))
        }
        _ => Err(DecodeErr::Shape),
    }
}

/// Parses a raw `[[px, size], …]` array (strings or numbers) into `out` at the given exponents.
/// `skip_zero` drops zero-size levels. Returns the count written; levels beyond `out` are
/// validated and dropped when `keep_extra`, an error (`Shape`) otherwise.
pub(crate) fn levels(
    raw: &[u8],
    pe: i8,
    qe: i8,
    skip_zero: bool,
    keep_extra: bool,
    out: &mut [hft_event::Level],
) -> Result<usize, DecodeErr> {
    let mut i = expect(raw, 0, b'[')?;
    let mut n = 0usize;
    let j = skip_ws(raw, i);
    if raw.get(j) == Some(&b']') {
        return Ok(0);
    }
    loop {
        i = expect(raw, i, b'[')?;
        let (px, k) = scalar(raw, i)?;
        i = expect(raw, k, b',')?;
        let (qty, k) = scalar(raw, i)?;
        i = expect(raw, k, b']')?;
        let px = parse_exact(px, pe).map_err(fx)?;
        let qty = parse_exact(qty, qe).map_err(fx)?;
        if !(skip_zero && qty == 0) {
            match out.get_mut(n) {
                Some(slot) => {
                    *slot = hft_event::Level {
                        px: hft_types::Px(px),
                        qty: hft_types::Qty(qty),
                    };
                    n += 1;
                }
                None if keep_extra => {}
                None => return Err(DecodeErr::Shape),
            }
        }
        let k = skip_ws(raw, i);
        match raw.get(k) {
            Some(b',') => i = k + 1,
            Some(b']') => return Ok(n),
            _ => return Err(DecodeErr::Shape),
        }
    }
}

/// The first element of a non-empty array (the rest is skipped); an empty array is an error.
pub(crate) struct One<T>(pub T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for One<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = One<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("non-empty array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<One<T>, A::Error> {
                let first = s
                    .next_element::<T>()?
                    .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
                while s.next_element::<IgnoredAny>()?.is_some() {}
                Ok(One(first))
            }
        }
        d.deserialize_seq(V(PhantomData))
    }
}

/// Up to `N` elements of an array kept in place; more → `over`.
pub(crate) struct Arr<T, const N: usize> {
    pub items: [Option<T>; N],
    pub over: bool,
}

impl<T, const N: usize> Arr<T, N> {
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter().map_while(Option::as_ref)
    }
}

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Arr<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, const N: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for V<T, N> {
            type Value = Arr<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("array")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Arr<T, N>, A::Error> {
                let mut out = Arr {
                    items: core::array::from_fn(|_| None),
                    over: false,
                };
                let mut n = 0usize;
                while let Some(x) = s.next_element::<T>()? {
                    match out.items.get_mut(n) {
                        Some(slot) => {
                            *slot = Some(x);
                            n += 1;
                        }
                        None => out.over = true,
                    }
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V::<T, N>(PhantomData))
    }
}

/// `arg` of a push or an event (UTA `topic`/`symbol`, Classic V2 `channel`/`instId`).
#[derive(Deserialize, Default)]
pub(crate) struct Arg<'a> {
    #[serde(alias = "channel", default, borrow)]
    pub topic: &'a str,
    #[serde(alias = "instId", default, borrow)]
    pub symbol: &'a str,
}
