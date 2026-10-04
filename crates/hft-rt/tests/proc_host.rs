//! On a real aarch64 Linux host the counter does not trap and is invariant (RT-077).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(all(target_os = "linux", target_arch = "aarch64", not(loom)))]

use hft_rt::{CpuSet, HeldBySelf, HostCheck, HostClass, HostNeeds, Isa, SysRoot, check_host};

#[test]
fn rt_077_cntvct_not_trapping() {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as u16)
        .unwrap_or(1);
    let need = HostNeeds {
        class: HostClass::Dev2,
        min_cpus: cpus,
        max_cpus: cpus,
        min_mem_mib: 0,
        isa: Isa::TARGET,
        hugepages_1g: 0,
        hugepages_2m: 0,
        shm_bytes: 0,
        held: HeldBySelf::default(),
        warm: CpuSet::from_cpus(&[0]),
        irq: CpuSet::new(),
    };
    let r = check_host(&SysRoot::real(), &CpuSet::new(), &CpuSet::from_cpus(&[0]), &need);
    assert!(!r.bits.contains(HostCheck::CNTVCT_TRAPS), "{:?}", r.lines);
    assert!(!r.bits.contains(HostCheck::TSC_NOT_INVARIANT), "{:?}", r.lines);
}
