//! `hostprobe collect`: reads every item of the tools/README.md "hostprobe" collection table and
//! builds the HostProfile JSON. A value that cannot be read is the string `"unreadable"`; its reason
//! is in the top-level `unreadable` map under the dotted key. A file that does not exist on this
//! host (feature absent) is `null`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use crate::cpus;
use crate::host::Host;
use crate::json;

/// Inputs of a collection that come from the expected profile (or the defaults).
#[derive(Debug, Clone)]
pub struct Cfg {
    /// NIC role (`eni_a`, `eni_b`, `eni_c`, `eni_b2`) → IMDS device index.
    pub roles: BTreeMap<String, u32>,
    /// IRQ-dedicated cores; `None` = the cores the ENI-B IRQs land on.
    pub irq_cpus: Option<BTreeSet<u32>>,
    /// Registered keepawake thread names (at most one per IRQ-dedicated core is expected).
    pub keepawake: Vec<String>,
    /// Text of the release's GRUB snippet (`pending_reboot`); `None` = no snippet.
    pub grub: Option<String>,
}

impl Default for Cfg {
    fn default() -> Cfg {
        Cfg {
            roles: [("eni_a", 0), ("eni_b", 1), ("eni_c", 2), ("eni_b2", 3)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            irq_cpus: None,
            keepawake: Vec::new(),
            grub: None,
        }
    }
}

/// Unreadable marker value.
pub const UNREADABLE: &str = "unreadable";
/// `/dev/cpu_dma_latency` value when no process holds a constraint (PM_QOS_CPU_LATENCY_DEFAULT_VALUE).
pub const DMA_LATENCY_DEFAULT: i64 = 2_000_000_000;

struct Ctx<'a> {
    h: &'a mut dyn Host,
    unreadable: BTreeMap<String, String>,
}

impl Ctx<'_> {
    fn bad(&mut self, key: &str, why: impl Into<String>) -> Value {
        self.unreadable.insert(key.to_string(), why.into());
        Value::String(UNREADABLE.into())
    }

    /// Trimmed file text; missing → `null`; other errors → unreadable.
    fn text(&mut self, key: &str, path: &str) -> Value {
        match self.h.read(path) {
            Ok(s) => Value::String(s.trim().to_string()),
            Err(e) if e == "not found" => Value::Null,
            Err(e) => self.bad(key, format!("{path}: {e}")),
        }
    }

    /// File parsed as an integer (non-integer text stays a string).
    fn int(&mut self, key: &str, path: &str) -> Value {
        match self.text(key, path) {
            Value::String(s) if s != UNREADABLE => {
                s.parse::<i64>().map(Value::from).unwrap_or(Value::String(s))
            }
            v => v,
        }
    }

    /// File holding a CPU list, normalised.
    fn cpulist(&mut self, key: &str, path: &str) -> Value {
        match self.text(key, path) {
            Value::String(s) if s != UNREADABLE => Value::String(cpus::normalize(&s)),
            v => v,
        }
    }

    fn imds(&mut self, key: &str, path: &str, missing_null: bool) -> Value {
        match self.h.imds(path) {
            Ok(s) => Value::String(s),
            Err(e) if missing_null && e == "HTTP 404" => Value::Null,
            Err(e) => self.bad(key, format!("imdsv2 {path}: {e}")),
        }
    }
}

fn s(v: &Value) -> Option<&str> {
    v.as_str().filter(|x| *x != UNREADABLE)
}

/// Runs the collection.
pub fn collect(h: &mut dyn Host, cfg: &Cfg) -> Value {
    let mut cx = Ctx {
        h,
        unreadable: BTreeMap::new(),
    };
    let mut p = Map::new();
    p.insert("schema".into(), json!(1));
    p.insert("instance".into(), instance(&mut cx));
    let (cmdline, tokens) = cmdline(&mut cx);
    p.insert("kernel".into(), kernel(&mut cx));
    let cpu = cpu(&mut cx);
    p.insert("memory".into(), memory(&mut cx));
    let (nic, ifnames) = nic(&mut cx, cfg);
    let irq = irq(&mut cx, &ifnames);
    p.insert("net".into(), net(&mut cx));
    p.insert("kthreads".into(), kthreads(&mut cx));
    p.insert("resctrl".into(), resctrl(&mut cx));
    p.insert("clock".into(), clock(&mut cx));
    let isolated = s(&cpu["isolated"]).and_then(cpus::parse_list).unwrap_or_default();
    let irq_cpus = cfg.irq_cpus.clone().unwrap_or_else(|| {
        irq["eni_b"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| s(&x["effective_affinity_list"]).and_then(cpus::parse_list))
            .flatten()
            .collect()
    });
    let occ = occupancy(&mut cx, cfg, &irq_cpus, &isolated, &cmdline, &cpu);
    p.insert("occupancy".into(), occ);
    p.insert("guard".into(), guard(&mut cx));
    p.insert("imds".into(), imds_lock(&mut cx));
    p.insert("cmdline".into(), cmdline);
    p.insert("cpu".into(), cpu);
    p.insert("nic".into(), nic);
    p.insert("irq".into(), irq);
    let pending = cfg
        .grub
        .as_deref()
        .map(|g| pending_reboot(g, &tokens))
        .unwrap_or(false);
    p.insert("pending_reboot".into(), json!(pending));
    p.insert(
        "unreadable".into(),
        Value::Object(
            cx.unreadable
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect(),
        ),
    );
    let mut v = Value::Object(p);
    let hash = json::profile_hash(&v);
    v["host_profile_hash"] = json!(hash);
    v
}

