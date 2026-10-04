//! `[rt]` configuration (defaults = README "配置项" table).

use std::path::PathBuf;

/// Calibration settings (`rt.calib.*`, `rt.phc.samples`, `rt.clock.step_threshold_ns`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CalibConfig {
    pub rounds: u8,
    pub samples: u8,
    pub phc_samples: u8,
    pub cpuid_tolerance_ppm: u32,
    pub step_threshold_ns: u32,
}

/// `rt.host.mode`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HostMode {
    Prod,
    Dev,
}

/// The `[rt]` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RtConfig {
    pub calib: CalibConfig,
    pub phc_device: PathBuf,
    pub shm_dir: PathBuf,
    pub stall_checks: u32,
    pub host_mode: HostMode,
}

impl Default for CalibConfig {
    fn default() -> CalibConfig {
        CalibConfig {
            rounds: 5,
            samples: 8,
            phc_samples: 10,
            cpuid_tolerance_ppm: 100,
            step_threshold_ns: 50_000,
        }
    }
}

impl Default for RtConfig {
    fn default() -> RtConfig {
        RtConfig {
            calib: CalibConfig::default(),
            phc_device: PathBuf::from("/dev/ptp_ena"),
            shm_dir: PathBuf::from("/dev/hugepages2M/hft/rings"),
            stall_checks: 3,
            host_mode: HostMode::Prod,
        }
    }
}

impl RtConfig {
    /// The first failing key: rounds, samples, stall_checks, step_threshold_ns ≥ 1;
    /// phc_samples ∈ [1, 25].
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.calib.rounds == 0 {
            return Err("rt.calib.rounds");
        }
        if self.calib.samples == 0 {
            return Err("rt.calib.samples");
        }
        if !(1..=25).contains(&self.calib.phc_samples) {
            return Err("rt.phc.samples");
        }
        if self.calib.step_threshold_ns == 0 {
            return Err("rt.clock.step_threshold_ns");
        }
        if self.stall_checks == 0 {
            return Err("rt.heartbeat.stall_checks");
        }
        Ok(())
    }
}
