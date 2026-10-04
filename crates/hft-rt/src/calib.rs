//! Calibration pairs (`sample_pair`), PHC offsets and `DriftTracker`.

use std::path::Path;

use crate::tsc::{CalibPair, ClockErr, ClockReads, Tsc, TscClock, tsc_ordered};

/// Real readings: the counter (`tsc_ordered`), `CLOCK_MONOTONIC_RAW`, `CLOCK_REALTIME`, sleep.
#[derive(Debug, Default, Clone, Copy)]
pub struct SysClock;

#[allow(clippy::disallowed_methods)] // allow-list: startup clock calibration reads the system clocks
impl ClockReads for SysClock {
    fn tsc(&mut self) -> Tsc {
        tsc_ordered()
    }

    fn mono_raw_ns(&mut self) -> i64 {
        #[cfg(target_os = "linux")]
        {
            crate::sys::clock_ns(libc::CLOCK_MONOTONIC_RAW)
        }
        #[cfg(not(target_os = "linux"))]
        {
            use std::sync::OnceLock;
            static START: OnceLock<std::time::Instant> = OnceLock::new();
            START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as i64
        }
    }

    fn realtime_ns(&mut self) -> i64 {
        #[cfg(target_os = "linux")]
        {
            crate::sys::clock_ns(libc::CLOCK_REALTIME)
        }
        #[cfg(not(target_os = "linux"))]
        {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0)
        }
    }

    fn sleep_ms(&mut self, ms: u32) {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    }
}

/// PHC ↔ system clock offset from the narrowest `PTP_SYS_OFFSET_EXTENDED` group.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PhcOffset {
    pub phc_ns: i64,
    /// Midpoint of the narrowest group.
    pub sys_ns: i64,
    /// Half the narrowest group's width (rounded up).
    pub err_ns: u32,
}

/// Warm: `sample_pair_with(&mut SysClock, clock.hz_milli, PHC reading, rt.calib.samples)`.
pub fn sample_pair(clock: &TscClock, phc: Option<&Phc>) -> Result<CalibPair, ClockErr> {
    let off = match phc {
        Some(p) => Some(p.offset(clock.phc_samples)?),
        None => None,
    };
    Ok(sample_pair_with(
        &mut SysClock,
        clock.hz_milli,
        off.as_ref(),
        clock.samples,
    ))
}

/// "TSC — REALTIME — TSC" × `samples`; narrowest window (earliest on ties); TSC midpoint rounded
/// down; `err_ns` = half the window in ns rounded up, plus the PHC error bound.
pub fn sample_pair_with<R: ClockReads>(
    src: &mut R,
    hz_milli: u64,
    phc: Option<&PhcOffset>,
    samples: u8,
) -> CalibPair {
    let mut best: Option<(u64, u64, i64)> = None;
    for _ in 0..samples.max(1) {
        let a = src.tsc().0;
        let r = src.realtime_ns();
        let b = src.tsc().0;
        let w = b.saturating_sub(a);
        if best.is_none_or(|(bw, _, _)| w < bw) {
            best = Some((w, a + w / 2, r));
        }
    }
    let (w, tsc, realtime_ns) = best.unwrap_or((0, 0, 0));
    let err = if hz_milli == 0 {
        u32::MAX as u128
    } else {
        (w as u128 * 1_000_000_000_000).div_ceil(2 * hz_milli as u128)
    };
    let mut err_ns = err.min(u32::MAX as u128) as u32;
    let mut phc_ns = 0;
    if let Some(o) = phc {
        phc_ns = realtime_ns.saturating_add(o.phc_ns.saturating_sub(o.sys_ns));
        err_ns = err_ns.saturating_add(o.err_ns);
    }
    CalibPair {
        tsc: Tsc(tsc),
        realtime_ns,
        phc_ns,
        err_ns,
    }
}

/// An open PTP hardware clock (`/dev/ptp*`).
#[derive(Debug)]
pub struct Phc {
    #[cfg(target_os = "linux")]
    fd: i32,
}

impl Phc {
    /// Open failure → `Phc(errno)`.
    pub fn open(dev: &Path) -> Result<Phc, ClockErr> {
        #[cfg(target_os = "linux")]
        {
            let c = crate::sys::cpath(dev).ok_or(ClockErr::Phc(libc::EINVAL))?;
            // SAFETY: valid NUL-terminated path.
            let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
            if fd < 0 {
                return Err(ClockErr::Phc(crate::sys::errno()));
            }
            Ok(Phc { fd })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = dev;
            Err(ClockErr::Phc(crate::sys::ENOSYS))
        }
    }

