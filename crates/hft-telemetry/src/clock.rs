//! `ClockCal`, the bounded calibration queue, interpolation and per-sample error bounds
//! (latency-budget §2.1; README "时钟校准序列").

use std::collections::VecDeque;

use hft_event::ClockStatus;

/// `ClockStatus.state` code `Stepped` (hft-event `ClockState`).
pub const CLOCK_STEPPED: u8 = 3;

/// Calibration queue bound (README "延迟与资源契约": ≤ 4 `ClockStatus`).
pub const CAL_QUEUE_MAX: usize = 4;

/// The fields of one `ClockStatus` the conversions use.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct ClockStatusRow {
    pub tsc: u64,
    pub realtime_ns: i64,
    pub phc_ns: i64,
    pub tsc_hz_milli: u64,
    /// Offset bound of the clock against UTC (exchange-clock trust, latency-budget §2.1).
    pub max_error_ns: u32,
    /// Error bound of this calibration pair.
    pub calib_err_ns: u32,
    /// `ClockState` code (1 Synced, 2 Unsynced, 3 Stepped).
    pub state: u8,
}

impl From<&ClockStatus> for ClockStatusRow {
    fn from(c: &ClockStatus) -> Self {
        ClockStatusRow {
            tsc: c.tsc.0,
            realtime_ns: c.realtime_ns,
            phc_ns: c.phc_ns,
            tsc_hz_milli: c.tsc_hz_milli,
            max_error_ns: c.max_error_ns,
            calib_err_ns: c.calib_err_ns,
            state: c.state,
        }
    }
}

/// Clock domain of a non-TSC time point.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Domain {
    /// NIC PTP hardware clock (`hw_rx` with `HW_TS`, `ack_hw_rx`).
    Phc,
    /// `CLOCK_REALTIME` (`sw_rx`, `hw_rx` with `SW_TS`).
    Realtime,
}

/// The interpolation interval of one sample: the `ClockStatus` before (`a`) and after (`b`) it,
/// plus the interval before `a` (`prev`, for the rate-change term of the error bound) and the
/// error threshold (`cal_max_err_ns`, latency-budget §2.1).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ClockCal {
    pub prev: Option<ClockStatusRow>,
    pub a: ClockStatusRow,
    pub b: Option<ClockStatusRow>,
    pub max_err_ns: u32,
}

/// `round(n / d)` for `d > 0` (half away from −∞).
fn div_round(n: i128, d: i128) -> i128 {
    (2 * n + d).div_euclid(2 * d)
}

/// `a × b / d` (truncated toward 0) for `d > 0`, in i64 when the product fits (exact either way).
#[inline]
fn mul_div(a: i64, b: i64, d: i64) -> i128 {
    match a.checked_mul(b) {
        Some(p) => i128::from(p / d),
        None => i128::from(a) * i128::from(b) / i128::from(d),
    }
}

/// `round(a × b / d)` for `d > 0`, in i64 when it fits (exact either way).
#[inline]
fn mul_div_round(a: i64, b: i64, d: i64) -> i128 {
    match a
        .checked_mul(b)
        .and_then(|p| p.checked_mul(2))
        .and_then(|p| p.checked_add(d))
        .zip(d.checked_mul(2))
    {
        Some((n, d2)) => i128::from(n.div_euclid(d2)),
        None => div_round(i128::from(a) * i128::from(b), i128::from(d)),
    }
}

