//! Exchange time: ISO-8601 UTC `YYYY-MM-DDTHH:MM:SS[.f{1,9}]Z` → Unix ns (README "时间戳").
//! The feed writes microseconds (`2026-11-06T15:58:52.472727Z`), the REST ticker nanoseconds; any
//! other shape is a `Number` error, never guessed.

use hft_venue_proto::DecodeErr;

fn digits(s: &[u8], at: usize, n: usize) -> Result<i64, DecodeErr> {
    let part = s.get(at..at + n).ok_or(DecodeErr::Number)?;
    let mut v = 0i64;
    for &b in part {
        if !b.is_ascii_digit() {
            return Err(DecodeErr::Number);
        }
        v = v * 10 + i64::from(b - b'0');
    }
    Ok(v)
}

fn sep(s: &[u8], at: usize, c: u8) -> Result<(), DecodeErr> {
    if s.get(at) == Some(&c) {
        Ok(())
    } else {
        Err(DecodeErr::Number)
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant, `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// `YYYY-MM-DDTHH:MM:SS[.f{1,9}]Z` → ns since the Unix epoch (years 1970–2261).
pub fn iso8601_ns(s: &[u8]) -> Result<i64, DecodeErr> {
    let y = digits(s, 0, 4)?;
    sep(s, 4, b'-')?;
    let mo = digits(s, 5, 2)?;
    sep(s, 7, b'-')?;
    let d = digits(s, 8, 2)?;
    sep(s, 10, b'T')?;
    let h = digits(s, 11, 2)?;
    sep(s, 13, b':')?;
    let mi = digits(s, 14, 2)?;
    sep(s, 16, b':')?;
    let se = digits(s, 17, 2)?;
    let mdays = [31, if leap(y) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let dim = usize::try_from(mo - 1)
        .ok()
        .and_then(|i| mdays.get(i).copied())
        .ok_or(DecodeErr::Number)?;
    if !(1970..=2261).contains(&y) || d < 1 || d > dim || h > 23 || mi > 59 || se > 59 {
        return Err(DecodeErr::Number);
    }
    let mut at = 19;
    let mut frac = 0i64;
    if s.get(at) == Some(&b'.') {
        at += 1;
        let n = s.get(at..).unwrap_or(&[]).iter().take_while(|b| b.is_ascii_digit()).count();
        if n == 0 || n > 9 {
            return Err(DecodeErr::Number);
        }
        frac = digits(s, at, n)?;
        for _ in n..9 {
            frac *= 10;
        }
        at += n;
    }
    if s.get(at) != Some(&b'Z') || s.len() != at + 1 {
        return Err(DecodeErr::Number);
    }
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3_600 + mi * 60 + se;
    secs.checked_mul(1_000_000_000)
        .and_then(|ns| ns.checked_add(frac))
        .ok_or(DecodeErr::Overflow)
}