fn instance(cx: &mut Ctx) -> Value {
    json!({
        "type": cx.imds("instance.type", "meta-data/instance-type", false),
        "az_id": cx.imds("instance.az_id", "meta-data/placement/availability-zone-id", false),
        "placement_group": cx.imds("instance.placement_group", "meta-data/placement/group-name", true),
        "ami_id": cx.imds("instance.ami_id", "meta-data/ami-id", false),
        "source": "imdsv2",
    })
}

fn kernel(cx: &mut Ctx) -> Value {
    json!({
        "release": cx.text("kernel.release", "/proc/sys/kernel/osrelease"),
        "source": "/proc/sys/kernel/osrelease (uname -r)",
    })
}

/// `/proc/cmdline` section and its tokens.
fn cmdline(cx: &mut Ctx) -> (Value, Vec<String>) {
    let raw = cx.text("cmdline.raw", "/proc/cmdline");
    let text = s(&raw).unwrap_or("").to_string();
    let tokens: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    let get = |k: &str| -> Option<String> {
        tokens
            .iter()
            .rev()
            .find_map(|t| t.strip_prefix(&format!("{k}=")).map(str::to_string))
    };
    let has = |k: &str| tokens.iter().any(|t| t == k);
    let list = |k: &str| {
        get(k)
            .map(|v| Value::String(cpus::normalize(&v)))
            .unwrap_or(Value::Null)
    };
    let opt = |k: &str| get(k).map(Value::String).unwrap_or(Value::Null);
    let v = if raw.as_str() == Some(UNREADABLE) {
        raw.clone()
    } else {
        json!({
            "raw": raw,
            "isolcpus": list("isolcpus"),
            "nohz_full": list("nohz_full"),
            "rcu_nocbs": list("rcu_nocbs"),
            "irqaffinity": list("irqaffinity"),
            "idle": opt("idle"),
            "mitigations": opt("mitigations"),
            "nosmt": has("nosmt"),
            "nohlt": has("nohlt"),
            "cpuidle_off": opt("cpuidle.off"),
            "transparent_hugepage": opt("transparent_hugepage"),
            "source": "/proc/cmdline",
        })
    };
    (v, tokens)
}

fn cpu(cx: &mut Ctx) -> Value {
    let info = match cx.h.read("/proc/cpuinfo") {
        Ok(t) => Some(t),
        Err(e) => {
            cx.bad("cpu.model", format!("/proc/cpuinfo: {e}"));
            None
        }
    };
    let field = |name: &str| -> Option<String> {
        info.as_deref()?.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == name).then(|| v.trim().to_string())
        })
    };
    let model = match &info {
        None => json!(UNREADABLE),
        Some(_) => match field("model name") {
            Some(m) => json!(m),
            None => json!(format!(
                "implementer {} part {}",
                field("CPU implementer").unwrap_or_default(),
                field("CPU part").unwrap_or_default()
            )),
        },
    };
    let flags: BTreeSet<String> = field("Features")
        .or_else(|| field("flags"))
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let smt = match cx.text("cpu.smt_active", "/sys/devices/system/cpu/smt/active") {
        Value::String(x) if x == "1" => json!(true),
        Value::String(x) if x == UNREADABLE => json!(UNREADABLE),
        _ => json!(false),
    };
    let clocksrc = cx.text(
        "cpu.clocksource",
        "/sys/devices/system/clocksource/clocksource0/current_clocksource",
    );
    json!({
        "model": model,
        "flags": flags.into_iter().collect::<Vec<_>>(),
        "online": cx.cpulist("cpu.online", "/sys/devices/system/cpu/online"),
        "isolated": cx.cpulist("cpu.isolated", "/sys/devices/system/cpu/isolated"),
        "nohz_full": cx.cpulist("cpu.nohz_full", "/sys/devices/system/cpu/nohz_full"),
        "smt_active": smt,
        "cpuidle_driver": cx.text("cpu.cpuidle_driver", "/sys/devices/system/cpu/cpuidle/current_driver"),
        "clocksource": clocksrc,
        "source": "/proc/cpuinfo, /sys/devices/system/cpu, /sys/devices/system/clocksource",
    })
}

const VM_KEYS: &[&str] = &[
    "dirty_background_ratio",
    "dirty_ratio",
    "max_map_count",
    "min_free_kbytes",
    "overcommit_memory",
    "stat_interval",
    "swappiness",
    "zone_reclaim_mode",
];

