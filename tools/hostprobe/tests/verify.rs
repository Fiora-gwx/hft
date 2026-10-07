//! `hostprobe verify` against the `upgrade-c32.toml` expected profile (tools/TESTS.md TOOL-106,
//! TOOL-108–TOOL-110). The fixture root initially matches every item (exit 0); each row changes one.

mod common;

use common::Fx;
use serde_json::{Value, json};

const TRACKING: &str = "root/.hostprobe/exec/chronyc -c tracking.out";
const MAC_B: &str = "root/.hostprobe/imds/meta-data/network/interfaces/macs/06_00_00_00_00_0b";

fn tracking(disp: &str) -> String {
    format!(
        "50484330,PHC0,1,1759800000.000000000,0.000000000,0.000000000,0.000000000,-3.512,0.000,0.010,0.000000000,{disp},1.0,Normal\n"
    )
}

#[test]
fn tool_106_required_mismatch() {
    let base = Fx::new("106-base");
    let r = base.verify();
    assert_eq!(r.code, 0, "initial fixture matches: {}", r.stderr);
    type Setup = fn(&Fx);
    let rows: Vec<(&str, Setup, i32)> = vec![
        (
            "TOOL-106.1",
            |f| {
                f.write("root/sys/devices/system/cpu/isolated", "2-14\n");
            },
            1,
        ),
        (
            "TOOL-106.2",
            |f| {
                f.replace("root/proc/cmdline", " nohz_full=2", "");
            },
            1,
        ),
        (
            "TOOL-106.3",
            |f| {
                f.replace("root/proc/cmdline", "rcu_nocbs=1-15", "rcu_nocbs=2-15");
            },
            1,
        ),
        (
            "TOOL-106.4",
            |f| {
                f.write(
                    "root/sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages",
                    "1023\n",
                );
            },
            1,
        ),
        (
            "TOOL-106.5",
            |f| {
                f.write(
                    "root/sys/kernel/mm/transparent_hugepage/enabled",
                    "[always] madvise never\n",
                );
            },
            1,
        ),
        (
            "TOOL-106.6",
            |f| {
                f.replace("root/.hostprobe/ethtool/ens6.json", "2.13.0g", "2.14.1g");
            },
            1,
        ),
        (
            "TOOL-106.7",
            |f| {
                f.write(&format!("{MAC_B}/device-number"), "3");
            },
            1,
        ),
        (
            "TOOL-106.8",
            |f| {
                f.write("root/proc/sys/net/core/busy_read", "50\n");
            },
            1,
        ),
        (
            "TOOL-106.9",
            |f| {
                f.write("root/proc/sys/net/core/busy_poll", "50\n");
            },
            1,
        ),
        (
            "TOOL-106.10",
            |f| {
                f.write("root/sys/class/net/ens6/napi_defer_hard_irqs", "2\n");
            },
            1,
        ),
        (
            "TOOL-106.11",
            |f| {
                f.write("root/sys/class/net/ens6/gro_flush_timeout", "20000\n");
            },
            1,
        ),
        (
            "TOOL-106.12",
            |f| {
                f.write("root/proc/irq/43/effective_affinity_list", "3\n");
            },
            1,
        ),
        (
            "TOOL-106.13",
            |f| {
                f.write("root/proc/irq/30/effective_affinity_list", "2\n");
            },
            1,
        ),
        (
            "TOOL-106.14",
            |f| {
                f.write("root/proc/irq/61/effective_affinity_list", "0-1\n");
            },
            1,
        ),
        (
            "TOOL-106.15",
            |f| {
                f.write("root/sys/class/net/ens6/queues/tx-3/xps_cpus", "00000040\n");
            },
            1,
        ),
        (
            "TOOL-106.16",
            |f| {
                f.write(TRACKING, &tracking("0.000040000"));
            },
            1,
        ),
        (
            "TOOL-106.17",
            |f| {
                f.write(TRACKING, &tracking("0.000039999"));
            },
            0,
        ),
        (
            "TOOL-106.18",
            |f| {
                f.write(
                    "root/sys/fs/cgroup/hft.slice/hft-sim.slice/cpuset.cpus",
                    "2,16-23\n",
                );
            },
            1,
        ),
        (
            "TOOL-106.19",
            |f| {
                f.replace("root/.hostprobe/ifaddrs", "ens6 10.0.1.10\n", "");
            },
            1,
        ),
        (
            "TOOL-106.20",
            |f| {
                f.remove(&format!("{MAC_B}/ipv4-associations/index"));
            },
            1,
        ),
        (
            "TOOL-106.21",
            |f| {
                f.write("root/.hostprobe/bpf/connect4", "");
            },
            1,
        ),
        (
            "TOOL-106.22",
            |f| {
                f.write(
                    "root/proc/modules",
                    "ena 151552 0 - Live 0x0\nnf_conntrack 172032 0 - Live 0x0\n",
                );
            },
            1,
        ),
        (
            "TOOL-106.23",
            |f| {
                f.write("root/sys/fs/cgroup/hftbatch.slice/cpu.idle", "0\n");
            },
            1,
        ),
        (
            "TOOL-106.24",
            |f| {
                f.replace("root/etc/chrony/chrony.conf", "dpoll 0", "dpoll -1");
            },
            1,
        ),
        ("TOOL-106.25", |_| {}, 0),
    ];
    for (row, setup, code) in rows {
        let f = Fx::new("106");
        setup(&f);
        let r = f.verify();
        assert_eq!(r.code, code, "{row}: {}", r.stderr);
        match row {
            "TOOL-106.1" => assert!(
                r.stderr
                    .lines()
                    .any(|l| l == "cpu.isolated expected=2-15 actual=2-14 required"),
                "{row}: {}",
                r.stderr
            ),
            "TOOL-106.8" => assert!(
                r.stderr.lines().any(|l| l.starts_with("net.core.busy_read ")),
                "{row}: {}",
                r.stderr
            ),
            "TOOL-106.24" => assert!(
                r.stderr
                    .lines()
                    .any(|l| l == "clock.chrony.refclock_phc_poll_hz expected=<=125 actual=126 required"),
                "{row}: {}",
                r.stderr
            ),
            _ => {}
        }
        if code == 1 {
            let v = f.json("v.json");
            assert!(
                v["verify"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|i| i["level"] == "required"),
                "{row}"
            );
        }
    }
}

