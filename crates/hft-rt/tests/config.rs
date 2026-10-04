//! `[rt]` defaults and validation (RT-130, RT-131).
#![cfg(not(loom))]

use std::path::PathBuf;

use hft_rt::{HostMode, RtConfig};

#[test]
fn rt_130_defaults() {
    let c = RtConfig::default();
    assert_eq!(c.calib.rounds, 5);
    assert_eq!(c.calib.samples, 8);
    assert_eq!(c.calib.phc_samples, 10);
    assert_eq!(c.calib.cpuid_tolerance_ppm, 100);
    assert_eq!(c.calib.step_threshold_ns, 50_000);
    assert_eq!(c.phc_device, PathBuf::from("/dev/ptp_ena"));
    assert_eq!(c.shm_dir, PathBuf::from("/dev/hugepages2M/hft/rings"));
    assert_eq!(c.stall_checks, 3);
    assert_eq!(c.host_mode, HostMode::Prod);
    assert_eq!(c.validate(), Ok(()));
}

#[test]
fn rt_131_validate_bounds() {
    type Edit = fn(&mut RtConfig);
    let rows: [(&str, Edit, Result<(), &str>); 13] = [
        ("RT-131.1", |c| c.calib.rounds = 0, Err("rt.calib.rounds")),
        ("RT-131.2", |c| c.calib.rounds = 1, Ok(())),
        ("RT-131.3", |c| c.calib.samples = 0, Err("rt.calib.samples")),
        ("RT-131.4", |c| c.calib.samples = 1, Ok(())),
        ("RT-131.5", |c| c.calib.phc_samples = 0, Err("rt.phc.samples")),
        ("RT-131.6", |c| c.calib.phc_samples = 1, Ok(())),
        ("RT-131.7", |c| c.calib.phc_samples = 25, Ok(())),
        ("RT-131.8", |c| c.calib.phc_samples = 26, Err("rt.phc.samples")),
        (
            "RT-131.9",
            |c| c.calib.step_threshold_ns = 0,
            Err("rt.clock.step_threshold_ns"),
        ),
        ("RT-131.10", |c| c.calib.step_threshold_ns = 1, Ok(())),
        (
            "RT-131.11",
            |c| c.stall_checks = 0,
            Err("rt.heartbeat.stall_checks"),
        ),
        ("RT-131.12", |c| c.stall_checks = 1, Ok(())),
        ("RT-131.13", |c| c.calib.cpuid_tolerance_ppm = 0, Ok(())),
    ];
    for (row, edit, want) in rows {
        let mut c = RtConfig::default();
        edit(&mut c);
        assert_eq!(c.validate(), want, "{row}");
    }
}