fn memory(cx: &mut Ctx) -> Value {
    let mut hp = Map::new();
    for d in cx.h.list("/sys/kernel/mm/hugepages").unwrap_or_default() {
        if let Some(size) = d.strip_prefix("hugepages-") {
            let base = format!("/sys/kernel/mm/hugepages/{d}");
            let total = cx.int(
                &format!("memory.hugepages.{size}.total"),
                &format!("{base}/nr_hugepages"),
            );
            let free = cx.int(
                &format!("memory.hugepages.{size}.free"),
                &format!("{base}/free_hugepages"),
            );
            hp.insert(size.to_string(), json!({"total": total, "free": free}));
        }
    }
    let thp = match cx.text("memory.thp", "/sys/kernel/mm/transparent_hugepage/enabled") {
        Value::String(t) if t != UNREADABLE => t
            .split_whitespace()
            .find_map(|w| w.strip_prefix('[').and_then(|x| x.strip_suffix(']')))
            .map(|x| json!(x))
            .unwrap_or(json!(t)),
        v => v,
    };
    let swaps = match cx.text("memory.swap_devices", "/proc/swaps") {
        Value::String(t) if t != UNREADABLE => json!(t.lines().count().saturating_sub(1)),
        v => v,
    };
    let mut vm = Map::new();
    for k in VM_KEYS {
        vm.insert(
            (*k).to_string(),
            cx.int(&format!("memory.vm.{k}"), &format!("/proc/sys/vm/{k}")),
        );
    }
    json!({
        "hugepages": hp,
        "thp": thp,
        "numa_balancing": cx.int("memory.numa_balancing", "/proc/sys/kernel/numa_balancing"),
        "swap_devices": swaps,
        "vm": vm,
        "ksm_run": cx.int("memory.ksm_run", "/sys/kernel/mm/ksm/run"),
        "source": "/sys/kernel/mm, /proc/sys/vm, /proc/swaps",
    })
}

