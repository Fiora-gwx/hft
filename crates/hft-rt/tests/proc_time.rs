//! No PHC: open fails, sampling still works (RT-058). Linux.
#![cfg(all(target_os = "linux", not(loom)))]

use std::path::Path;

use hft_rt::{CalibPair, ClockErr, Phc, TscClock, sample_pair};

#[test]
fn rt_058_no_phc() {
    assert_eq!(
        Phc::open(Path::new("/dev/ptp_nonexistent")).err(),
        Some(ClockErr::Phc(2))
    );
    let clock = TscClock::from_hz_milli(1_000_000_000_000, CalibPair::default());
    let p = sample_pair(&clock, None).unwrap();
    assert_eq!(p.phc_ns, 0);
    assert!(p.err_ns > 0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    assert!((p.realtime_ns - now).abs() < 1_000_000_000);
}
