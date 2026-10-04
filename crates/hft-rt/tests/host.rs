//! Host check bits and fixture-driven checks (RT-072 – RT-076).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![cfg(not(loom))]

#[allow(dead_code)]
mod common {
    pub mod fixture;
}

use common::fixture::*;
use hft_rt::{HostCheck, HostClass, HostNeeds, SysRoot, check_host};

const COMMON_MASK: u64 = (1 << 14) - 1;

#[test]
fn rt_072_host_check_bits() {
    let rows: [(&str, HostCheck, u64); 20] = [
        ("RT-072.1", HostCheck::CLASS_MISMATCH, 1 << 0),
        ("RT-072.2", HostCheck::TSC_NOT_INVARIANT, 1 << 1),
        ("RT-072.3", HostCheck::HOT_NOT_ISOLATED, 1 << 2),
        ("RT-072.4", HostCheck::WARM_ON_ISOLATED, 1 << 3),
        ("RT-072.5", HostCheck::HOT_NOT_NOHZ, 1 << 4),
        ("RT-072.6", HostCheck::HOT_NOT_RCU_NOCB, 1 << 5),
        ("RT-072.7", HostCheck::IRQ_ON_HOT, 1 << 6),
        ("RT-072.8", HostCheck::TASK_ON_HOT, 1 << 7),
        ("RT-072.9", HostCheck::HUGEPAGES_SHORT, 1 << 8),
        ("RT-072.10", HostCheck::SHM_SHORT, 1 << 9),
        ("RT-072.11", HostCheck::MLOCK_FAILED, 1 << 10),
        ("RT-072.12", HostCheck::SWAP_ON, 1 << 11),
        ("RT-072.13", HostCheck::NUMA_BALANCING_ON, 1 << 12),
        ("RT-072.14", HostCheck::TIMER_MIGRATION_ON, 1 << 13),
        ("RT-072.15", HostCheck::THP_NOT_MADVISE, 1 << 16),
        ("RT-072.16", HostCheck::CNTVCT_TRAPS, 1 << 17),
        ("RT-072.17", HostCheck::CPU_NOT_NEOVERSE_V1_COMPAT, 1 << 18),
        ("RT-072.18", HostCheck::THP_ON, 1 << 24),
        ("RT-072.19", HostCheck::SMT_ON, 1 << 25),
        ("RT-072.20", HostCheck::CSTATE_DEEP, 1 << 26),
    ];
    for (row, c, want) in rows {
        assert_eq!(c.0, want, "{row}");
    }
    for (i, (_, a, _)) in rows.iter().enumerate() {
        for (_, b, _) in rows.iter().skip(i + 1) {
            assert_eq!(a.0 & b.0, 0);
        }
    }
}

type Edit = fn(&mut Fixture, &mut HostNeeds);

fn run(f: &Fixture, n: &HostNeeds) -> hft_rt::HostReport {
    check_host(&SysRoot::fixture(&f.dir), &hot(), &hk(), n)
}

fn metal(n: &mut HostNeeds) {
    n.class = HostClass::Metal;
    n.min_cpus = 24;
    n.max_cpus = 256;
}

fn physical_cores(f: &mut Fixture, k: u32) {
    for c in 0..32u32 {
        f.set(
            &format!("sys/devices/system/cpu/cpu{c}/topology/core_id"),
            &format!("{}\n", c % k),
        );
    }
}

fn shm_case(f: &mut Fixture, n: &mut HostNeeds, avail_mib: u64, held_mib: u64) {
    n.hugepages_1g = 0;
    n.hugepages_2m = 0;
    n.shm_bytes = GIB;
    n.held.shm_bytes = held_mib * MIB;
    f.set("statvfs", &format!("/dev/shm {}\n", avail_mib * MIB));
}