fn sat_i64(v: i128) -> i64 {
    v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

impl ClockCal {
    pub fn new(
        prev: Option<ClockStatusRow>,
        a: ClockStatusRow,
        b: Option<ClockStatusRow>,
        max_err_ns: u32,
    ) -> ClockCal {
        ClockCal {
            prev,
            a,
            b,
            max_err_ns,
        }
    }

    /// Nominal counter frequency in milli-Hz (`a`, else `b`, else 1 GHz).
    fn hz_milli(&self) -> u64 {
        match (self.a.tsc_hz_milli, self.b.map(|b| b.tsc_hz_milli)) {
            (0, Some(h)) if h != 0 => h,
            (0, _) => 1_000_000_000_000,
            (h, _) => h,
        }
    }

    /// Pure TSC segment `to − from` in ns: `Δtsc × 10⁹ / tsc_hz` (i128, truncated toward 0).
    pub fn tsc_ns(&self, from: u64, to: u64) -> i64 {
        let hz_milli = self.hz_milli();
        let d = to.wrapping_sub(from) as i64;
        if (to >= from) == (d >= 0) {
            // Integral Hz (the usual case): Δ × 10⁹ / hz in i64 when it fits.
            if hz_milli.is_multiple_of(1_000)
                && let Ok(hz) = i64::try_from(hz_milli / 1_000)
            {
                return sat_i64(mul_div(d, 1_000_000_000, hz));
            }
        }
        let d = i128::from(to) - i128::from(from);
        sat_i64(d * 1_000_000_000_000 / i128::from(hz_milli))
    }

    fn col(r: &ClockStatusRow, d: Domain) -> i64 {
        match d {
            Domain::Phc => r.phc_ns,
            Domain::Realtime => r.realtime_ns,
        }
    }

    /// TSC → `d` by linear interpolation between `a` and `b` (not the step value); without `b`,
    /// extrapolated from `a` at the nominal frequency.
    pub fn to_domain(&self, tsc: u64, d: Domain) -> i64 {
        let a = &self.a;
        let base = Self::col(a, d);
        match self.b {
            Some(b) if b.tsc > a.tsc => {
                let (Ok(dt), Some(dcol), Ok(len)) = (
                    i64::try_from(i128::from(tsc) - i128::from(a.tsc)),
                    Self::col(&b, d).checked_sub(base),
                    i64::try_from(b.tsc - a.tsc),
                ) else {
                    let dt = i128::from(tsc) - i128::from(a.tsc);
                    let num = dt * (i128::from(Self::col(&b, d)) - i128::from(base));
                    return sat_i64(i128::from(base) + div_round(num, i128::from(b.tsc - a.tsc)));
                };
                sat_i64(i128::from(base) + mul_div_round(dt, dcol, len))
            }
            _ => base.saturating_add(self.tsc_ns(a.tsc, tsc)),
        }
    }

    /// `PHC − REALTIME` offset at realtime `rt`, interpolated between `a` and `b`.
    fn offset_at_rt(&self, rt: i64) -> i64 {
        let a = &self.a;
        let off_a = i128::from(a.phc_ns) - i128::from(a.realtime_ns);
        match self.b {
            // Same offset at both ends (the usual case): no division.
            Some(b) if i128::from(b.phc_ns) - i128::from(b.realtime_ns) == off_a => sat_i64(off_a),
            Some(b) if b.realtime_ns > a.realtime_ns => {
                let off_b = i128::from(b.phc_ns) - i128::from(b.realtime_ns);
                let num = (i128::from(rt) - i128::from(a.realtime_ns)) * (off_b - off_a);
                sat_i64(off_a + div_round(num, i128::from(b.realtime_ns) - i128::from(a.realtime_ns)))
            }
            _ => sat_i64(off_a),
        }
    }

    /// A time point of domain `from` expressed in domain `to`.
    pub fn convert(&self, v: i64, from: Domain, to: Domain) -> i64 {
        match (from, to) {
            (Domain::Realtime, Domain::Phc) => v.saturating_add(self.offset_at_rt(v)),
            (Domain::Phc, Domain::Realtime) => v.saturating_sub(self.offset_at_rt(v)),
            _ => v,
        }
    }

    /// Per-sample error bound (latency-budget §2.1): `max(a.calib_err, b.calib_err)` + |rate(a→b) −
    /// rate(prev→a)| × length(a→b) / 2 (rounded to the nearest ns; rate term 0 without `prev`). `u32::MAX`
    /// without `b`.
    pub fn err_bound_ns(&self) -> u32 {
        let Some(b) = self.b else {
            return u32::MAX;
        };
        let a = &self.a;
        let base = u128::from(a.calib_err_ns.max(b.calib_err_ns));
        let term = match self.prev {
            Some(p) if b.tsc > a.tsc && a.tsc > p.tsc => {
                let (dp_ab, dt_ab) = (
                    i128::from(b.phc_ns) - i128::from(a.phc_ns),
                    i128::from(b.tsc - a.tsc),
                );
                let (dp_pa, dt_pa) = (
                    i128::from(a.phc_ns) - i128::from(p.phc_ns),
                    i128::from(a.tsc - p.tsc),
                );
                // |dp_ab/dt_ab − dp_pa/dt_pa| × dp_ab / 2
                let num = (dp_ab * dt_pa - dp_pa * dt_ab).unsigned_abs() * dp_ab.unsigned_abs();
                let den = (2 * dt_ab * dt_pa).unsigned_abs();
                (2 * num + den) / (2 * den)
            }
            _ => 0,
        };
        u32::try_from(base + term).unwrap_or(u32::MAX)
    }

    /// Whether either end of the interval is `Stepped`.
    pub fn stepped(&self) -> bool {
        self.a.state == CLOCK_STEPPED || self.b.is_some_and(|b| b.state == CLOCK_STEPPED)
    }

    /// Calibration usable for PHC-endpoint segments: `b` present, no `Stepped` end, error bound
    /// ≤ `max_err_ns`.
    pub fn ok(&self) -> bool {
        self.b.is_some() && !self.stepped() && self.err_bound_ns() <= self.max_err_ns
    }

    /// Offset bound of the exchange-clock comparison: `max(a.max_error_ns, b.max_error_ns)`.
    pub fn ex_offset_ns(&self) -> u32 {
        self.a.max_error_ns.max(self.b.map_or(0, |b| b.max_error_ns))
    }
}

/// Bounded queue of the most recent `ClockStatus` rows (in seq order, ≤ `CAL_QUEUE_MAX`).
#[derive(Clone, Debug, Default)]
pub struct CalQueue {
    rows: VecDeque<ClockStatusRow>,
}

/// Where a TSC instant falls relative to the queue.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Bracket {
    /// Interpolation interval found.
    Cal(ClockCal),
    /// After the newest row: wait for the next `ClockStatus`.
    Wait,
    /// Before the oldest row kept (or no row at all): no interval will ever exist.
    None,
}