/// NIC section and `role → ifname` for the IRQ classification.
fn nic(cx: &mut Ctx, cfg: &Cfg) -> (Value, BTreeMap<String, String>) {
    let mut out = Map::new();
    let mut ifnames = BTreeMap::new();
    // device index → mac (IMDS).
    let macs: Result<BTreeMap<u32, String>, String> = (|| {
        let list = cx.h.imds("meta-data/network/interfaces/macs/")?;
        let mut m = BTreeMap::new();
        for e in list
            .lines()
            .map(|l| l.trim().trim_end_matches('/'))
            .filter(|l| !l.is_empty())
        {
            let dev =
                cx.h.imds(&format!("meta-data/network/interfaces/macs/{e}/device-number"))?;
            if let Ok(d) = dev.trim().parse::<u32>() {
                m.insert(d, e.to_string());
            }
        }
        Ok(m)
    })();
    let addrs = cx.h.if_addrs().unwrap_or_default();
    let mut role_mac: BTreeMap<String, String> = BTreeMap::new();
    for (role, dev) in &cfg.roles {
        let key = format!("nic.{role}");
        let mut r = Map::new();
        r.insert("device_index".into(), json!(dev));
        let mac = match &macs {
            Err(e) => {
                let why = format!("imdsv2: {e}");
                for f in [
                    "device_index",
                    "present",
                    "mac",
                    "mac_match",
                    "ifname",
                    "private_ips",
                ] {
                    let v = cx.bad(&format!("{key}.{f}"), why.clone());
                    r.insert(f.into(), v);
                }
                nic_fields_fill(&mut r, &json!(UNREADABLE));
                out.insert(role.clone(), Value::Object(r));
                continue;
            }
            Ok(m) => m.get(dev).cloned(),
        };
        let Some(mac) = mac else {
            r.insert("present".into(), json!(false));
            for f in ["mac", "mac_match", "ifname", "private_ips"] {
                r.insert(f.into(), Value::Null);
            }
            nic_fields_fill(&mut r, &Value::Null);
            out.insert(role.clone(), Value::Object(r));
            continue;
        };
        r.insert("present".into(), json!(true));
        role_mac.insert(role.clone(), mac.clone());
        let ips: Vec<String> =
            cx.h.imds(&format!("meta-data/network/interfaces/macs/{mac}/local-ipv4s"))
                .map(|t| {
                    t.lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect()
                })
                .unwrap_or_default();
        // Interface: the one carrying a private IP of this ENI, else the one with this MAC.
        let mut ifname = addrs
            .iter()
            .find(|(_, a)| ips.contains(a))
            .map(|(i, _)| i.clone());
        if ifname.is_none() {
            for i in cx.h.list("/sys/class/net").unwrap_or_default() {
                if cx
                    .h
                    .read(&format!("/sys/class/net/{i}/address"))
                    .map(|a| a.trim().to_string())
                    .ok()
                    == Some(mac.clone())
                {
                    ifname = Some(i);
                    break;
                }
            }
        }
        r.insert("mac".into(), json!(mac));
        r.insert("private_ips".into(), json!(ips));
        let Some(ifn) = ifname else {
            let v = cx.bad(
                &format!("{key}.ifname"),
                "no interface carries this ENI's IP or MAC",
            );
            r.insert("ifname".into(), v);
            r.insert("mac_match".into(), Value::Null);
            nic_fields_fill(&mut r, &json!(UNREADABLE));
            out.insert(role.clone(), Value::Object(r));
            continue;
        };
        ifnames.insert(role.clone(), ifn.clone());
        let sys_mac = cx.text(
            &format!("{key}.mac_match"),
            &format!("/sys/class/net/{ifn}/address"),
        );
        r.insert("mac_match".into(), json!(s(&sys_mac) == Some(mac.as_str())));
        r.insert("ifname".into(), json!(ifn));
        match cx.h.ethtool(&ifn) {
            Ok(Value::Object(m)) => {
                for k in ETH_KEYS {
                    let v = match m.get(*k) {
                        Some(Value::String(x)) if x == UNREADABLE => {
                            cx.bad(&format!("{key}.{k}"), format!("SIOCETHTOOL {ifn}: ioctl failed"))
                        }
                        Some(v) => v.clone(),
                        None => cx.bad(&format!("{key}.{k}"), format!("SIOCETHTOOL {ifn}: not reported")),
                    };
                    r.insert((*k).to_string(), v);
                }
            }
            Ok(_) | Err(_) => {
                for k in ETH_KEYS {
                    let v = cx.bad(&format!("{key}.{k}"), format!("SIOCETHTOOL {ifn} failed"));
                    r.insert((*k).to_string(), v);
                }
            }
        }
        let phc = r
            .get("phc_index")
            .and_then(Value::as_i64)
            .filter(|i| *i >= 0)
            .map(|i| json!(format!("/dev/ptp{i}")));
        r.insert("phc".into(), phc.unwrap_or(Value::Null));
        let base = format!("/sys/class/net/{ifn}");
        r.insert(
            "napi_defer_hard_irqs".into(),
            cx.int(
                &format!("{key}.napi_defer_hard_irqs"),
                &format!("{base}/napi_defer_hard_irqs"),
            ),
        );
        r.insert(
            "gro_flush_timeout".into(),
            cx.int(
                &format!("{key}.gro_flush_timeout"),
                &format!("{base}/gro_flush_timeout"),
            ),
        );
        r.insert(
            "threaded".into(),
            cx.int(&format!("{key}.threaded"), &format!("{base}/threaded")),
        );
        let mut xps = Map::new();
        for q in cx.h.list(&format!("{base}/queues")).unwrap_or_default() {
            if q.starts_with("tx-") {
                let v = match cx.text(&format!("{key}.xps.{q}"), &format!("{base}/queues/{q}/xps_cpus")) {
                    Value::String(m) if m != UNREADABLE => cpus::parse_mask(&m)
                        .map(|x| json!(cpus::format_list(&x)))
                        .unwrap_or(json!(m)),
                    v => v,
                };
                xps.insert(q, v);
            }
        }
        r.insert("xps".into(), Value::Object(xps));
        out.insert(role.clone(), Value::Object(r));
    }
    let imds_err = macs.as_ref().err().cloned();
    out.insert("egress".into(), egress(cx, &ifnames, &role_mac, &addrs, imds_err));
    out.insert(
        "source".into(),
        json!("imdsv2 network/interfaces/macs, SIOCETHTOOL, /sys/class/net, /etc/hft/egress.toml"),
    );
    (Value::Object(out), ifnames)
}

/// Keys of [`Host::ethtool`] copied into a NIC role.
pub const ETH_KEYS: &[&str] = &[
    "driver",
    "driver_version",
    "queues",
    "max_queues",
    "rss_indir_size",
    "coalesce",
    "ring",
    "offload",
    "ntuple_rules",
    "hw_rx_timestamp",
    "phc_index",
    "allowance_exceeded",
];
/// Keys of a NIC role read from sysfs.
const SYSFS_KEYS: &[&str] = &[
    "phc",
    "napi_defer_hard_irqs",
    "gro_flush_timeout",
    "threaded",
    "xps",
];

/// Fields read from the interface: `fill` when the role has no interface (`null`) or the IMDS
/// identification failed (`"unreadable"`).
fn nic_fields_fill(r: &mut Map<String, Value>, fill: &Value) {
    for f in ETH_KEYS.iter().chain(SYSFS_KEYS) {
        r.entry(f.to_string()).or_insert(fill.clone());
    }
}

