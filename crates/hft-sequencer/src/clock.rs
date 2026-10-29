//! Engine time (README "引擎时间(engine_ns)"): the base `E` of the current run, the
//! multiply-shift conversion with ±1 correction and its range checks.

use hft_strategy_sdk::EngineClockAnchor;
use hft_types::Tsc;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Section 25 row (journal-format): the current run's engine clock base.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct EngineClockRow {
    pub base_engine_ns: i64,
    /// Record-header tsc of the `EngineClock` record.
    pub base_tsc: u64,
    pub tsc_hz_milli: u64,
    /// Seq of the `EngineClock` record.
    pub clock_seq: u64,
}
const _: () = assert!(core::mem::size_of::<EngineClockRow>() == 32);

/// Section 20 row: `last_day_rolled` (UTC day number).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SeqDayRow {
    pub last_day_rolled: u32,
    pub _pad: u32,
}
const _: () = assert!(core::mem::size_of::<SeqDayRow>() == 8);

/// Shift of the multiplier (`mult = ⌊2^58 × 10^12 / H⌋`).
pub const SHIFT: u32 = 58;
/// Largest Δ (exclusive): `2^58`.
pub const DELTA_MAX: u64 = 1 << SHIFT;
const TERA: u128 = 1_000_000_000_000;

/// The conversion range is broken (`Fatal(ClockRange)`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ClockRange;

/// Section 32 row (journal-format): the engine UTC line of the current run after its last
/// applied `UtcAnchor` (README "引擎 UTC"). All zero = no anchor applied in this run: the UTC of a
/// record is then the current segment's calibration pair converted with `EngineClock.tsc_hz_milli`.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct UtcLineRow {
    /// Record-header tsc of the `UtcAnchor` that set the line.
    pub tsc: u64,
    /// UTC of the anchor's target line at `tsc`.
    pub utc_ns: i64,
    /// Counter frequency × 1000 of the line (the anchor's measured rate, or `EngineClock`'s).
    pub hz_milli: u64,
    /// How far the engine UTC was ahead of the target line at `tsc`; removed at `UTC_SLEW_PPM`.
    pub lead_ns: u64,
}
const _: () = assert!(core::mem::size_of::<UtcLineRow>() == 32);
const _: () = assert!(core::mem::offset_of!(UtcLineRow, utc_ns) == 8);
const _: () = assert!(core::mem::offset_of!(UtcLineRow, hz_milli) == 16);
const _: () = assert!(core::mem::offset_of!(UtcLineRow, lead_ns) == 24);

/// `UTC_SLEW_PPM`: rate at which an engine UTC that is ahead of CLOCK_REALTIME is slowed down
/// (never stepped back), 500 ppm. `UTC_RATE_TOL_PPM`: a `UtcAnchor.hz_milli` further than this
/// from `EngineClock.tsc_hz_milli` is not used. Protocol constants (replay recomputes them),
/// defined once in hft-rt with the published warm-reader line.
pub use hft_rt::{UTC_RATE_TOL_PPM, UTC_SLEW_PPM};

/// `base + ⌊(t − t0) × 10^12 / hz_milli⌋` (floor, i128 intermediate, saturating; `hz_milli = 0` →
/// `base`): the SDK `EngineClockAnchor::utc_of` formula.
fn add_scaled(base: i64, t: u64, t0: u64, hz_milli: u64) -> i64 {
    if hz_milli == 0 {
        return base;
    }
    let num = (i128::from(t) - i128::from(t0)) * 1_000_000_000_000;
    let r = i128::from(base) + num.div_euclid(i128::from(hz_milli));
    r.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

impl UtcLineRow {
    /// A `UtcAnchor` has been applied in this run.
    #[inline]
    pub fn is_set(&self) -> bool {
        self.hz_milli != 0
    }

    /// Engine UTC of a record-header tsc `t` ≥ `self.tsc`:
    /// `utc_ns + e + max(0, lead_ns − ⌊e × UTC_SLEW_PPM / 10^6⌋)`, `e = ⌊(t − tsc) × 10^12 / hz⌋`.
    /// Non-decreasing in `t` (the lead shrinks by at most 500 ns per 1 ms of line time).
    #[inline]
    pub fn utc_at(&self, t: Tsc) -> i64 {
        self.with_elapsed(add_scaled(0, t.0, self.tsc, self.hz_milli))
    }

    /// The line's UTC after `e` ns of line time (`e` from the floor formula of `utc_at`).
    #[inline]
    fn with_elapsed(&self, e: i64) -> i64 {
        let decay = (i128::from(e.max(0)) * i128::from(UTC_SLEW_PPM) / 1_000_000) as u128;
        let lead = u128::from(self.lead_ns).saturating_sub(decay);
        let v = i128::from(self.utc_ns) + i128::from(e) + lead as i128;
        v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }

    /// The line after the `UtcAnchor` `a` recorded at `t`, when the engine UTC at `t` was `cur`:
    /// target `T(x) = a.realtime_ns + ⌊(x − a.tsc) × 10^12 / H⌋` (`H` = `a.hz_milli` when within
    /// `UTC_RATE_TOL_PPM` of `tsc_hz_milli`, else `tsc_hz_milli`). The engine UTC at `t` becomes
    /// `max(cur, T(t))`: behind → steps forward onto the target; ahead → keeps `cur` and slews.
    pub fn anchored(cur: i64, t: Tsc, a: &hft_event::UtcAnchor, tsc_hz_milli: u64) -> UtcLineRow {
        let tol = u128::from(tsc_hz_milli) * u128::from(UTC_RATE_TOL_PPM) / 1_000_000;
        let hz = if a.hz_milli != 0 && u128::from(a.hz_milli.abs_diff(tsc_hz_milli)) <= tol {
            a.hz_milli
        } else {
            tsc_hz_milli
        };
        let target = add_scaled(a.realtime_ns, t.0, a.tsc.0, hz);
        let lead = (i128::from(cur) - i128::from(target)).clamp(0, i128::from(i64::MAX)) as u64;
        UtcLineRow {
            tsc: t.0,
            utc_ns: target,
            hz_milli: hz,
            lead_ns: lead,
        }
    }
}

/// The base of the current run plus the precomputed multiplier, and the run's UTC line.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct EngineClock {
    pub row: EngineClockRow,
    mult: u64,
    /// Section 32; reset by every `EngineClock` (a new run starts from its segment's pair).
    utc: UtcLineRow,
    /// `multiplier(utc.hz_milli)` (0 without a line).
    utc_mult: u64,
}

