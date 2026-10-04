//! `SysRoot` fixture F0 and `HostNeeds` N0 of RT-073 (every check passes), with edit helpers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use hft_rt::{CpuSet, HeldBySelf, HostClass, HostNeeds, Isa, required_cpu_features};

pub const GIB: u64 = 1 << 30;
pub const MIB: u64 = 1 << 20;

/// A fixture directory; `set`/`remove` write through immediately and remember the base content
/// so `restore` can undo every edit.
pub struct Fixture {
    pub dir: PathBuf,
    base: BTreeMap<String, String>,
    touched: BTreeMap<String, Option<String>>,
}

static SEQ: AtomicU32 = AtomicU32::new(0);

fn stat_line(pid: u32, cpu: u16) -> String {
    // Fields 3.. after the comm; processor is field 39 (index 36 after the ')').
    let mut f: Vec<String> = vec!["0".into(); 50];
    f[0] = "S".into();
    f[36] = cpu.to_string();
    format!("{pid} (worker) {}\n", f.join(" "))
}

pub fn task_files(pid: u32, cpu: u16, allowed: &str) -> [(String, String); 2] {
    [
        (format!("proc/{pid}/task/{pid}/stat"), stat_line(pid, cpu)),
        (
            format!("proc/{pid}/task/{pid}/status"),
            format!("Name:\tworker\nCpus_allowed_list:\t{allowed}\n"),
        ),
    ]
}

pub fn cpuinfo(drop_feature: Option<&str>, drop_flag: Option<&str>) -> String {
    let feats: Vec<&str> = ["fp", "asimd"]
        .into_iter()
        .chain(required_cpu_features())
        .filter(|f| Some(*f) != drop_feature)
        .collect();
    let flags: Vec<&str> = ["fpu", "tsc", "constant_tsc", "nonstop_tsc"]
        .into_iter()
        .filter(|f| Some(*f) != drop_flag)
        .collect();
    format!(
        "processor\t: 0\nflags\t\t: {}\nFeatures\t: {}\n",
        flags.join(" "),
        feats.join(" ")
    )
}

pub fn cmdline(rcu: &str, idle: Option<&str>) -> String {
    let mut s = format!("BOOT_IMAGE=/vmlinuz root=/dev/nvme0n1p1 rcu_nocbs={rcu}");
    if let Some(i) = idle {
        s.push(' ');
        s.push_str(i);
    }
    s.push('\n');
    s
}

pub const CLOCKSOURCE: &str = "sys/devices/system/clocksource/clocksource0/current_clocksource";
pub const THP: &str = "sys/kernel/mm/transparent_hugepage/enabled";

pub fn base_files() -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        m.insert(k.to_string(), v);
    };
    put("sys/devices/system/cpu/online", "0-31\n".into());
    for c in 0..32 {
        put(
            &format!("sys/devices/system/cpu/cpu{c}/topology/core_id"),
            format!("{c}\n"),
        );
        put(
            &format!("sys/devices/system/cpu/cpu{c}/topology/physical_package_id"),
            "0\n".into(),
        );
    }
    put(
        "proc/meminfo",
        "MemTotal:       67108864 kB\nMemFree:        60000000 kB\n".into(),
    );
    let native = if cfg!(target_arch = "aarch64") {
        "arch_sys_counter"
    } else {
        "tsc"
    };
    put(CLOCKSOURCE, format!("{native}\n"));
    put("proc/cpuinfo", cpuinfo(None, None));
    put("sys/devices/system/cpu/isolated", "2-27\n".into());
    put("sys/devices/system/cpu/nohz_full", "2-27\n".into());
    put("proc/cmdline", cmdline("1-27", Some("idle=poll")));
    put("proc/irq/1/smp_affinity_list", "0\n".into());
    put("proc/irq/2/smp_affinity_list", "28-31\n".into());
    put("proc/irq/45/smp_affinity_list", "0\n".into());
    for (k, v) in task_files(100, 0, "0") {
        put(&k, v);
    }
    put(
        "sys/kernel/mm/hugepages/hugepages-1048576kB/free_hugepages",
        "8\n".into(),
    );
    put(
        "sys/kernel/mm/hugepages/hugepages-2048kB/free_hugepages",
        "1024\n".into(),
    );
    put(
        "proc/self/status",
        "Name:\thft-core\nVmLck:\t 1048576 kB\n".into(),
    );
    put(
        "proc/swaps",
        "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n".into(),
    );
    put("proc/sys/kernel/numa_balancing", "0\n".into());
    put("proc/sys/kernel/timer_migration", "0\n".into());
    put("statvfs", format!("/dev/shm {}\n", 4 * GIB));
    let thp = if cfg!(target_arch = "aarch64") {
        "always [madvise] never"
    } else {
        "always madvise [never]"
    };
    put(THP, format!("{thp}\n"));
    put("sys/devices/system/cpu/smt/active", "0\n".into());
    m
}

impl Fixture {
    pub fn new() -> Fixture {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("hft-rt-host-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let base = base_files();
        for (k, v) in &base {
            write(&dir, k, v);
        }
        Fixture {
            dir,
            base,
            touched: BTreeMap::new(),
        }
    }

    pub fn set(&mut self, rel: &str, content: &str) {
        self.touched
            .entry(rel.to_string())
            .or_insert_with(|| self.base.get(rel).cloned());
        write(&self.dir, rel, content);
    }

    pub fn remove(&mut self, rel: &str) {
        self.touched
            .entry(rel.to_string())
            .or_insert_with(|| self.base.get(rel).cloned());
        let _ = std::fs::remove_file(self.dir.join(rel));
    }

    pub fn restore(&mut self) {
        for (rel, orig) in std::mem::take(&mut self.touched) {
            match orig {
                Some(v) => write(&self.dir, &rel, &v),
                None => {
                    let _ = std::fs::remove_file(self.dir.join(&rel));
                }
            }
        }
    }

    /// Every file under the fixture with its bytes (for the no-write check).
    pub fn contents(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        walk(&self.dir, &mut out);
        out
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn walk(d: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if let Ok(b) = std::fs::read(&p) {
                out.insert(p, b);
            }
        }
    }
}

fn write(dir: &Path, rel: &str, content: &str) {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
}

pub fn n0() -> HostNeeds {
    HostNeeds {
        class: HostClass::C32,
        min_cpus: 32,
        max_cpus: 32,
        min_mem_mib: 61_440,
        isa: Isa::TARGET,
        hugepages_1g: 4,
        hugepages_2m: 512,
        shm_bytes: 0,
        held: HeldBySelf::default(),
        warm: CpuSet::from_cpus(&[28, 29, 30, 31]),
        irq: CpuSet::from_cpus(&[1]),
    }
}

pub fn hot() -> CpuSet {
    CpuSet::from_cpus(&[2, 3])
}

pub fn hk() -> CpuSet {
    CpuSet::from_cpus(&[0])
}

pub fn other_isa() -> Isa {
    match Isa::TARGET {
        Isa::Aarch64 => Isa::X86_64,
        Isa::X86_64 => Isa::Aarch64,
    }
}