impl CalQueue {
    pub fn push(&mut self, r: ClockStatusRow) {
        self.rows.push_back(r);
        while self.rows.len() > CAL_QUEUE_MAX {
            self.rows.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn rows(&self) -> impl Iterator<Item = &ClockStatusRow> {
        self.rows.iter()
    }

    pub fn last(&self) -> Option<&ClockStatusRow> {
        self.rows.back()
    }

    /// Second newest row (the newest complete interval's start).
    pub fn second_last(&self) -> Option<&ClockStatusRow> {
        self.rows.len().checked_sub(2).and_then(|i| self.rows.get(i))
    }

    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// The interval containing `tsc`.
    pub fn bracket(&self, tsc: u64, max_err_ns: u32) -> Bracket {
        let n = self.rows.len();
        let (Some(first), Some(last)) = (self.rows.front(), self.rows.back()) else {
            return Bracket::None;
        };
        if tsc < first.tsc {
            return Bracket::None;
        }
        if tsc > last.tsc || n == 1 {
            return Bracket::Wait;
        }
        for i in 0..n - 1 {
            let (a, b) = (self.rows[i], self.rows[i + 1]);
            if a.tsc <= tsc && tsc <= b.tsc {
                let prev = i.checked_sub(1).map(|j| self.rows[j]);
                return Bracket::Cal(ClockCal::new(prev, a, Some(b), max_err_ns));
            }
        }
        Bracket::Wait
    }

    /// The last row as an open interval (no `b`): used when the journal ends before the next
    /// `ClockStatus` (the sample is then `unsynced_cal`).
    pub fn open_tail(&self, max_err_ns: u32) -> Option<ClockCal> {
        let n = self.rows.len();
        let a = *self.rows.back()?;
        let prev = n.checked_sub(2).map(|j| self.rows[j]);
        Some(ClockCal::new(prev, a, None, max_err_ns))
    }
}
