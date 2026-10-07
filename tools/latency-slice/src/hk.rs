//! `hk` thread work: counter calibration and one `ClockStatus` per 1 000 ms (README "线程布局").

use std::path::Path;

use hft_event::ClockStatus;
use hft_rt::{
    CalibConfig, CalibPair, ClockVerdict, DriftTracker, Phc, SysClock, TscClock, sample_pair,
    sample_pair_with, tsc_hz_nominal,
};

/// `ClockSource` codes (hft-event).
const SRC_PHC: u8 = 1;
const SRC_NTP: u8 = 2;
/// `ClockState` codes.
const SYNCED: u8 = 1;
const UNSYNCED: u8 = 2;
const STEPPED: u8 = 3;

/// The calibrated counter clock: `TscClock::calibrate`, or (off Linux and where the invariance
/// check fails, e.g. some VMs) the same measurement without that check.
pub fn calibrate() -> Result<TscClock, String> {
    let cfg = CalibConfig::default();
    match TscClock::calibrate(&cfg) {
        Ok(c) => Ok(c),
        Err(_) => {
            let relaxed = CalibConfig {
                cpuid_tolerance_ppm: u32::MAX,
                ..cfg
            };
            TscClock::calibrate_with(&mut SysClock, None, &relaxed)
                .map_err(|e| format!("TscClock calibration: {e:?}"))
        }
    }
}

/// `ClockStatus` sampler.
#[derive(Debug)]
pub struct Hk {
    clock: TscClock,
    phc: Option<Phc>,
    drift: DriftTracker,
}

impl Hk {
    /// `phc = None` and `detect`: the first of `/dev/ptp_ena`, `/dev/ptp0` that opens, if any.
    pub fn new(clock: TscClock, phc: Option<&Path>, detect: bool) -> Result<Hk, String> {
        let phc = match phc {
            Some(p) => Some(Phc::open(p).map_err(|e| format!("{}: {e:?}", p.display()))?),
            None if detect => ["/dev/ptp_ena", "/dev/ptp0"]
                .iter()
                .find_map(|d| Phc::open(Path::new(d)).ok()),
            None => None,
        };
        Ok(Hk {
            clock,
            phc,
            drift: DriftTracker::new(CalibConfig::default().step_threshold_ns),
        })
    }

    /// Whether a PHC is read.
    pub fn has_phc(&self) -> bool {
        self.phc.is_some()
    }

    /// One calibration pair as a `ClockStatus`.
    pub fn sample(&mut self) -> ClockStatus {
        let p: CalibPair = match sample_pair(&self.clock, self.phc.as_ref()) {
            Ok(p) => p,
            Err(_) => sample_pair_with(&mut SysClock, self.clock.hz_milli, None, 8),
        };
        let state = match self.drift.push(p) {
            ClockVerdict::Synced => SYNCED,
            ClockVerdict::Unsynced => UNSYNCED,
            ClockVerdict::Stepped { .. } => STEPPED,
        };
        ClockStatus {
            tsc: p.tsc,
            realtime_ns: p.realtime_ns,
            // Without a PHC there are no HW stamps; the PHC domain is REALTIME then.
            phc_ns: if self.phc.is_some() {
                p.phc_ns
            } else {
                p.realtime_ns
            },
            tsc_hz_milli: self.clock.hz_milli,
            chrony_offset_ns: 0,
            max_error_ns: max_error_ns(),
            rms_ns: 0,
            calib_err_ns: p.err_ns,
            source: if self.phc.is_some() { SRC_PHC } else { SRC_NTP },
            state,
            _pad: 0,
        }
    }
}

/// The kernel's estimated maximum error of `CLOCK_REALTIME` (`adjtimex`), ns; 0 when unknown.
pub fn max_error_ns() -> u32 {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: `timex` is plain data; zeroed is a valid "read only" request (`modes = 0`), and
        // `adjtimex` only writes into the struct we own.
        let mut t: libc::timex = unsafe { std::mem::zeroed() };
        // SAFETY: valid pointer to an owned `timex`.
        let r = unsafe { libc::adjtimex(&mut t) };
        if r < 0 {
            return 0;
        }
        u32::try_from(t.maxerror.saturating_mul(1_000)).unwrap_or(u32::MAX)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// CNTVCT_EL0 / TSC nominal frequency (Hz), recorded in the report.
pub fn nominal_hz() -> u64 {
    tsc_hz_nominal().unwrap_or(0)
}