fn egress(
    cx: &mut Ctx,
    ifnames: &BTreeMap<String, String>,
    role_mac: &BTreeMap<String, String>,
    addrs: &[(String, String)],
    imds_err: Option<String>,
) -> Value {
    let text = match cx.h.read("/etc/hft/egress.toml") {
        Ok(t) => t,
        Err(e) if e == "not found" => return json!([]),
        Err(e) => return cx.bad("nic.egress", format!("/etc/hft/egress.toml: {e}")),
    };
    let t: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => return cx.bad("nic.egress", format!("/etc/hft/egress.toml: {e}")),
    };
    let mut out = Vec::new();
    for list in ["prod", "prod_bulk", "sim"] {
        for e in t.get(list).and_then(|v| v.as_array()).into_iter().flatten() {
            let eni = e.get("eni").and_then(|v| v.as_str()).unwrap_or("");
            let ip = e.get("ip").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let role = format!("eni_{eni}");
            let nic = ifnames.get(&role).cloned();
            let mut on_nic = json!(
                nic.as_ref()
                    .is_some_and(|n| addrs.iter().any(|(i, a)| i == n && *a == ip))
            );
            if let Some(e) = &imds_err {
                on_nic = cx.bad(&format!("nic.egress.{ip}.on_nic"), format!("imdsv2: {e}"));
                let eip = cx.bad(&format!("nic.egress.{ip}.eip"), format!("imdsv2: {e}"));
                out.push(json!({"list": list, "eni": eni, "ip": ip, "nic": UNREADABLE, "on_nic": on_nic, "eip": eip}));
                continue;
            }
            let eip = match role_mac.get(&role) {
                None => Value::Null,
                Some(mac) => {
                    let base = format!("meta-data/network/interfaces/macs/{mac}/ipv4-associations/");
                    match cx.h.imds(&base) {
                        Ok(list) => {
                            let mut found = Value::Null;
                            for pubip in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
                                if cx
                                    .h
                                    .imds(&format!("{base}{pubip}"))
                                    .map(|x| x.trim() == ip)
                                    .unwrap_or(false)
                                {
                                    found = json!(pubip);
                                    break;
                                }
                            }
                            found
                        }
                        Err(e) if e == "HTTP 404" => Value::Null,
                        Err(e) => cx.bad(&format!("nic.egress.{ip}.eip"), format!("imdsv2: {e}")),
                    }
                }
            };
            out.push(json!({"list": list, "eni": eni, "ip": ip, "nic": nic, "on_nic": on_nic, "eip": eip}));
        }
    }
    Value::Array(out)
}

fn irq(cx: &mut Ctx, ifnames: &BTreeMap<String, String>) -> Value {
    let mut groups: BTreeMap<String, Vec<Value>> = ["eni_a", "eni_b", "eni_c", "eni_b2", "managed", "other"]
        .into_iter()
        .map(|k| (k.to_string(), Vec::new()))
        .collect();
    let text = match cx.h.read("/proc/interrupts") {
        Ok(t) => t,
        Err(e) => return cx.bad("irq", format!("/proc/interrupts: {e}")),
    };
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(first) = it.next() else { continue };
        let Some(n) = first.strip_suffix(':').and_then(|x| x.parse::<u32>().ok()) else {
            continue;
        };
        let name = line.split_whitespace().last().unwrap_or("").to_string();
        let mut group = "other".to_string();
        let mut queue = Value::Null;
        for (role, ifn) in ifnames {
            if let Some(rest) = name.strip_prefix(&format!("{ifn}-")) {
                group = role.clone();
                queue = rest
                    .rsplit('-')
                    .next()
                    .and_then(|q| q.parse::<u32>().ok())
                    .map(Value::from)
                    .unwrap_or(Value::Null);
            }
        }
        if group == "other" && name.starts_with("nvme") {
            group = "managed".into();
        }
        let base = format!("/proc/irq/{n}");
        let smp = cx.cpulist(
            &format!("irq.{n}.smp_affinity_list"),
            &format!("{base}/smp_affinity_list"),
        );
        let eff = cx.cpulist(
            &format!("irq.{n}.effective_affinity_list"),
            &format!("{base}/effective_affinity_list"),
        );
        let mut e = json!({"irq": n, "name": name, "smp_affinity_list": smp, "effective_affinity_list": eff});
        if !queue.is_null() {
            e["queue"] = queue;
        }
        groups.entry(group).or_default().push(e);
    }
    let mut out: Map<String, Value> = groups.into_iter().map(|(k, v)| (k, Value::Array(v))).collect();
    let irqb = cx.h.exists("/run/systemd/units/invocation:irqbalance.service");
    out.insert("irqbalance_active".into(), json!(irqb));
    out.insert(
        "source".into(),
        json!("/proc/interrupts, /proc/irq/*/{smp,effective}_affinity_list, systemd"),
    );
    Value::Object(out)
}

fn net(cx: &mut Ctx) -> Value {
    let conntrack = match cx.h.read("/proc/modules") {
        Ok(t) => json!(
            t.lines()
                .any(|l| l.split_whitespace().next() == Some("nf_conntrack"))
        ),
        Err(e) => cx.bad("net.nf_conntrack_loaded", format!("/proc/modules: {e}")),
    };
    json!({
        "core": {
            "busy_poll": cx.int("net.core.busy_poll", "/proc/sys/net/core/busy_poll"),
            "busy_read": cx.int("net.core.busy_read", "/proc/sys/net/core/busy_read"),
            "default_qdisc": cx.text("net.core.default_qdisc", "/proc/sys/net/core/default_qdisc"),
        },
        "ipv4": {
            "tcp_autocorking": cx.int("net.ipv4.tcp_autocorking", "/proc/sys/net/ipv4/tcp_autocorking"),
            "tcp_slow_start_after_idle": cx.int("net.ipv4.tcp_slow_start_after_idle", "/proc/sys/net/ipv4/tcp_slow_start_after_idle"),
        },
        "nf_conntrack_loaded": conntrack,
        "source": "/proc/sys/net, /proc/modules",
    })
}

