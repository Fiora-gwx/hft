//! `check_host`: read-only host verification against `HostNeeds`; every bit is a pure function
//! of the files under `SysRoot` (a fixture directory works on any platform). Only the counter
//! register check (`TSC_NOT_INVARIANT`) and the `CNTVCT_TRAPS` timing read real hardware, and
//! only when the root is the real `/`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use hft_types::HostClass;

use crate::cpu::CpuSet;

#[cfg(target_arch = "aarch64")]
use crate::host_aarch64 as isa;
#[cfg(not(target_arch = "aarch64"))]
use crate::host_x86 as isa;

/// Host check bits (`HostProfile.host_checks`); append-only.
#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Hash)]
pub struct HostCheck(pub u64);

impl HostCheck {
    pub const CLASS_MISMATCH: HostCheck = HostCheck(1 << 0);
    pub const TSC_NOT_INVARIANT: HostCheck = HostCheck(1 << 1);
    pub const HOT_NOT_ISOLATED: HostCheck = HostCheck(1 << 2);
    pub const WARM_ON_ISOLATED: HostCheck = HostCheck(1 << 3);
    pub const HOT_NOT_NOHZ: HostCheck = HostCheck(1 << 4);
    pub const HOT_NOT_RCU_NOCB: HostCheck = HostCheck(1 << 5);
    pub const IRQ_ON_HOT: HostCheck = HostCheck(1 << 6);
    pub const TASK_ON_HOT: HostCheck = HostCheck(1 << 7);
    pub const HUGEPAGES_SHORT: HostCheck = HostCheck(1 << 8);
    pub const SHM_SHORT: HostCheck = HostCheck(1 << 9);
    pub const MLOCK_FAILED: HostCheck = HostCheck(1 << 10);
    pub const SWAP_ON: HostCheck = HostCheck(1 << 11);
    pub const NUMA_BALANCING_ON: HostCheck = HostCheck(1 << 12);
    pub const TIMER_MIGRATION_ON: HostCheck = HostCheck(1 << 13);
    // aarch64 table
    pub const THP_NOT_MADVISE: HostCheck = HostCheck(1 << 16);
    pub const CNTVCT_TRAPS: HostCheck = HostCheck(1 << 17);
    pub const CPU_NOT_NEOVERSE_V1_COMPAT: HostCheck = HostCheck(1 << 18);
    // x86_64 table
    pub const THP_ON: HostCheck = HostCheck(1 << 24);
    pub const SMT_ON: HostCheck = HostCheck(1 << 25);
    pub const CSTATE_DEEP: HostCheck = HostCheck(1 << 26);

    /// Every defined bit with its name (bit order).
    pub const ALL: [(HostCheck, &'static str); 20] = [
        (Self::CLASS_MISMATCH, "CLASS_MISMATCH"),
        (Self::TSC_NOT_INVARIANT, "TSC_NOT_INVARIANT"),
        (Self::HOT_NOT_ISOLATED, "HOT_NOT_ISOLATED"),
        (Self::WARM_ON_ISOLATED, "WARM_ON_ISOLATED"),
        (Self::HOT_NOT_NOHZ, "HOT_NOT_NOHZ"),
        (Self::HOT_NOT_RCU_NOCB, "HOT_NOT_RCU_NOCB"),
        (Self::IRQ_ON_HOT, "IRQ_ON_HOT"),
        (Self::TASK_ON_HOT, "TASK_ON_HOT"),
        (Self::HUGEPAGES_SHORT, "HUGEPAGES_SHORT"),
        (Self::SHM_SHORT, "SHM_SHORT"),
        (Self::MLOCK_FAILED, "MLOCK_FAILED"),
        (Self::SWAP_ON, "SWAP_ON"),
        (Self::NUMA_BALANCING_ON, "NUMA_BALANCING_ON"),
        (Self::TIMER_MIGRATION_ON, "TIMER_MIGRATION_ON"),
        (Self::THP_NOT_MADVISE, "THP_NOT_MADVISE"),
        (Self::CNTVCT_TRAPS, "CNTVCT_TRAPS"),
        (Self::CPU_NOT_NEOVERSE_V1_COMPAT, "CPU_NOT_NEOVERSE_V1_COMPAT"),
        (Self::THP_ON, "THP_ON"),
        (Self::SMT_ON, "SMT_ON"),
        (Self::CSTATE_DEEP, "CSTATE_DEEP"),
    ];

    pub const fn contains(self, o: HostCheck) -> bool {
        self.0 & o.0 == o.0
    }
}

impl core::ops::BitOr for HostCheck {
    type Output = HostCheck;
    fn bitor(self, o: HostCheck) -> HostCheck {
        HostCheck(self.0 | o.0)
    }
}

impl core::ops::BitAnd for HostCheck {
    type Output = HostCheck;
    fn bitand(self, o: HostCheck) -> HostCheck {
        HostCheck(self.0 & o.0)
    }
}

/// Instruction set of a build/host.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Isa {
    Aarch64,
    X86_64,
}