#[test]
fn tool_108_unreadable_and_input_errors() {
    // TOOL-108.1
    let f = Fx::new("108");
    f.non_root();
    let r = f.verify();
    assert_eq!(r.code, 1, "TOOL-108.1");
    assert!(
        r.stderr
            .lines()
            .any(|l| l == "nic.eni_b.device_index expected=1 actual=unreadable required"),
        "TOOL-108.1: {}",
        r.stderr
    );
    // TOOL-108.2: MAC differs from IMDS (advisory only).
    let f = Fx::new("108");
    f.write("root/sys/class/net/ens6/address", "06:00:00:00:00:ff\n");
    let r = f.verify();
    assert_eq!(r.code, 0, "TOOL-108.2: {}", r.stderr);
    assert!(
        r.stderr
            .lines()
            .any(|l| l == "nic.eni_b.mac_match expected=true actual=false advisory"),
        "TOOL-108.2: {}",
        r.stderr
    );
    let v = f.json("v.json");
    assert_eq!(
        v["verify"],
        json!([{"key": "nic.eni_b.mac_match", "expected": true, "actual": false, "level": "advisory"}]),
        "TOOL-108.2"
    );
    // TOOL-108.3
    let f = Fx::new("108");
    assert_eq!(
        f.run(&["verify", "--expected", "nope.toml", "--out", "v.json"])
            .code,
        2,
        "TOOL-108.3"
    );
    // TOOL-108.4
    f.write("bad.toml", "[[expect]\nkey = ");
    assert_eq!(
        f.run(&["verify", "--expected", "bad.toml", "--out", "v.json"])
            .code,
        2,
        "TOOL-108.4"
    );
    // TOOL-108.5: the parent of --out is a file, so the directory is not writable.
    f.write("file", "x");
    let r = f.run(&["verify", "--expected", "upgrade-c32.toml", "--out", "file/v.json"]);
    assert_eq!(r.code, 2, "TOOL-108.5: {}", r.stderr);
}

#[test]
fn tool_109_pending_reboot() {
    // TOOL-109.1
    let f = Fx::new("109");
    assert_eq!(f.verify().code, 0, "TOOL-109.1");
    assert_eq!(f.json("v.json")["pending_reboot"], false, "TOOL-109.1");
    // TOOL-109.2: the snippet has one more parameter that is not a required item.
    let f = Fx::new("109");
    f.replace(
        "grub/50-hft.cfg",
        "transparent_hugepage=madvise",
        "transparent_hugepage=madvise idle=poll",
    );
    assert_eq!(f.verify().code, 0, "TOOL-109.2");
    assert_eq!(f.json("v.json")["pending_reboot"], true, "TOOL-109.2");
    // TOOL-109.3: the release moved to isolcpus 2-14 (snippet and expected profile); not rebooted.
    let f = Fx::new("109");
    f.replace("grub/50-hft.cfg", "domain,2-15", "domain,2-14");
    f.replace("upgrade-c32.toml", "eq = \"2-15\"", "eq = \"2-14\"");
    let r = f.verify();
    assert_eq!(r.code, 1, "TOOL-109.3");
    assert!(
        r.stderr
            .lines()
            .any(|l| l == "cpu.isolated expected=2-14 actual=2-15 required"),
        "TOOL-109.3: {}",
        r.stderr
    );
    assert_eq!(f.json("v.json")["pending_reboot"], true, "TOOL-109.3");
}

#[test]
fn tool_110_verify_section() {
    let f = Fx::new("110");
    f.write("root/proc/sys/net/core/busy_read", "50\n");
    assert_eq!(f.run(&["collect", "--out", "c.json"]).code, 0);
    assert_eq!(f.verify().code, 1);
    let mut v = f.json("v.json");
    assert_eq!(
        v["verify"],
        json!([{"key": "net.core.busy_read", "expected": 0, "actual": 50, "level": "required"}])
    );
    v.as_object_mut().unwrap().remove("verify");
    assert_eq!(hostprobe::json::pretty(&v), f.read("c.json"));
    let c: Value = f.json("c.json");
    assert_eq!(v["host_profile_hash"], c["host_profile_hash"]);
}