fn kthreads(cx: &mut Ctx) -> Value {
    json!({
        "workqueue_cpumask": cx.text("kthreads.workqueue_cpumask", "/sys/devices/virtual/workqueue/cpumask"),
        "timer_migration": cx.int("kthreads.timer_migration", "/proc/sys/kernel/timer_migration"),
        "stat_interval": cx.int("kthreads.stat_interval", "/proc/sys/vm/stat_interval"),
        "source": "/sys/devices/virtual/workqueue/cpumask, /proc/sys/kernel, /proc/sys/vm",
    })
}

fn resctrl(cx: &mut Ctx) -> Value {
    let mounted = cx.h.exists("/sys/fs/resctrl/info");
    if !mounted {
        return json!({"mounted": false, "l3_cat": null, "mba": null, "num_closids": null, "hot_schemata": null, "source": "/sys/fs/resctrl"});
    }
    json!({
        "mounted": true,
        "l3_cat": cx.h.exists("/sys/fs/resctrl/info/L3"),
        "mba": cx.h.exists("/sys/fs/resctrl/info/MB"),
        "num_closids": cx.int("resctrl.num_closids", "/sys/fs/resctrl/info/L3/num_closids"),
        "hot_schemata": cx.text("resctrl.hot_schemata", "/sys/fs/resctrl/hot/schemata"),
        "source": "/sys/fs/resctrl",
    })
}

fn chrony(cx: &mut Ctx) -> Value {
    let mut c = Map::new();
    match cx.h.exec("chronyc", &["-c", "tracking"]) {
        Ok(t) => {
            let f: Vec<&str> = t.lines().next().unwrap_or("").split(',').collect();
            let num = |i: usize| f.get(i).and_then(|x| x.trim().parse::<f64>().ok());
            let ns = |x: f64| json!((x * 1e9).round() as i64);
            c.insert(
                "reference".into(),
                json!(f.get(1).map(|x| x.trim()).unwrap_or("")),
            );
            c.insert("leap".into(), json!(f.get(13).map(|x| x.trim()).unwrap_or("")));
            match (num(4), num(10), num(11)) {
                (Some(off), Some(delay), Some(disp)) => {
                    c.insert("offset_ns".into(), ns(off));
                    c.insert("max_error_ns".into(), ns(off.abs() + disp + delay / 2.0));
                }
                _ => {
                    for k in ["offset_ns", "max_error_ns"] {
                        let v = cx.bad(
                            &format!("clock.chrony.{k}"),
                            "chronyc -c tracking: unexpected output",
                        );
                        c.insert(k.into(), v);
                    }
                }
            }
        }
        Err(e) => {
            for k in ["reference", "leap", "offset_ns", "max_error_ns"] {
                let v = cx.bad(&format!("clock.chrony.{k}"), format!("chronyc -c tracking: {e}"));
                c.insert(k.into(), v);
            }
        }
    }
    // PHC refclock and its read rate: `dpoll` of the config's `refclock PHC` line (default 0 =
    // 1 read/s), else the `poll` column of `chronyc -c sources`.
    let mut conf = String::new();
    for p in ["/etc/chrony/chrony.conf", "/etc/chrony.conf"] {
        if let Ok(t) = cx.h.read(p) {
            conf.push_str(&t);
            conf.push('\n');
        }
    }
    for d in cx.h.list("/etc/chrony/conf.d").unwrap_or_default() {
        if let Ok(t) = cx.h.read(&format!("/etc/chrony/conf.d/{d}")) {
            conf.push_str(&t);
            conf.push('\n');
        }
    }
    let refline = conf
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("refclock") && l.split_whitespace().nth(1) == Some("PHC"));
    match cx.h.exec("chronyc", &["-c", "sources"]) {
        Ok(t) => {
            let phc = t.lines().find(|l| {
                let f: Vec<&str> = l.split(',').collect();
                f.first() == Some(&"#") && f.get(2).is_some_and(|n| n.starts_with("PHC"))
            });
            c.insert("refclock_phc".into(), json!(phc.is_some()));
            let dpoll = refline.and_then(|l| {
                let w: Vec<&str> = l.split_whitespace().collect();
                w.iter()
                    .position(|x| *x == "dpoll")
                    .and_then(|i| w.get(i + 1))
                    .and_then(|x| x.parse::<i32>().ok())
            });
            let poll = dpoll.or_else(|| {
                if refline.is_some() {
                    Some(0)
                } else {
                    phc.and_then(|l| l.split(',').nth(4))
                        .and_then(|x| x.trim().parse::<i32>().ok())
                }
            });
            let hz = match (phc.is_some(), poll) {
                (true, Some(p)) => json!(2f64.powi(-p)),
                _ => json!(0),
            };
            c.insert("refclock_phc_poll_hz".into(), normalize_num(hz));
        }
        Err(e) => {
            for k in ["refclock_phc", "refclock_phc_poll_hz"] {
                let v = cx.bad(&format!("clock.chrony.{k}"), format!("chronyc -c sources: {e}"));
                c.insert(k.into(), v);
            }
        }
    }
    c.insert(
        "refclock_line".into(),
        refline.map(|l| json!(l)).unwrap_or(Value::Null),
    );
    Value::Object(c)
}