impl Isa {
    /// The compile target (`cfg!(target_arch)`; non-aarch64 builds use the x86_64 table).
    pub const TARGET: Isa = if cfg!(target_arch = "aarch64") {
        Isa::Aarch64
    } else {
        Isa::X86_64
    };
}

/// Space held by this instance's own segments (`hft-journal::held_by_instance`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct HeldBySelf {
    pub hugepages_1g: u32,
    pub hugepages_2m: u32,
    pub shm_bytes: u64,
}

/// What the instance needs from the host (filled by hft-core per host class).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HostNeeds {
    pub class: HostClass,
    pub min_cpus: u16,
    pub max_cpus: u16,
    pub min_mem_mib: u32,
    pub isa: Isa,
    pub hugepages_1g: u32,
    pub hugepages_2m: u32,
    pub shm_bytes: u64,
    pub held: HeldBySelf,
    pub warm: CpuSet,
    pub irq: CpuSet,
}

/// `check_host` result; `lines` are human-readable (one per hit bit, bit order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostReport {
    pub bits: HostCheck,
    pub hard: HostCheck,
    pub lines: Vec<String>,
}

/// Root of the `/sys` and `/proc` trees to read: the real `/` or a fixture directory. A fixture
/// gives file-system free space in a `statvfs` file (`<mount point> <available bytes>` per line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SysRoot {
    path: PathBuf,
    real: bool,
}

impl SysRoot {
    /// The real root `/` (statvfs(2), counter registers and timing are read for real).
    pub fn real() -> SysRoot {
        SysRoot {
            path: PathBuf::from("/"),
            real: true,
        }
    }

    /// A fixture directory laid out like `/`.
    pub fn fixture(dir: impl Into<PathBuf>) -> SysRoot {
        SysRoot {
            path: dir.into(),
            real: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_real(&self) -> bool {
        self.real
    }

    pub(crate) fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.path.join(rel)).ok()
    }

    pub(crate) fn read_trim(&self, rel: &str) -> Option<String> {
        self.read(rel).map(|s| s.trim().to_string())
    }

    fn list_dir(&self, rel: &str) -> Vec<String> {
        std::fs::read_dir(self.path.join(rel))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn avail_bytes(&self, mount: &str) -> Option<u64> {
        if self.real {
            return statvfs_avail(mount);
        }
        let s = self.read("statvfs")?;
        s.lines().find_map(|l| {
            let mut it = l.split_whitespace();
            (it.next()? == mount).then(|| it.next()?.parse().ok())?
        })
    }
}

#[cfg(target_os = "linux")]
fn statvfs_avail(mount: &str) -> Option<u64> {
    let c = std::ffi::CString::new(mount).ok()?;
    // SAFETY: zeroed statvfs is a valid out-buffer.
    let mut st: libc::statvfs = unsafe { core::mem::zeroed() };
    // SAFETY: valid path and out-buffer.
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
}

#[cfg(not(target_os = "linux"))]
fn statvfs_avail(_: &str) -> Option<u64> {
    None
}

/// Kernel CPU list → every listed CPU (no 256 limit; used for counting).
pub(crate) fn parse_cpu_list(s: &str) -> Option<Vec<u32>> {
    let mut v = Vec::new();
    for part in s.trim().split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?),
            None => {
                let x = part.parse::<u32>().ok()?;
                (x, x)
            }
        };
        if a > b || b > 65_535 {
            return None;
        }
        v.extend(a..=b);
    }
    Some(v)
}

fn cpuset_of(s: Option<String>) -> CpuSet {
    s.and_then(|s| CpuSet::parse_list(&s)).unwrap_or_default()
}

/// The `[x]` choice of a sysfs selector line such as `always [madvise] never`.
pub(crate) fn selected(s: &str) -> Option<&str> {
    let a = s.find('[')?;
    let b = s[a..].find(']')? + a;
    Some(&s[a + 1..b])
}

