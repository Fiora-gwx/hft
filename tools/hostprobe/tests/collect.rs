//! `hostprobe collect` (tools/TESTS.md TOOL-100–TOOL-104) on the `c32-root` fixture root.

mod common;

use std::collections::BTreeSet;

use common::{Fx, at};
use hostprobe::collect::{Cfg, collect};
use hostprobe::host::FixtureHost;
use serde_json::{Value, json};

#[test]
fn tool_100_categories_present() {
    let f = Fx::new("100");
    let r = f.run(&["collect", "--out", "a.json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let p = f.json("a.json");
    let rows: Vec<(&str, &str, Value)> = vec![
        ("TOOL-100.1", "instance.type", json!("c8g.8xlarge")),
        ("TOOL-100.1", "instance.az_id", json!("apne1-az4")),
        ("TOOL-100.1", "instance.source", json!("imdsv2")),
        ("TOOL-100.2", "cmdline.isolcpus", json!("2-15")),
        ("TOOL-100.2", "cmdline.source", json!("/proc/cmdline")),
        ("TOOL-100.3", "cpu.isolated", json!("2-15")),
        ("TOOL-100.3", "cpu.smt_active", json!(false)),
        ("TOOL-100.3", "cpu.clocksource", json!("arch_sys_counter")),
        ("TOOL-100.4", "memory.hugepages.2048kB.total", json!(1024)),
        ("TOOL-100.4", "memory.thp", json!("madvise")),
        ("TOOL-100.5", "nic.eni_b.ifname", json!("ens6")),
        ("TOOL-100.5", "nic.eni_b.device_index", json!(1)),
        ("TOOL-100.5", "nic.eni_b.queues", json!(8)),
        ("TOOL-100.5", "nic.eni_b.allowance_exceeded.bw_in", json!(0)),
        ("TOOL-100.7", "net.core.busy_poll", json!(0)),
        ("TOOL-100.7", "net.nf_conntrack_loaded", json!(false)),
        ("TOOL-100.8", "kthreads.workqueue_cpumask", json!("0001")),
        ("TOOL-100.9", "resctrl.mounted", json!(false)),
        ("TOOL-100.10", "clock.chrony.max_error_ns", json!(10010)),
        ("TOOL-100.10", "clock.chrony.refclock_phc_poll_hz", json!(1)),
        ("TOOL-100.11", "occupancy.irq_cores_tasks", json!([])),
        ("TOOL-100.12", "guard.slices.hftbatch.slice.cpu_idle", json!(1)),
        ("TOOL-100.12", "guard.slices.hftbatch.slice.parent", json!("/")),
        ("TOOL-100.13", "imds.bpf.connect4", json!(["hft_imds4"])),
        ("TOOL-100.13", "imds.bpf.connect6", json!(["hft_imds6"])),
    ];
    for (row, key, want) in rows {
        assert_eq!(at(&p, key), want, "{row}: {key}");
    }
    // TOOL-100.6: every ENI-B queue IRQ lands on core 1; NVMe IRQs are listed separately.
    let b = p["irq"]["eni_b"].as_array().unwrap();
    assert_eq!(b.len(), 8, "TOOL-100.6");
    assert!(
        b.iter().all(|e| e["effective_affinity_list"] == "1"),
        "TOOL-100.6"
    );
    let managed: Vec<&str> = p["irq"]["managed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(managed, ["nvme0q0", "nvme0q1", "nvme0q2"], "TOOL-100.6");
    // TOOL-100.14: every egress address is on its NIC and has its EIP.
    let eg = p["nic"]["egress"].as_array().unwrap();
    assert_eq!(eg.len(), 2, "TOOL-100.14");
    assert_eq!(eg[0]["ip"], "10.0.1.10");
    assert_eq!(eg[0]["on_nic"], true, "TOOL-100.14");
    assert_eq!(eg[0]["eip"], "54.0.0.10", "TOOL-100.14");
    assert_eq!(eg[1]["on_nic"], true, "TOOL-100.14");
    assert_eq!(eg[1]["eip"], "54.0.0.20", "TOOL-100.14");
    assert_eq!(p["unreadable"], json!({}));
}

#[test]
fn tool_101_canonical_json_hash() {
    let f = Fx::new("101");
    assert_eq!(f.run(&["collect", "--out", "a.json"]).code, 0);
    assert_eq!(f.run(&["collect", "--out", "b.json"]).code, 0);
    let (a, b) = (f.read("a.json"), f.read("b.json"));
    assert_eq!(a, b);
    let v: Value = serde_json::from_str(&a).unwrap();
    // The file is the sorted-key canonical form of its own content.
    assert_eq!(a, hostprobe::json::pretty(&v));
    let mut keys_sorted = true;
    let mut last = String::new();
    for line in a.lines() {
        if line.starts_with("  \"") {
            let k = line.trim_start().split('"').nth(1).unwrap().to_string();
            keys_sorted &= k > last;
            last = k;
        }
    }
    assert!(keys_sorted, "top-level keys in order");
    let mut without = v.clone();
    without.as_object_mut().unwrap().remove("host_profile_hash");
    let hash = {
        use sha2::Digest;
        let d = sha2::Sha256::digest(hostprobe::json::compact(&without).as_bytes());
        d.iter().map(|x| format!("{x:02x}")).collect::<String>()
    };
    assert_eq!(v["host_profile_hash"], json!(hash));
    assert_eq!(
        serde_json::from_str::<Value>(&b).unwrap()["host_profile_hash"],
        json!(hash)
    );
}

/// Object keys down to item level (`nic.eni_b.coalesce`; an unreadable item is one string, so
/// its sub-keys are not compared; array elements are not entered), `unreadable` excluded.
fn keys(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    if prefix.matches('.').count() >= 3 {
        return;
    }
    if let Value::Object(m) = v {
        for (k, x) in m {
            if prefix.is_empty() && k == "unreadable" {
                continue;
            }
            let p = format!("{prefix}.{k}");
            out.insert(p.clone());
            keys(x, &p, out);
        }
    }
}

#[test]
fn tool_102_unreadable_marked() {
    let f = Fx::new("102");
    assert_eq!(f.run(&["collect", "--out", "root.json"]).code, 0);
    f.non_root();
    let r = f.run(&["collect", "--out", "a.json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let p = f.json("a.json");
    for k in ["type", "az_id", "placement_group", "ami_id"] {
        assert_eq!(p["instance"][k], "unreadable", "instance.{k}");
        let why = p["unreadable"][format!("instance.{k}")].as_str().unwrap();
        assert!(why.contains("connection refused"), "{why}");
    }
    assert_eq!(p["nic"]["eni_b"]["device_index"], "unreadable");
    let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
    keys(&f.json("root.json"), "", &mut a);
    keys(&p, "", &mut b);
    assert_eq!(a, b);
}

#[test]
fn tool_103_ioctl_and_chronyc_only() {
    let f = Fx::new("103");
    let mut h = FixtureHost::new(&f.root());
    let p = collect(&mut h, &Cfg::default());
    assert_eq!(h.trace.execs, ["chronyc -c tracking", "chronyc -c sources"]);
    for i in ["ens5", "ens6", "ens7"] {
        assert!(h.trace.ioctls.contains(&format!("SIOCETHTOOL {i}")), "{i}");
    }
    assert_eq!(p["nic"]["eni_b"]["driver"], "ena");
}

/// Every object key anywhere in `v`, arrays included.
fn all_keys(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                out.insert(k.clone());
                all_keys(x, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| all_keys(x, out)),
        _ => {}
    }
}

#[test]
fn tool_104_no_connection_checks() {
    let f = Fx::new("104");
    assert!(f.path("root/proc/net/tcp").is_file());
    assert!(f.path("root/proc/700/fd/3").is_file());
    let mut h = FixtureHost::new(&f.root());
    let p = collect(&mut h, &Cfg::default());
    for path in &h.trace.files {
        assert!(!path.starts_with("/proc/net/tcp"), "{path}");
        let parts: Vec<&str> = path.split('/').collect();
        assert!(
            !(parts.len() >= 4 && parts[1] == "proc" && parts[3] == "fd"),
            "{path}"
        );
    }
    let mut k = BTreeSet::new();
    all_keys(&p, &mut k);
    for bad in [
        "connections",
        "sockets",
        "socket",
        "conn",
        "napi_id",
        "local_address",
    ] {
        assert!(!k.contains(bad), "{bad}");
    }
}
