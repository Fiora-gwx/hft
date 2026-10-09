//! held_by_instance (JNL-099) and the host space check with crash leftovers (JNL-100).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use std::path::Path;

use common::harness::*;
use hft_journal::testing;
use hft_journal::*;
use hft_rt::HeldBySelf;

const MIB: usize = 1 << 20;

fn seg_file(p: &Path, magic: bool, len: usize) {
    let mut b = vec![0u8; len];
    if magic && len >= 8 {
        b[..8].copy_from_slice(b"HFTJRNL1");
    }
    if !testing::is_mem(p) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    }
    testing::create(p, &b).unwrap();
}

#[test]
fn jnl_099_held_by_instance() {
    for (row, extra) in [
        ("JNL-099.1", None),
        ("JNL-099.3", Some(("notes.txt", true, MIB))),
        ("JNL-099.4", Some(("0000000009.tmp", true, MIB))),
        ("JNL-099.5", Some(("0000000009.jrn", false, MIB))),
        ("JNL-099.6", Some(("0000000009.jrn", true, 5))),
    ] {
        let hot = linux_hot();
        let prod = hot.join("prod").join("seq");
        let sim = hot.join("sim").join("seq");
        for n in 1..=3 {
            seg_file(&prod.join(format!("{n:010}.jrn")), true, MIB);
        }
        for n in 1..=2 {
            seg_file(&sim.join(format!("{n:010}.jrn")), true, MIB);
        }
        if let Some((name, magic, len)) = extra {
            seg_file(&prod.join(name), magic, len);
        }
        let want = HeldBySelf {
            hugepages_1g: 0,
            hugepages_2m: 0,
            shm_bytes: 3_145_728,
        };
        assert_eq!(
            held_by_instance(&[&hot], InstanceName::Prod).unwrap(),
            want,
            "{row}"
        );
        if row == "JNL-099.1" {
            let s = held_by_instance(&[&hot], InstanceName::Sim).unwrap();
            assert_eq!(s.shm_bytes, 2_097_152, "JNL-099.2");
        }
    }
}

/// CI prerequisite: `sudo mount -t tmpfs -o size=8M none /mnt/shm8`, path in `HFT_TEST_SHM8`.
#[test]
fn jnl_100_held_space_check() {
    #[cfg(target_os = "linux")]
    {
        use hft_rt::{CpuSet, HostCheck, HostNeeds, Isa, SysRoot, check_host};
        use hft_types::HostClass;
        let Ok(mnt) = std::env::var("HFT_TEST_SHM8") else {
            eprintln!(
                "JNL-100: HFT_TEST_SHM8 not set (the CI job mounts an 8 MiB tmpfs); prerequisite missing"
            );
            return;
        };
        let mnt = std::path::PathBuf::from(mnt);
        for (inst, short) in [("prod", false), ("sim", true)] {
            let _ = std::fs::remove_dir_all(mnt.join("prod"));
            let _ = std::fs::remove_dir_all(mnt.join("sim"));
            let d = mnt.join(inst).join("seq");
            for n in 1..=6 {
                seg_file(&d.join(format!("{n:010}.jrn")), true, MIB);
            }
            let held = held_by_instance(&[&mnt], InstanceName::Prod).unwrap();
            let c = std::ffi::CString::new(mnt.to_str().unwrap()).unwrap();
            // SAFETY: zeroed statvfs is a valid out-buffer; `c` is NUL-terminated.
            let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
            // SAFETY: as above.
            assert_eq!(unsafe { libc::statvfs(c.as_ptr(), &mut st) }, 0);
            let avail = st.f_bavail as u64 * st.f_frsize as u64;
            let fx = disk_dir("sysroot");
            std::fs::write(fx.join("statvfs"), format!("/dev/shm {avail}\n")).unwrap();
            let need = HostNeeds {
                class: HostClass::Dev2,
                min_cpus: 0,
                max_cpus: u16::MAX,
                min_mem_mib: 0,
                isa: Isa::TARGET,
                hugepages_1g: 0,
                hugepages_2m: 0,
                shm_bytes: 6 * MIB as u64,
                held,
                warm: CpuSet::default(),
                irq: CpuSet::default(),
            };
            let rep = check_host(
                &SysRoot::fixture(fx),
                &CpuSet::default(),
                &CpuSet::default(),
                &need,
            );
            assert_eq!(rep.bits.0 & HostCheck::SHM_SHORT.0 != 0, short, "{inst}");
        }
        let _ = std::fs::remove_dir_all(mnt.join("prod"));
        let _ = std::fs::remove_dir_all(mnt.join("sim"));
    }
}
