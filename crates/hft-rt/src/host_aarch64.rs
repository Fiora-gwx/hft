//! aarch64 host check table (main path, Graviton).

#![allow(clippy::disallowed_macros, clippy::disallowed_methods)] // allow-list: startup host self-check, not on a hot thread

use crate::host::{Hits, HostCheck, SysRoot, selected};

/// aarch64 hard bits beyond the common ones.
pub(crate) const HARD: HostCheck =
    HostCheck(HostCheck::CNTVCT_TRAPS.0 | HostCheck::CPU_NOT_NEOVERSE_V1_COMPAT.0);

/// Compile-time target features → `/proc/cpuinfo` Features names.
const FEATURES: &[(bool, &str)] = &[
    (cfg!(target_feature = "neon"), "asimd"),
    (cfg!(target_feature = "aes"), "aes"),
    (cfg!(target_feature = "sha2"), "sha2"),
    (cfg!(target_feature = "crc"), "crc32"),
    (cfg!(target_feature = "lse"), "atomics"),
    (cfg!(target_feature = "rdm"), "asimdrdm"),
    (cfg!(target_feature = "fp16"), "fphp"),
    (cfg!(target_feature = "dotprod"), "asimddp"),
    (cfg!(target_feature = "rcpc"), "lrcpc"),
    (cfg!(target_feature = "rcpc2"), "ilrcpc"),
    (cfg!(target_feature = "sha3"), "sha3"),
    (cfg!(target_feature = "sm4"), "sm4"),
    (cfg!(target_feature = "fhm"), "asimdfhm"),
    (cfg!(target_feature = "dit"), "dit"),
    (cfg!(target_feature = "flagm"), "flagm"),
    (cfg!(target_feature = "ssbs"), "ssbs"),
    (cfg!(target_feature = "sb"), "sb"),
    (cfg!(target_feature = "paca"), "paca"),
    (cfg!(target_feature = "pacg"), "pacg"),
    (cfg!(target_feature = "dpb"), "dcpop"),
    (cfg!(target_feature = "dpb2"), "dcpodp"),
    (cfg!(target_feature = "sve"), "sve"),
    (cfg!(target_feature = "sve2"), "sve2"),
    (cfg!(target_feature = "i8mm"), "i8mm"),
    (cfg!(target_feature = "bf16"), "bf16"),
    (cfg!(target_feature = "rand"), "rng"),
    (cfg!(target_feature = "frintts"), "frint"),
    (cfg!(target_feature = "lse2"), "uscat"),
    (cfg!(target_feature = "jsconv"), "jscvt"),
    (cfg!(target_feature = "fcma"), "fcma"),
    (cfg!(target_feature = "bti"), "bti"),
];

pub(crate) fn required_features() -> Vec<&'static str> {
    FEATURES.iter().filter(|(on, _)| *on).map(|(_, n)| *n).collect()
}

/// Calibration step 1 (aarch64): clocksource `arch_sys_counter`; on the real root CNTFRQ_EL0 ≠ 0.
pub(crate) fn tsc_invariant(root: &SysRoot) -> Result<(), String> {
    let cs = root
        .read_trim("sys/devices/system/clocksource/clocksource0/current_clocksource")
        .unwrap_or_default();
    if cs != "arch_sys_counter" {
        return Err(format!("clocksource is {cs:?}, not \"arch_sys_counter\""));
    }
    if root.is_real() && crate::tsc::tsc_hz_nominal().is_none() {
        return Err("CNTFRQ_EL0 is 0".into());
    }
    Ok(())
}

/// Minimum per-read cost of CNTVCT_EL0 over 1 000 reads, in ns (by CLOCK_MONOTONIC).
fn counter_read_ns() -> u64 {
    let mut best = u64::MAX;
    for _ in 0..10 {
        let t0 = std::time::Instant::now();
        for _ in 0..1000 {
            core::hint::black_box(crate::tsc::tsc_now());
        }
        best = best.min(t0.elapsed().as_nanos() as u64 / 1000);
    }
    best
}

pub(crate) fn checks(root: &SysRoot, _cmdline: &str, h: &mut Hits) {
    let thp = root
        .read_trim("sys/kernel/mm/transparent_hugepage/enabled")
        .unwrap_or_default();
    if selected(&thp) != Some("madvise") {
        h.hit(HostCheck::THP_NOT_MADVISE, format!("THP is {thp:?}"));
    }
    if root.is_real() {
        let ns = counter_read_ns();
        if ns > 100 {
            h.hit(HostCheck::CNTVCT_TRAPS, format!("CNTVCT_EL0 read costs {ns} ns"));
        }
    }
    let have: Vec<String> = root
        .read("proc/cpuinfo")
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("Features")
                    .and_then(|r| r.split_once(':'))
                    .map(|(_, v)| v.to_string())
            })
        })
        .map(|v| v.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let missing: Vec<&str> = required_features()
        .into_iter()
        .filter(|f| !have.iter().any(|x| x == f))
        .collect();
    if !missing.is_empty() {
        h.hit(
            HostCheck::CPU_NOT_NEOVERSE_V1_COMPAT,
            format!("cpuinfo lacks {}", missing.join(" ")),
        );
    }
}
