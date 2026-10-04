//! Small OS helpers shared by the Linux-only modules.

/// errno for "not supported on this platform" (Linux value, used on every platform).
#[allow(dead_code)]
pub(crate) const ENOSYS: i32 = 38;

#[cfg(target_os = "linux")]
pub(crate) fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(target_os = "linux")]
pub(crate) fn cpath(p: &std::path::Path) -> Option<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(p.as_os_str().as_bytes()).ok()
}

/// Process start time (field 22 of `/proc/<pid>/stat`, clock ticks since boot); `None` when the
/// process does not exist.
#[cfg(target_os = "linux")]
pub(crate) fn proc_start(pid: u32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &s[s.rfind(')')? + 1..];
    // Fields after the comm start at field 3 (state); starttime is field 22.
    rest.split_ascii_whitespace().nth(19)?.parse().ok()
}

#[cfg(target_os = "linux")]
pub(crate) fn clock_ns(clock: libc::clockid_t) -> i64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec; the clock ids used are always supported.
    unsafe { libc::clock_gettime(clock, &mut ts) };
    ts.tv_sec.saturating_mul(1_000_000_000).saturating_add(ts.tv_nsec)
}

/// `/proc/sys/kernel/random/boot_id` as 16 bytes (all zero if unreadable).
#[cfg(target_os = "linux")]
pub(crate) fn boot_id() -> [u8; 16] {
    let mut out = [0u8; 16];
    if let Ok(s) = std::fs::read_to_string("/proc/sys/kernel/random/boot_id") {
        let hex: Vec<u8> = s.bytes().filter(|b| b.is_ascii_hexdigit()).collect();
        for (i, o) in out.iter_mut().enumerate() {
            let hi = hex.get(2 * i).and_then(|c| (*c as char).to_digit(16));
            let lo = hex.get(2 * i + 1).and_then(|c| (*c as char).to_digit(16));
            if let (Some(h), Some(l)) = (hi, lo) {
                *o = (h * 16 + l) as u8;
            }
        }
    }
    out
}