/// Whole floats as integers (`1.0` → `1`).
fn normalize_num(v: Value) -> Value {
    match v.as_f64() {
        Some(f) if f.fract() == 0.0 && f.abs() < 1e15 => json!(f as i64),
        _ => v,
    }
}

fn clock(cx: &mut Ctx) -> Value {
    let ptp: Vec<String> =
        cx.h.list("/dev")
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d.starts_with("ptp"))
            .map(|d| format!("/dev/{d}"))
            .collect();
    json!({
        "chrony": chrony(cx),
        "ptp_devices": ptp,
        "source": "chronyc -c tracking, chronyc -c sources, chrony refclock line, /dev/ptp*",
    })
}

/// Runnable user-space tasks per core: `(cpu, comm, tid)`. Kernel threads (`PF_KTHREAD`) are per-CPU
/// housekeeping and not counted.
fn runnable(cx: &mut Ctx) -> Vec<(u32, String, u32)> {
    const PF_KTHREAD: u64 = 0x0020_0000;
    let me = std::process::id();
    let mut out = Vec::new();
    for pid in cx.h.list("/proc").unwrap_or_default() {
        let Ok(p) = pid.parse::<u32>() else { continue };
        if p == me {
            continue;
        }
        let tids: Vec<String> =
            cx.h.list(&format!("/proc/{pid}/task"))
                .unwrap_or_else(|_| vec![pid.clone()]);
        for tid in tids {
            let path = if cx.h.exists(&format!("/proc/{pid}/task/{tid}/stat")) {
                format!("/proc/{pid}/task/{tid}/stat")
            } else {
                format!("/proc/{pid}/stat")
            };
            let Ok(st) = cx.h.read(&path) else { continue };
            let (Some(l), Some(r)) = (st.find('('), st.rfind(')')) else {
                continue;
            };
            let comm = st[l + 1..r].to_string();
            let f: Vec<&str> = st[r + 1..].split_whitespace().collect();
            let state = f.first().copied().unwrap_or("");
            let flags = f.get(6).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            let cpu = f.get(36).and_then(|x| x.parse::<u32>().ok());
            if state == "R"
                && flags & PF_KTHREAD == 0
                && let Some(c) = cpu
            {
                out.push((c, comm, tid.parse().unwrap_or(0)));
            }
        }
    }
    out.sort();
    out
}