    /// `ioctl(PTP_SYS_OFFSET_EXTENDED)` with `n` (1–25) groups; the narrowest group wins.
    pub fn offset(&self, n: u8) -> Result<PhcOffset, ClockErr> {
        #[cfg(target_os = "linux")]
        {
            #[repr(C)]
            #[derive(Copy, Clone)]
            struct PtpClockTime {
                sec: i64,
                nsec: u32,
                reserved: u32,
            }
            const _: () = assert!(size_of::<PtpClockTime>() == 16);
            #[repr(C)]
            struct PtpSysOffsetExtended {
                n_samples: u32,
                rsv: [u32; 3],
                ts: [[PtpClockTime; 3]; 25],
            }
            const SIZE: u64 = size_of::<PtpSysOffsetExtended>() as u64;
            // _IOWR('=', 9, struct ptp_sys_offset_extended)
            const REQ: u64 = (3 << 30) | (SIZE << 16) | ((b'=' as u64) << 8) | 9;
            let n = n.clamp(1, 25);
            let zero = PtpClockTime {
                sec: 0,
                nsec: 0,
                reserved: 0,
            };
            let mut req = PtpSysOffsetExtended {
                n_samples: n as u32,
                rsv: [0; 3],
                ts: [[zero; 3]; 25],
            };
            // SAFETY: `req` is the kernel's struct layout for this request and outlives the call.
            let r = unsafe { libc::ioctl(self.fd, REQ as _, &mut req as *mut PtpSysOffsetExtended) };
            if r != 0 {
                return Err(ClockErr::Phc(crate::sys::errno()));
            }
            let ns = |t: PtpClockTime| t.sec.saturating_mul(1_000_000_000).saturating_add(t.nsec as i64);
            let mut best: Option<(i64, PhcOffset)> = None;
            for g in req.ts.iter().take(n as usize) {
                let (before, phc, after) = (ns(g[0]), ns(g[1]), ns(g[2]));
                let w = after.saturating_sub(before).max(0);
                if best.is_none_or(|(bw, _)| w < bw) {
                    best = Some((
                        w,
                        PhcOffset {
                            phc_ns: phc,
                            sys_ns: before + w / 2,
                            err_ns: ((w as u64).div_ceil(2)).min(u32::MAX as u64) as u32,
                        },
                    ));
                }
            }
            best.map(|(_, o)| o).ok_or(ClockErr::Phc(libc::EINVAL))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = n;
            Err(ClockErr::Phc(crate::sys::ENOSYS))
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for Phc {
    fn drop(&mut self) {
        // SAFETY: the descriptor was opened in `open` and is closed once.
        unsafe { libc::close(self.fd) };
    }
}

/// `DriftTracker::push` verdict.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ClockVerdict {
    Synced,
    Unsynced,
    Stepped { jump_ns: i64 },
}

const WINDOW: usize = 60;

/// Least-squares fit of `realtime_ns` against `tsc` over the last 60 pairs.
#[derive(Debug)]
pub struct DriftTracker {
    step_threshold_ns: u32,
    win: [(u64, i64); WINDOW],
    start: usize,
    len: usize,
}

impl DriftTracker {
    pub fn new(step_threshold_ns: u32) -> DriftTracker {
        DriftTracker {
            step_threshold_ns,
            win: [(0, 0); WINDOW],
            start: 0,
            len: 0,
        }
    }

    /// < 2 pairs in the window → `Unsynced`; |measured − predicted| > threshold → `Stepped` and
    /// the window is cleared; else `Synced`. The pair then joins the window (oldest evicted).
    pub fn push(&mut self, p: CalibPair) -> ClockVerdict {
        let v = if self.len < 2 {
            ClockVerdict::Unsynced
        } else {
            let pred = self.predict(p.tsc.0);
            let jump = (p.realtime_ns as i128 - pred).clamp(i64::MIN as i128, i64::MAX as i128) as i64;
            if jump.unsigned_abs() > self.step_threshold_ns as u64 {
                self.len = 0;
                self.start = 0;
                ClockVerdict::Stepped { jump_ns: jump }
            } else {
                ClockVerdict::Synced
            }
        };
        let idx = (self.start + self.len) % WINDOW;
        self.win[idx] = (p.tsc.0, p.realtime_ns);
        if self.len < WINDOW {
            self.len += 1;
        } else {
            self.start = (self.start + 1) % WINDOW;
        }
        v
    }

    fn at(&self, i: usize) -> (u64, i64) {
        self.win[(self.start + i) % WINDOW]
    }

    /// Predicted realtime at `x` (exact integer arithmetic relative to the oldest point).
    fn predict(&self, x: u64) -> i128 {
        let (x0, y0) = self.at(0);
        let n = self.len as i128;
        let (mut sx, mut sy, mut sxx, mut sxy) = (0i128, 0i128, 0i128, 0i128);
        for i in 0..self.len {
            let (xi, yi) = self.at(i);
            let dx = xi as i128 - x0 as i128;
            let dy = yi as i128 - y0 as i128;
            sx += dx;
            sy += dy;
            sxx += dx * dx;
            sxy += dx * dy;
        }
        let d = n * sxx - sx * sx;
        let num = n * sxy - sx * sy;
        let t = n * (x as i128 - x0 as i128) - sx;
        // n·(pred − y0) = sy + num·t / d
        let term = if d == 0 {
            0
        } else {
            match num.checked_mul(t) {
                Some(v) => v / d,
                None => (num / d)
                    .saturating_mul(t)
                    .saturating_add((num % d).saturating_mul(t) / d),
            }
        };
        y0 as i128 + (sy + term) / n
    }
}
