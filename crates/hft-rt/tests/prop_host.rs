//! `hard` is exactly `bits ∩ H(class)` for any class and any set of hits (RT-079).

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

use std::cell::RefCell;

use common::fixture::*;
use hft_rt::{CpuSet, HostCheck as H, HostClass, HostNeeds, SysRoot, check_host};
use proptest::prelude::*;

const CLASSES: [HostClass; 6] = [
    HostClass::Dev2,
    HostClass::C4,
    HostClass::C8,
    HostClass::C16,
    HostClass::C32,
    HostClass::Metal,
];

/// One change of RT-073 – RT-075 (current ISA); `iso` marks the isolation checks.
struct Change {
    bit: H,
    iso: bool,
    apply: fn(&mut Fixture, &mut HostNeeds, &mut Cmd),
}

/// The kernel command line is assembled from independent parts.
#[derive(Clone)]
struct Cmd {
    rcu: &'static str,
    idle: bool,
}

fn changes() -> Vec<Change> {
    let mut v = vec![
        Change {
            bit: H::CLASS_MISMATCH,
            iso: false,
            apply: |f, _, _| f.set("sys/devices/system/cpu/online", "0-15\n"),
        },
        Change {
            bit: H::CLASS_MISMATCH,
            iso: false,
            apply: |f, _, _| f.set("proc/meminfo", &format!("MemTotal: {} kB\n", 61_439 * 1024)),
        },
        Change {
            bit: H::CLASS_MISMATCH,
            iso: false,
            apply: |_, n, _| n.isa = other_isa(),
        },
        Change {
            bit: H::TSC_NOT_INVARIANT,
            iso: false,
            apply: |f, _, _| f.set(CLOCKSOURCE, "kvm-clock\n"),
        },
        Change {
            bit: H::HOT_NOT_ISOLATED,
            iso: true,
            apply: |f, _, _| f.set("sys/devices/system/cpu/isolated", "3-27\n"),
        },
        Change {
            bit: H::WARM_ON_ISOLATED,
            iso: true,
            apply: |_, n, _| n.warm = CpuSet::from_cpus(&[27, 28]),
        },
        Change {
            bit: H::HOT_NOT_NOHZ,
            iso: false,
            apply: |f, _, _| f.set("sys/devices/system/cpu/nohz_full", "4-27\n"),
        },
        Change {
            bit: H::HOT_NOT_RCU_NOCB,
            iso: false,
            apply: |_, _, c| c.rcu = "2-27",
        },
        Change {
            bit: H::IRQ_ON_HOT,
            iso: false,
            apply: |f, _, _| f.set("proc/irq/45/smp_affinity_list", "2\n"),
        },
        Change {
            bit: H::TASK_ON_HOT,
            iso: false,
            apply: |f, _, _| {
                for (k, v) in task_files(200, 3, "0-31") {
                    f.set(&k, &v)
                }
            },
        },
        Change {
            bit: H::HUGEPAGES_SHORT,
            iso: false,
            apply: |f, _, _| f.set("sys/kernel/mm/hugepages/hugepages-2048kB/free_hugepages", "511\n"),
        },
        Change {
            bit: H::SHM_SHORT,
            iso: false,
            apply: |f, n, _| {
                n.shm_bytes = GIB;
                n.held.shm_bytes = 256 * MIB;
                f.set("statvfs", &format!("/dev/shm {}\n", 767 * MIB));
            },
        },
        Change {
            bit: H::MLOCK_FAILED,
            iso: false,
            apply: |f, _, _| f.set("proc/self/status", "VmLck:\t       0 kB\n"),
        },
        Change {
            bit: H::SWAP_ON,
            iso: false,
            apply: |f, _, _| {
                f.set(
                    "proc/swaps",
                    "Filename\tType\tSize\tUsed\tPriority\n/swapfile file 1 0 -2\n",
                )
            },
        },
        Change {
            bit: H::NUMA_BALANCING_ON,
            iso: false,
            apply: |f, _, _| f.set("proc/sys/kernel/numa_balancing", "1\n"),
        },
        Change {
            bit: H::TIMER_MIGRATION_ON,
            iso: false,
            apply: |f, _, _| f.set("proc/sys/kernel/timer_migration", "1\n"),
        },
    ];
    if cfg!(target_arch = "aarch64") {
        v.push(Change {
            bit: H::THP_NOT_MADVISE,
            iso: false,
            apply: |f, _, _| f.set(THP, "[always] madvise never\n"),
        });
        v.push(Change {
            bit: H::CPU_NOT_NEOVERSE_V1_COMPAT,
            iso: false,
            apply: |f, _, _| {
                f.set(
                    "proc/cpuinfo",
                    &cpuinfo(hft_rt::required_cpu_features().first().copied(), None),
                )
            },
        });
    } else {
        v.push(Change {
            bit: H::THP_ON,
            iso: false,
            apply: |f, _, _| f.set(THP, "[always] madvise never\n"),
        });
        v.push(Change {
            bit: H::SMT_ON,
            iso: false,
            apply: |f, _, _| f.set("sys/devices/system/cpu/smt/active", "1\n"),
        });
        v.push(Change {
            bit: H::CSTATE_DEEP,
            iso: false,
            apply: |_, _, c| c.idle = false,
        });
    }
    v
}

/// H(class) from the README table.
fn hard_ref(c: HostClass) -> H {
    let mut h = H::CLASS_MISMATCH | H::TSC_NOT_INVARIANT | H::SHM_SHORT | H::MLOCK_FAILED;
    if cfg!(target_arch = "aarch64") {
        h = h | H::CNTVCT_TRAPS | H::CPU_NOT_NEOVERSE_V1_COMPAT;
    }
    if !matches!(c, HostClass::Dev2 | HostClass::C4) {
        h = h | H::HOT_NOT_ISOLATED | H::WARM_ON_ISOLATED | H::HUGEPAGES_SHORT;
    }
    h
}

thread_local! {
    static FIX: RefCell<Option<Fixture>> = const { RefCell::new(None) };
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    #[test]
    fn rt_079_hard_is_class_subset(
        class in prop::sample::select(CLASSES.to_vec()),
        picks in prop::collection::vec(prop::bool::weighted(0.3), 19),
    ) {
        FIX.with(|cell| {
            let mut slot = cell.borrow_mut();
            let f = slot.get_or_insert_with(Fixture::new);
            let mut n = n0();
            n.class = class;
            let mut cmd = Cmd { rcu: "1-27", idle: true };
            let mut want = H(0);
            let upgrade = !matches!(class, HostClass::Dev2 | HostClass::C4);
            for (ch, pick) in changes().iter().zip(picks.iter()) {
                if *pick {
                    (ch.apply)(f, &mut n, &mut cmd);
                    if !ch.iso || upgrade {
                        want = want | ch.bit;
                    }
                }
            }
            f.set("proc/cmdline", &cmdline(cmd.rcu, cmd.idle.then_some("idle=poll")));
            let r = check_host(&SysRoot::fixture(&f.dir), &hot(), &hk(), &n);
            f.restore();
            prop_assert_eq!(r.bits, want, "class {:?}; lines {:?}", class, r.lines);
            prop_assert_eq!(r.hard, r.bits & hard_ref(class));
            if !upgrade {
                prop_assert!(!r.bits.contains(H::HOT_NOT_ISOLATED) && !r.bits.contains(H::WARM_ON_ISOLATED));
            }
            Ok(())
        })?;
    }
}
