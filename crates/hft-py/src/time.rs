//! Time arguments (README "参数语义": ns integers and zone-qualified ISO strings) and the TSC → UTC
//! conversion of `proc_ns` (event-model: segment-header calibration pair, then each `ClockStatus`).

use crate::error::{Error, Res};

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn digits(s: &[u8], at: usize, n: usize) -> Option<i64> {
    let b = s.get(at..at + n)?;
    if !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut v = 0i64;
    for c in b {
        v = v * 10 + i64::from(c - b'0');
    }
    Some(v)
}

/// `YYYY-MM-DDTHH:MM:SS[.f{1,9}](Z|±HH:MM)` → ns since the epoch; anything else (no zone, date
/// only, another format) → `ValueError`.
pub fn parse_iso(s: &str) -> Res<i64> {
    let bad = || {
        Error::value(format!(
            "time `{s}`: expected ISO 8601 with a zone (`Z` or `±hh:mm`)"
        ))
    };
    let b = s.as_bytes();
    if b.len() < 20
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || !matches!(b.get(10), Some(b'T') | Some(b't') | Some(b' '))
        || b.get(13) != Some(&b':')
        || b.get(16) != Some(&b':')
    {
        return Err(bad());
    }
    let y = digits(b, 0, 4).ok_or_else(bad)?;
    let mo = digits(b, 5, 2).ok_or_else(bad)?;
    let d = digits(b, 8, 2).ok_or_else(bad)?;
    let h = digits(b, 11, 2).ok_or_else(bad)?;
    let mi = digits(b, 14, 2).ok_or_else(bad)?;
    let se = digits(b, 17, 2).ok_or_else(bad)?;
    if !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) || h > 23 || mi > 59 || se > 59 {
        return Err(bad());
    }
    let mut i = 19;
    let mut frac = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        let n = i - start;
        if n == 0 || n > 9 {
            return Err(bad());
        }
        frac = digits(b, start, n).ok_or_else(bad)?;
        for _ in n..9 {
            frac *= 10;
        }
    }
    let off_s = match b.get(i) {
        Some(b'Z') | Some(b'z') if i + 1 == b.len() => 0,
        Some(c @ (b'+' | b'-')) if i + 6 == b.len() && b.get(i + 3) == Some(&b':') => {
            let oh = digits(b, i + 1, 2).ok_or_else(bad)?;
            let om = digits(b, i + 4, 2).ok_or_else(bad)?;
            if oh > 23 || om > 59 {
                return Err(bad());
            }
            let o = oh * 3600 + om * 60;
            if *c == b'+' { o } else { -o }
        }
        _ => return Err(bad()),
    };
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se - off_s;
    secs.checked_mul(1_000_000_000)
        .and_then(|x| x.checked_add(frac))
        .ok_or_else(bad)
}

/// TSC → UTC: the latest calibration pair.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    pub tsc: u64,
    pub realtime_ns: i64,
    /// TSC Hz × 1000; 0 = no calibration (`proc_ns` = 0).
    pub hz_milli: u64,
}

impl Clock {
    pub fn proc_ns(&self, tsc: u64) -> i64 {
        if self.hz_milli == 0 {
            return 0;
        }
        let d = i128::from(tsc) - i128::from(self.tsc);
        let ns = d * 1_000_000_000_000 / i128::from(self.hz_milli);
        let v = i128::from(self.realtime_ns) + ns;
        v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }
}