fn occupancy(
    cx: &mut Ctx,
    cfg: &Cfg,
    irq_cpus: &BTreeSet<u32>,
    isolated: &BTreeSet<u32>,
    cmdline: &Value,
    cpu: &Value,
) -> Value {
    let tasks = runnable(cx);
    let mut keep_seen: BTreeSet<u32> = BTreeSet::new();
    let mut irq_tasks = Vec::new();
    let mut iso_tasks = Vec::new();
    for (c, comm, tid) in &tasks {
        if irq_cpus.contains(c) {
            if cfg.keepawake.contains(comm) && keep_seen.insert(*c) {
                continue;
            }
            irq_tasks.push(json!({"cpu": c, "comm": comm, "tid": tid}));
        } else if isolated.contains(c) {
            iso_tasks.push(json!({"cpu": c, "comm": comm, "tid": tid}));
        }
    }
    // Keep-awake means other than `nohlt` and registered keepawake threads (tools/README.md hostprobe).
    let mut bad = Vec::new();
    if !cmdline["cpuidle_off"].is_null() {
        bad.push(json!("cpuidle.off"));
    }
    if s(&cpu["cpuidle_driver"]) == Some("haltpoll") {
        bad.push(json!("haltpoll"));
    }
    let dma = match cx.h.read_bytes("/dev/cpu_dma_latency") {
        Ok(b) if b.len() >= 4 => json!(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        Ok(b) => match std::str::from_utf8(&b)
            .ok()
            .and_then(|t| i64::from_str_radix(t.trim(), 16).ok())
        {
            Some(v) => json!(v),
            None => cx.bad(
                "occupancy.cpu_dma_latency",
                "unexpected /dev/cpu_dma_latency content",
            ),
        },
        Err(e) if e == "not found" => Value::Null,
        Err(e) => cx.bad("occupancy.cpu_dma_latency", format!("/dev/cpu_dma_latency: {e}")),
    };
    if let Some(v) = dma.as_i64()
        && v < DMA_LATENCY_DEFAULT
    {
        bad.push(json!(format!("cpu_dma_latency={v}")));
    }
    json!({
        "irq_cpus": cpus::format_list(irq_cpus),
        "irq_cores_tasks": irq_tasks,
        "isolated_cores_tasks": iso_tasks,
        "cpu_dma_latency": dma,
        "keepawake_violations": bad,
        "source": "/proc/*/task/*/stat (processor), /proc/cmdline, cpuidle driver, /dev/cpu_dma_latency",
    })
}

const SLICES: &[&str] = &["hft.slice", "hft-core.slice", "hft-sim.slice", "hftbatch.slice"];

fn guard(cx: &mut Ctx) -> Value {
    let mut slices = Map::new();
    for name in SLICES {
        let cands = [format!("/{name}"), format!("/hft.slice/{name}")];
        let found = cands
            .iter()
            .find(|c| cx.h.exists(&format!("/sys/fs/cgroup{c}")))
            .cloned();
        let Some(cg) = found else {
            slices.insert((*name).to_string(), json!({"present": false}));
            continue;
        };
        let base = format!("/sys/fs/cgroup{cg}");
        let parent = cg
            .rsplit_once('/')
            .map(|(p, _)| if p.is_empty() { "/" } else { p })
            .unwrap_or("/")
            .to_string();
        let k = |f: &str| format!("guard.slices.{name}.{f}");
        let cpu_max = cx.text(&k("cpu_max"), &format!("{base}/cpu.max"));
        let quota = match s(&cpu_max) {
            Some(m) => match m.split_whitespace().next() {
                Some("max") | None => Value::Null,
                Some(q) => q.parse::<i64>().map(Value::from).unwrap_or(Value::Null),
            },
            None => cpu_max.clone(),
        };
        slices.insert(
            (*name).to_string(),
            json!({
                "present": true,
                "path": cg,
                "parent": parent,
                "cpu_max": cpu_max,
                "cpu_quota_us": quota,
                "cpu_weight": cx.int(&k("cpu_weight"), &format!("{base}/cpu.weight")),
                "cpu_idle": cx.int(&k("cpu_idle"), &format!("{base}/cpu.idle")),
                "allowed_cpus": cx.cpulist(&k("allowed_cpus"), &format!("{base}/cpuset.cpus")),
                "memory_max": cx.text(&k("memory_max"), &format!("{base}/memory.max")),
                "memory_swap_max": cx.text(&k("memory_swap_max"), &format!("{base}/memory.swap.max")),
                "io_max": cx.text(&k("io_max"), &format!("{base}/io.max")),
            }),
        );
    }
    let first = |v: Value| match v {
        Value::String(t) if t != UNREADABLE => json!(t.lines().next().unwrap_or("")),
        o => o,
    };
    let cpu_p = cx.text("guard.pressure.cpu", "/sys/fs/cgroup/system.slice/cpu.pressure");
    let mem_p = cx.text(
        "guard.pressure.memory",
        "/sys/fs/cgroup/system.slice/memory.pressure",
    );
    json!({
        "slices": slices,
        "subtree_control": cx.text("guard.subtree_control", "/sys/fs/cgroup/cgroup.subtree_control"),
        "pressure": {"cpu": first(cpu_p), "memory": first(mem_p)},
        "dev2_guard_active": cx.h.exists("/run/systemd/units/invocation:hft-dev2-guard.service"),
        "source": "/sys/fs/cgroup, systemd",
    })
}

fn imds_lock(cx: &mut Ctx) -> Value {
    let mut bpf = Map::new();
    for (k, v6) in [("connect4", false), ("connect6", true)] {
        let v = match cx.h.bpf_connect_progs(v6) {
            Ok(names) => json!(names),
            Err(e) => cx.bad(&format!("imds.bpf.{k}"), format!("bpf(BPF_PROG_QUERY) {k}: {e}")),
        };
        bpf.insert(k.into(), v);
    }
    bpf.insert(
        "pinned".into(),
        json!(cx.h.list("/sys/fs/bpf/hft-imds").unwrap_or_default()),
    );
    let tables = match cx.h.nft_tables() {
        Ok(t) => json!(t),
        Err(e) => cx.bad("imds.nft.tables", format!("nftables netlink: {e}")),
    };
    json!({
        "bpf": bpf,
        "nft": {"tables": tables},
        "source": "bpf(BPF_PROG_QUERY) on the root cgroup, /sys/fs/bpf/hft-imds, nftables netlink",
    })
}

/// `true` when a parameter of the GRUB snippet's kernel command line is not in `/proc/cmdline`.
pub fn pending_reboot(grub: &str, cmdline: &[String]) -> bool {
    for line in grub.lines() {
        let l = line.trim();
        let Some((k, v)) = l.split_once('=') else { continue };
        if !k.trim().starts_with("GRUB_CMDLINE_LINUX") {
            continue;
        }
        let v = v.trim().trim_matches('"');
        for tok in v.split_whitespace() {
            if tok.starts_with('$') {
                continue;
            }
            if !cmdline.iter().any(|c| c == tok) {
                return true;
            }
        }
    }
    false
}
