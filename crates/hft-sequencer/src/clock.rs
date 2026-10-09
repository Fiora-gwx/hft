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

/// The base of the current run plus the precomputed multiplier.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct EngineClock {
    pub row: EngineClockRow,
    mult: u64,
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
        })
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
    pub fn is_set(&self) -> bool {
        self.mult != 0
    }
}

/// UTC day number of a UTC ns (days since 1970-01-01).
pub fn utc_day(utc_ns: i64) -> u32 {
    u32::try_from(utc_ns.div_euclid(86_400_000_000_000)).unwrap_or(0)
}