/// `mult = ⌊2^58 × 10^12 / H⌋`; requires `mult < 2^64` (H > 10^12 / 64).
pub fn multiplier(hz_milli: u64) -> Result<u64, ClockRange> {
    if hz_milli == 0 {
        return Err(ClockRange);
    }
    let m = (TERA << SHIFT) / u128::from(hz_milli);
    u64::try_from(m).map_err(|_| ClockRange)
}

/// `⌊Δ × 10^12 / H⌋` by multiply-shift with the ±1 correction (bit-identical to the definition).
#[inline]
pub fn scaled(delta: u64, hz_milli: u64, mult: u64) -> u64 {
    let q = ((u128::from(delta) * u128::from(mult)) >> SHIFT) as u64;
    // q ∈ {exact − 1, exact}: one u128 comparison decides the +1.
    if (u128::from(q) + 1) * u128::from(hz_milli) <= u128::from(delta) * TERA {
        q + 1
    } else {
        q
    }
}

impl EngineClock {
    /// Base from an `EngineClock` record or a section-25 row.
    pub fn new(row: EngineClockRow) -> Result<EngineClock, ClockRange> {
        Ok(EngineClock {
            mult: multiplier(row.tsc_hz_milli)?,
            row,
            utc: UtcLineRow::default(),
            utc_mult: 0,
        })
    }
    /// The run's UTC line (section 32; all zero = none).
    #[inline]
    pub fn utc(&self) -> &UtcLineRow {
        &self.utc
    }
    /// Installs a UTC line (`UtcAnchor`, snapshot restore); all zero clears it. A line frequency
    /// outside the conversion range → `ClockRange`.
    pub fn set_utc(&mut self, l: UtcLineRow) -> Result<(), ClockRange> {
        self.utc_mult = if l.is_set() { multiplier(l.hz_milli)? } else { 0 };
        self.utc = l;
        Ok(())
    }
    /// `UtcLineRow::utc_at` without a u128 division for `t ≥ utc.tsc` (multiply-shift with the
    /// ±1 correction, as `ens_of`).
    #[inline]
    fn line_utc(&self, t: Tsc) -> i64 {
        let l = &self.utc;
        match t.0.checked_sub(l.tsc) {
            Some(d) if d < DELTA_MAX => {
                let e = scaled(d, l.hz_milli, self.utc_mult);
                l.with_elapsed(i64::try_from(e).unwrap_or(i64::MAX))
            }
            _ => l.utc_at(t),
        }
    }
    /// The engine time of a record-header tsc of this run.
    #[inline]
    pub fn ens_of(&self, t: Tsc) -> Result<i64, ClockRange> {
        let delta = t.0.checked_sub(self.row.base_tsc).ok_or(ClockRange)?;
        if delta >= DELTA_MAX {
            return Err(ClockRange);
        }
        let q = scaled(delta, self.row.tsc_hz_milli, self.mult);
        self.row
            .base_engine_ns
            .checked_add(i64::try_from(q).map_err(|_| ClockRange)?)
            .ok_or(ClockRange)
    }
    /// The SDK anchor of this run with the segment's UTC calibration pair.
    pub fn anchor(&self, utc_tsc: Tsc, utc_ns: i64) -> EngineClockAnchor {
        EngineClockAnchor {
            base_engine_ns: self.row.base_engine_ns,
            base_tsc: Tsc(self.row.base_tsc),
            tsc_hz_milli: self.row.tsc_hz_milli,
            utc_tsc,
            utc_ns,
        }
    }
    /// The SDK anchor of the record at `t`: the segment pair `(seg_tsc, seg_utc_ns)` until a
    /// `UtcAnchor` has been applied in this run, afterwards `(t, utc.utc_at(t))`.
    #[inline]
    pub fn anchor_at(&self, t: Tsc, seg_tsc: Tsc, seg_utc_ns: i64) -> EngineClockAnchor {
        if self.utc.is_set() {
            self.anchor(t, self.line_utc(t))
        } else {
            self.anchor(seg_tsc, seg_utc_ns)
        }
    }
    pub fn is_set(&self) -> bool {
        self.mult != 0
    }
}

/// UTC day number of a UTC ns (days since 1970-01-01).
pub fn utc_day(utc_ns: i64) -> u32 {
    u32::try_from(utc_ns.div_euclid(86_400_000_000_000)).unwrap_or(0)
}
