//! Counter reads (`tsc_now`, `tsc_ordered`, `tsc_hz_nominal`), `TscClock` and `TscUtc`.
//!
//! "TSC" is the repo-wide name of the CPU counter: `CNTVCT_EL0` on aarch64 (main path), `RDTSC`
//! on x86_64 (optional metal profile); other targets fall back to a monotonic nanosecond clock.

pub use hft_types::Tsc;

/// Fastest counter read (rx stamps).
#[inline(always)]
pub fn tsc_now() -> Tsc {
    #[cfg(target_arch = "aarch64")]
    {
        let v: u64;
        // SAFETY: reading CNTVCT_EL0 has no side effects (user access enabled by the kernel).
        unsafe {
            core::arch::asm!("mrs {}, cntvct_el0", out(reg) v, options(nomem, nostack, preserves_flags))
        };
        Tsc(v)
    }
    #[cfg(target_arch = "x86_64")]
    {
        #[allow(unused_unsafe)]
        // SAFETY: RDTSC has no preconditions.
        let v = unsafe { core::arch::x86_64::_rdtsc() };
        Tsc(v)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        Tsc(fallback_ns())
    }
}

/// Counter read ordered after preceding instructions (interval end points).
#[inline(always)]
pub fn tsc_ordered() -> Tsc {
    #[cfg(target_arch = "aarch64")]
    {
        let v: u64;
        // SAFETY: ISB + read of CNTVCT_EL0; no memory effects (deliberately not `nomem`, so the
        // compiler keeps it ordered with surrounding memory accesses).
        unsafe {
            core::arch::asm!("isb", "mrs {}, cntvct_el0", out(reg) v, options(nostack, preserves_flags))
        };
        Tsc(v)
    }
    #[cfg(target_arch = "x86_64")]
    {
        let mut aux = 0u32;
        #[allow(unused_unsafe)]
        // SAFETY: RDTSCP writes the TSC_AUX value into `aux`, a valid u32.
        let v = unsafe { core::arch::x86_64::__rdtscp(&mut aux) };
        Tsc(v)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        Tsc(fallback_ns())
    }
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
#[allow(clippy::disallowed_methods)] // allow-list: counter fallback on targets without a cycle counter
fn fallback_ns() -> u64 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}

/// Nominal counter frequency in Hz: aarch64 `CNTFRQ_EL0`; x86 CPUID 0x15 when it reports a
/// crystal; `None` otherwise.
pub fn tsc_hz_nominal() -> Option<u64> {
    #[cfg(target_arch = "aarch64")]
    {
        let v: u64;
        // SAFETY: reading CNTFRQ_EL0 has no side effects.
        unsafe {
            core::arch::asm!("mrs {}, cntfrq_el0", out(reg) v, options(nomem, nostack, preserves_flags))
        };
        if v == 0 { None } else { Some(v) }
    }
    #[cfg(target_arch = "x86_64")]
    {
        use core::arch::x86_64::{__cpuid, __get_cpuid_max};
        #[allow(unused_unsafe)]
        // SAFETY: CPUID is available on every x86_64 CPU.
        let (max, _) = unsafe { __get_cpuid_max(0) };
        if max < 0x15 {
            return None;
        }
        #[allow(unused_unsafe)]
        // SAFETY: leaf 0x15 is supported (checked above).
        let r = unsafe { __cpuid(0x15) };
        if r.eax == 0 || r.ebx == 0 || r.ecx == 0 {
            return None;
        }
        Some((r.ecx as u64).saturating_mul(r.ebx as u64) / r.eax as u64)
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        Some(1_000_000_000)
    }
}

/// TSC ↔ UTC calibration pair.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CalibPair {
    pub tsc: Tsc,
    pub realtime_ns: i64,
    /// 0 = no PHC.
    pub phc_ns: i64,
    /// Error bound.
    pub err_ns: u32,
}

impl Default for CalibPair {
    fn default() -> CalibPair {
        CalibPair {
            tsc: Tsc(0),
            realtime_ns: 0,
            phc_ns: 0,
            err_ns: 0,
        }
    }
}

/// TSC → UTC line; distributed to warm and gw readers through `UtcClock`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct TscUtc {
    pub base_tsc: Tsc,
    pub base_utc_ns: i64,
    pub mult: u64,
    pub shift: u32,
    pub err_ns: u32,
}

#[inline(always)]
fn utc(base_tsc: Tsc, base_utc_ns: i64, mult: u64, shift: u32, t: Tsc) -> i64 {
    let d = t.0 as i128 - base_tsc.0 as i128;
    let off = d.saturating_mul(mult as i128) >> shift.min(127);
    let v = (base_utc_ns as i128).saturating_add(off);
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

impl TscUtc {
    /// `base_utc_ns + ((t − base_tsc) × mult) >> shift` (i128, signed difference).
    #[inline]
    pub fn utc_ns(&self, t: Tsc) -> i64 {
        utc(self.base_tsc, self.base_utc_ns, self.mult, self.shift, t)
    }
}

/// Calibration failures.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ClockErr {
    NotInvariant,
    Unstable,
    Mismatch,
    /// PHC open/ioctl failure (errno).
    Phc(i32),
    /// Other system call failure (errno).
    Os(i32),
}

/// Every reading calibration needs; `SysClock` reads the real ones, tests inject sequences.
pub trait ClockReads {
    fn tsc(&mut self) -> Tsc;
    fn mono_raw_ns(&mut self) -> i64;
    fn realtime_ns(&mut self) -> i64;
    fn sleep_ms(&mut self, ms: u32);
}