#[test]
fn rt_073_common_checks() {
    use HostCheck as H;
    let z = HostCheck(0);
    let rows: [(&str, Edit, HostCheck, HostCheck); 31] = [
        ("RT-073.1", |_, _| {}, z, z),
        (
            "RT-073.2",
            |f, _| f.set("sys/devices/system/cpu/online", "0-15\n"),
            H::CLASS_MISMATCH,
            H::CLASS_MISMATCH,
        ),
        (
            "RT-073.3",
            |f, _| f.set("sys/devices/system/cpu/online", "0-32\n"),
            H::CLASS_MISMATCH,
            H::CLASS_MISMATCH,
        ),
        (
            "RT-073.4",
            |f, _| f.set("proc/meminfo", &format!("MemTotal: {} kB\n", 61_439 * 1024)),
            H::CLASS_MISMATCH,
            H::CLASS_MISMATCH,
        ),
        (
            "RT-073.5",
            |f, _| f.set("proc/meminfo", &format!("MemTotal: {} kB\n", 61_440 * 1024)),
            z,
            z,
        ),
        (
            "RT-073.6",
            |_, n| n.isa = other_isa(),
            H::CLASS_MISMATCH,
            H::CLASS_MISMATCH,
        ),
        (
            "RT-073.7",
            |f, n| {
                metal(n);
                physical_cores(f, 23)
            },
            H::CLASS_MISMATCH,
            H::CLASS_MISMATCH,
        ),
        (
            "RT-073.8",
            |f, n| {
                metal(n);
                physical_cores(f, 24)
            },
            z,
            z,
        ),
        (
            "RT-073.9",
            |f, _| f.set(CLOCKSOURCE, "kvm-clock\n"),
            H::TSC_NOT_INVARIANT,
            H::TSC_NOT_INVARIANT,
        ),
        (
            "RT-073.10",
            |f, _| f.set("sys/devices/system/cpu/isolated", "3-27\n"),
            H::HOT_NOT_ISOLATED,
            H::HOT_NOT_ISOLATED,
        ),
        (
            "RT-073.11",
            |f, n| {
                f.set("sys/devices/system/cpu/isolated", "3-27\n");
                n.class = HostClass::Dev2
            },
            z,
            z,
        ),
        (
            "RT-073.12",
            |_, n| n.warm = hft_rt::CpuSet::from_cpus(&[27, 28]),
            H::WARM_ON_ISOLATED,
            H::WARM_ON_ISOLATED,
        ),
        (
            "RT-073.13",
            |_, n| {
                n.warm = hft_rt::CpuSet::from_cpus(&[27, 28]);
                n.class = HostClass::C4
            },
            z,
            z,
        ),
        (
            "RT-073.14",
            |f, _| f.set("sys/devices/system/cpu/nohz_full", "4-27\n"),
            H::HOT_NOT_NOHZ,
            z,
        ),
        (
            "RT-073.15",
            |f, _| f.set("proc/cmdline", &cmdline("2-27", Some("idle=poll"))),
            H::HOT_NOT_RCU_NOCB,
            z,
        ),
        (
            "RT-073.16",
            |f, _| f.set("proc/irq/45/smp_affinity_list", "2\n"),
            H::IRQ_ON_HOT,
            z,
        ),
        (
            "RT-073.17",
            |f, _| {
                for (k, v) in task_files(200, 3, "0-31") {
                    f.set(&k, &v)
                }
            },
            H::TASK_ON_HOT,
            z,
        ),
        (
            "RT-073.18",
            |f, _| {
                for (k, v) in task_files(201, 1, "0-31") {
                    f.set(&k, &v)
                }
            },
            H::TASK_ON_HOT,
            z,
        ),
        (
            "RT-073.19",
            |f, _| {
                for (k, v) in task_files(202, 29, "28-31") {
                    f.set(&k, &v)
                }
            },
            z,
            z,
        ),
        (
            "RT-073.20",
            |f, n| {
                n.held.hugepages_1g = 1;
                f.set(
                    "sys/kernel/mm/hugepages/hugepages-1048576kB/free_hugepages",
                    "3\n",
                )
            },
            z,
            z,
        ),
        (
            "RT-073.21",
            |f, n| {
                n.held.hugepages_1g = 1;
                f.set(
                    "sys/kernel/mm/hugepages/hugepages-1048576kB/free_hugepages",
                    "2\n",
                )
            },
            H::HUGEPAGES_SHORT,
            H::HUGEPAGES_SHORT,
        ),
        (
            "RT-073.22",
            |f, _| f.set("sys/kernel/mm/hugepages/hugepages-2048kB/free_hugepages", "511\n"),
            H::HUGEPAGES_SHORT,
            H::HUGEPAGES_SHORT,
        ),
        (
            "RT-073.23",
            |f, _| f.set("sys/kernel/mm/hugepages/hugepages-2048kB/free_hugepages", "512\n"),
            z,
            z,
        ),
        ("RT-073.24", |f, n| shm_case(f, n, 768, 256), z, z),
        (
            "RT-073.25",
            |f, n| shm_case(f, n, 767, 256),
            H::SHM_SHORT,
            H::SHM_SHORT,
        ),
        (
            "RT-073.26",
            |f, n| shm_case(f, n, 768, 0),
            H::SHM_SHORT,
            H::SHM_SHORT,
        ),
        (
            "RT-073.27",
            |f, _| f.set("proc/self/status", "Name:\thft-core\nVmLck:\t       0 kB\n"),
            H::MLOCK_FAILED,
            H::MLOCK_FAILED,
        ),
        (
            "RT-073.28",
            |f, _| {
                f.set(
                    "proc/swaps",
                    "Filename\tType\tSize\tUsed\tPriority\n/swapfile file 1048572 0 -2\n",
                )
            },
            H::SWAP_ON,
            z,
        ),
        (
            "RT-073.29",
            |f, _| f.set("proc/sys/kernel/numa_balancing", "1\n"),
            H::NUMA_BALANCING_ON,
            z,
        ),
        (
            "RT-073.30",
            |f, _| f.set("proc/sys/kernel/timer_migration", "1\n"),
            H::TIMER_MIGRATION_ON,
            z,
        ),
        (
            "RT-073.31",
            |f, n| {
                f.set(
                    "sys/kernel/mm/hugepages/hugepages-1048576kB/free_hugepages",
                    "0\n",
                );
                n.class = HostClass::Dev2;
                n.hugepages_1g = 0;
                n.hugepages_2m = 0;
            },
            z,
            z,
        ),
    ];
    for (row, edit, bits, hard) in rows {
        let mut f = Fixture::new();
        let mut n = n0();
        edit(&mut f, &mut n);
        let r = run(&f, &n);
        assert_eq!(r.bits.0 & COMMON_MASK, bits.0, "{row} bits; lines {:?}", r.lines);
        assert_eq!(r.hard.0 & COMMON_MASK, hard.0, "{row} hard; lines {:?}", r.lines);
    }
}

