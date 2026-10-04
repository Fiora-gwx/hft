//! `CpuSet`, pinning and thread naming.

use crate::RtErr;

/// Up to 256 logical CPUs.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Hash)]
pub struct CpuSet([u64; 4]);

impl CpuSet {
    pub const fn new() -> CpuSet {
        CpuSet([0; 4])
    }

    /// CPUs ≥ 256 are ignored.
    pub fn from_cpus(cpus: &[u16]) -> CpuSet {
        let mut s = CpuSet::new();
        for &c in cpus {
            s.insert(c);
        }
        s
    }

    /// Kernel list syntax (`"0-3,8,10-11"`, empty = empty set); `None` on a syntax error or a
    /// CPU number ≥ 256.
    pub fn parse_list(s: &str) -> Option<CpuSet> {
        let mut out = CpuSet::new();
        for part in s.trim().split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let (a, b) = match part.split_once('-') {
                Some((a, b)) => (a.trim().parse::<u16>().ok()?, b.trim().parse::<u16>().ok()?),
                None => {
                    let v = part.parse::<u16>().ok()?;
                    (v, v)
                }
            };
            if a > b || b >= 256 {
                return None;
            }
            for c in a..=b {
                out.insert(c);
            }
        }
        Some(out)
    }

    pub fn insert(&mut self, cpu: u16) {
        if let Some(w) = self.0.get_mut((cpu / 64) as usize) {
            *w |= 1u64 << (cpu % 64);
        }
    }

    pub fn contains(&self, cpu: u16) -> bool {
        self.0
            .get((cpu / 64) as usize)
            .is_some_and(|w| w & (1u64 << (cpu % 64)) != 0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|w| *w == 0)
    }

    pub fn count(&self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }

    pub fn union(&self, o: &CpuSet) -> CpuSet {
        CpuSet([
            self.0[0] | o.0[0],
            self.0[1] | o.0[1],
            self.0[2] | o.0[2],
            self.0[3] | o.0[3],
        ])
    }

    pub fn intersects(&self, o: &CpuSet) -> bool {
        self.0.iter().zip(o.0.iter()).any(|(a, b)| a & b != 0)
    }

    pub fn is_subset(&self, o: &CpuSet) -> bool {
        self.0.iter().zip(o.0.iter()).all(|(a, b)| a & !b == 0)
    }

    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        (0u16..256).filter(|c| self.contains(*c))
    }
}

/// `sched_setaffinity` to one CPU, then `sched_getcpu` must agree (`PinMismatch`); system call
/// failure → `Os(errno)`.
pub fn pin_current_thread(cpu: u16) -> Result<(), RtErr> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: an all-zero cpu_set_t is the empty set.
        let mut set: libc::cpu_set_t = unsafe { core::mem::zeroed() };
        // SAFETY: `cpu < 1024 = CPU_SETSIZE`, inside the set.
        unsafe { libc::CPU_SET(cpu as usize, &mut set) };
        // SAFETY: `set` is a valid cpu_set_t of the given size; pid 0 = this thread.
        if unsafe { libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), &set) } != 0 {
            return Err(RtErr::Os(crate::sys::errno()));
        }
        // SAFETY: no arguments.
        let got = unsafe { libc::sched_getcpu() };
        if got < 0 {
            return Err(RtErr::Os(crate::sys::errno()));
        }
        if got as u16 != cpu {
            return Err(RtErr::PinMismatch {
                want: cpu,
                got: got as u16,
            });
        }
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = cpu;
        Err(RtErr::Os(crate::sys::ENOSYS))
    }
}

/// Thread name for `/proc` and perf: at most 15 bytes (longer names are cut by bytes; an
/// interior NUL ends the name). No-op off Linux.
pub fn name_current_thread(name: &str) {
    #[cfg(target_os = "linux")]
    {
        let mut buf = [0u8; 16];
        for (o, b) in buf.iter_mut().zip(name.bytes().take(15).take_while(|b| *b != 0)) {
            *o = b;
        }
        // SAFETY: `buf` is NUL-terminated (byte 15 is always 0) and outlives the call.
        unsafe { libc::prctl(libc::PR_SET_NAME, buf.as_ptr() as libc::c_ulong, 0, 0, 0) };
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
    }
}