/// Calibrated counter clock.
#[derive(Copy, Clone, Debug)]
pub struct TscClock {
    pub hz_milli: u64,
    mult: u64,
    shift: u32,
    pub base: CalibPair,
    pub(crate) samples: u8,
    pub(crate) phc_samples: u8,
}

impl TscClock {
    /// Startup, cold, ~`rounds` × 200 ms: invariance check, then `calibrate_with(&mut SysClock)`.
    pub fn calibrate(cfg: &crate::CalibConfig) -> Result<TscClock, ClockErr> {
        if !crate::host::counter_invariant_here() {
            return Err(ClockErr::NotInvariant);
        }
        let nominal = tsc_hz_nominal().map(|hz| hz.saturating_mul(1000));
        let mut c = TscClock::calibrate_with(&mut crate::calib::SysClock, nominal, cfg)?;
        c.phc_samples = cfg.phc_samples;
        Ok(c)
    }

    /// Steps 2–5 of the calibration with injected readings.
    pub fn calibrate_with<R: ClockReads>(
        src: &mut R,
        nominal_hz_milli: Option<u64>,
        cfg: &crate::CalibConfig,
    ) -> Result<TscClock, ClockErr> {
        let rounds = cfg.rounds.max(1) as usize;
        let samples = cfg.samples.max(1);
        let mut hz = [0u64; 255];
        for slot in hz.iter_mut().take(rounds) {
            let (t0, m0) = narrowest_mono(src, samples);
            src.sleep_ms(200);
            let (t1, m1) = narrowest_mono(src, samples);
            let dns = m1.saturating_sub(m0);
            if dns <= 0 || t1 < t0 {
                return Err(ClockErr::Unstable);
            }
            let dt = (t1 - t0) as u128;
            let dns = dns as u128;
            *slot = ((dt * 1_000_000_000_000 + dns / 2) / dns).min(u64::MAX as u128) as u64;
        }
        let v = &mut hz[..rounds];
        v.sort_unstable();
        let median = if rounds % 2 == 1 {
            v[rounds / 2]
        } else {
            ((v[rounds / 2 - 1] as u128 + v[rounds / 2] as u128) / 2) as u64
        };
        let spread = v[rounds - 1] - v[0];
        if spread as u128 * 1_000_000 > median as u128 {
            return Err(ClockErr::Unstable);
        }
        if let Some(nom) = nominal_hz_milli {
            let dev = median.abs_diff(nom) as u128;
            if dev * 1_000_000 > cfg.cpuid_tolerance_ppm as u128 * nom as u128 {
                return Err(ClockErr::Mismatch);
            }
        }
        let base = crate::calib::sample_pair_with(src, median, None, samples);
        let mut c = TscClock::from_hz_milli(median, base);
        c.samples = samples;
        c.phc_samples = cfg.phc_samples;
        Ok(c)
    }

    /// `shift = 32`, `mult = round(10^12 × 2^32 / hz_milli)`.
    pub fn from_hz_milli(hz_milli: u64, base: CalibPair) -> TscClock {
        let mult = if hz_milli == 0 {
            0
        } else {
            let num: u128 = 1_000_000_000_000u128 << 32;
            ((num + hz_milli as u128 / 2) / hz_milli as u128).min(u64::MAX as u128) as u64
        };
        TscClock {
            hz_milli,
            mult,
            shift: 32,
            base,
            samples: 8,
            phc_samples: 10,
        }
    }

    /// `(Δ × mult) >> shift` with a u128 intermediate; `to < from` → 0; saturates at u64::MAX.
    #[inline(always)]
    pub fn delta_ns(&self, from: Tsc, to: Tsc) -> u64 {
        if to.0 <= from.0 {
            return 0;
        }
        let v = ((to.0 - from.0) as u128 * self.mult as u128) >> self.shift;
        if v > u64::MAX as u128 { u64::MAX } else { v as u64 }
    }

    /// UTC by the `base` pair; same formula as `TscUtc::utc_ns`.
    #[inline]
    pub fn to_utc_ns(&self, t: Tsc) -> i64 {
        utc(self.base.tsc, self.base.realtime_ns, self.mult, self.shift, t)
    }

    /// A `TscUtc` based on `pair` with this clock's mult/shift.
    pub fn tsc_utc(&self, pair: &CalibPair) -> TscUtc {
        TscUtc {
            base_tsc: pair.tsc,
            base_utc_ns: pair.realtime_ns,
            mult: self.mult,
            shift: self.shift,
            err_ns: pair.err_ns,
        }
    }

    /// Microseconds → counter ticks (u128 intermediate, saturating).
    pub(crate) fn us_to_ticks(&self, us: u64) -> u64 {
        let v = us as u128 * self.hz_milli as u128 / 1_000_000_000;
        v.min(u64::MAX as u128) as u64
    }
}

/// "TSC — mono — TSC" × `samples`; the narrowest window (earliest on ties); TSC at its midpoint.
fn narrowest_mono<R: ClockReads>(src: &mut R, samples: u8) -> (u64, i64) {
    let mut best: Option<(u64, u64, i64)> = None;
    for _ in 0..samples.max(1) {
        let a = src.tsc().0;
        let m = src.mono_raw_ns();
        let b = src.tsc().0;
        let w = b.saturating_sub(a);
        if best.is_none_or(|(bw, _, _)| w < bw) {
            best = Some((w, a + w / 2, m));
        }
    }
    let (_, t, m) = best.unwrap_or((0, 0, 0));
    (t, m)
}