#[cfg(target_arch = "aarch64")]
#[test]
fn rt_074_aarch64_checks() {
    use HostCheck as H;
    let z = HostCheck(0);
    let rows: [(&str, Edit, HostCheck, HostCheck); 5] = [
        ("RT-074.1", |_, _| {}, z, z),
        (
            "RT-074.2",
            |f, _| f.set(THP, "[always] madvise never\n"),
            H::THP_NOT_MADVISE,
            z,
        ),
        (
            "RT-074.3",
            |f, _| f.set(THP, "always madvise [never]\n"),
            H::THP_NOT_MADVISE,
            z,
        ),
        (
            "RT-074.4",
            |f, _| {
                f.set(
                    "proc/cpuinfo",
                    &cpuinfo(hft_rt::required_cpu_features().first().copied(), None),
                )
            },
            H::CPU_NOT_NEOVERSE_V1_COMPAT,
            H::CPU_NOT_NEOVERSE_V1_COMPAT,
        ),
        (
            "RT-074.5",
            |f, _| f.set(CLOCKSOURCE, "tsc\n"),
            H::TSC_NOT_INVARIANT,
            H::TSC_NOT_INVARIANT,
        ),
    ];
    for (row, edit, bits, hard) in rows {
        let mut f = Fixture::new();
        let mut n = n0();
        edit(&mut f, &mut n);
        let r = run(&f, &n);
        assert_eq!(r.bits, bits, "{row} bits; lines {:?}", r.lines);
        assert_eq!(r.hard, hard, "{row} hard");
        // RT-074.6: a fixture root never measures the counter.
        assert!(!r.bits.contains(H::CNTVCT_TRAPS), "RT-074.6 ({row})");
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn rt_075_x86_checks() {
    use HostCheck as H;
    let z = HostCheck(0);
    let rows: [(&str, Edit, HostCheck, HostCheck); 7] = [
        ("RT-075.1", |_, _| {}, z, z),
        (
            "RT-075.2",
            |f, _| f.set(THP, "[always] madvise never\n"),
            H::THP_ON,
            z,
        ),
        (
            "RT-075.3",
            |f, _| f.set(THP, "always [madvise] never\n"),
            H::THP_ON,
            z,
        ),
        (
            "RT-075.4",
            |f, _| f.set("sys/devices/system/cpu/smt/active", "1\n"),
            H::SMT_ON,
            z,
        ),
        (
            "RT-075.5",
            |f, _| f.set("proc/cmdline", &cmdline("1-27", None)),
            H::CSTATE_DEEP,
            z,
        ),
        (
            "RT-075.6",
            |f, _| f.set("proc/cmdline", &cmdline("1-27", Some("processor.max_cstate=0"))),
            z,
            z,
        ),
        (
            "RT-075.7",
            |f, _| f.set("proc/cpuinfo", &cpuinfo(None, Some("nonstop_tsc"))),
            H::TSC_NOT_INVARIANT,
            H::TSC_NOT_INVARIANT,
        ),
    ];
    for (row, edit, bits, hard) in rows {
        let mut f = Fixture::new();
        let mut n = n0();
        edit(&mut f, &mut n);
        let r = run(&f, &n);
        assert_eq!(r.bits, bits, "{row} bits; lines {:?}", r.lines);
        assert_eq!(r.hard, hard, "{row} hard");
    }
}

#[test]
fn rt_076_report_lines_no_writes() {
    let mut f = Fixture::new();
    f.set("sys/devices/system/cpu/online", "0-15\n");
    f.set("sys/devices/system/cpu/nohz_full", "4-27\n");
    f.set("proc/self/status", "Name:\thft-core\nVmLck:\t       0 kB\n");
    let before = f.contents();
    let r = run(&f, &n0());
    let after = f.contents();
    use HostCheck as H;
    assert_eq!(
        r.bits,
        H::CLASS_MISMATCH | H::HOT_NOT_NOHZ | H::MLOCK_FAILED,
        "{:?}",
        r.lines
    );
    assert_eq!(r.hard, H::CLASS_MISMATCH | H::MLOCK_FAILED);
    assert!(r.lines.len() >= 3);
    let pos = |k: &str| r.lines.iter().position(|l| l.contains(k)).unwrap();
    assert!(pos("CLASS_MISMATCH") < pos("HOT_NOT_NOHZ"));
    assert!(pos("HOT_NOT_NOHZ") < pos("MLOCK_FAILED"));
    assert_eq!(before, after);
}
