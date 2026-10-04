//! x86_64 host check table (optional metal profile).

use crate::host::{Hits, HostCheck, SysRoot, selected};

/// x86_64 hard bits beyond the common ones: none.
pub(crate) const HARD: HostCheck = HostCheck(0);

fn cpuinfo_flags(root: &SysRoot) -> Vec<String> {
    root.read("proc/cpuinfo")
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("flags")
                    .and_then(|r| r.split_once(':'))
                    .map(|(_, v)| v.to_string())
            })
        })
        .map(|v| v.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Calibration step 1 (x86): clocksource `tsc`, cpuinfo `constant_tsc nonstop_tsc`, and on the
/// real root CPUID 0x8000_0007 EDX[8].
pub(crate) fn tsc_invariant(root: &SysRoot) -> Result<(), String> {
    let cs = root
        .read_trim("sys/devices/system/clocksource/clocksource0/current_clocksource")
        .unwrap_or_default();
    if cs != "tsc" {
        return Err(format!("clocksource is {cs:?}, not \"tsc\""));
    }
    let flags = cpuinfo_flags(root);
    for f in ["constant_tsc", "nonstop_tsc"] {
        if !flags.iter().any(|x| x == f) {
            return Err(format!("cpuinfo flags lack {f}"));
        }
    }
    if root.is_real() && !cpuid_invariant() {
        return Err("CPUID 0x80000007 EDX[8] not set".into());
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn cpuid_invariant() -> bool {
    use core::arch::x86_64::{__cpuid, __get_cpuid_max};
    #[allow(unused_unsafe)]
    // SAFETY: CPUID exists on every x86_64 CPU.
    let (max, _) = unsafe { __get_cpuid_max(0x8000_0000) };
    if max < 0x8000_0007 {
        return false;
    }
    #[allow(unused_unsafe)]
    // SAFETY: the leaf is supported (checked above).
    let r = unsafe { __cpuid(0x8000_0007) };
    r.edx & (1 << 8) != 0
}

#[cfg(not(target_arch = "x86_64"))]
fn cpuid_invariant() -> bool {
    false
}

pub(crate) fn checks(root: &SysRoot, cmdline: &str, h: &mut Hits) {
    let thp = root
        .read_trim("sys/kernel/mm/transparent_hugepage/enabled")
        .unwrap_or_default();
    if selected(&thp) != Some("never") {
        h.hit(HostCheck::THP_ON, format!("THP is {thp:?}"));
    }
    if root.read_trim("sys/devices/system/cpu/smt/active").as_deref() == Some("1") {
        h.hit(HostCheck::SMT_ON, "SMT active".into());
    }
    let shallow = cmdline
        .split_whitespace()
        .any(|t| t == "idle=poll" || t.ends_with("max_cstate=0"));
    if !shallow {
        h.hit(
            HostCheck::CSTATE_DEEP,
            "cmdline lacks idle=poll / max_cstate=0".into(),
        );
    }
}

pub(crate) fn required_features() -> Vec<&'static str> {
    Vec::new()
}