fn kib_field(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|l| {
        let rest = l.strip_prefix(key)?.strip_prefix(':')?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

/// Classes that check isolation and huge pages as hard items.
fn upgrade_class(c: HostClass) -> bool {
    !matches!(c, HostClass::Dev2 | HostClass::C4)
}

fn hard_set(c: HostClass) -> HostCheck {
    let mut h = HostCheck::CLASS_MISMATCH
        | HostCheck::TSC_NOT_INVARIANT
        | HostCheck::SHM_SHORT
        | HostCheck::MLOCK_FAILED
        | isa::HARD;
    if upgrade_class(c) {
        h = h | HostCheck::HOT_NOT_ISOLATED | HostCheck::WARM_ON_ISOLATED | HostCheck::HUGEPAGES_SHORT;
    }
    h
}

/// Accumulates hit bits with their explanation lines.
pub(crate) struct Hits(Vec<(HostCheck, String)>);

impl Hits {
    pub(crate) fn hit(&mut self, b: HostCheck, why: String) {
        self.0.push((b, why));
    }
}

/// Whether this machine's counter is invariant (calibration step 1), from the real root.
pub(crate) fn counter_invariant_here() -> bool {
    cfg!(target_os = "linux") && isa::tsc_invariant(&SysRoot::real()).is_ok()
}

/// Verifies the host; never modifies anything. `rt.host.mode` is applied by the caller.
pub fn check_host(root: &SysRoot, hot: &CpuSet, hk: &CpuSet, need: &HostNeeds) -> HostReport {
    let _ = hk;
    let mut h = Hits(Vec::new());

    // CLASS_MISMATCH
    let online = root
        .read("sys/devices/system/cpu/online")
        .and_then(|s| parse_cpu_list(&s))
        .unwrap_or_default();
    let n = online.len();
    let mut why = Vec::new();
    if n < need.min_cpus as usize || n > need.max_cpus as usize {
        why.push(format!(
            "{n} online CPUs not in [{}, {}]",
            need.min_cpus, need.max_cpus
        ));
    }
    let mem_mib = root
        .read("proc/meminfo")
        .and_then(|s| kib_field(&s, "MemTotal"))
        .unwrap_or(0)
        / 1024;
    if mem_mib < need.min_mem_mib as u64 {
        why.push(format!("MemTotal {mem_mib} MiB < {} MiB", need.min_mem_mib));
    }
    if need.isa != Isa::TARGET {
        why.push(format!("build ISA {:?} != needed {:?}", Isa::TARGET, need.isa));
    }
    if need.class == HostClass::Metal {
        let mut cores = BTreeSet::new();
        for c in &online {
            let base = format!("sys/devices/system/cpu/cpu{c}/topology");
            let core = root.read_trim(&format!("{base}/core_id")).unwrap_or_default();
            let pkg = root
                .read_trim(&format!("{base}/physical_package_id"))
                .unwrap_or_default();
            cores.insert((pkg, core));
        }
        if cores.len() < need.min_cpus as usize {
            why.push(format!("{} physical cores < {}", cores.len(), need.min_cpus));
        }
    }
    if !why.is_empty() {
        h.hit(HostCheck::CLASS_MISMATCH, why.join("; "));
    }

    // TSC_NOT_INVARIANT
    if let Err(w) = isa::tsc_invariant(root) {
        h.hit(HostCheck::TSC_NOT_INVARIANT, w);
    }

    // Isolation (upgrade classes only)
    let isolated = cpuset_of(root.read("sys/devices/system/cpu/isolated"));
    if upgrade_class(need.class) {
        if !hot.is_subset(&isolated) {
            h.hit(
                HostCheck::HOT_NOT_ISOLATED,
                "hot cores not all in isolated".into(),
            );
        }
        if need.warm.intersects(&isolated) {
            h.hit(HostCheck::WARM_ON_ISOLATED, "a warm core is in isolated".into());
        }
    }
    let nohz = cpuset_of(root.read("sys/devices/system/cpu/nohz_full"));
    if !hot.is_subset(&nohz) {
        h.hit(HostCheck::HOT_NOT_NOHZ, "hot cores not all in nohz_full".into());
    }
    let cmdline = root.read("proc/cmdline").unwrap_or_default();
    let rcu = cmdline
        .split_whitespace()
        .find_map(|t| t.strip_prefix("rcu_nocbs="))
        .and_then(CpuSet::parse_list)
        .unwrap_or_default();
    let hot_irq = hot.union(&need.irq);
    if !hot_irq.is_subset(&rcu) {
        h.hit(
            HostCheck::HOT_NOT_RCU_NOCB,
            "hot or IRQ cores not all in rcu_nocbs".into(),
        );
    }

    // IRQ_ON_HOT
    let mut irqs: Vec<String> = root
        .list_dir("proc/irq")
        .into_iter()
        .filter(|d| d.parse::<u32>().is_ok())
        .collect();
    irqs.sort();
    let on_hot: Vec<String> = irqs
        .into_iter()
        .filter(|i| {
            let aff = cpuset_of(root.read(&format!("proc/irq/{i}/smp_affinity_list")));
            aff.intersects(hot)
        })
        .collect();
    if !on_hot.is_empty() {
        h.hit(
            HostCheck::IRQ_ON_HOT,
            format!("IRQ affinity includes a hot core: {}", on_hot.join(",")),
        );
    }

    // TASK_ON_HOT
    let me = std::process::id().to_string();
    let mut tasks = Vec::new();
    for pid in root
        .list_dir("proc")
        .into_iter()
        .filter(|d| d.parse::<u32>().is_ok())
    {
        if root.real && pid == me {
            continue;
        }
        for tid in root
            .list_dir(&format!("proc/{pid}/task"))
            .into_iter()
            .filter(|d| d.parse::<u32>().is_ok())
        {
            let base = format!("proc/{pid}/task/{tid}");
            let Some(stat) = root.read(&format!("{base}/stat")) else {
                continue;
            };
            let Some(cpu) = stat
                .rfind(')')
                .and_then(|i| stat[i + 1..].split_whitespace().nth(36))
                .and_then(|v| v.parse::<u16>().ok())
            else {
                continue;
            };
            if !hot_irq.contains(cpu) {
                continue;
            }
            let allowed = root
                .read(&format!("{base}/status"))
                .and_then(|s| {
                    s.lines()
                        .find_map(|l| l.strip_prefix("Cpus_allowed_list:").map(|v| v.trim().to_string()))
                })
                .and_then(|v| CpuSet::parse_list(&v))
                .unwrap_or_default();
            if allowed.intersects(&hot_irq) {
                tasks.push(format!("{pid}/{tid}@{cpu}"));
            }
        }
    }
    if !tasks.is_empty() {
        h.hit(
            HostCheck::TASK_ON_HOT,
            format!("tasks ran on hot/IRQ cores: {}", tasks.join(",")),
        );
    }

    // HUGEPAGES_SHORT
    let free = |sz: &str| {
        root.read_trim(&format!("sys/kernel/mm/hugepages/hugepages-{sz}/free_hugepages"))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let need_1g = need.hugepages_1g.saturating_sub(need.held.hugepages_1g) as u64;
    let need_2m = need.hugepages_2m.saturating_sub(need.held.hugepages_2m) as u64;
    let (f1, f2) = (free("1048576kB"), free("2048kB"));
    if f1 < need_1g || f2 < need_2m {
        h.hit(
            HostCheck::HUGEPAGES_SHORT,
            format!("free huge pages 1G {f1} / 2M {f2} < needed {need_1g} / {need_2m}"),
        );
    }

    // SHM_SHORT
    let need_shm = need.shm_bytes.saturating_sub(need.held.shm_bytes);
    if need_shm > 0 {
        let avail = root.avail_bytes("/dev/shm").unwrap_or(0);
        if avail < need_shm {
            h.hit(
                HostCheck::SHM_SHORT,
                format!("/dev/shm available {avail} B < needed {need_shm} B"),
            );
        }
    }

    // MLOCK_FAILED
    let vmlck = root
        .read("proc/self/status")
        .and_then(|s| kib_field(&s, "VmLck"))
        .unwrap_or(0);
    if vmlck == 0 {
        h.hit(HostCheck::MLOCK_FAILED, "VmLck is 0 kB".into());
    }

    // SWAP_ON
    let swaps = root.read("proc/swaps").unwrap_or_default();
    if swaps.lines().skip(1).any(|l| !l.trim().is_empty()) {
        h.hit(HostCheck::SWAP_ON, "swap enabled".into());
    }

    let sysctl_on = |rel: &str| root.read_trim(rel).is_some_and(|v| v != "0");
    if sysctl_on("proc/sys/kernel/numa_balancing") {
        h.hit(HostCheck::NUMA_BALANCING_ON, "kernel.numa_balancing != 0".into());
    }
    if sysctl_on("proc/sys/kernel/timer_migration") {
        h.hit(
            HostCheck::TIMER_MIGRATION_ON,
            "kernel.timer_migration != 0".into(),
        );
    }

    isa::checks(root, &cmdline, &mut h);

    let mut hits = h.0;
    hits.sort_by_key(|(b, _)| b.0);
    let bits = HostCheck(hits.iter().fold(0, |a, (b, _)| a | b.0));
    let name = |b: HostCheck| {
        HostCheck::ALL
            .iter()
            .find(|(x, _)| *x == b)
            .map_or("?", |(_, n)| n)
    };
    let lines = hits.iter().map(|(b, w)| format!("{}: {w}", name(*b))).collect();
    HostReport {
        bits,
        hard: bits & hard_set(need.class),
        lines,
    }
}

/// `/proc/cpuinfo` feature names this binary requires (compile-time `target_feature` set
/// mapped to cpuinfo names; aarch64 only, empty elsewhere).
pub fn required_cpu_features() -> Vec<&'static str> {
    isa::required_features()
}
